//! Deterministic local token-ledger runtime for QUR manifests.
//! This is a single-process prototype, not a network, consensus engine, or wallet.
use std::collections::BTreeMap;
use sha2::{Digest, Sha256};
use super::qur::QurManifest;

const DOMAIN: &[u8] = b"QORVYNT_QUR_TOKEN_LEDGER_V1\\0";
const MAX_ACCOUNT_BYTES: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QurLedger {
    supplies: BTreeMap<String, u64>,
    balances: BTreeMap<(String, String), u64>,
}

impl QurLedger {
    /// Allocate each declared token's complete genesis supply to the reserved genesis account.
    pub fn from_manifest(manifest: &QurManifest) -> Result<Self, String> {
        let mut ledger = Self { supplies: BTreeMap::new(), balances: BTreeMap::new() };
        for token in &manifest.tokens {
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

    /// Apply a transfer atomically; failed checks never mutate the ledger.
    pub fn transfer(&mut self, token: &str, from: &str, to: &str, amount: u64) -> Result<(), String> {
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
        for item in value.get("tokens").and_then(|v| v.as_array()).ok_or("ledger tokens must be an array")? {
            let name = item.get("name").and_then(|v| v.as_str()).ok_or("ledger token name is missing")?;
            let supply = item.get("supply").and_then(|v| v.as_u64()).ok_or("ledger token supply must be an unsigned integer")?;
            if supply == 0 || ledger.supplies.insert(name.to_owned(), supply).is_some() {
                return Err("invalid or duplicate ledger token".into());
            }
        }
        for item in value.get("balances").and_then(|v| v.as_array()).ok_or("ledger balances must be an array")? {
            let token = item.get("token").and_then(|v| v.as_str()).ok_or("ledger balance token is missing")?;
            let account = item.get("account").and_then(|v| v.as_str()).ok_or("ledger balance account is missing")?;
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
