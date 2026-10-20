//! Experimental signed contract-call admission and fee settlement.
//!
//! This is a local runtime harness, not a consensus protocol. Failed admitted calls consume
//! the transaction nonce and the full declared fee cap because the current executor does not
//! expose gas used on errors. Successful calls pay gas_used * gas_price and receive the remainder.
use std::collections::BTreeMap;

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use super::chain_protocol::address_from_public_key;
use super::contract_gas::BASE_EXECUTION_GAS;
use super::contract_runtime::{execute_with_gas_usage, ExecutionReceipt};
use super::contract_state::{AccessMode, ContractState};

const SIGNING_DOMAIN: &[u8] = b"ZENVORA_SIGNED_CONTRACT_CALL_V1\0";
const MAX_CONTRACT_ID: usize = 64;
const MAX_INVOCATION: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedContractCall {
    pub chain_id: u64,
    pub sender: String,
    pub nonce: u64,
    pub contract_id: String,
    pub invocation: String,
    pub gas_limit: u64,
    pub gas_price: u64,
    pub max_fee: u64,
    pub signature: Vec<u8>,
}

impl SignedContractCall {
    /// Canonical domain-separated bytes. The signature is excluded from its own payload.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        if self.sender.is_empty() || self.sender.len() > 64 || !self.sender.is_ascii() {
            return Err("sender address is invalid".into());
        }
        if self.contract_id.is_empty() || self.contract_id.len() > MAX_CONTRACT_ID
            || !self.contract_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err("contract ID is invalid".into());
        }
        if self.invocation.is_empty() || self.invocation.len() > MAX_INVOCATION
            || self.invocation.contains('\n') || self.invocation.contains('\r')
            || !self.invocation.trim_start().starts_with("call ")
        {
            return Err("signed transactions must contain one bounded action invocation".into());
        }
        if self.gas_limit == 0 || self.gas_price == 0 {
            return Err("gas limit and gas price must be nonzero".into());
        }
        let required_cap = self.gas_limit.checked_mul(self.gas_price)
            .ok_or_else(|| "maximum fee calculation overflow".to_string())?;
        if self.max_fee < required_cap {
            return Err("max_fee must cover gas_limit * gas_price".into());
        }
        let mut out = Vec::new();
        out.extend_from_slice(SIGNING_DOMAIN);
        out.extend_from_slice(&1_u16.to_be_bytes());
        out.extend_from_slice(&self.chain_id.to_be_bytes());
        put_bytes(&mut out, self.sender.as_bytes())?;
        out.extend_from_slice(&self.nonce.to_be_bytes());
        put_bytes(&mut out, self.contract_id.as_bytes())?;
        put_bytes(&mut out, self.invocation.as_bytes())?;
        out.extend_from_slice(&self.gas_limit.to_be_bytes());
        out.extend_from_slice(&self.gas_price.to_be_bytes());
        out.extend_from_slice(&self.max_fee.to_be_bytes());
        Ok(out)
    }

    pub fn transaction_id(&self) -> Result<[u8; 32], String> {
        if self.signature.len() != 64 {
            return Err("Ed25519 signature must be exactly 64 bytes".into());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"ZENVORA_SIGNED_CONTRACT_CALL_ID_V1\0");
        hasher.update(self.signing_bytes()?);
        hasher.update(&self.signature);
        Ok(hasher.finalize().into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractCallReceipt {
    pub transaction_id: [u8; 32],
    pub sender: String,
    pub nonce: u64,
    pub success: bool,
    pub gas_used: u64,
    pub fee_paid: u64,
    pub state_root: [u8; 32],
    pub output: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractFeeLedger {
    balances: BTreeMap<String, u64>,
    nonces: BTreeMap<String, u64>,
    receipts: Vec<ContractCallReceipt>,
}

impl ContractFeeLedger {
    pub fn new() -> Self { Self::default() }

    pub fn set_genesis_balance(&mut self, address: &str, balance: u64) -> Result<(), String> {
        if !self.receipts.is_empty() {
            return Err("genesis balances cannot change after transaction execution begins".into());
        }
        if address.is_empty() || address.len() > 64 || !address.is_ascii() {
            return Err("invalid account address".into());
        }
        self.balances.insert(address.to_owned(), balance);
        Ok(())
    }

    pub fn balance(&self, address: &str) -> u64 { *self.balances.get(address).unwrap_or(&0) }
    pub fn nonce(&self, address: &str) -> u64 { *self.nonces.get(address).unwrap_or(&0) }
    pub fn receipts(&self) -> &[ContractCallReceipt] { &self.receipts }

    /// Verify, reserve, execute, and settle a signed action atomically with respect to contract
    /// state. Invalid admission never mutates state or balances. An admitted execution failure
    /// leaves contract state untouched but consumes its nonce and fee cap.
    pub fn apply_signed_action(
        &mut self,
        transaction: &SignedContractCall,
        public_key_bytes: &[u8],
        expected_chain_id: u64,
        source: &str,
        state: &mut ContractState,
    ) -> Result<ContractCallReceipt, String> {
        let signing_bytes = transaction.signing_bytes()?;
        if transaction.chain_id != expected_chain_id {
            return Err("wrong chain ID".into());
        }
        if public_key_bytes.len() != 32 {
            return Err("Ed25519 public key must be exactly 32 bytes".into());
        }
        let public_key: [u8; 32] = public_key_bytes.try_into()
            .map_err(|_| "Ed25519 public key must be exactly 32 bytes")?;
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|_| "invalid Ed25519 public key")?;
        if address_from_public_key(public_key_bytes)? != transaction.sender {
            return Err("transaction sender does not match public key".into());
        }
        if transaction.signature.len() != 64 {
            return Err("Ed25519 signature must be exactly 64 bytes".into());
        }
        let signature = Signature::from_slice(&transaction.signature)
            .map_err(|_| "invalid Ed25519 signature")?;
        verifying_key.verify_strict(&signing_bytes, &signature)
            .map_err(|_| "invalid contract transaction signature")?;
        if transaction.nonce != self.nonce(&transaction.sender) {
            return Err("transaction nonce does not match account nonce".into());
        }
        let reserve = transaction.gas_limit.checked_mul(transaction.gas_price)
            .ok_or_else(|| "fee reservation overflow".to_string())?;
        if transaction.max_fee < reserve {
            return Err("max_fee must cover gas_limit * gas_price".into());
        }
        if self.balance(&transaction.sender) < transaction.max_fee {
            return Err("insufficient balance for maximum fee reservation".into());
        }
        let next_nonce = transaction.nonce.checked_add(1)
            .ok_or_else(|| "account nonce overflow".to_string())?;
        let transaction_id = transaction.transaction_id()?;
        let mut candidate_state = state.clone();
        let execution = execute_with_gas_usage(
            source,
            &transaction.contract_id,
            &transaction.invocation,
            AccessMode::Action,
            &mut candidate_state,
            transaction.gas_limit,
        );

        let (success, gas_used, fee_paid, output, error) = match execution {
            Ok(ExecutionReceipt { output, gas_used, .. }) => {
                let actual_fee = gas_used.checked_mul(transaction.gas_price)
                    .ok_or_else(|| "actual fee calculation overflow".to_string())?;
                if actual_fee > transaction.max_fee {
                    return Err("actual execution fee exceeds signed fee cap".into());
                }
                *state = candidate_state;
                (true, gas_used, actual_fee, output, None)
            }
            Err((error, measured_gas)) => {
                // Out-of-gas consumes the full limit. Other admitted failures pay measured gas,
                // with a minimum base charge so malformed/missing methods are not free.
                let charged_gas = if error.contains("out of gas") {
                    transaction.gas_limit
                } else {
                    measured_gas.max(BASE_EXECUTION_GAS).min(transaction.gas_limit)
                };
                let failure_fee = charged_gas.checked_mul(transaction.gas_price)
                    .ok_or_else(|| "failed execution fee calculation overflow".to_string())?;
                if failure_fee > transaction.max_fee {
                    return Err("failed execution fee exceeds signed fee cap".into());
                }
                (false, charged_gas, failure_fee, Vec::new(), Some(error))
            }
        };

        let remaining = self.balance(&transaction.sender)
            .checked_sub(fee_paid)
            .ok_or_else(|| "fee settlement underflow".to_string())?;
        self.balances.insert(transaction.sender.clone(), remaining);
        self.nonces.insert(transaction.sender.clone(), next_nonce);
        let receipt = ContractCallReceipt {
            transaction_id,
            sender: transaction.sender.clone(),
            nonce: transaction.nonce,
            success,
            gas_used,
            fee_paid,
            state_root: state.state_root(),
            output,
            error,
        };
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }
}


#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedContractCallEnvelope {
    pub transaction: SignedContractCall,
    pub public_key: Vec<u8>,
}

/// Replay ordered signed contract calls against cloned state and fee ledger. The candidate is
/// published only if every envelope is admitted and both committed roots match.
pub fn replay_signed_contract_block(
    ledger: &mut ContractFeeLedger,
    state: &mut ContractState,
    transactions: &[SignedContractCallEnvelope],
    expected_chain_id: u64,
    source: &str,
    expected_state_root: [u8; 32],
    expected_receipt_root: [u8; 32],
) -> Result<Vec<ContractCallReceipt>, String> {
    let mut candidate_ledger = ledger.clone();
    let mut candidate_state = state.clone();
    let start = candidate_ledger.receipts().len();
    for (index, envelope) in transactions.iter().enumerate() {
        candidate_ledger.apply_signed_action(
            &envelope.transaction,
            &envelope.public_key,
            expected_chain_id,
            source,
            &mut candidate_state,
        ).map_err(|error| format!("signed contract transaction {} rejected during replay: {error}", index + 1))?;
    }
    let receipts = candidate_ledger.receipts()[start..].to_vec();
    let state_root = candidate_state.state_root();
    if state_root != expected_state_root {
        return Err("replayed contract state root does not match block commitment".into());
    }
    if contract_receipt_root(&receipts) != expected_receipt_root {
        return Err("replayed contract receipt root does not match block commitment".into());
    }
    *ledger = candidate_ledger;
    *state = candidate_state;
    Ok(receipts)
}

/// Deterministic receipt commitment for the experimental signed-call harness.
pub fn contract_receipt_root(receipts: &[ContractCallReceipt]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"ZENVORA_CONTRACT_RECEIPTS_V1\0");
    hasher.update((receipts.len() as u64).to_be_bytes());
    for receipt in receipts {
        hasher.update(receipt.transaction_id);
        hasher.update((receipt.sender.len() as u32).to_be_bytes());
        hasher.update(receipt.sender.as_bytes());
        hasher.update(receipt.nonce.to_be_bytes());
        hasher.update([u8::from(receipt.success)]);
        hasher.update(receipt.gas_used.to_be_bytes());
        hasher.update(receipt.fee_paid.to_be_bytes());
        hasher.update(receipt.state_root);
        match &receipt.error {
            Some(error) => {
                hasher.update([1]);
                hasher.update((error.len() as u32).to_be_bytes());
                hasher.update(error.as_bytes());
            }
            None => hasher.update([0]),
        }
        hasher.update((receipt.output.len() as u32).to_be_bytes());
        for line in &receipt.output {
            hasher.update((line.len() as u32).to_be_bytes());
            hasher.update(line.as_bytes());
        }
    }
    hasher.finalize().into()
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), String> {
    let len = u32::try_from(bytes.len()).map_err(|_| "signed transaction field too large")?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{ContractFeeLedger, SignedContractCall};
    use super::super::chain_protocol::address_from_public_key;
    use super::super::contract_state::ContractState;
    use ed25519_dalek::{Signer, SigningKey};

    const COUNTER: &str = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview current() {\nreturn count\n}\n}\n";

    fn signed_call(key: &SigningKey, nonce: u64, invocation: &str, gas_limit: u64) -> SignedContractCall {
        let public_key = key.verifying_key().to_bytes();
        let mut tx = SignedContractCall {
            chain_id: 19,
            sender: address_from_public_key(&public_key).unwrap(),
            nonce,
            contract_id: "counter".into(),
            invocation: invocation.into(),
            gas_limit,
            gas_price: 1,
            max_fee: gas_limit,
            signature: Vec::new(),
        };
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        tx
    }

    #[test]
    fn signed_action_verifies_signature_settles_actual_gas_and_prevents_replay() {
        let key = SigningKey::from_bytes(&[31; 32]);
        let public_key = key.verifying_key().to_bytes();
        let tx = signed_call(&key, 0, "call increment(4)", 100_000);
        let mut ledger = ContractFeeLedger::new();
        ledger.set_genesis_balance(&tx.sender, 200_000).unwrap();
        let mut state = ContractState::new();
        let receipt = ledger.apply_signed_action(&tx, &public_key, 19, COUNTER, &mut state).unwrap();
        assert!(receipt.success, "{:?}", receipt.error);
        assert_eq!(receipt.nonce, 0);
        assert_eq!(ledger.nonce(&tx.sender), 1);
        assert_eq!(ledger.balance(&tx.sender), 200_000 - receipt.fee_paid);
        assert_eq!(state.get("counter", "count").unwrap(), Some(&b"4"[..]));
        assert!(ledger.apply_signed_action(&tx, &public_key, 19, COUNTER, &mut state).is_err());
        assert_eq!(state.get("counter", "count").unwrap(), Some(&b"4"[..]));
        assert_eq!(ledger.receipts().len(), 1);
    }

    #[test]
    fn tampering_or_wrong_chain_never_mutates_anything() {
        let key = SigningKey::from_bytes(&[32; 32]);
        let public_key = key.verifying_key().to_bytes();
        let tx = signed_call(&key, 0, "call increment(4)", 100_000);
        let mut ledger = ContractFeeLedger::new();
        ledger.set_genesis_balance(&tx.sender, 200_000).unwrap();
        let mut state = ContractState::new();
        let before_root = state.state_root();
        let mut tampered = tx.clone();
        tampered.invocation = "call increment(99)".into();
        assert!(ledger.apply_signed_action(&tampered, &public_key, 19, COUNTER, &mut state).is_err());
        assert!(ledger.apply_signed_action(&tx, &public_key, 20, COUNTER, &mut state).is_err());
        assert_eq!(ledger.balance(&tx.sender), 200_000);
        assert_eq!(ledger.nonce(&tx.sender), 0);
        assert_eq!(state.state_root(), before_root);
        assert!(ledger.receipts().is_empty());
    }

    #[test]
    fn failed_call_charges_measured_or_base_gas_and_rolls_back_state() {
        let key = SigningKey::from_bytes(&[33; 32]);
        let public_key = key.verifying_key().to_bytes();
        let tx = signed_call(&key, 0, "call missing_method(4)", 100_000);
        let mut ledger = ContractFeeLedger::new();
        ledger.set_genesis_balance(&tx.sender, 200_000).unwrap();
        let mut state = ContractState::new();
        let before_root = state.state_root();
        let receipt = ledger.apply_signed_action(&tx, &public_key, 19, COUNTER, &mut state).unwrap();
        assert!(!receipt.success);
        assert!(receipt.error.is_some());
        assert!(receipt.fee_paid > 0);
        assert!(receipt.fee_paid <= tx.max_fee);
        assert_eq!(receipt.fee_paid, receipt.gas_used * tx.gas_price);
        assert!(receipt.gas_used < tx.gas_limit, "non-OOG failure should not burn the full gas limit");
        assert_eq!(ledger.nonce(&tx.sender), 1);
        assert_eq!(state.state_root(), before_root);
    }


    #[test]
    fn signed_block_replay_checks_roots_and_is_atomic_on_mismatch() {
        use super::{contract_receipt_root, replay_signed_contract_block, SignedContractCallEnvelope};
        let key = SigningKey::from_bytes(&[35; 32]);
        let public_key = key.verifying_key().to_bytes().to_vec();
        let tx = signed_call(&key, 0, "call increment(6)", 100_000);
        let envelope = SignedContractCallEnvelope { transaction: tx.clone(), public_key: public_key.clone() };
        let mut expected_ledger = ContractFeeLedger::new();
        expected_ledger.set_genesis_balance(&tx.sender, 200_000).unwrap();
        let mut expected_state = ContractState::new();
        let expected_receipt = expected_ledger.apply_signed_action(
            &tx, &public_key, 19, COUNTER, &mut expected_state
        ).unwrap();
        let expected_state_root = expected_state.state_root();
        let expected_receipt_root = contract_receipt_root(&[expected_receipt]);

        let mut replay_ledger = ContractFeeLedger::new();
        replay_ledger.set_genesis_balance(&tx.sender, 200_000).unwrap();
        let mut replay_state = ContractState::new();
        let receipts = replay_signed_contract_block(
            &mut replay_ledger, &mut replay_state, &[envelope.clone()], 19, COUNTER,
            expected_state_root, expected_receipt_root,
        ).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(replay_state.state_root(), expected_state_root);
        assert_eq!(replay_ledger.nonce(&tx.sender), 1);

        let mut rejected_ledger = ContractFeeLedger::new();
        rejected_ledger.set_genesis_balance(&tx.sender, 200_000).unwrap();
        let mut rejected_state = ContractState::new();
        assert!(replay_signed_contract_block(
            &mut rejected_ledger, &mut rejected_state, &[envelope], 19, COUNTER,
            expected_state_root, [0; 32],
        ).is_err());
        assert_eq!(rejected_state.state_root(), ContractState::new().state_root());
        assert_eq!(rejected_ledger.nonce(&tx.sender), 0);
        assert_eq!(rejected_ledger.balance(&tx.sender), 200_000);
        assert!(rejected_ledger.receipts().is_empty());
    }

    #[test]
    fn fee_cap_and_balance_are_checked_before_admission() {
        let key = SigningKey::from_bytes(&[34; 32]);
        let public_key = key.verifying_key().to_bytes();
        let mut tx = signed_call(&key, 0, "call increment(1)", 100);
        tx.max_fee = 99;
        assert!(tx.signing_bytes().is_err());
        let mut ledger = ContractFeeLedger::new();
        tx.max_fee = 100;
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        ledger.set_genesis_balance(&tx.sender, 99).unwrap();
        let mut state = ContractState::new();
        assert!(ledger.apply_signed_action(&tx, &public_key, 19, COUNTER, &mut state).is_err());
        assert_eq!(ledger.nonce(&tx.sender), 0);
        assert_eq!(ledger.balance(&tx.sender), 99);
        assert!(ledger.receipts().is_empty());
    }
}
