//! Deterministic local token-ledger runtime for QUR manifests.
//! This is a single-process prototype, not a network, consensus engine, or wallet.
use std::collections::BTreeMap;
use sha2::{Digest, Sha256};
use super::qur::QurManifest;
use super::contract_gas::{GasMeter, BASE_EXECUTION_GAS, STORAGE_READ_GAS, STORAGE_WRITE_GAS};
use super::contract_state::{AccessMode, ContractState};

const DOMAIN: &[u8] = b"QORVYNT_QUR_TOKEN_LEDGER_V1\\0";
const MAX_ACCOUNT_BYTES: usize = 128;
const MAX_TOKEN_NAME_BYTES: usize = 128;
const MAX_LEDGER_TOKENS: usize = 4096;
const MAX_LEDGER_BALANCES: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QurLedger {
    supplies: BTreeMap<String, u64>,
    balances: BTreeMap<(String, String), u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QurTransferEvent {
    pub name: &'static str,
    pub token: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub event_hash: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QurTransferReceipt {
    pub token: String,
    pub from: String,
    pub to: String,
    pub amount: u64,
    pub gas_used: u64,
    pub contract_state_root: [u8; 32],
    pub event: QurTransferEvent,
}

impl QurLedger {
    /// Allocate each declared token's complete genesis supply to the reserved genesis account.
    pub fn from_manifest(manifest: &QurManifest) -> Result<Self, String> {
        let mut ledger = Self { supplies: BTreeMap::new(), balances: BTreeMap::new() };
        if manifest.tokens.len() > MAX_LEDGER_TOKENS {
            return Err("QUR ledger token count exceeds the 4096-token safety limit".into());
        }
        for token in &manifest.tokens {
            validate_token_name(&token.name)?;
            if ledger.supplies.insert(token.name.clone(), token.supply).is_some() {
                return Err(format!("duplicate token '{}' in QUR ledger", token.name));
            }
            ledger.balances.insert((token.name.clone(), "genesis".into()), token.supply);
        }
        ledger.check_invariants()?;
        Ok(ledger)
    }

    pub fn balance(&self, token: &str, account: &str) -> u64 {
        self.balances.get(&(token.to_owned(), account.to_owned())).copied().unwrap_or(0)
    }

    pub fn total_supply(&self, token: &str) -> Option<u64> { self.supplies.get(token).copied() }

    /// Return the deterministic, non-zero token-holder set for governance snapshots.
    pub fn token_holders(&self, token: &str) -> Result<Vec<(String, u64)>, String> {
        validate_token_name(token)?;
        if !self.supplies.contains_key(token) { return Err(format!("unknown token '{token}'")); }
        Ok(self.balances.iter()
            .filter(|((name, _), balance)| name.as_str() == token && **balance > 0)
            .map(|((_, account), balance)| (account.as_str().to_owned(), *balance))
            .collect())
    }

    /// Apply a transfer atomically; failed checks never mutate the ledger.
    pub fn transfer(&mut self, token: &str, from: &str, to: &str, amount: u64) -> Result<(), String> {
        validate_token_name(token)?;
        validate_account(from)?;
        validate_account(to)?;
        if amount == 0 { return Err("token transfer amount must be greater than zero".into()); }
        if from == to { return Err("token transfer sender and recipient must differ".into()); }
        let supply = self.supplies.get(token).copied().ok_or_else(|| format!("unknown token '{token}'"))?;
        let from_balance = self.balance(token, from);
        let remaining = from_balance.checked_sub(amount).ok_or_else(|| "insufficient token balance".to_string())?;
        let to_balance = self.balance(token, to);
        let next_to = to_balance.checked_add(amount).ok_or_else(|| "token balance overflow".to_string())?;
        let mut candidate = self.clone();
        if remaining == 0 { candidate.balances.remove(&(token.to_owned(), from.to_owned())); }
        else { candidate.balances.insert((token.to_owned(), from.to_owned()), remaining); }
        candidate.balances.insert((token.to_owned(), to.to_owned()), next_to);
        candidate.check_invariants()?;
        if candidate.supplies.get(token) != Some(&supply) { return Err("token supply changed during transfer".into()); }
        *self = candidate;
        Ok(())
    }


    /// Apply a token transfer with deterministic gas charges and an atomic contract-storage overlay.
    pub fn transfer_with_contract_gas(&mut self, state: &mut ContractState, contract_id: &str,
        token: &str, from: &str, to: &str, amount: u64, gas_limit: u64) -> Result<QurTransferReceipt, String> {
        let mut meter = GasMeter::new(gas_limit);
        meter.charge(BASE_EXECUTION_GAS)?;
        meter.charge(STORAGE_READ_GAS.checked_mul(2).ok_or("gas calculation overflow")?)?;
        meter.charge(STORAGE_WRITE_GAS.checked_mul(2).ok_or("gas calculation overflow")?)?;
        let mut candidate = self.clone();
        candidate.transfer(token, from, to, amount)?;
        let mut event_bytes = Vec::new();
        for field in [token.as_bytes(), from.as_bytes(), to.as_bytes()] {
            event_bytes.extend_from_slice(&(field.len() as u32).to_be_bytes());
            event_bytes.extend_from_slice(field);
        }
        event_bytes.extend_from_slice(&amount.to_be_bytes());
        meter.charge_event(event_bytes.len())?;
        let mut event_hasher = Sha256::new();
        event_hasher.update(b"QORVYNT_QUR_TOKEN_TRANSFER_EVENT_V1\\0");
        event_hasher.update(&event_bytes);
        let event_hash: [u8; 32] = event_hasher.finalize().into();
        let supply = candidate.total_supply(token).ok_or_else(|| format!("unknown token '{token}'"))?;
        let mut overlay = state.begin(contract_id, AccessMode::Action)?;
        overlay.set(&format!("token/{token}/supply"), supply.to_string().into_bytes())?;
        for account in [from, to] {
            let key = format!("token/{token}/balance/{account}");
            let balance = candidate.balance(token, account);
            if balance == 0 { overlay.delete(&key)?; }
            else { overlay.set(&key, balance.to_string().into_bytes())?; }
        }
        state.commit(overlay)?;
        *self = candidate;
        Ok(QurTransferReceipt {
            token: token.into(), from: from.into(), to: to.into(), amount,
            gas_used: meter.used(), contract_state_root: state.state_root(),
            event: QurTransferEvent { name: "TokenTransfer", token: token.into(), from: from.into(),
                to: to.into(), amount, event_hash }
        })
    }

    pub fn state_root(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(DOMAIN);
        h.update((self.supplies.len() as u32).to_be_bytes());
        for (token, supply) in &self.supplies {
            h.update((token.len() as u32).to_be_bytes());
            h.update(token.as_bytes());
            h.update(supply.to_be_bytes());
        }
        h.update((self.balances.len() as u32).to_be_bytes());
        for ((token, account), balance) in &self.balances {
            h.update((token.len() as u32).to_be_bytes());
            h.update(token.as_bytes());
            h.update((account.len() as u32).to_be_bytes());
            h.update(account.as_bytes());
            h.update(balance.to_be_bytes());
        }
        h.finalize().into()
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "schema": "qorvynt.qur.token-ledger.v1",
            "tokens": self.supplies.iter().map(|(name, supply)| serde_json::json!({"name": name, "supply": supply})).collect::<Vec<_>>(),
            "balances": self.balances.iter().map(|((token, account), balance)| serde_json::json!({"token": token, "account": account, "balance": balance})).collect::<Vec<_>>(),
            "state_root": hex(&self.state_root())
        })
    }

    pub fn from_json(value: &serde_json::Value) -> Result<Self, String> {
        if value.get("schema").and_then(|v| v.as_str()) != Some("qorvynt.qur.token-ledger.v1") {
            return Err("unsupported QUR token ledger schema".into());
        }
        let mut ledger = Self { supplies: BTreeMap::new(), balances: BTreeMap::new() };
        let tokens = value.get("tokens").and_then(|v| v.as_array()).ok_or("ledger tokens must be an array")?;
        if tokens.len() > MAX_LEDGER_TOKENS {
            return Err("ledger token count exceeds the 4096-token safety limit".into());
        }
        for item in tokens {
            let name = item.get("name").and_then(|v| v.as_str()).ok_or("ledger token name is missing")?;
            validate_token_name(name)?;
            let supply = item.get("supply").and_then(|v| v.as_u64()).ok_or("ledger token supply must be an unsigned integer")?;
            if supply == 0 || ledger.supplies.insert(name.to_owned(), supply).is_some() {
                return Err("invalid or duplicate ledger token".into());
            }
        }
        let balances = value.get("balances").and_then(|v| v.as_array()).ok_or("ledger balances must be an array")?;
        if balances.len() > MAX_LEDGER_BALANCES {
            return Err("ledger balance count exceeds the 1000000-entry safety limit".into());
        }
        for item in balances {
            let token = item.get("token").and_then(|v| v.as_str()).ok_or("ledger balance token is missing")?;
            let account = item.get("account").and_then(|v| v.as_str()).ok_or("ledger balance account is missing")?;
            validate_token_name(token)?;
            validate_account(account)?;
            let balance = item.get("balance").and_then(|v| v.as_u64()).ok_or("ledger balance must be an unsigned integer")?;
            if !ledger.supplies.contains_key(token) || balance == 0
                || ledger.balances.insert((token.to_owned(), account.to_owned()), balance).is_some() {
                return Err("invalid or duplicate ledger balance".into());
            }
        }
        ledger.check_invariants()?;
        let expected = value.get("state_root").and_then(|v| v.as_str()).ok_or("ledger state_root is missing")?;
        if expected != hex(&ledger.state_root()) { return Err("QUR token ledger state_root mismatch".into()); }
        Ok(ledger)
    }

    fn check_invariants(&self) -> Result<(), String> {
        for token in self.balances.keys().map(|(token, _)| token) {
            if !self.supplies.contains_key(token) { return Err(format!("balance references unknown token '{token}'")); }
        }
        for (token, supply) in &self.supplies {
            let total = self.balances.iter().filter(|((name, _), _)| name == token)
                .try_fold(0u128, |sum, (_, balance)| sum.checked_add(*balance as u128))
                .ok_or_else(|| "token supply accounting overflow".to_string())?;
            if total != *supply as u128 { return Err(format!("token supply invariant failed for '{token}'")); }
        }
        Ok(())
    }
}

fn validate_token_name(token: &str) -> Result<(), String> {
    let mut bytes = token.bytes();
    let valid_first = matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'));
    if !valid_first || token.len() > MAX_TOKEN_NAME_BYTES
        || !bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("invalid token identifier (expected an ASCII identifier up to 128 bytes)".into());
    }
    Ok(())
}

fn validate_account(account: &str) -> Result<(), String> {
    if account.is_empty() || account.len() > MAX_ACCOUNT_BYTES
        || !account.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err("invalid token account identifier".into());
    }
    Ok(())
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes { out.push(DIGITS[(byte >> 4) as usize] as char); out.push(DIGITS[(byte & 15) as usize] as char); }
    out
}

#[cfg(test)]
mod tests {
    use super::QurLedger;
    use super::super::qur;

    const MANIFEST: &str = r#"chain MyChain {
consensus = "proof_of_stake"
native_token = "QOR"
}
token GOLD {
standard = "fungible"
supply = 100000000
}
"#;

    #[test]
    fn genesis_allocates_full_supply_and_transfer_preserves_it() {
        let manifest = qur::parse(MANIFEST).unwrap();
        let mut ledger = QurLedger::from_manifest(&manifest).unwrap();
        assert_eq!(ledger.balance("GOLD", "genesis"), 100_000_000);
        ledger.transfer("GOLD", "genesis", "alice", 250).unwrap();
        assert_eq!(ledger.balance("GOLD", "genesis"), 99_999_750);
        assert_eq!(ledger.balance("GOLD", "alice"), 250);
        assert_eq!(ledger.total_supply("GOLD"), Some(100_000_000));
    }

    #[test]
    fn failed_transfers_are_atomic() {
        let manifest = qur::parse(MANIFEST).unwrap();
        let mut ledger = QurLedger::from_manifest(&manifest).unwrap();
        let root = ledger.state_root();
        assert!(ledger.transfer("GOLD", "alice", "bob", 1).is_err());
        assert!(ledger.transfer("GOLD", "genesis", "bob", 0).is_err());
        assert!(ledger.transfer("MISSING", "genesis", "bob", 1).is_err());
        assert_eq!(ledger.state_root(), root);
    }

    #[test]
    fn gas_metered_transfer_updates_contract_storage_atomically() {
        use super::super::contract_state::ContractState;
        let manifest = qur::parse(MANIFEST).unwrap();
        let mut ledger = QurLedger::from_manifest(&manifest).unwrap();
        let mut state = ContractState::new();
        let receipt = ledger.transfer_with_contract_gas(&mut state, "qur_tokens", "GOLD", "genesis", "alice", 25, 30_000).unwrap();
        assert!(receipt.gas_used > 21_000);
        assert_eq!(receipt.event.name, "TokenTransfer");
        assert_eq!(receipt.event.amount, 25);
        assert_eq!(state.get("qur_tokens", "token/GOLD/supply").unwrap(), Some(&b"100000000"[..]));
        assert_eq!(state.get("qur_tokens", "token/GOLD/balance/alice").unwrap(), Some(&b"25"[..]));
        assert_eq!(state.get("qur_tokens", "token/GOLD/balance/genesis").unwrap(), Some(&b"99999975"[..]));
        assert_eq!(receipt.contract_state_root, state.state_root());
    }

    #[test]
    fn out_of_gas_does_not_change_ledger_or_contract_storage() {
        use super::super::contract_state::ContractState;
        let manifest = qur::parse(MANIFEST).unwrap();
        let mut ledger = QurLedger::from_manifest(&manifest).unwrap();
        let mut state = ContractState::new();
        let ledger_root = ledger.state_root();
        let storage_root = state.state_root();
        assert!(ledger.transfer_with_contract_gas(&mut state, "qur_tokens", "GOLD", "genesis", "alice", 25, 21_001).is_err());
        assert_eq!(ledger.state_root(), ledger_root);
        assert_eq!(state.state_root(), storage_root);
    }

    #[test]
    fn ledger_loader_rejects_invalid_token_identifiers_and_excessive_entries() {
        let manifest = qur::parse(MANIFEST).unwrap();
        let ledger = QurLedger::from_manifest(&manifest).unwrap().to_json();

        let mut invalid_name = ledger.clone();
        invalid_name["tokens"][0]["name"] = serde_json::json!("bad-token");
        assert!(QurLedger::from_json(&invalid_name).unwrap_err().contains("invalid token identifier"));

        let mut empty_name = ledger.clone();
        empty_name["tokens"][0]["name"] = serde_json::json!("");
        assert!(QurLedger::from_json(&empty_name).is_err());

        let mut oversized_balances = ledger;
        oversized_balances["balances"] = serde_json::json!(vec![serde_json::json!({
            "token": "GOLD", "account": "alice", "balance": 1
        }); MAX_LEDGER_BALANCES + 1]);
        assert!(QurLedger::from_json(&oversized_balances).unwrap_err().contains("1000000-entry safety limit"));
    }

    #[test]
    fn ledger_json_round_trip_verifies_state_root() {
        let manifest = qur::parse(MANIFEST).unwrap();
        let mut ledger = QurLedger::from_manifest(&manifest).unwrap();
        ledger.transfer("GOLD", "genesis", "alice", 7).unwrap();
        let encoded = ledger.to_json();
        let decoded = QurLedger::from_json(&encoded).unwrap();
        assert_eq!(decoded.state_root(), ledger.state_root());
        let mut tampered = encoded;
        tampered["balances"][0]["balance"] = serde_json::json!(999);
        assert!(QurLedger::from_json(&tampered).is_err());
    }
}
