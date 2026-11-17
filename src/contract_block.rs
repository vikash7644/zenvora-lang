//! Deterministic QVM 2.0 contract-block execution and replay commitments.
//! This is the contract execution layer; consensus/proposer authorization and durable
//! integration into the legacy transfer-only BlockStore remain separate integration steps.

use sha2::{Digest, Sha256};
use super::contract_executor::{ContractAccountState, ContractTransactionV1, ContractOperation};

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
