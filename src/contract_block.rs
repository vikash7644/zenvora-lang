//! Deterministic QVM 2.0 contract-block execution and replay commitments.
//! This is the contract execution layer; consensus/proposer authorization and durable
//! integration into the legacy transfer-only BlockStore remain separate integration steps.

use sha2::{Digest, Sha256};
use super::contract_executor::ContractAccountState;
use super::contract_protocol::{ContractTransactionV1, ContractOperation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBlockHeaderV1 {
    pub chain_id: u64,
    pub height: u64,
    pub timestamp_secs: u64,
    pub parent_hash: [u8; 32],
    pub transaction_root: [u8; 32],
    pub receipt_root: [u8; 32],
    pub state_root: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedContractEnvelope {
    pub transaction: ContractTransactionV1,
    pub public_key: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractBlockReceipt {
    Deployment {
        transaction_id: [u8; 32], contract_id: String, sender: String,
        nonce: u64, gas_charged: u64, fee_charged: u64,
    },
    Call {
        transaction_id: [u8; 32], contract_id: String, sender: String,
        nonce: u64, success: bool, gas_charged: u64, fee_charged: u64,
        output_hash: [u8; 32],
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractBlockExecution {
    pub receipts: Vec<ContractBlockReceipt>,
    pub transaction_root: [u8; 32],
    pub receipt_root: [u8; 32],
    pub state_root: [u8; 32],
    pub block_hash: [u8; 32],
}

/// Execute and commit one contract block atomically. Invalid signatures, replayed nonces,
/// bad linkage, or any root mismatch leave the caller's state unchanged.
pub fn execute_contract_block(
    state: &mut ContractAccountState,
    header: &ContractBlockHeaderV1,
    transactions: &[SignedContractEnvelope],
    expected_chain_id: u64,
    previous: Option<&ContractBlockHeaderV1>,
) -> Result<ContractBlockExecution, String> {
    if header.chain_id != expected_chain_id { return Err("contract block chain ID mismatch".into()); }
    match previous {
        Some(parent) => {
            if parent.chain_id != expected_chain_id { return Err("previous contract block chain ID mismatch".into()); }
            if header.height != parent.height.checked_add(1).ok_or("contract block height overflow")? {
                return Err("contract block height is not contiguous".into());
            }
            if header.parent_hash != contract_block_hash(parent)? { return Err("contract block parent hash mismatch".into()); }
            if header.timestamp_secs < parent.timestamp_secs { return Err("contract block timestamp precedes parent".into()); }
        }
        None => {
            if header.height != 0 || header.parent_hash != [0; 32] || !transactions.is_empty() {
                return Err("contract genesis block must be empty at height zero".into());
            }
        }
    }

    let transaction_root = contract_transaction_root(transactions)?;
    if transaction_root != header.transaction_root { return Err("contract transaction root mismatch".into()); }
    let mut candidate = state.clone();
    let mut receipts = Vec::with_capacity(transactions.len());
    for envelope in transactions {
        let tx = &envelope.transaction;
        match &tx.operation {
            ContractOperation::Deploy { .. } => {
                let r = candidate.execute_signed_deployment(tx, &envelope.public_key, expected_chain_id, header.height)?;
                receipts.push(ContractBlockReceipt::Deployment {
                    transaction_id: r.transaction_id, contract_id: r.contract_id, sender: r.sender,
                    nonce: r.nonce, gas_charged: r.gas_charged, fee_charged: r.fee_charged,
                });
            }
            ContractOperation::Call { contract_id, .. } => {
                let r = candidate.execute_signed_registered_call(tx, &envelope.public_key, expected_chain_id, header.height)?;
                let output = r.execution.as_ref().map(|x| x.output.join("\n")).unwrap_or_else(|e| e.clone());
                receipts.push(ContractBlockReceipt::Call {
                    transaction_id: r.transaction_id, contract_id: contract_id.clone(), sender: r.sender,
                    nonce: r.nonce, success: r.execution.is_ok(), gas_charged: r.gas_charged,
                    fee_charged: r.fee_charged, output_hash: hash(b"QORVYNT_CALL_OUTPUT_V1\\0", output.as_bytes()),
                });
            }
        }
    }
    let receipt_root = contract_receipt_root(&receipts)?;
    if receipt_root != header.receipt_root { return Err("contract receipt root mismatch".into()); }
    let state_root = contract_state_root(&candidate)?;
    if state_root != header.state_root { return Err("contract state root mismatch".into()); }
    let block_hash = contract_block_hash(header)?;
    *state = candidate;
    Ok(ContractBlockExecution { receipts, transaction_root, receipt_root, state_root, block_hash })
}

pub fn contract_transaction_root(envelopes: &[SignedContractEnvelope]) -> Result<[u8; 32], String> {
    let mut leaves = Vec::with_capacity(envelopes.len());
    for envelope in envelopes {
        envelope.transaction.verify_signature(&envelope.public_key)?;
        leaves.push(hash(b"QORVYNT_CONTRACT_TX_LEAF_V1\\0", &envelope.transaction.transaction_id()?));
    }
    Ok(merkle_root(leaves, b"QORVYNT_CONTRACT_TX_EMPTY_V1\\0", b"QORVYNT_CONTRACT_TX_NODE_V1\\0"))
}

pub fn contract_receipt_root(receipts: &[ContractBlockReceipt]) -> Result<[u8; 32], String> {
    let mut leaves = Vec::with_capacity(receipts.len());
    for receipt in receipts {
        let mut bytes = Vec::new();
        match receipt {
            ContractBlockReceipt::Deployment { transaction_id, contract_id, sender, nonce, gas_charged, fee_charged } => {
                bytes.push(0); bytes.extend_from_slice(transaction_id); put_string(&mut bytes, contract_id)?;
                put_string(&mut bytes, sender)?; bytes.extend_from_slice(&nonce.to_be_bytes());
                bytes.extend_from_slice(&gas_charged.to_be_bytes()); bytes.extend_from_slice(&fee_charged.to_be_bytes());
            }
            ContractBlockReceipt::Call { transaction_id, contract_id, sender, nonce, success, gas_charged, fee_charged, output_hash } => {
                bytes.push(1); bytes.extend_from_slice(transaction_id); put_string(&mut bytes, contract_id)?;
                put_string(&mut bytes, sender)?; bytes.extend_from_slice(&nonce.to_be_bytes());
                bytes.push(u8::from(*success)); bytes.extend_from_slice(&gas_charged.to_be_bytes());
                bytes.extend_from_slice(&fee_charged.to_be_bytes()); bytes.extend_from_slice(output_hash);
            }
        }
        leaves.push(hash(b"QORVYNT_CONTRACT_RECEIPT_LEAF_V1\\0", &bytes));
    }
    Ok(merkle_root(leaves, b"QORVYNT_CONTRACT_RECEIPT_EMPTY_V1\\0", b"QORVYNT_CONTRACT_RECEIPT_NODE_V1\\0"))
}

pub fn contract_state_root(state: &ContractAccountState) -> Result<[u8; 32], String> {
    Ok(hash(b"QORVYNT_CONTRACT_STATE_ROOT_V1\\0", &state.encode_snapshot()?))
}

pub fn contract_block_hash(header: &ContractBlockHeaderV1) -> Result<[u8; 32], String> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&header.chain_id.to_be_bytes());
    bytes.extend_from_slice(&header.height.to_be_bytes());
    bytes.extend_from_slice(&header.timestamp_secs.to_be_bytes());
    bytes.extend_from_slice(&header.parent_hash);
    bytes.extend_from_slice(&header.transaction_root);
    bytes.extend_from_slice(&header.receipt_root);
    bytes.extend_from_slice(&header.state_root);
    Ok(hash(b"QORVYNT_CONTRACT_BLOCK_V1\\0", &bytes))
}

fn put_string(out: &mut Vec<u8>, value: &str) -> Result<(), String> {
    let len = u32::try_from(value.len()).map_err(|_| "receipt string too large")?;
    out.extend_from_slice(&len.to_be_bytes()); out.extend_from_slice(value.as_bytes()); Ok(())
}
fn hash(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new(); h.update(domain); h.update(bytes); h.finalize().into()
}
fn merkle_root(mut leaves: Vec<[u8; 32]>, empty: &[u8], node: &[u8]) -> [u8; 32] {
    if leaves.is_empty() { return hash(empty, &[]); }
    while leaves.len() > 1 {
        let mut next = Vec::with_capacity((leaves.len() + 1) / 2);
        for pair in leaves.chunks(2) {
            let right = pair.get(1).unwrap_or(&pair[0]);
            let mut bytes = Vec::with_capacity(64); bytes.extend_from_slice(&pair[0]); bytes.extend_from_slice(right);
            next.push(hash(node, &bytes));
        }
        leaves = next;
    }
    leaves[0]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use super::super::contract_protocol::{ContractOperation, ContractTransactionV1};

    fn deployment(key: &SigningKey, nonce: u64, source: &str) -> SignedContractEnvelope {
        let public = key.verifying_key().to_bytes();
        let artifact = serde_json::to_vec(&super::super::contract_abi_and_artifact(source).unwrap()).unwrap();
        let mut tx = ContractTransactionV1 {
            chain_id: 77,
            sender: super::super::chain_protocol::address_from_public_key(&public).unwrap(),
            nonce, gas_limit: 200_000, max_fee: 500_000, gas_price: 1,
            valid_until_height: Some(500),
            operation: ContractOperation::Deploy { artifact }, signature: Vec::new(),
        };
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        SignedContractEnvelope { transaction: tx, public_key: public.to_vec() }
    }

    fn call(key: &SigningKey, nonce: u64, id: &str, amount: &str) -> SignedContractEnvelope {
        let public = key.verifying_key().to_bytes();
        let mut tx = ContractTransactionV1 {
            chain_id: 77,
            sender: super::super::chain_protocol::address_from_public_key(&public).unwrap(),
            nonce, gas_limit: 200_000, max_fee: 500_000, gas_price: 1,
            valid_until_height: Some(500),
            operation: ContractOperation::Call { contract_id: id.into(), method: "increment".into(), calldata: amount.as_bytes().to_vec() },
            signature: Vec::new(),
        };
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        SignedContractEnvelope { transaction: tx, public_key: public.to_vec() }
    }

    fn header(
        height: u64, parent_hash: [u8; 32], timestamp_secs: u64,
        tx_root: [u8; 32], receipt_root: [u8; 32], state_root: [u8; 32],
    ) -> ContractBlockHeaderV1 {
        ContractBlockHeaderV1 { chain_id: 77, height, timestamp_secs, parent_hash,
            transaction_root: tx_root, receipt_root, state_root }
    }

    #[test]
    fn empty_genesis_block_commits_initial_contract_state() {
        let mut state = ContractAccountState::new();
        let h = header(0, [0; 32], 1, contract_transaction_root(&[]).unwrap(),
            contract_receipt_root(&[]).unwrap(), contract_state_root(&state).unwrap());
        let result = execute_contract_block(&mut state, &h, &[], 77, None).unwrap();
        assert_eq!(result.state_root, contract_state_root(&state).unwrap());
    }

    #[test]
    fn deployment_and_registered_call_replay_deterministically_across_blocks() {
        let key = SigningKey::from_bytes(&[91; 32]);
        let public = key.verifying_key().to_bytes();
        let address = super::super::chain_protocol::address_from_public_key(&public).unwrap();
        let source = "contract Counter {\\nstate count := 0\\naction increment(amount) {\\ncount := count + amount\\n}\\nview current() {\\nreturn count\\n}\\n}\\n";
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 2_000_000).unwrap();
        let genesis = header(0, [0; 32], 1, contract_transaction_root(&[]).unwrap(),
            contract_receipt_root(&[]).unwrap(), contract_state_root(&state).unwrap());
        execute_contract_block(&mut state, &genesis, &[], 77, None).unwrap();

        let deploy = deployment(&key, 0, source);
        let mut after_deploy = state.clone();
        let deployed = after_deploy.execute_signed_deployment(&deploy.transaction, &public, 77, 1).unwrap();
        let deploy_receipt = ContractBlockReceipt::Deployment {
            transaction_id: deployed.transaction_id, contract_id: deployed.contract_id.clone(),
            sender: deployed.sender, nonce: deployed.nonce, gas_charged: deployed.gas_charged,
            fee_charged: deployed.fee_charged,
        };
        let h1 = header(1, contract_block_hash(&genesis).unwrap(), 2,
            contract_transaction_root(&[deploy.clone()]).unwrap(),
            contract_receipt_root(&[deploy_receipt]).unwrap(), contract_state_root(&after_deploy).unwrap());
        let applied = execute_contract_block(&mut state, &h1, &[deploy], 77, Some(&genesis)).unwrap();
        assert_eq!(state, after_deploy);

        let call_tx = call(&key, 1, &deployed.contract_id, "11");
        let mut after_call = state.clone();
        let call_result = after_call.execute_signed_registered_call(&call_tx.transaction, &public, 77, 2).unwrap();
        let output = call_result.execution.as_ref().map(|r| r.output.join("\\n")).unwrap_or_default();
        let call_receipt = ContractBlockReceipt::Call {
            transaction_id: call_result.transaction_id, contract_id: deployed.contract_id.clone(),
            sender: call_result.sender, nonce: call_result.nonce, success: call_result.execution.is_ok(),
            gas_charged: call_result.gas_charged, fee_charged: call_result.fee_charged,
            output_hash: hash(b"QORVYNT_CALL_OUTPUT_V1\\0", output.as_bytes()),
        };
        let h2 = header(2, applied.block_hash, 3,
            contract_transaction_root(&[call_tx.clone()]).unwrap(),
            contract_receipt_root(&[call_receipt]).unwrap(), contract_state_root(&after_call).unwrap());
        execute_contract_block(&mut state, &h2, &[call_tx], 77, Some(&h1)).unwrap();
        assert_eq!(state, after_call);
        assert_eq!(state.storage.get(&deployed.contract_id, "count").unwrap(), Some(&b"11"[..]));
    }

    #[test]
    fn invalid_state_root_does_not_mutate_contract_state() {
        let key = SigningKey::from_bytes(&[92; 32]);
        let source = "contract Counter {\\nstate count := 0\\naction increment(amount) {\\ncount := count + amount\\n}\\nview current() {\\nreturn count\\n}\\n}\\n";
        let mut state = ContractAccountState::new();
        let public = key.verifying_key().to_bytes();
        let address = super::super::chain_protocol::address_from_public_key(&public).unwrap();
        state.set_balance(&address, 1_000_000).unwrap();
        let original = state.clone();
        let tx = deployment(&key, 0, source);
        let mut candidate = state.clone();
        let receipt = candidate.execute_signed_deployment(&tx.transaction, &public, 77, 1).unwrap();
        let block_receipt = ContractBlockReceipt::Deployment {
            transaction_id: receipt.transaction_id, contract_id: receipt.contract_id,
            sender: receipt.sender, nonce: receipt.nonce, gas_charged: receipt.gas_charged,
            fee_charged: receipt.fee_charged,
        };
        let mut bad_root = contract_state_root(&candidate).unwrap(); bad_root[0] ^= 1;
        let genesis = header(0, [0; 32], 1, contract_transaction_root(&[]).unwrap(),
            contract_receipt_root(&[]).unwrap(), contract_state_root(&state).unwrap());
        let h1 = header(1, contract_block_hash(&genesis).unwrap(), 2,
            contract_transaction_root(&[tx.clone()]).unwrap(),
            contract_receipt_root(&[block_receipt]).unwrap(), bad_root);
        assert!(execute_contract_block(&mut state, &h1, &[tx], 77, Some(&genesis)).is_err());
        assert_eq!(state, original);
    }
}
