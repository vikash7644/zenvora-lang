//! Experimental signed contract-call admission and fee settlement.
//!
//! This is a local runtime harness, not a consensus protocol. Failed admitted calls consume
//! the transaction nonce and the full declared fee cap because the current executor does not
//! expose gas used on errors. Successful calls pay gas_used * gas_price and receive the remainder.
use std::collections::BTreeMap;

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

use super::chain_protocol::address_from_public_key;
use super::contract_runtime::{execute, ExecutionReceipt};
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
        let execution = execute(
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
            Err(error) => (false, transaction.gas_limit, transaction.max_fee, Vec::new(), Some(error)),
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
    fn failed_call_charges_fee_cap_and_rolls_back_state() {
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
        assert_eq!(receipt.fee_paid, tx.max_fee);
        assert_eq!(ledger.nonce(&tx.sender), 1);
        assert_eq!(state.state_root(), before_root);
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
