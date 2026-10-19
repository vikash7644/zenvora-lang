//! Minimal framed TCP peer protocol for status and verified block synchronization.
//!
//! This prototype supports block sync only. It has no transaction gossip, peer discovery,
//! authentication, fork choice, or consensus. Bind addresses are explicitly supplied by the operator.

use std::io::{Read, Write};
use std::net::{IpAddr, SocketAddr, TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};

use super::block_store::BlockStore;
use super::chain_block::{execute_block, execute_block_candidate, SignedTransactionEnvelope};
use super::chain_consensus::{add_validator_signature, verify_partial_consensus_proof};
use super::chain_protocol::{BlockHeaderV1, TransactionV1};
use super::tx_pool::TransactionPool;

const PROTOCOL_VERSION: u64 = 1;
const MAX_FRAME_BYTES: usize = 16 * 1024 * 1024;
const PEER_TIMEOUT: Duration = Duration::from_secs(5);

pub fn serve(store_path: String, chain_id: u64, bind_ip: &str, port: u16) -> Result<(), String> {
    serve_inner(store_path, chain_id, None, None, bind_ip, port)
}

pub fn serve_with_pool(
    store_path: String,
    chain_id: u64,
    pool_path: String,
    bind_ip: &str,
    port: u16,
) -> Result<(), String> {
    serve_inner(store_path, chain_id, Some(pool_path), None, bind_ip, port)
}

pub fn serve_with_validator(
    store_path: String,
    chain_id: u64,
    pool_path: String,
    validator_key_path: String,
    bind_ip: &str,
    port: u16,
) -> Result<(), String> {
    serve_inner(store_path, chain_id, Some(pool_path), Some(validator_key_path), bind_ip, port)
}

fn serve_inner(
    store_path: String,
    chain_id: u64,
    pool_path: Option<String>,
    validator_key_path: Option<String>,
    bind_ip: &str,
    port: u16,
) -> Result<(), String> {
    let ip: IpAddr = bind_ip.parse().map_err(|_| "bind IP must be a valid IPv4 or IPv6 address")?;
    let address = SocketAddr::from((ip, port));
    let listener = TcpListener::bind(address)
        .map_err(|error| format!("cannot bind P2P listener at {address}: {error}"))?;
    println!("Zenvora peer service listening at {address}");
    println!("Block sync{} enabled. No peer authentication or full consensus; use only on a trusted network.",
        if pool_path.is_some() { " and transaction gossip" } else { "" });
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                let _ = stream.set_read_timeout(Some(PEER_TIMEOUT));
                let _ = stream.set_write_timeout(Some(PEER_TIMEOUT));
                let result = read_frame(&mut stream)
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes)
                        .map_err(|error| format!("invalid peer message JSON: {error}")))
                    .map(|request| handle_message_with_pool(
                        &request, &store_path, chain_id, pool_path.as_deref(), validator_key_path.as_deref(),
                    ));
                let response = match result {
                    Ok(value) => value,
                    Err(error) => json!({"ok": false, "error": error}),
                };
                if let Err(error) = write_frame(&mut stream, response.to_string().as_bytes()) {
                    eprintln!("P2P response failed: {error}");
                }
            }
            Err(error) => eprintln!("P2P connection failed: {error}"),
        }
    }
    Ok(())
}

pub fn sync_from_peer(store_path: &str, chain_id: u64, peer: &str) -> Result<usize, String> {
    let peer_address: SocketAddr = peer.parse()
        .map_err(|_| "peer must be an IP:port socket address")?;
    let (mut store, mut runtime) = BlockStore::load_from_file(store_path, chain_id)?;
    let status = request_peer(peer_address, &json!({"method":"status"}))?;
    let remote_chain_id = status.get("chainId").and_then(Value::as_u64)
        .ok_or_else(|| "peer status has no valid chain ID".to_string())?;
    if remote_chain_id != chain_id {
        return Err("peer belongs to a different chain ID".into());
    }
    let local_genesis = store.blocks().first()
        .ok_or_else(|| "local store has no genesis block".to_string())?;
    let remote_genesis_hash = status.get("genesisHash").and_then(Value::as_str)
        .ok_or_else(|| "peer status has no genesis header hash".to_string())?;
    if remote_genesis_hash != hex_encode(&local_genesis.header.header_hash()?) {
        return Err("peer genesis header does not match local genesis".into());
    }
    let remote_genesis_root = status.get("genesisStateRoot").and_then(Value::as_str)
        .ok_or_else(|| "peer status has no genesis state fingerprint".to_string())?;
    if remote_genesis_root != hex_encode(&local_genesis.header.state_root) {
        return Err("peer genesis state does not match local genesis".into());
    }
    let remote_height = status.get("height").and_then(Value::as_u64)
        .ok_or_else(|| "peer status has no valid height".to_string())?;
    let local_tip = store.blocks().last().ok_or_else(|| "local store has no tip".to_string())?;
    if remote_height < local_tip.header.height {
        return Err("peer is behind the local chain; rollback/fork choice is not implemented".into());
    }
    if remote_height == local_tip.header.height {
        let remote_hash = status.get("tipHash").and_then(Value::as_str)
            .ok_or_else(|| "peer status has no tip hash".to_string())?;
        if remote_hash != hex_encode(&local_tip.header.header_hash()?) {
            return Err("peer has a different tip at the same height; fork choice is not implemented".into());
        }
        return Ok(0);
    }

    let mut synced = 0_usize;
    let mut height = local_tip.header.height.checked_add(1)
        .ok_or_else(|| "local block height overflow".to_string())?;
    while height <= remote_height {
        let response = request_peer(peer_address, &json!({"method":"get_block","height":height}))?;
        let header_bytes = hex_decode(response.get("header").and_then(Value::as_str)
            .ok_or_else(|| "peer block response is missing header bytes".to_string())?)?;
        let header = BlockHeaderV1::from_canonical_bytes(&header_bytes)?;
        if header.height != height {
            return Err("peer returned a block at the wrong height".into());
        }
        let transaction_values = response.get("transactions").and_then(Value::as_array)
            .ok_or_else(|| "peer block response is missing transaction list".to_string())?;
        let mut transactions = Vec::with_capacity(transaction_values.len());
        for item in transaction_values {
            let public_key = hex_decode(item.get("publicKey").and_then(Value::as_str)
                .ok_or_else(|| "peer transaction is missing public key".to_string())?)?;
            let transaction_bytes = hex_decode(item.get("transaction").and_then(Value::as_str)
                .ok_or_else(|| "peer transaction is missing canonical bytes".to_string())?)?;
            let transaction = TransactionV1::from_canonical_bytes(&transaction_bytes)?;
            transactions.push(SignedTransactionEnvelope { transaction, public_key });
        }
        let previous = store.blocks().last().cloned()
            .ok_or_else(|| "local store has no previous block".to_string())?;
        let receipts = execute_block(
            &mut runtime, &header, &transactions, chain_id, Some(&previous.header),
        )?;
        store.append_executed_block(store_path, &runtime, &header, &transactions, &receipts)?;
        synced += 1;
        height = height.checked_add(1).ok_or_else(|| "block height overflow".to_string())?;
    }
    Ok(synced)
}

/// Send every pending local transaction to a peer that has pool mode enabled.
pub fn gossip_pool_to_peer(store_path: &str, chain_id: u64, pool_path: &str, peer: &str) -> Result<usize, String> {
    let peer_address: SocketAddr = peer.parse()
        .map_err(|_| "peer must be an IP:port socket address")?;
    let (store, runtime) = BlockStore::load_from_file(store_path, chain_id)?;
    let height = store.blocks().last().and_then(|block| block.header.height.checked_add(1))
        .ok_or_else(|| "cannot determine next block height".to_string())?;
    let pool = TransactionPool::load_from_file(pool_path, &runtime, chain_id, height)?;
    let mut accepted = 0_usize;
    for envelope in pool.pending() {
        let response = request_peer(peer_address, &json!({
            "method":"submit_transaction",
            "publicKey":hex_encode(&envelope.public_key),
            "transaction":hex_encode(&envelope.transaction.canonical_bytes()?)
        }))?;
        if response.get("accepted").and_then(Value::as_bool) == Some(true) {
            accepted += 1;
        }
    }
    Ok(accepted)
}

/// Ask a validator peer to verify a candidate block and return its updated signature proof.
pub fn request_block_signature(
    peer: &str,
    header: &BlockHeaderV1,
    transactions: &[SignedTransactionEnvelope],
) -> Result<Vec<u8>, String> {
    let peer_address: SocketAddr = peer.parse()
        .map_err(|_| "peer must be an IP:port socket address")?;
    let encoded_transactions: Vec<Value> = transactions.iter().map(|envelope| json!({
        "publicKey":hex_encode(&envelope.public_key),
        "transaction":envelope.transaction.canonical_bytes().map(|bytes|hex_encode(&bytes)).unwrap_or_default()
    })).collect();
    let response = request_peer(peer_address, &json!({
        "method":"sign_block",
        "header":hex_encode(&header.canonical_bytes()?),
        "transactions":encoded_transactions
    }))?;
    hex_decode(response.get("consensusProof").and_then(Value::as_str)
        .ok_or_else(|| "validator peer response is missing consensus proof".to_string())?)
}

fn request_peer(peer: SocketAddr, request: &Value) -> Result<Value, String> {
    let mut stream = TcpStream::connect_timeout(&peer, PEER_TIMEOUT)
        .map_err(|error| format!("cannot connect to peer {peer}: {error}"))?;
    stream.set_read_timeout(Some(PEER_TIMEOUT)).map_err(|error| error.to_string())?;
    stream.set_write_timeout(Some(PEER_TIMEOUT)).map_err(|error| error.to_string())?;
    write_frame(&mut stream, request.to_string().as_bytes())?;
    let response = serde_json::from_slice::<Value>(&read_frame(&mut stream)?)
        .map_err(|error| format!("invalid peer response JSON: {error}"))?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(response.get("error").and_then(Value::as_str).unwrap_or("peer request failed").to_string());
    }
    response.get("result").cloned().ok_or_else(|| "peer response is missing result".into())
}

fn handle_message(request: &Value, store_path: &str, expected_chain_id: u64) -> Value {
    handle_message_with_pool(request, store_path, expected_chain_id, None, None)
}

fn handle_message_with_pool(
    request: &Value,
    store_path: &str,
    expected_chain_id: u64,
    pool_path: Option<&str>,
    validator_key_path: Option<&str>,
) -> Value {
    match request.get("method").and_then(Value::as_str) {
        Some("status") => {
            let (store, _) = match BlockStore::load_from_file(store_path, expected_chain_id) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":format!("local block store failed verification: {error}")}),
            };
            let (genesis, tip) = match (store.blocks().first(), store.blocks().last()) {
                (Some(genesis), Some(tip)) => (genesis, tip),
                _ => return json!({"ok":false,"error":"block store has no genesis/tip"}),
            };
            json!({"ok":true,"result":{
                "protocolVersion":PROTOCOL_VERSION,
                "chainId":store.chain_id(),
                "blockCount":store.blocks().len(),
                "height":tip.header.height,
                "tipHash":tip.header.header_hash().map(|hash|hex_encode(&hash)).unwrap_or_default(),
                "genesisHash":genesis.header.header_hash().map(|hash|hex_encode(&hash)).unwrap_or_default(),
                "genesisStateRoot":hex_encode(&genesis.header.state_root)
            }})
        }
        Some("get_block") => {
            let height = match request.get("height").and_then(Value::as_u64) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"get_block requires numeric height"}),
            };
            let (store, _) = match BlockStore::load_from_file(store_path, expected_chain_id) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":format!("local block store failed verification: {error}")}),
            };
            let block = match store.blocks().iter().find(|block| block.header.height == height) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"requested block height not found"}),
            };
            let header = match block.header.canonical_bytes() {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let transactions: Vec<Value> = block.transactions.iter().map(|envelope| json!({
                "publicKey":hex_encode(&envelope.public_key),
                "transaction":envelope.transaction.canonical_bytes().map(|bytes|hex_encode(&bytes)).unwrap_or_default()
            })).collect();
            json!({"ok":true,"result":{"header":hex_encode(&header),"transactions":transactions}})
        }
        Some("sign_block") => {
            let key_path = match validator_key_path {
                Some(value) => value,
                None => return json!({"ok":false,"error":"validator signing is disabled on this peer"}),
            };
            let header_bytes = match request.get("header").and_then(Value::as_str).and_then(|value|hex_decode(value).ok()) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"sign_block requires canonical header hex"}),
            };
            let mut header = match BlockHeaderV1::from_canonical_bytes(&header_bytes) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let transaction_values = match request.get("transactions").and_then(Value::as_array) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"sign_block requires a transaction array"}),
            };
            let mut transactions = Vec::with_capacity(transaction_values.len());
            for item in transaction_values {
                let public_key = match item.get("publicKey").and_then(Value::as_str).and_then(|value|hex_decode(value).ok()) {
                    Some(value) => value,
                    None => return json!({"ok":false,"error":"proposal transaction is missing public key"}),
                };
                let transaction_bytes = match item.get("transaction").and_then(Value::as_str).and_then(|value|hex_decode(value).ok()) {
                    Some(value) => value,
                    None => return json!({"ok":false,"error":"proposal transaction is missing canonical bytes"}),
                };
                let transaction = match TransactionV1::from_canonical_bytes(&transaction_bytes) {
                    Ok(value) => value,
                    Err(error) => return json!({"ok":false,"error":error}),
                };
                transactions.push(SignedTransactionEnvelope { transaction, public_key });
            }
            let (store, runtime) = match BlockStore::load_from_file(store_path, expected_chain_id) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":format!("local block store failed verification: {error}")}),
            };
            let previous = match store.blocks().last() {
                Some(value) => value,
                None => return json!({"ok":false,"error":"local store has no tip"}),
            };
            if header.chain_id != expected_chain_id
                || header.height != previous.header.height.saturating_add(1)
                || header.parent_hash != match previous.header.header_hash() {
                    Ok(value) => value,
                    Err(error) => return json!({"ok":false,"error":error}),
                } {
                return json!({"ok":false,"error":"proposal does not extend this peer's current tip"});
            }
            if let Err(error) = verify_partial_consensus_proof(&header, runtime.genesis_balances()) {
                return json!({"ok":false,"error":format!("invalid existing proposal signatures: {error}")});
            }
            let mut candidate = runtime.clone();
            if let Err(error) = execute_block_candidate(
                &mut candidate, &header, &transactions, expected_chain_id, Some(&previous.header),
            ) {
                return json!({"ok":false,"error":format!("candidate block validation failed: {error}")});
            }
            let key = match super::read_chain_key(key_path) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            if let Err(error) = add_validator_signature(&mut header, &key) {
                return json!({"ok":false,"error":error});
            }
            if let Err(error) = verify_partial_consensus_proof(&header, runtime.genesis_balances()) {
                return json!({"ok":false,"error":format!("local validator is not authorized: {error}")});
            }
            json!({"ok":true,"result":{
                "signer":header.proposer,
                "consensusProof":hex_encode(&header.consensus_proof)
            }})
        }
        Some("get_pending_transactions") => {
            let pool_path = match pool_path {
                Some(value) => value,
                None => return json!({"ok":false,"error":"transaction pool is disabled on this peer"}),
            };
            let (store, runtime) = match BlockStore::load_from_file(store_path, expected_chain_id) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":format!("local block store failed verification: {error}")}),
            };
            let height = match store.blocks().last().and_then(|block| block.header.height.checked_add(1)) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"cannot determine next block height"}),
            };
            let pool = match TransactionPool::load_from_file(pool_path, &runtime, expected_chain_id, height) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let transactions: Vec<Value> = pool.pending().iter().map(|envelope| json!({
                "publicKey":hex_encode(&envelope.public_key),
                "transaction":envelope.transaction.canonical_bytes().map(|bytes|hex_encode(&bytes)).unwrap_or_default()
            })).collect();
            json!({"ok":true,"result":{"transactions":transactions,"count":transactions.len()}})
        }
        Some("submit_transaction") => {
            let pool_path = match pool_path {
                Some(value) => value,
                None => return json!({"ok":false,"error":"transaction pool is disabled on this peer"}),
            };
            let public_key = match request.get("publicKey").and_then(Value::as_str).and_then(|value|hex_decode(value).ok()) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"submit_transaction requires publicKey hex"}),
            };
            let transaction_bytes = match request.get("transaction").and_then(Value::as_str).and_then(|value|hex_decode(value).ok()) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"submit_transaction requires canonical transaction hex"}),
            };
            let transaction = match TransactionV1::from_canonical_bytes(&transaction_bytes) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let (store, runtime) = match BlockStore::load_from_file(store_path, expected_chain_id) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":format!("local block store failed verification: {error}")}),
            };
            let height = match store.blocks().last().and_then(|block| block.header.height.checked_add(1)) {
                Some(value) => value,
                None => return json!({"ok":false,"error":"cannot determine next block height"}),
            };
            let mut pool = match TransactionPool::load_from_file(pool_path, &runtime, expected_chain_id, height) {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            let transaction_id = match transaction.transaction_id() {
                Ok(value) => value,
                Err(error) => return json!({"ok":false,"error":error}),
            };
            if pool.pending().iter().any(|entry| entry.transaction.transaction_id().ok() == Some(transaction_id)) {
                return json!({"ok":true,"result":{"accepted":false,"duplicate":true,"transactionId":hex_encode(&transaction_id)}});
            }
            if let Err(error) = pool.submit(&runtime, SignedTransactionEnvelope { transaction, public_key }) {
                return json!({"ok":false,"error":error});
            }
            if let Err(error) = pool.save_to_file(pool_path) {
                return json!({"ok":false,"error":error});
            }
            json!({"ok":true,"result":{"accepted":true,"duplicate":false,"transactionId":hex_encode(&transaction_id),"poolSize":pool.len()}})
        }
        _ => json!({"ok":false,"error":"unsupported peer method"}),
    }
}

fn write_frame(stream: &mut TcpStream, payload: &[u8]) -> Result<(), String> {
    if payload.is_empty() || payload.len() > MAX_FRAME_BYTES {
        return Err("peer frame size is outside the permitted range".into());
    }
    let length = u32::try_from(payload.len()).map_err(|_| "peer frame is too large")?;
    stream.write_all(&length.to_be_bytes()).and_then(|_| stream.write_all(payload))
        .map_err(|error| format!("cannot write peer frame: {error}"))
}

fn read_frame(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut length_bytes = [0_u8; 4];
    stream.read_exact(&mut length_bytes).map_err(|error| format!("cannot read peer frame length: {error}"))?;
    let length = u32::from_be_bytes(length_bytes) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err("peer frame size is outside the permitted range".into());
    }
    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload).map_err(|error| format!("cannot read peer frame: {error}"))?;
    Ok(payload)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.is_empty() || value.len() % 2 != 0
        || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err("invalid lowercase hexadecimal peer field".into());
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        let low = (pair[1] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        output.push(((high << 4) | low) as u8);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{gossip_pool_to_peer, handle_message, handle_message_with_pool, sync_from_peer};
    use super::super::block_store::BlockStore;
    use super::super::chain_block::{execute_block, SignedTransactionEnvelope};
    use super::super::chain_protocol::{address_from_public_key, transaction_merkle_root, BlockHeaderV1, TransactionV1};
    use super::super::chain_consensus::{sign_header_as_proposer, verify_consensus_proof};
    use super::super::chain_runtime::ChainRuntime;
    use super::super::tx_pool::TransactionPool;
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    fn build_store(path: &str, include_transfer: bool) -> (ChainRuntime, BlockStore, SigningKey) {
        let key = SigningKey::from_bytes(&[91; 32]);
        let public_key = key.verifying_key().to_bytes();
        let address = address_from_public_key(&public_key).unwrap();
        let mut allocations = BTreeMap::new();
        allocations.insert(address.clone(), 1_000);
        allocations.insert("bob".to_string(), 0);
        let mut runtime = ChainRuntime::new();
        for (account, balance) in &allocations {
            runtime.set_genesis_balance(account, *balance).unwrap();
        }
        let genesis = BlockHeaderV1 {
            chain_id: 71, height: 0, timestamp_secs: 1_800_000_000, parent_hash: [0; 32],
            transaction_root: transaction_merkle_root(&[]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[]).unwrap(), state_root: runtime.state_root(),
            proposer: address.clone(), consensus_proof: Vec::new(),
        };
        execute_block(&mut runtime, &genesis, &[], 71, None).unwrap();
        let mut store = BlockStore::new(71, allocations).unwrap();
        store.append_executed_block(path, &runtime, &genesis, &[], &[]).unwrap();

        if include_transfer {
            let mut transaction = TransactionV1 {
                chain_id: 71, sender: address, nonce: 0, recipient: "bob".into(),
                amount: 125, max_fee: 3, gas_limit: 21_000, valid_until_height: None,
                signature: Vec::new(),
            };
            transaction.signature = key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
            let envelope = SignedTransactionEnvelope { transaction: transaction.clone(), public_key: public_key.to_vec() };
            let mut candidate = runtime.clone();
            let receipt = candidate.apply_signed_transaction(&transaction, &public_key, 71, 1).unwrap();
            let mut header = BlockHeaderV1 {
                chain_id: 71, height: 1, timestamp_secs: 1_800_000_001,
                parent_hash: genesis.header_hash().unwrap(),
                transaction_root: transaction_merkle_root(&[transaction]).unwrap(),
                receipt_root: ChainRuntime::receipt_root_for(&[receipt]).unwrap(),
                state_root: candidate.state_root(), proposer: address_from_public_key(&public_key).unwrap(),
                consensus_proof: Vec::new(),
            };
            sign_header_as_proposer(&mut header, &key).unwrap();
            let receipts = execute_block(&mut runtime, &header, &[envelope.clone()], 71, Some(&genesis)).unwrap();
            store.append_executed_block(path, &runtime, &header, &[envelope], &receipts).unwrap();
        }
        (runtime, store, key)
    }

    #[test]
    fn peer_status_and_block_messages_are_bounded_and_well_formed() {
        let path = std::env::temp_dir().join(format!("zenvora-peer-status-{}.zvb", std::process::id()));
        let path_text = path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&path);
        let (_, _, _) = build_store(&path_text, true);
        let status = handle_message(&json!({"method":"status"}), &path_text, 71);
        assert_eq!(status["ok"], true);
        assert_eq!(status["result"]["height"], 1);
        let block = handle_message(&json!({"method":"get_block","height":1}), &path_text, 71);
        assert_eq!(block["ok"], true);
        assert_eq!(block["result"]["transactions"].as_array().unwrap().len(), 1);
        let bad = handle_message(&json!({"method":"get_block","height":"bad"}), &path_text, 71);
        assert_eq!(bad["ok"], false);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn remote_validator_signs_only_a_valid_candidate_and_completes_quorum() {
        let suffix = std::process::id();
        let store_path = std::env::temp_dir().join(format!("zenvora-peer-vote-{suffix}.zvb"));
        let key_path = std::env::temp_dir().join(format!("zenvora-peer-vote-{suffix}.zvk"));
        let _ = std::fs::remove_file(&store_path);
        let _ = std::fs::remove_file(&key_path);
        let store_text = store_path.to_string_lossy().to_string();
        let key_text = key_path.to_string_lossy().to_string();

        let proposer_key = SigningKey::from_bytes(&[171; 32]);
        let validator_key = SigningKey::from_bytes(&[172; 32]);
        let proposer_public = proposer_key.verifying_key().to_bytes();
        let validator_public = validator_key.verifying_key().to_bytes();
        let proposer_address = address_from_public_key(&proposer_public).unwrap();
        let validator_address = address_from_public_key(&validator_public).unwrap();
        let mut allocations = BTreeMap::new();
        allocations.insert(proposer_address.clone(), 1_000);
        allocations.insert(validator_address.clone(), 1_000);
        allocations.insert("bob".to_string(), 0);
        let mut runtime = ChainRuntime::new();
        for (address, balance) in &allocations {
            runtime.set_genesis_balance(address, *balance).unwrap();
        }
        let genesis = BlockHeaderV1 {
            chain_id: 71, height: 0, timestamp_secs: 0, parent_hash: [0; 32],
            transaction_root: transaction_merkle_root(&[]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[]).unwrap(),
            state_root: runtime.state_root(), proposer: proposer_address.clone(), consensus_proof: Vec::new(),
        };
        execute_block(&mut runtime, &genesis, &[], 71, None).unwrap();
        let mut store = BlockStore::new(71, allocations).unwrap();
        store.append_executed_block(&store_text, &runtime, &genesis, &[], &[]).unwrap();

        let mut transaction = TransactionV1 {
            chain_id: 71, sender: proposer_address.clone(), nonce: 0, recipient: "bob".into(),
            amount: 100, max_fee: 1, gas_limit: 21_000, valid_until_height: Some(10),
            signature: Vec::new(),
        };
        transaction.signature = proposer_key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
        let envelope = SignedTransactionEnvelope { transaction: transaction.clone(), public_key: proposer_public.to_vec() };
        let mut candidate = runtime.clone();
        let receipt = candidate.apply_signed_transaction(&transaction, &proposer_public, 71, 1).unwrap();
        let mut header = BlockHeaderV1 {
            chain_id: 71, height: 1, timestamp_secs: 1, parent_hash: genesis.header_hash().unwrap(),
            transaction_root: transaction_merkle_root(&[transaction]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[receipt]).unwrap(),
            state_root: candidate.state_root(), proposer: proposer_address, consensus_proof: Vec::new(),
        };
        sign_header_as_proposer(&mut header, &proposer_key).unwrap();

        let key_file = format!(
            "ZENVORA_KEY_V1\nprivate={}\npublic={}\naddress={}\n",
            super::hex_encode(&[172; 32]), super::hex_encode(&validator_public), validator_address
        );
        std::fs::write(&key_path, key_file).unwrap();
        let request = json!({
            "method":"sign_block",
            "header":super::hex_encode(&header.canonical_bytes().unwrap()),
            "transactions":[{
                "publicKey":super::hex_encode(&envelope.public_key),
                "transaction":super::hex_encode(&envelope.transaction.canonical_bytes().unwrap())
            }]
        });
        let response = handle_message_with_pool(&request, &store_text, 71, None, Some(&key_text));
        assert_eq!(response["ok"], true, "{}", response);
        header.consensus_proof = super::hex_decode(response["result"]["consensusProof"].as_str().unwrap()).unwrap();
        verify_consensus_proof(&header, runtime.genesis_balances()).unwrap();

        std::fs::remove_file(store_path).unwrap();
        std::fs::remove_file(key_path).unwrap();
    }

    #[test]
    fn peer_gossip_submits_signed_transactions_to_remote_pool() {
        let suffix = std::process::id();
        let local_store_path = std::env::temp_dir().join(format!("zenvora-gossip-local-{suffix}.zvb"));
        let peer_store_path = std::env::temp_dir().join(format!("zenvora-gossip-peer-{suffix}.zvb"));
        let local_pool_path = std::env::temp_dir().join(format!("zenvora-gossip-local-{suffix}.zvp"));
        let peer_pool_path = std::env::temp_dir().join(format!("zenvora-gossip-peer-{suffix}.zvp"));
        for path in [&local_store_path, &peer_store_path, &local_pool_path, &peer_pool_path] {
            let _ = std::fs::remove_file(path);
        }
        let local_text = local_store_path.to_string_lossy().to_string();
        let peer_text = peer_store_path.to_string_lossy().to_string();
        let local_pool_text = local_pool_path.to_string_lossy().to_string();
        let peer_pool_text = peer_pool_path.to_string_lossy().to_string();
        let (local_runtime, _, key) = build_store(&local_text, false);
        let (peer_runtime, _, _) = build_store(&peer_text, false);
        let public_key = key.verifying_key().to_bytes();
        let mut transaction = TransactionV1 {
            chain_id: 71, sender: address_from_public_key(&public_key).unwrap(), nonce: 0,
            recipient: "bob".into(), amount: 75, max_fee: 1, gas_limit: 21_000,
            valid_until_height: Some(100), signature: Vec::new(),
        };
        transaction.signature = key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
        let envelope = SignedTransactionEnvelope { transaction, public_key: public_key.to_vec() };
        let mut local_pool = TransactionPool::new(71, 1).unwrap();
        local_pool.submit(&local_runtime, envelope).unwrap();
        local_pool.save_to_file(&local_pool_text).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let peer_text_for_thread = peer_text.clone();
        let peer_pool_for_thread = peer_pool_text.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut length_bytes = [0_u8; 4];
            stream.read_exact(&mut length_bytes).unwrap();
            let length = u32::from_be_bytes(length_bytes) as usize;
            let mut bytes = vec![0_u8; length];
            stream.read_exact(&mut bytes).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            let response = handle_message_with_pool(
                &request, &peer_text_for_thread, 71, Some(&peer_pool_for_thread), None,
            ).to_string();
            stream.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });
        assert_eq!(gossip_pool_to_peer(&local_text, 71, &local_pool_text, &peer.to_string()).unwrap(), 1);
        server.join().unwrap();
        let peer_pool = TransactionPool::load_from_file(&peer_pool_text, &peer_runtime, 71, 1).unwrap();
        assert_eq!(peer_pool.len(), 1);
        assert_eq!(peer_runtime.balance("bob"), 0);
        for path in [&local_store_path, &peer_store_path, &local_pool_path, &peer_pool_path] {
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn peer_sync_replays_and_appends_remote_blocks() {
        let remote_path = std::env::temp_dir().join(format!("zenvora-peer-remote-{}.zvb", std::process::id()));
        let local_path = std::env::temp_dir().join(format!("zenvora-peer-local-{}.zvb", std::process::id()));
        let remote_text = remote_path.to_string_lossy().to_string();
        let local_text = local_path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&remote_path);
        let _ = std::fs::remove_file(&local_path);
        let (remote_runtime, remote_store, _) = build_store(&remote_text, true);
        let (local_runtime, mut local_store, _) = build_store(&local_text, false);
        assert_eq!(local_runtime.balance("bob"), 0);
        assert_eq!(remote_store.blocks().len(), 2);
        assert_eq!(remote_runtime.balance("bob"), 125);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let remote_text_for_thread = remote_text.clone();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut length_bytes = [0_u8; 4];
                stream.read_exact(&mut length_bytes).unwrap();
                let length = u32::from_be_bytes(length_bytes) as usize;
                let mut bytes = vec![0_u8; length];
                stream.read_exact(&mut bytes).unwrap();
                let request: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
                let response = handle_message(&request, &remote_text_for_thread, 71).to_string();
                stream.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let synced = sync_from_peer(&local_text, 71, &peer.to_string()).unwrap();
        server.join().unwrap();
        assert_eq!(synced, 1);
        let (loaded, replayed) = BlockStore::load_from_file(&local_text, 71).unwrap();
        assert_eq!(loaded.blocks().len(), 2);
        assert_eq!(replayed.balance("bob"), 125);
        std::fs::remove_file(remote_path).unwrap();
        std::fs::remove_file(local_path).unwrap();
    }

    #[test]
    fn peer_sync_rejects_same_chain_id_with_different_genesis_identity() {
        let local_path = std::env::temp_dir().join(format!(
            "zenvora-peer-genesis-mismatch-{}.zvb",
            std::process::id()
        ));
        let local_text = local_path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&local_path);
        let (_runtime, _store, _key) = build_store(&local_text, false);

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut length_bytes = [0_u8; 4];
            stream.read_exact(&mut length_bytes).unwrap();
            let length = u32::from_be_bytes(length_bytes) as usize;
            let mut request_bytes = vec![0_u8; length];
            stream.read_exact(&mut request_bytes).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&request_bytes).unwrap();
            assert_eq!(request["method"], "status");

            let response = json!({
                "ok": true,
                "result": {
                    "chainId": 71,
                    "genesisHash": "00".repeat(32),
                    "genesisStateRoot": "00".repeat(32),
                    "height": 0
                }
            }).to_string();
            stream.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });

        let error = sync_from_peer(&local_text, 71, &peer.to_string()).unwrap_err();
        server.join().unwrap();
        assert!(
            error.contains("genesis header does not match"),
            "unexpected error: {error}"
        );
        std::fs::remove_file(local_path).unwrap();
    }


    #[test]
    fn peer_sync_rejects_mismatched_genesis_state_fingerprint() {
        let local_path = std::env::temp_dir().join(format!(
            "zenvora-peer-genesis-state-mismatch-{}.zvb",
            std::process::id()
        ));
        let local_text = local_path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&local_path);
        let (_runtime, store, _key) = build_store(&local_text, false);
        let local_genesis_hash = super::hex_encode(
            &store.blocks().first().unwrap().header.header_hash().unwrap()
        );

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut length_bytes = [0_u8; 4];
            stream.read_exact(&mut length_bytes).unwrap();
            let length = u32::from_be_bytes(length_bytes) as usize;
            let mut request_bytes = vec![0_u8; length];
            stream.read_exact(&mut request_bytes).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&request_bytes).unwrap();
            assert_eq!(request["method"], "status");

            let response = json!({
                "ok": true,
                "result": {
                    "chainId": 71,
                    "genesisHash": local_genesis_hash,
                    "genesisStateRoot": "00".repeat(32),
                    "height": 0
                }
            }).to_string();
            stream.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });

        let error = sync_from_peer(&local_text, 71, &peer.to_string()).unwrap_err();
        server.join().unwrap();
        assert!(
            error.contains("genesis state does not match"),
            "unexpected error: {error}"
        );
        std::fs::remove_file(local_path).unwrap();
    }


    #[test]
    fn peer_sync_rejects_status_without_genesis_state_fingerprint() {
        let local_path = std::env::temp_dir().join(format!(
            "zenvora-peer-genesis-state-missing-{}.zvb",
            std::process::id()
        ));
        let local_text = local_path.to_string_lossy().to_string();
        let _ = std::fs::remove_file(&local_path);
        let (_runtime, store, _key) = build_store(&local_text, false);
        let local_genesis_hash = super::hex_encode(
            &store.blocks().first().unwrap().header.header_hash().unwrap()
        );

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut length_bytes = [0_u8; 4];
            stream.read_exact(&mut length_bytes).unwrap();
            let length = u32::from_be_bytes(length_bytes) as usize;
            let mut request_bytes = vec![0_u8; length];
            stream.read_exact(&mut request_bytes).unwrap();
            let request: serde_json::Value = serde_json::from_slice(&request_bytes).unwrap();
            assert_eq!(request["method"], "status");

            let response = json!({
                "ok": true,
                "result": {
                    "chainId": 71,
                    "genesisHash": local_genesis_hash,
                    "height": 0
                }
            }).to_string();
            stream.write_all(&(response.len() as u32).to_be_bytes()).unwrap();
            stream.write_all(response.as_bytes()).unwrap();
        });

        let error = sync_from_peer(&local_text, 71, &peer.to_string()).unwrap_err();
        server.join().unwrap();
        assert!(
            error.contains("peer status has no genesis state fingerprint"),
            "unexpected error: {error}"
        );
        std::fs::remove_file(local_path).unwrap();
    }

}
