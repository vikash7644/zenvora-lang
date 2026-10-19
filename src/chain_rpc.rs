//! Loopback-only, read-only JSON-RPC for the local Zenvora Chain prototype.
//!
//! The server deliberately binds only to 127.0.0.1. It replays the block store on each request,
//! favoring correctness and simplicity over performance. It is not a public RPC service.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use serde_json::{json, Value};

use super::block_store::BlockStore;
use super::chain_block::SignedTransactionEnvelope;
use super::chain_protocol::TransactionV1;
use super::tx_pool::TransactionPool;

const MAX_HTTP_REQUEST_BYTES: usize = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(3);

pub fn serve(store_path: String, chain_id: u64, port: u16) -> Result<(), String> {
    serve_inner(store_path, chain_id, port, None)
}

/// Start loopback RPC with a persisted transaction pool enabled.
pub fn serve_with_pool(store_path: String, chain_id: u64, pool_path: String, port: u16) -> Result<(), String> {
    serve_inner(store_path, chain_id, port, Some(pool_path))
}

fn serve_inner(
    store_path: String,
    chain_id: u64,
    port: u16,
    pool_path: Option<String>,
) -> Result<(), String> {
    if chain_id == 0 {
        return Err("chain ID must be nonzero".into());
    }
    let address = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&address)
        .map_err(|error| format!("cannot bind local RPC at {address}: {error}"))?;
    println!("Zenvora RPC listening at http://{address}");
    println!("Only loopback clients can connect. Press Ctrl+C to stop.");
    if pool_path.is_some() {
        println!("Signed transaction submission is enabled; block production remains a separate validator command.");
    } else {
        println!("Read-only mode: transaction submission is disabled.");
    }
    for incoming in listener.incoming() {
        match incoming {
            Ok(mut stream) => {
                let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
                let _ = stream.set_write_timeout(Some(READ_TIMEOUT));
                match read_http_body(&mut stream) {
                    Ok(body) => {
                        let response = handle_json_rpc_with_pool(
                            &body, &store_path, chain_id, pool_path.as_deref(),
                        ).to_string();
                        if let Err(error) = write_http_response(&mut stream, "200 OK", &response) {
                            // A client that disconnects while reading must not stop the RPC listener.
                            eprintln!("RPC response write failed: {error}");
                        }
                    }
                    Err(error) => {
                        let response = json!({
                            "jsonrpc": "2.0",
                            "id": Value::Null,
                            "error": { "code": -32600, "message": error }
                        }).to_string();
                        if let Err(write_error) = write_http_response(&mut stream, "400 Bad Request", &response) {
                            eprintln!("RPC error-response write failed: {write_error}");
                        }
                    }
                }
            }
            Err(error) => eprintln!("RPC connection failed: {error}"),
        }
    }
    Ok(())
}

fn read_http_body(stream: &mut TcpStream) -> Result<Vec<u8>, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        if buffer.len() > MAX_HTTP_REQUEST_BYTES {
            return Err("HTTP request exceeds maximum size".into());
        }
        if let Some(header_end) = buffer.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = std::str::from_utf8(&buffer[..header_end])
                .map_err(|_| "HTTP headers are not valid UTF-8")?;
            let mut lines = headers.split("\r\n");
            let request_line = lines.next().unwrap_or("");
            if request_line != "POST / HTTP/1.1" && request_line != "POST / HTTP/1.0" {
                return Err("only POST / is supported".into());
            }
            let mut content_length = None;
            for line in lines {
                let (name, value) = line.split_once(':')
                    .ok_or_else(|| "malformed HTTP header".to_string())?;
                if name.eq_ignore_ascii_case("transfer-encoding") {
                    return Err("chunked transfer encoding is not supported".into());
                }
                if name.eq_ignore_ascii_case("content-length") {
                    if content_length.is_some() {
                        return Err("duplicate Content-Length is not supported".into());
                    }
                    content_length = Some(value.trim().parse::<usize>()
                        .map_err(|_| "invalid Content-Length")?);
                }
            }
            let length = content_length.ok_or_else(|| "Content-Length is required".to_string())?;
            let body_start = header_end + 4;
            let body_end = body_start.checked_add(length)
                .ok_or_else(|| "HTTP body length overflow".to_string())?;
            if body_end > MAX_HTTP_REQUEST_BYTES {
                return Err("HTTP request exceeds maximum size".into());
            }
            if buffer.len() >= body_end {
                return Ok(buffer[body_start..body_end].to_vec());
            }
        }
        let read = stream.read(&mut chunk).map_err(|error| format!("cannot read HTTP request: {error}"))?;
        if read == 0 {
            return Err("connection closed before HTTP body completed".into());
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
}

fn write_http_response(stream: &mut TcpStream, status: &str, body: &str) -> Result<(), String> {
    let header = format!(
        "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        body.len()
    );
    stream.write_all(header.as_bytes()).and_then(|_| stream.write_all(body.as_bytes()))
        .map_err(|error| format!("cannot write HTTP response: {error}"))
}

pub fn handle_json_rpc(body: &[u8], store_path: &str, expected_chain_id: u64) -> Value {
    handle_json_rpc_with_pool(body, store_path, expected_chain_id, None)
}

fn handle_json_rpc_with_pool(
    body: &[u8],
    store_path: &str,
    expected_chain_id: u64,
    pool_path: Option<&str>,
) -> Value {
    let request: Value = match serde_json::from_slice(body) {
        Ok(value) => value,
        Err(_) => return rpc_error(Value::Null, -32700, "parse error"),
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    if request.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return rpc_error(id, -32600, "jsonrpc must be 2.0");
    }
    let method = match request.get("method").and_then(Value::as_str) {
        Some(value) => value,
        None => return rpc_error(id, -32600, "method must be a string"),
    };
    let params = request.get("params").cloned().unwrap_or_else(|| json!([]));
    let (store, runtime) = match BlockStore::load_from_file(store_path, expected_chain_id) {
        Ok(value) => value,
        Err(error) => return rpc_error(id, -32000, &format!("block-store verification failed: {error}")),
    };

    let result = match method {
        "chain_getInfo" => {
            let tip = match store.blocks().last() {
                Some(value) => value,
                None => return rpc_error(id, -32000, "block store has no genesis block"),
            };
            json!({
                "chainId": store.chain_id(),
                "blockCount": store.blocks().len(),
                "height": tip.header.height,
                "tipHash": hex_encode(&tip.header.header_hash().unwrap_or([0; 32])),
                "stateRoot": hex_encode(&runtime.state_root()),
                "consensus": "not implemented",
                "storage": "local replay-verified file"
            })
        }
        "chain_getBalance" => {
            let address = match string_param(&params, "address", 0) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "expected address parameter"),
            };
            json!({ "address": address, "balance": runtime.balance(address), "nonce": runtime.account_nonce(address) })
        }
        "chain_getBlock" => {
            let height = match number_param(&params, "height", 0) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "expected numeric height parameter"),
            };
            let block = match store.blocks().iter().find(|block| block.header.height == height) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "block height not found"),
            };
            let transactions: Vec<Value> = block.transactions.iter().map(|envelope| {
                json!({
                    "transactionId": envelope.transaction.transaction_id().map(|id| hex_encode(&id)).unwrap_or_default(),
                    "sender": envelope.transaction.sender,
                    "recipient": envelope.transaction.recipient,
                    "nonce": envelope.transaction.nonce,
                    "amount": envelope.transaction.amount,
                    "maxFee": envelope.transaction.max_fee
                })
            }).collect();
            json!({
                "chainId": block.header.chain_id,
                "height": block.header.height,
                "timestamp": block.header.timestamp_secs,
                "parentHash": hex_encode(&block.header.parent_hash),
                "transactionRoot": hex_encode(&block.header.transaction_root),
                "receiptRoot": hex_encode(&block.header.receipt_root),
                "stateRoot": hex_encode(&block.header.state_root),
                "proposer": block.header.proposer,
                "transactions": transactions
            })
        }
        "chain_submitTransaction" => {
            let pool_path = match pool_path {
                Some(value) => value,
                None => return rpc_error(id, -32601, "transaction submission is disabled on read-only RPC"),
            };
            let public_key = match string_param(&params, "publicKey", 0) {
                Some(value) => match hex_decode(value) {
                    Ok(bytes) => bytes,
                    Err(error) => return rpc_error(id, -32602, &error),
                },
                None => return rpc_error(id, -32602, "expected publicKey hex parameter"),
            };
            let transaction_bytes = match string_param(&params, "transaction", 1) {
                Some(value) => match hex_decode(value) {
                    Ok(bytes) => bytes,
                    Err(error) => return rpc_error(id, -32602, &error),
                },
                None => return rpc_error(id, -32602, "expected canonical transaction hex parameter"),
            };
            let transaction = match TransactionV1::from_canonical_bytes(&transaction_bytes) {
                Ok(value) => value,
                Err(error) => return rpc_error(id, -32602, &error),
            };
            let envelope = SignedTransactionEnvelope { transaction, public_key };
            let next_height = match store.blocks().last().and_then(|block| block.header.height.checked_add(1)) {
                Some(value) => value,
                None => return rpc_error(id, -32000, "cannot determine next block height"),
            };
            let mut pool = match TransactionPool::load_from_file(pool_path, &runtime, expected_chain_id, next_height) {
                Ok(value) => value,
                Err(error) => return rpc_error(id, -32000, &error),
            };
            let transaction_id = match pool.submit(&runtime, envelope) {
                Ok(value) => value,
                Err(error) => return rpc_error(id, -32010, &error),
            };
            if let Err(error) = pool.save_to_file(pool_path) {
                return rpc_error(id, -32000, &error);
            }
            json!({
                "transactionId": hex_encode(&transaction_id),
                "poolSize": pool.len(),
                "nextBlockHeight": next_height
            })
        }
        "chain_getPendingTransactions" => {
            let pool_path = match pool_path {
                Some(value) => value,
                None => return rpc_error(id, -32601, "transaction pool is disabled on read-only RPC"),
            };
            let next_height = match store.blocks().last().and_then(|block| block.header.height.checked_add(1)) {
                Some(value) => value,
                None => return rpc_error(id, -32000, "cannot determine next block height"),
            };
            let pool = match TransactionPool::load_from_file(pool_path, &runtime, expected_chain_id, next_height) {
                Ok(value) => value,
                Err(error) => return rpc_error(id, -32000, &error),
            };
            let transactions: Vec<Value> = pool.pending().iter().map(|envelope| json!({
                "transactionId": envelope.transaction.transaction_id().map(|id|hex_encode(&id)).unwrap_or_default(),
                "sender": envelope.transaction.sender,
                "recipient": envelope.transaction.recipient,
                "nonce": envelope.transaction.nonce,
                "amount": envelope.transaction.amount
            })).collect();
            json!({"count": pool.len(), "transactions": transactions})
        }
        "chain_getTransaction" => {
            let id_text = match string_param(&params, "transactionId", 0) {
                Some(value) => value,
                None => return rpc_error(id, -32602, "expected transactionId parameter"),
            };
            let found = store.blocks().iter().find_map(|block| {
                block.transactions.iter().enumerate().find_map(|(index, envelope)| {
                    envelope.transaction.transaction_id().ok().filter(|txid| hex_encode(txid) == id_text)
                        .map(|_| (block.header.height, index, &envelope.transaction))
                })
            });
            match found {
                Some((height, index, transaction)) => json!({
                    "transactionId": id_text,
                    "blockHeight": height,
                    "transactionIndex": index,
                    "chainId": transaction.chain_id,
                    "sender": transaction.sender,
                    "recipient": transaction.recipient,
                    "nonce": transaction.nonce,
                    "amount": transaction.amount,
                    "maxFee": transaction.max_fee,
                    "gasLimit": transaction.gas_limit,
                    "validUntilHeight": transaction.valid_until_height
                }),
                None => return rpc_error(id, -32602, "transaction ID not found"),
            }
        }
        _ => return rpc_error(id, -32601, "method not found"),
    };
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn string_param<'a>(params: &'a Value, object_key: &str, index: usize) -> Option<&'a str> {
    params.get(object_key).and_then(Value::as_str)
        .or_else(|| params.get(index).and_then(Value::as_str))
}

fn number_param(params: &Value, object_key: &str, index: usize) -> Option<u64> {
    params.get(object_key).and_then(Value::as_u64)
        .or_else(|| params.get(index).and_then(Value::as_u64))
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn hex_decode(value: &str) -> Result<Vec<u8>, String> {
    if value.is_empty() || value.len() % 2 != 0
        || !value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)) {
        return Err("invalid lowercase hexadecimal value".into());
    }
    let mut output = Vec::with_capacity(value.len() / 2);
    for pair in value.as_bytes().chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        let low = (pair[1] as char).to_digit(16).ok_or_else(|| "invalid hex digit".to_string())?;
        output.push(((high << 4) | low) as u8);
    }
    Ok(output)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{handle_json_rpc, handle_json_rpc_with_pool, hex_encode};
    use super::super::block_store::BlockStore;
    use super::super::chain_block::execute_block;
    use super::super::chain_protocol::{address_from_public_key, transaction_merkle_root, BlockHeaderV1, TransactionV1};
    use super::super::chain_runtime::ChainRuntime;
    use serde_json::{json, Value};
    use ed25519_dalek::{Signer, SigningKey};
    use std::collections::BTreeMap;

    fn test_store() -> (String, String) {
        let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("zenvora-rpc-{}-{unique}.zvb", std::process::id()));
        let path_text = path.to_string_lossy().to_string();
        let mut allocations = BTreeMap::new();
        allocations.insert("alice".to_string(), 100);
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance("alice", 100).unwrap();
        let header = BlockHeaderV1 {
            chain_id: 71, height: 0, timestamp_secs: 1_800_000_000,
            parent_hash: [0; 32], transaction_root: transaction_merkle_root(&[]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[]).unwrap(), state_root: runtime.state_root(),
            proposer: "validator_1".into(), consensus_proof: Vec::new(),
        };
        execute_block(&mut runtime, &header, &[], 71, None).unwrap();
        let mut store = BlockStore::new(71, allocations).unwrap();
        store.append_executed_block(&path_text, &runtime, &header, &[], &[]).unwrap();
        (path_text, path.to_string_lossy().to_string())
    }

    #[test]
    fn writable_rpc_admits_signed_transactions_to_persisted_pool() {
        let key = SigningKey::from_bytes(&[151; 32]);
        let public_key = key.verifying_key().to_bytes();
        let address = address_from_public_key(&public_key).unwrap();
        let store_path = std::env::temp_dir().join(format!("zenvora-rpc-pool-store-{}.zvb", std::process::id()));
        let pool_path = std::env::temp_dir().join(format!("zenvora-rpc-pool-{}.zvp", std::process::id()));
        let _ = std::fs::remove_file(&store_path);
        let _ = std::fs::remove_file(&pool_path);
        let store_text = store_path.to_string_lossy().to_string();
        let pool_text = pool_path.to_string_lossy().to_string();

        let mut allocations = BTreeMap::new();
        allocations.insert(address.clone(), 1_000);
        allocations.insert("bob".to_string(), 0);
        let mut runtime = ChainRuntime::new();
        for (account, balance) in &allocations {
            runtime.set_genesis_balance(account, *balance).unwrap();
        }
        let genesis = BlockHeaderV1 {
            chain_id: 71, height: 0, timestamp_secs: 0, parent_hash: [0; 32],
            transaction_root: transaction_merkle_root(&[]).unwrap(),
            receipt_root: ChainRuntime::receipt_root_for(&[]).unwrap(),
            state_root: runtime.state_root(), proposer: address.clone(), consensus_proof: Vec::new(),
        };
        super::super::chain_block::execute_block(&mut runtime, &genesis, &[], 71, None).unwrap();
        let mut store = BlockStore::new(71, allocations).unwrap();
        store.append_executed_block(&store_text, &runtime, &genesis, &[], &[]).unwrap();

        let mut transaction = TransactionV1 {
            chain_id: 71, sender: address, nonce: 0, recipient: "bob".into(),
            amount: 100, max_fee: 1, gas_limit: 21_000, valid_until_height: Some(20),
            signature: Vec::new(),
        };
        transaction.signature = key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
        let request = json!({
            "jsonrpc":"2.0", "id":4, "method":"chain_submitTransaction",
            "params":{
                "publicKey":hex_encode(&public_key),
                "transaction":hex_encode(&transaction.canonical_bytes().unwrap())
            }
        });
        let response = handle_json_rpc_with_pool(
            request.to_string().as_bytes(), &store_text, 71, Some(&pool_text),
        );
        assert_eq!(response["result"]["poolSize"], 1);
        let pending = handle_json_rpc_with_pool(
            br#"{"jsonrpc":"2.0","id":5,"method":"chain_getPendingTransactions","params":[]}"#,
            &store_text, 71, Some(&pool_text),
        );
        assert_eq!(pending["result"]["count"], 1);
        let pool = super::super::tx_pool::TransactionPool::load_from_file(&pool_text, &runtime, 71, 1).unwrap();
        assert_eq!(pool.len(), 1);
        assert_eq!(runtime.balance("bob"), 0);
        std::fs::remove_file(store_path).unwrap();
        std::fs::remove_file(pool_path).unwrap();
    }

    #[test]
    fn malformed_json_and_invalid_json_rpc_envelopes_return_protocol_errors() {
        let malformed = handle_json_rpc(b"{", "/no-store-needed-for-parse-error", 71);
        assert_eq!(malformed["error"]["code"], -32700);
        assert_eq!(malformed["id"], Value::Null);

        let wrong_version = handle_json_rpc(
            br#"{"jsonrpc":"1.0","id":7,"method":"chain_getInfo","params":[]}"#,
            "/no-store-needed-for-invalid-envelope", 71,
        );
        assert_eq!(wrong_version["error"]["code"], -32600);
        assert_eq!(wrong_version["id"], 7);

        let missing_method = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":"bad","params":[]}"#,
            "/no-store-needed-for-invalid-envelope", 71,
        );
        assert_eq!(missing_method["error"]["code"], -32600);
        assert_eq!(missing_method["id"], "bad");
    }

    #[test]
    fn read_only_rpc_rejects_transaction_submission_explicitly() {
        let (path, cleanup_path) = test_store();
        let response = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":9,"method":"chain_submitTransaction","params":[]}"#,
            &path, 71,
        );
        assert_eq!(response["error"]["code"], -32601);
        assert_eq!(
            response["error"]["message"],
            "transaction submission is disabled on read-only RPC"
        );
        std::fs::remove_file(cleanup_path).unwrap();
    }

    #[test]
    fn block_lookup_rejects_missing_height_parameter() {
        let (path, cleanup_path) = test_store();
        let response = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":10,"method":"chain_getBlock","params":[]}"#,
            &path, 71,
        );
        assert_eq!(response["error"]["code"], -32602);
        assert_eq!(response["id"], 10);
        std::fs::remove_file(cleanup_path).unwrap();
    }

    #[test]
    fn json_rpc_info_balance_and_unknown_method_work() {
        let (path, cleanup_path) = test_store();
        let info = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":1,"method":"chain_getInfo","params":[]}"#,
            &path, 71,
        );
        assert_eq!(info["result"]["chainId"], 71);
        assert_eq!(info["result"]["height"], 0);

        let balance = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":2,"method":"chain_getBalance","params":{"address":"alice"}}"#,
            &path, 71,
        );
        assert_eq!(balance["result"]["balance"], 100);
        assert_eq!(balance["result"]["nonce"], 0);

        let missing = handle_json_rpc(
            br#"{"jsonrpc":"2.0","id":3,"method":"no_such_method","params":[]}"#,
            &path, 71,
        );
        assert_eq!(missing["error"]["code"], -32601);
        std::fs::remove_file(cleanup_path).unwrap();
    }
}
