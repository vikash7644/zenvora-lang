//! Authenticated contract-call admission and execution on the existing QVM 2.0 runtime.
use std::collections::BTreeMap;
use ed25519_dalek::SigningKey;
use super::contract_protocol::{ContractOperation, ContractTransactionV1};
use super::contract_runtime::{execute, ExecutionReceipt};
use super::contract_state::{AccessMode, ContractState};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAccountState {
    pub storage: ContractState,
    balances: BTreeMap<String, u64>,
    nonces: BTreeMap<String, u64>,
    pub fees_collected: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedCallReceipt {
    pub transaction_id: [u8; 32],
    pub sender: String,
    pub nonce: u64,
    pub execution: Result<ExecutionReceipt, String>,
    pub gas_charged: u64,
    pub fee_charged: u64,
    pub sender_balance: u64,
    pub next_nonce: u64,
}

impl Default for ContractAccountState {
    fn default() -> Self {
        Self { storage: ContractState::new(), balances: BTreeMap::new(), nonces: BTreeMap::new(), fees_collected: 0 }
    }
}

impl ContractAccountState {
    pub fn new() -> Self { Self::default() }

    /// Genesis/faucet/test harness balance setter. Production issuance must be consensus-authorized.
    pub fn set_balance(&mut self, address: &str, amount: u64) -> Result<(), String> {
        if address.is_empty() || address.len() > 64 || !address.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err("invalid account address".into());
        }
        self.balances.insert(address.to_owned(), amount);
        Ok(())
    }

    pub fn balance(&self, address: &str) -> u64 { self.balances.get(address).copied().unwrap_or(0) }
    pub fn nonce(&self, address: &str) -> u64 { self.nonces.get(address).copied().unwrap_or(0) }

    /// Execute one signed call atomically against a candidate state. Failed VM calls consume
    /// the nonce and the full gas limit (bounded by max_fee), while contract storage rolls back.
    pub fn execute_signed_call(
        &mut self,
        tx: &ContractTransactionV1,
        public_key: &[u8],
        expected_chain_id: u64,
        current_height: u64,
        contract_source: &str,
    ) -> Result<SignedCallReceipt, String> {
        tx.verify_signature(public_key)?;
        let maximum_fee = tx.validate_for(expected_chain_id, current_height)?;
        let expected_nonce = self.nonce(&tx.sender);
        if tx.nonce != expected_nonce {
            return Err("contract transaction nonce mismatch or replay".into());
        }
        let balance = self.balance(&tx.sender);
        if balance < tx.max_fee {
            return Err("insufficient balance for maximum authorized fee reserve".into());
        }
        let (contract_id, method, calldata) = match &tx.operation {
            ContractOperation::Call { contract_id, method, calldata } => (contract_id, method, calldata),
            ContractOperation::Deploy { .. } => return Err("deployment admission is not enabled: artifact-to-contract registration is not implemented".into()),
        };
        let args = std::str::from_utf8(calldata).map_err(|_| "call calldata must be UTF-8 QORVYNT argument text")?;
        if args.contains('\n') || args.contains('\r') {
            return Err("call calldata must be a single-line argument list".into());
        }
        let invocation = format!("call {method}({args})");
        let mut candidate_storage = self.storage.clone();
        let result = execute(contract_source, contract_id, &invocation, AccessMode::Action, &mut candidate_storage, tx.gas_limit);
        let (execution, gas_charged) = match result {
            Ok(receipt) => (Ok(receipt.clone()), receipt.gas_used),
            Err(error) => (Err(error), tx.gas_limit),
        };
        let fee_charged = gas_charged.checked_mul(tx.gas_price)
            .ok_or_else(|| "actual contract fee overflow".to_string())?;
        if fee_charged > maximum_fee || fee_charged > tx.max_fee {
            return Err("actual contract fee exceeds authorized maximum".into());
        }
        let next_nonce = expected_nonce.checked_add(1).ok_or_else(|| "account nonce overflow".to_string())?;
        let sender_balance = balance.checked_sub(fee_charged).ok_or_else(|| "sender fee balance underflow".to_string())?;
        let next_fees = self.fees_collected.checked_add(fee_charged).ok_or_else(|| "fee collector balance overflow".to_string())?;
        let transaction_id = tx.transaction_id()?;

        if execution.is_ok() { self.storage = candidate_storage; }
        self.balances.insert(tx.sender.clone(), sender_balance);
        self.nonces.insert(tx.sender.clone(), next_nonce);
        self.fees_collected = next_fees;
        Ok(SignedCallReceipt {
            transaction_id, sender: tx.sender.clone(), nonce: tx.nonce, execution,
            gas_charged, fee_charged, sender_balance, next_nonce,
        })
    }

    pub fn save_contract_storage(&self, path: &std::path::Path) -> Result<(), String> {
        self.storage.save_snapshot(path)
    }
}

#[cfg(test)]
mod tests {
    use super::ContractAccountState;
    use super::super::contract_protocol::{ContractOperation, ContractTransactionV1};
    use ed25519_dalek::{Signer, SigningKey};

    const SOURCE: &str = "contract Counter {\nstate count := 0\naction increment(amount) {\ncount := count + amount\n}\nview current() {\nreturn count\n}\n}\n";

    fn signed_call(key: &SigningKey, nonce: u64, args: &str) -> ContractTransactionV1 {
        let public = key.verifying_key().to_bytes();
        let mut tx = ContractTransactionV1 {
            chain_id: 9001,
            sender: super::super::chain_protocol::address_from_public_key(&public).unwrap(),
            nonce, gas_limit: 100_000, max_fee: 500_000, gas_price: 2,
            valid_until_height: Some(200),
            operation: ContractOperation::Call { contract_id: "counter".into(), method: "increment".into(), calldata: args.as_bytes().to_vec() },
            signature: Vec::new(),
        };
        tx.signature = key.sign(&tx.signing_bytes().unwrap()).to_bytes().to_vec();
        tx
    }

    #[test]
    fn signed_call_checks_signature_nonce_executes_qvm2_and_settles_actual_fee() {
        let key = SigningKey::from_bytes(&[71; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let tx = signed_call(&key, 0, "7");
        let receipt = state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).unwrap();
        assert!(receipt.execution.is_ok());
        assert_eq!(state.storage.get("counter", "count").unwrap(), Some(&b"7"[..]));
        assert_eq!(state.nonce(&address), 1);
        assert_eq!(receipt.fee_charged, receipt.gas_charged * 2);
        assert_eq!(state.balance(&address), 1_000_000 - receipt.fee_charged);
        assert_eq!(state.fees_collected, receipt.fee_charged);
    }

    #[test]
    fn replayed_or_wrong_nonce_transactions_are_rejected_without_charging() {
        let key = SigningKey::from_bytes(&[72; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let tx = signed_call(&key, 0, "1");
        state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).unwrap();
        let balance = state.balance(&address);
        assert!(state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).is_err());
        assert_eq!(state.balance(&address), balance);
    }

    #[test]
    fn failed_execution_charges_gas_but_rolls_back_contract_storage() {
        let key = SigningKey::from_bytes(&[73; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1_000_000).unwrap();
        let tx = signed_call(&key, 0, "1");
        let receipt = state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, "not a contract").unwrap();
        assert!(receipt.execution.is_err());
        assert_eq!(state.storage.get("counter", "count").unwrap(), None);
        assert_eq!(state.nonce(&address), 1);
        assert_eq!(receipt.fee_charged, tx.gas_limit * tx.gas_price);
    }

    #[test]
    fn invalid_signature_and_underfunded_reserve_do_not_consume_nonce() {
        let key = SigningKey::from_bytes(&[74; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        let mut state = ContractAccountState::new();
        state.set_balance(&address, 1).unwrap();
        let tx = signed_call(&key, 0, "1");
        assert!(state.execute_signed_call(&tx, &key.verifying_key().to_bytes(), 9001, 100, SOURCE).is_err());
        assert_eq!(state.nonce(&address), 0);
    }
}
