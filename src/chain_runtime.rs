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

const LEDGER_VERSION: &str = "ZENVORA_LEDGER_V2";
const MAX_LEDGER_BYTES: u64 = 64 * 1024 * 1024;
const MAX_ACCOUNTS: usize = 100_000;
const MAX_RECEIPTS: usize = 1_000_000;

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
    genesis_balances: BTreeMap<String, i64>,
    balances: BTreeMap<String, i64>,
    receipts: Vec<TransferReceipt>,
}

impl ChainRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Genesis allocations are immutable after the first transfer.
    pub fn set_genesis_balance(&mut self, address: &str, amount: i64) -> Result<(), String> {
        if !valid_address(address) {
            return Err("invalid address: use 1-64 ASCII letters, digits, or underscores".into());
        }
        if amount < 0 {
            return Err("genesis balance cannot be negative".into());
        }
        if !self.receipts.is_empty() {
            return Err("genesis balances cannot change after transfers begin".into());
        }
        if self.genesis_balances.len() >= MAX_ACCOUNTS && !self.genesis_balances.contains_key(address) {
            return Err("genesis account limit exceeded".into());
        }
        self.genesis_balances.insert(address.to_string(), amount);
        self.balances.insert(address.to_string(), amount);
        Ok(())
    }

    pub fn balance(&self, address: &str) -> i64 {
        *self.balances.get(address).unwrap_or(&0)
    }

    pub fn receipts(&self) -> &[TransferReceipt] {
        &self.receipts
    }

    /// Saves genesis allocations and the receipt log. Current balances are derived by replay,
    /// rather than trusted as independent snapshot data.
    pub fn save_to_file(&self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        use std::io::Write;
        let path = path.as_ref();
        if self.genesis_balances.len() > MAX_ACCOUNTS || self.receipts.len() > MAX_RECEIPTS {
            return Err("ledger exceeds configured account or receipt limit".into());
        }
        let temp_path = path.with_extension("zvr.tmp");
        let write_result = (|| -> Result<(), String> {
            let mut file = std::fs::File::create(&temp_path)
                .map_err(|e| format!("cannot create ledger temp file: {e}"))?;
            writeln!(file, "{LEDGER_VERSION}").map_err(|e| format!("cannot write ledger: {e}"))?;
            for (address, balance) in &self.genesis_balances {
                writeln!(file, "G\t{address}\t{balance}")
                    .map_err(|e| format!("cannot write genesis allocation: {e}"))?;
            }
            for receipt in &self.receipts {
                writeln!(file, "R\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    receipt.sequence, receipt.from, receipt.to, receipt.amount,
                    receipt.accepted, receipt.sender_balance, receipt.recipient_balance)
                    .map_err(|e| format!("cannot write ledger receipt: {e}"))?;
            }
            file.sync_all().map_err(|e| format!("cannot sync ledger file: {e}"))?;
            let size = file.metadata().map_err(|e| format!("cannot inspect ledger file: {e}"))?.len();
            if size > MAX_LEDGER_BYTES {
                return Err("ledger exceeds configured file-size limit".into());
            }
            std::fs::rename(&temp_path, path)
                .map_err(|e| format!("cannot replace ledger file: {e}"))?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }
        write_result
    }

    /// Loads a V2 ledger by replaying every receipt from the declared genesis state.
    /// A malformed or inconsistent receipt rejects the entire file without partial state.
    pub fn load_from_file(path: impl AsRef<std::path::Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let metadata = std::fs::metadata(path)
            .map_err(|e| format!("cannot inspect ledger file: {e}"))?;
        if metadata.len() > MAX_LEDGER_BYTES {
            return Err("ledger exceeds configured file-size limit".into());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read ledger file: {e}"))?;
        let mut lines = content.lines();
        if lines.next() != Some(LEDGER_VERSION) {
            return Err("unsupported or missing ledger file version".into());
        }

        let mut runtime = Self::new();
        let mut seen_receipt = false;
        for (index, line) in lines.enumerate() {
            let line_number = index + 2;
            if line.is_empty() { continue; }
            if line.len() > 4096 {
                return Err(format!("ledger line {line_number} exceeds maximum length"));
            }
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["G", address, balance] if !seen_receipt => {
                    if !valid_address(address) {
                        return Err(format!("ledger line {line_number} has invalid genesis address"));
                    }
                    let balance: i64 = balance.parse()
                        .map_err(|_| format!("ledger line {line_number} has invalid genesis balance"))?;
                    if balance < 0 || runtime.genesis_balances.contains_key(*address) {
                        return Err(format!("ledger line {line_number} has negative balance or duplicate genesis address"));
                    }
                    if runtime.genesis_balances.len() >= MAX_ACCOUNTS {
                        return Err("ledger account limit exceeded".into());
                    }
                    runtime.genesis_balances.insert((*address).to_string(), balance);
                    runtime.balances.insert((*address).to_string(), balance);
                }
                ["R", sequence, from, to, amount, accepted, sender_balance, recipient_balance] => {
                    seen_receipt = true;
                    if runtime.receipts.len() >= MAX_RECEIPTS {
                        return Err("ledger receipt limit exceeded".into());
                    }
                    if !valid_address(from) || !valid_address(to) || from == to {
                        return Err(format!("ledger line {line_number} has invalid receipt addresses"));
                    }
                    let sequence: u64 = sequence.parse()
                        .map_err(|_| format!("ledger line {line_number} has invalid receipt sequence"))?;
                    let amount: i64 = amount.parse()
                        .map_err(|_| format!("ledger line {line_number} has invalid amount"))?;
                    let accepted: bool = accepted.parse()
                        .map_err(|_| format!("ledger line {line_number} has invalid accepted flag"))?;
                    let recorded_sender: i64 = sender_balance.parse()
                        .map_err(|_| format!("ledger line {line_number} has invalid sender balance"))?;
                    let recorded_recipient: i64 = recipient_balance.parse()
                        .map_err(|_| format!("ledger line {line_number} has invalid recipient balance"))?;
                    if sequence != runtime.receipts.len() as u64 + 1 {
                        return Err(format!("ledger line {line_number} has non-contiguous receipt sequence"));
                    }

                    let sender_before = runtime.balance(from);
                    let recipient_before = runtime.balance(to);
                    let should_accept = amount > 0 && sender_before >= amount;
                    if accepted != should_accept {
                        return Err(format!("ledger line {line_number} has an invalid acceptance decision"));
                    }
                    let (sender_after, recipient_after) = if accepted {
                        let sender_after = sender_before.checked_sub(amount)
                            .ok_or_else(|| format!("ledger line {line_number} sender arithmetic overflow"))?;
                        let recipient_after = recipient_before.checked_add(amount)
                            .ok_or_else(|| format!("ledger line {line_number} recipient arithmetic overflow"))?;
                        (sender_after, recipient_after)
                    } else {
                        (sender_before, recipient_before)
                    };
                    if recorded_sender != sender_after || recorded_recipient != recipient_after {
                        return Err(format!("ledger line {line_number} receipt balances do not match replayed state"));
                    }
                    if accepted {
                        runtime.balances.insert(from.to_string(), sender_after);
                        runtime.balances.insert(to.to_string(), recipient_after);
                    }
                    runtime.receipts.push(TransferReceipt {
                        sequence, from: (*from).to_string(), to: (*to).to_string(), amount,
                        accepted, sender_balance: sender_after, recipient_balance: recipient_after,
                    });
                }
                _ => return Err(format!("ledger line {line_number} has an invalid record or misplaced genesis entry")),
            }
        }
        Ok(runtime)
    }

    /// Runs the transfer rule through the actual Zenvora Lang compiler and VM.
    /// Ledger balances are committed only after a successful VM result.
    pub fn transfer(&mut self, from: &str, to: &str, amount: i64) -> Result<TransferReceipt, String> {
        if !valid_address(from) || !valid_address(to) {
            return Err("invalid address: use 1-64 ASCII letters, digits, or underscores".into());
        }
        if from == to {
            return Err("self-transfers are not supported in this prototype".into());
        }
        if self.receipts.len() >= MAX_RECEIPTS {
            return Err("ledger receipt limit exceeded".into());
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
            if amount <= 0 || sender_before < amount
                || sender_before.checked_sub(amount) != Some(sender_after)
                || recipient_before.checked_add(amount) != Some(recipient_after) {
                return Err("Zenvora VM returned inconsistent accepted-transfer balances".into());
            }
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
    fn genesis_cannot_be_changed_after_first_transfer() {
        let mut runtime = funded_runtime();
        runtime.transfer("alice", "bob", 10).unwrap();
        assert!(runtime.set_genesis_balance("alice", 10_000).is_err());
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
    fn ledger_round_trips_balances_and_receipts_across_restart() {
        let path = std::env::temp_dir().join(format!("zenvora-ledger-{}-17.zvr", std::process::id()));
        let mut runtime = funded_runtime();
        runtime.transfer("alice", "bob", 125).unwrap();
        runtime.transfer("bob", "alice", 25).unwrap();
        runtime.save_to_file(&path).unwrap();
        let restored = ChainRuntime::load_from_file(&path).unwrap();
        assert_eq!(restored.balance("alice"), 900);
        assert_eq!(restored.balance("bob"), 100);
        assert_eq!(restored.receipts(), runtime.receipts());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn ledger_loader_rejects_tampered_receipt_and_duplicate_genesis() {
        let path = std::env::temp_dir().join(format!("zenvora-ledger-bad-{}.zvr", std::process::id()));
        std::fs::write(&path, concat!(
            "ZENVORA_LEDGER_V2\n",
            "G\talice\t10\n",
            "G\tbob\t0\n",
            "R\t1\talice\tbob\t5\ttrue\t4\t6\n",
            "R\t2\talice\tbob\t5\ttrue\t999\t11\n"
        )).unwrap();
        assert!(ChainRuntime::load_from_file(&path).unwrap_err().contains("receipt balances"));
        std::fs::write(&path, "ZENVORA_LEDGER_V2\nG\talice\t10\nG\talice\t20\n").unwrap();
        assert!(ChainRuntime::load_from_file(&path).is_err());
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn loader_rejects_legacy_or_unknown_versions_explicitly() {
        let path = std::env::temp_dir().join(format!("zenvora-ledger-version-{}.zvr", std::process::id()));
        std::fs::write(&path, "ZENVORA_LEDGER_V1\n").unwrap();
        assert!(ChainRuntime::load_from_file(&path).unwrap_err().contains("unsupported"));
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn arithmetic_overflow_in_vm_does_not_commit_partial_state() {
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance("alice", i64::MAX).unwrap();
        runtime.set_genesis_balance("bob", 1).unwrap();
        let error = runtime.transfer("alice", "bob", i64::MAX).unwrap_err();
        assert!(error.contains("VM error") || error.contains("inconsistent"));
        assert_eq!(runtime.balance("alice"), i64::MAX);
        assert_eq!(runtime.balance("bob"), 1);
        assert!(runtime.receipts().is_empty());
    }
}
