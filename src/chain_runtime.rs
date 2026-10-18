//! Deterministic transaction runtime prototype powered by the Zenvora Lang VM.
//! This is a local execution harness, not a consensus protocol or production chain.

use std::collections::{BTreeMap, BTreeSet};

use super::chain_protocol::TransactionV1;
use sha2::{Digest, Sha256};

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

const LEDGER_VERSION: &str = "ZENVORA_LEDGER_V3";
const LEGACY_LEDGER_VERSION: &str = "ZENVORA_LEDGER_V2";
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
    /// Present only when the receipt was produced by authenticated transaction admission.
    pub nonce: Option<u64>,
    /// Lowercase hexadecimal transaction ID, present for signed transactions.
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ChainRuntime {
    genesis_balances: BTreeMap<String, i64>,
    balances: BTreeMap<String, i64>,
    receipts: Vec<TransferReceipt>,
    account_nonces: BTreeMap<String, u64>,
    seen_transaction_ids: BTreeSet<String>,
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

    /// Read-only genesis allocation set used to anchor deterministic block replay.
    pub fn genesis_balances(&self) -> &BTreeMap<String, i64> {
        &self.genesis_balances
    }

    pub fn receipts(&self) -> &[TransferReceipt] {
        &self.receipts
    }

    /// Next expected nonce for an account. Signed transactions start at nonce zero.
    pub fn account_nonce(&self, address: &str) -> u64 {
        *self.account_nonces.get(address).unwrap_or(&0)
    }

    /// Deterministic state commitment over sorted balances and account nonces.
    pub fn state_root(&self) -> [u8; 32] {
        let addresses: BTreeSet<&str> = self.balances.keys()
            .chain(self.account_nonces.keys())
            .map(String::as_str)
            .collect();
        let mut encoded = Vec::new();
        encoded.extend_from_slice(&(addresses.len() as u32).to_be_bytes());
        for address in addresses {
            let bytes = address.as_bytes();
            encoded.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
            encoded.extend_from_slice(bytes);
            encoded.extend_from_slice(&self.balance(address).to_be_bytes());
            encoded.extend_from_slice(&self.account_nonce(address).to_be_bytes());
        }
        hash_tagged(b"ZENVORA_STATE_ROOT_V1\0", &encoded)
    }

    /// Deterministic Merkle commitment for an ordered set of execution receipts.
    pub fn receipt_root_for(receipts: &[TransferReceipt]) -> Result<[u8; 32], String> {
        let mut level = Vec::with_capacity(receipts.len());
        for receipt in receipts {
            if !valid_address(&receipt.from) || !valid_address(&receipt.to) || receipt.from == receipt.to {
                return Err("receipt contains invalid addresses".into());
            }
            if receipt.nonce.is_some() != receipt.transaction_id.is_some() {
                return Err("receipt has incomplete signed transaction metadata".into());
            }
            if receipt.transaction_id.as_ref().is_some_and(|id| !valid_transaction_id(id)) {
                return Err("receipt contains an invalid transaction ID".into());
            }
            let mut encoded = Vec::new();
            encoded.extend_from_slice(&receipt.sequence.to_be_bytes());
            append_string(&mut encoded, &receipt.from)?;
            append_string(&mut encoded, &receipt.to)?;
            encoded.extend_from_slice(&receipt.amount.to_be_bytes());
            encoded.push(u8::from(receipt.accepted));
            encoded.extend_from_slice(&receipt.sender_balance.to_be_bytes());
            encoded.extend_from_slice(&receipt.recipient_balance.to_be_bytes());
            match receipt.nonce {
                Some(nonce) => {
                    encoded.push(1);
                    encoded.extend_from_slice(&nonce.to_be_bytes());
                    append_string(&mut encoded, receipt.transaction_id.as_deref().unwrap_or(""))?;
                }
                None => encoded.push(0),
            }
            level.push(hash_tagged(b"ZENVORA_RECEIPT_LEAF_V1\0", &encoded));
        }
        if level.is_empty() {
            return Ok(hash_tagged(b"ZENVORA_RECEIPT_EMPTY_V1\0", &[]));
        }
        while level.len() > 1 {
            let mut next = Vec::with_capacity((level.len() + 1) / 2);
            for pair in level.chunks(2) {
                let right = pair.get(1).unwrap_or(&pair[0]);
                let mut encoded = Vec::with_capacity(64);
                encoded.extend_from_slice(&pair[0]);
                encoded.extend_from_slice(right);
                next.push(hash_tagged(b"ZENVORA_RECEIPT_NODE_V1\0", &encoded));
            }
            level = next;
        }
        Ok(level[0])
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
                let nonce = receipt.nonce.map(|value| value.to_string()).unwrap_or_else(|| "-".into());
                let transaction_id = receipt.transaction_id.as_deref().unwrap_or("-");
                writeln!(file, "R\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    receipt.sequence, receipt.from, receipt.to, receipt.amount,
                    receipt.accepted, receipt.sender_balance, receipt.recipient_balance,
                    nonce, transaction_id)
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
        // Bound the actual read as well as checking metadata: a file can grow or be
        // replaced between metadata inspection and reading, so metadata alone is not a
        // reliable allocation limit for untrusted ledger input.
        use std::io::Read;
        let mut content = String::new();
        std::fs::File::open(path)
            .map_err(|e| format!("cannot open ledger file: {e}"))?
            .take(MAX_LEDGER_BYTES + 1)
            .read_to_string(&mut content)
            .map_err(|e| format!("cannot read ledger file: {e}"))?;
        if content.len() as u64 > MAX_LEDGER_BYTES {
            return Err("ledger exceeds configured file-size limit".into());
        }
        let mut lines = content.lines();
        let version = lines.next().ok_or_else(|| "missing ledger file version".to_string())?;
        if version != LEDGER_VERSION && version != LEGACY_LEDGER_VERSION {
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
            let mut fields: Vec<&str> = line.split('\t').collect();
            if fields.first() == Some(&"R") {
                if version == LEGACY_LEDGER_VERSION {
                    if fields.len() != 8 {
                        return Err(format!("ledger line {line_number} has invalid legacy receipt fields"));
                    }
                    fields.push("-");
                    fields.push("-");
                } else if fields.len() != 10 {
                    return Err(format!("ledger line {line_number} has invalid receipt fields"));
                }
            }
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
                ["R", sequence, from, to, amount, accepted, sender_balance, recipient_balance, nonce_field, transaction_id_field] => {
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

                    let nonce = if *nonce_field == "-" {
                        None
                    } else {
                        Some(nonce_field.parse::<u64>()
                            .map_err(|_| format!("ledger line {line_number} has invalid signed nonce"))?)
                    };
                    let transaction_id = if *transaction_id_field == "-" {
                        None
                    } else {
                        let id = (*transaction_id_field).to_string();
                        if !valid_transaction_id(&id) {
                            return Err(format!("ledger line {line_number} has invalid transaction ID"));
                        }
                        Some(id)
                    };
                    match (nonce, transaction_id.as_ref()) {
                        (None, None) => {}
                        (Some(nonce), Some(id)) => {
                            let expected_nonce = runtime.account_nonce(from);
                            if nonce != expected_nonce {
                                return Err(format!("ledger line {line_number} has non-contiguous account nonce"));
                            }
                            if runtime.seen_transaction_ids.contains(id) {
                                return Err(format!("ledger line {line_number} repeats a transaction ID"));
                            }
                            let next_nonce = nonce.checked_add(1)
                                .ok_or_else(|| format!("ledger line {line_number} account nonce overflow"))?;
                            runtime.account_nonces.insert((*from).to_string(), next_nonce);
                            runtime.seen_transaction_ids.insert(id.clone());
                        }
                        _ => return Err(format!("ledger line {line_number} has incomplete signed transaction metadata")),
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
                        nonce, transaction_id,
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
            nonce: None,
            transaction_id: None,
        };
        self.receipts.push(receipt.clone());
        Ok(receipt)
    }

    /// Authenticate, admit, and execute a signed transfer through the Zenvora Lang VM.
    /// The account nonce and transaction ID are committed only after transfer execution succeeds.
    /// Fees are reserved during admission but are not charged until fee accounting is implemented.
    pub fn apply_signed_transaction(
        &mut self,
        transaction: &TransactionV1,
        public_key: &[u8],
        expected_chain_id: u64,
        current_height: u64,
    ) -> Result<TransferReceipt, String> {
        let sender = transaction.sender.clone();
        let expected_nonce = self.account_nonce(&sender);
        let sender_balance = self.balance(&sender);
        if sender_balance < 0 {
            return Err("sender balance cannot be negative".into());
        }
        if expected_nonce == u64::MAX {
            return Err("account nonce exhausted".into());
        }
        transaction.validate_signed_transfer(
            public_key,
            expected_chain_id,
            current_height,
            expected_nonce,
            sender_balance as u64,
        )?;
        let amount = i64::try_from(transaction.amount)
            .map_err(|_| "transaction amount exceeds the local VM's integer range")?;
        let id_bytes = transaction.transaction_id()?;
        let transaction_id = hex_encode(&id_bytes);
        if self.seen_transaction_ids.contains(&transaction_id) {
            return Err("transaction has already been applied".into());
        }

        let receipt = self.transfer(&sender, &transaction.recipient, amount)?;
        if !receipt.accepted {
            self.receipts.pop();
            return Err("signed transfer was unexpectedly rejected by the VM".into());
        }
        let next_nonce = expected_nonce + 1;
        let committed_receipt = self.receipts.last_mut()
            .ok_or_else(|| "internal error: transfer receipt missing after execution".to_string())?;
        committed_receipt.nonce = Some(expected_nonce);
        committed_receipt.transaction_id = Some(transaction_id.clone());
        let committed_receipt = committed_receipt.clone();
        self.account_nonces.insert(sender, next_nonce);
        self.seen_transaction_ids.insert(transaction_id);
        Ok(committed_receipt)
    }
}

fn hash_tagged(domain: &[u8], payload: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(payload);
    hasher.finalize().into()
}

fn append_string(output: &mut Vec<u8>, value: &str) -> Result<(), String> {
    let length = u32::try_from(value.len()).map_err(|_| "receipt field is too large")?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn valid_transaction_id(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
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
    use super::super::chain_protocol::{address_from_public_key, TransactionV1};
    use ed25519_dalek::{Signer, SigningKey};

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
    fn state_root_is_deterministic_and_changes_when_state_changes() {
        let mut first = funded_runtime();
        let second = funded_runtime();
        assert_eq!(first.state_root(), second.state_root());
        first.transfer("alice", "bob", 10).unwrap();
        assert_ne!(first.state_root(), second.state_root());
    }

    #[test]
    fn receipt_root_is_deterministic_order_sensitive_and_handles_empty_input() {
        let mut runtime = funded_runtime();
        let first = runtime.transfer("alice", "bob", 10).unwrap();
        let second = runtime.transfer("bob", "alice", 3).unwrap();
        let a = ChainRuntime::receipt_root_for(&[first.clone(), second.clone()]).unwrap();
        let b = ChainRuntime::receipt_root_for(&[first.clone(), second.clone()]).unwrap();
        let reversed = ChainRuntime::receipt_root_for(&[second, first]).unwrap();
        let empty = ChainRuntime::receipt_root_for(&[]).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, reversed);
        assert_ne!(a, empty);
    }

    #[test]
    fn signed_transactions_consume_and_persist_nonces_and_reject_replay() {
        let signing_key = SigningKey::from_bytes(&[61; 32]);
        let public_key = signing_key.verifying_key().to_bytes();
        let sender = address_from_public_key(&public_key).unwrap();
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance(&sender, 1_000).unwrap();
        runtime.set_genesis_balance("bob", 0).unwrap();

        let mut transaction = TransactionV1 {
            chain_id: 71,
            sender: sender.clone(),
            nonce: 0,
            recipient: "bob".into(),
            amount: 125,
            max_fee: 3,
            gas_limit: 21_000,
            valid_until_height: Some(100),
            signature: Vec::new(),
        };
        transaction.signature = signing_key
            .sign(&transaction.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();

        let receipt = runtime.apply_signed_transaction(&transaction, &public_key, 71, 1).unwrap();
        assert!(receipt.accepted);
        assert_eq!(receipt.nonce, Some(0));
        assert!(receipt.transaction_id.is_some());
        assert_eq!(runtime.account_nonce(&sender), 1);
        assert_eq!(runtime.balance(&sender), 875);
        assert_eq!(runtime.balance("bob"), 125);

        let path = std::env::temp_dir().join(format!("zenvora-signed-ledger-{}.zvr", std::process::id()));
        runtime.save_to_file(&path).unwrap();
        let mut restored = ChainRuntime::load_from_file(&path).unwrap();
        assert_eq!(restored.account_nonce(&sender), 1);
        assert_eq!(restored.balance(&sender), 875);
        assert!(restored.apply_signed_transaction(&transaction, &public_key, 71, 1).is_err());
        assert_eq!(restored.balance(&sender), 875);
        assert_eq!(restored.receipts().len(), 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn signed_transaction_with_tampered_amount_does_not_mutate_ledger() {
        let signing_key = SigningKey::from_bytes(&[62; 32]);
        let public_key = signing_key.verifying_key().to_bytes();
        let sender = address_from_public_key(&public_key).unwrap();
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance(&sender, 1_000).unwrap();
        runtime.set_genesis_balance("bob", 0).unwrap();
        let mut transaction = TransactionV1 {
            chain_id: 71, sender: sender.clone(), nonce: 0, recipient: "bob".into(),
            amount: 125, max_fee: 3, gas_limit: 21_000, valid_until_height: Some(100),
            signature: Vec::new(),
        };
        transaction.signature = signing_key.sign(&transaction.signing_bytes().unwrap()).to_bytes().to_vec();
        transaction.amount += 1;
        assert!(runtime.apply_signed_transaction(&transaction, &public_key, 71, 1).is_err());
        assert_eq!(runtime.account_nonce(&sender), 0);
        assert_eq!(runtime.balance(&sender), 1_000);
        assert_eq!(runtime.balance("bob"), 0);
        assert!(runtime.receipts().is_empty());
    }
    #[test]
    fn signed_transaction_admission_rejects_wrong_chain_expiry_nonce_gap_and_fee_reserve_atomically() {
        let signing_key = SigningKey::from_bytes(&[63; 32]);
        let public_key = signing_key.verifying_key().to_bytes();
        let sender = address_from_public_key(&public_key).unwrap();
        let mut runtime = ChainRuntime::new();
        runtime.set_genesis_balance(&sender, 1_000).unwrap();
        runtime.set_genesis_balance("bob", 0).unwrap();

        let mut transaction = TransactionV1 {
            chain_id: 71,
            sender: sender.clone(),
            nonce: 0,
            recipient: "bob".into(),
            amount: 125,
            max_fee: 3,
            gas_limit: 21_000,
            valid_until_height: Some(10),
            signature: Vec::new(),
        };
        transaction.signature = signing_key
            .sign(&transaction.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();

        assert!(runtime.apply_signed_transaction(&transaction, &public_key, 72, 1).is_err());
        assert!(runtime.apply_signed_transaction(&transaction, &public_key, 71, 11).is_err());

        transaction.nonce = 1;
        transaction.valid_until_height = None;
        transaction.signature = signing_key
            .sign(&transaction.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();
        assert!(runtime.apply_signed_transaction(&transaction, &public_key, 71, 1).is_err());

        assert_eq!(runtime.account_nonce(&sender), 0);
        assert_eq!(runtime.balance(&sender), 1_000);
        assert_eq!(runtime.balance("bob"), 0);
        assert!(runtime.receipts().is_empty());

        let mut low_balance_runtime = ChainRuntime::new();
        low_balance_runtime.set_genesis_balance(&sender, 100).unwrap();
        low_balance_runtime.set_genesis_balance("bob", 0).unwrap();
        transaction.nonce = 0;
        transaction.amount = 98;
        transaction.max_fee = 3;
        transaction.signature = signing_key
            .sign(&transaction.signing_bytes().unwrap())
            .to_bytes()
            .to_vec();

        assert!(low_balance_runtime
            .apply_signed_transaction(&transaction, &public_key, 71, 1)
            .unwrap_err()
            .contains("insufficient balance"));
        assert_eq!(low_balance_runtime.account_nonce(&sender), 0);
        assert_eq!(low_balance_runtime.balance(&sender), 100);
        assert_eq!(low_balance_runtime.balance("bob"), 0);
        assert!(low_balance_runtime.receipts().is_empty());
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
