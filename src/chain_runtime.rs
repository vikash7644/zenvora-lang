//! Deterministic transaction runtime prototype powered by the Zenvora Lang VM.
//! This is a local execution harness, not a consensus protocol or production chain.

use std::collections::BTreeMap;

use super::{compile, run_with_output};

const TRANSFER_RULE: &str = r#"
define sender_balance := 0
define recipient_balance := 0
define transfer_amount := 0

if transfer_amount > 0 {
    if sender_balance >= transfer_amount {
        sender_balance := sender_balance - transfer_amount
        recipient_balance := recipient_balance + transfer_amount
        show "ACCEPTED"
    } otherwise {
        show "REJECTED"
    }
} otherwise {
    show "REJECTED"
}

show sender_balance
show recipient_balance
"#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferReceipt {
    pub sequence: u64,
    pub from: String,
    pub to: String,
    pub amount: i64,
    pub accepted: bool,
    pub sender_balance: i64,
    pub recipient_balance: i64,
}

#[derive(Debug, Clone, Default)]
pub struct ChainRuntime {
    balances: BTreeMap<String, i64>,
    receipts: Vec<TransferReceipt>,
}

impl ChainRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_genesis_balance(&mut self, address: &str, amount: i64) -> Result<(), String> {
        if !valid_address(address) {
            return Err("invalid address: use 1-64 ASCII letters, digits, or underscores".into());
        }
        if amount < 0 {
            return Err("genesis balance cannot be negative".into());
        }
        self.balances.insert(address.to_string(), amount);
        Ok(())
    }

    pub fn balance(&self, address: &str) -> i64 {
        *self.balances.get(address).unwrap_or(&0)
    }

    pub fn receipts(&self) -> &[TransferReceipt] {
        &self.receipts
    }

    /// Runs the transfer rule through the actual Zenvora Lang compiler and VM.
    /// Ledger balances are only committed after a successful VM result.
    pub fn transfer(&mut self, from: &str, to: &str, amount: i64) -> Result<TransferReceipt, String> {
        if !valid_address(from) || !valid_address(to) {
            return Err("invalid address: use 1-64 ASCII letters, digits, or underscores".into());
        }
        if from == to {
            return Err("self-transfers are not supported in this prototype".into());
        }

        let sender_before = self.balance(from);
        let recipient_before = self.balance(to);
        let source = TRANSFER_RULE
            .replace("define sender_balance := 0", &format!("define sender_balance := {sender_before}"))
            .replace("define recipient_balance := 0", &format!("define recipient_balance := {recipient_before}"))
            .replace("define transfer_amount := 0", &format!("define transfer_amount := {amount}"));

        let bytecode = compile(&source).map_err(|e| format!("Zenvora compile error: {e}"))?;
        let mut output = Vec::new();
        run_with_output(&bytecode, &mut |line| output.push(line))
            .map_err(|e| format!("Zenvora VM error: {e}"))?;

        if output.len() != 3 {
            return Err("Zenvora VM returned an invalid transfer result".into());
        }
        let accepted = match output[0].as_str() {
            "ACCEPTED" => true,
            "REJECTED" => false,
            _ => return Err("Zenvora VM returned an unknown transfer status".into()),
        };
        let sender_after: i64 = output[1].parse().map_err(|_| "Zenvora VM returned an invalid sender balance")?;
        let recipient_after: i64 = output[2].parse().map_err(|_| "Zenvora VM returned an invalid recipient balance")?;

        if accepted {
            self.balances.insert(from.to_string(), sender_after);
            self.balances.insert(to.to_string(), recipient_after);
        } else if sender_after != sender_before || recipient_after != recipient_before {
            return Err("rejected transfer unexpectedly changed balances".into());
        }

        let receipt = TransferReceipt {
            sequence: self.receipts.len() as u64 + 1,
            from: from.to_string(),
            to: to.to_string(),
            amount,
            accepted,
            sender_balance: sender_after,
            recipient_balance: recipient_after,
        };
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }
}

fn valid_address(address: &str) -> bool {
    !address.is_empty()
        && address.len() <= 64
        && address
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::ChainRuntime;

    fn funded_runtime() -> ChainRuntime {
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance("alice", 1_000).unwrap();
        runtime.set_genesis_balance("bob", 0).unwrap();
        runtime
    }

    #[test]
    fn valid_transfer_executes_in_zenvora_vm_and_commits_state() {
        let mut runtime = funded_runtime();
        let receipt = runtime.transfer("alice", "bob", 125).unwrap();

        assert!(receipt.accepted);
        assert_eq!(receipt.sequence, 1);
        assert_eq!(runtime.balance("alice"), 875);
        assert_eq!(runtime.balance("bob"), 125);
        assert_eq!(runtime.receipts(), &[receipt]);
    }

    #[test]
    fn overspend_is_rejected_without_mutating_state() {
        let mut runtime = funded_runtime();
        let receipt = runtime.transfer("alice", "bob", 1_001).unwrap();

        assert!(!receipt.accepted);
        assert_eq!(runtime.balance("alice"), 1_000);
        assert_eq!(runtime.balance("bob"), 0);
    }

    #[test]
    fn zero_and_negative_transfers_are_rejected_atomically() {
        let mut runtime = funded_runtime();
        assert!(!runtime.transfer("alice", "bob", 0).unwrap().accepted);
        assert!(!runtime.transfer("alice", "bob", -1).unwrap().accepted);
        assert_eq!(runtime.balance("alice"), 1_000);
        assert_eq!(runtime.balance("bob"), 0);
    }

    #[test]
    fn accepted_transactions_compose_against_committed_state() {
        let mut runtime = funded_runtime();
        assert!(runtime.transfer("alice", "bob", 300).unwrap().accepted);
        assert!(runtime.transfer("bob", "alice", 50).unwrap().accepted);
        assert_eq!(runtime.balance("alice"), 750);
        assert_eq!(runtime.balance("bob"), 250);
        assert_eq!(runtime.receipts().len(), 2);
    }

    #[test]
    fn invalid_addresses_and_self_transfers_are_rejected_before_execution() {
        let mut runtime = funded_runtime();
        assert!(runtime.transfer("alice; show 999", "bob", 1).is_err());
        assert!(runtime.transfer("alice", "alice", 1).is_err());
        assert!(runtime.set_genesis_balance("bob", -1).is_err());
        assert!(runtime.receipts().is_empty());
    }

    #[test]
    fn arithmetic_overflow_in_vm_does_not_commit_partial_state() {
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance("alice", i64::MAX).unwrap();
        runtime.set_genesis_balance("bob", 1).unwrap();
        let error = runtime.transfer("alice", "bob", i64::MAX).unwrap_err();
        assert!(error.contains("VM error"));
        assert_eq!(runtime.balance("alice"), i64::MAX);
        assert_eq!(runtime.balance("bob"), 1);
        assert!(runtime.receipts().is_empty());
    }
}
