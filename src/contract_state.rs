//! Deterministic, isolated contract storage primitives.
//!
//! This module is a runtime building block only. It is not yet wired into transaction
//! admission, persistent block replay, gas charging, or deployment.
use std::collections::BTreeMap;
use sha2::{Digest, Sha256};

pub const MAX_CONTRACT_ID_BYTES: usize = 64;
pub const MAX_STORAGE_KEY_BYTES: usize = 256;
pub const MAX_STORAGE_VALUE_BYTES: usize = 64 * 1024;
pub const MAX_CONTRACT_STORAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessMode {
    Action,
    View,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ContractState {
    values: BTreeMap<(String, String), Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateOverlay {
    contract_id: String,
    mode: AccessMode,
    writes: BTreeMap<String, Option<Vec<u8>>>,
}

impl Default for AccessMode {
    fn default() -> Self { Self::Action }
}

impl ContractState {
    pub fn new() -> Self { Self::default() }

    pub fn begin(&self, contract_id: &str, mode: AccessMode) -> Result<StateOverlay, String> {
        validate_contract_id(contract_id)?;
        Ok(StateOverlay {
            contract_id: contract_id.to_owned(),
            mode,
            writes: BTreeMap::new(),
        })
    }

    pub fn get(&self, contract_id: &str, key: &str) -> Result<Option<&[u8]>, String> {
        validate_contract_id(contract_id)?;
        validate_key(key)?;
        Ok(self.values.get(&(contract_id.to_owned(), key.to_owned())).map(Vec::as_slice))
    }

    /// Commit an action's overlay atomically. A view overlay is never allowed to commit.
    pub fn commit(&mut self, overlay: StateOverlay) -> Result<(), String> {
        if overlay.mode == AccessMode::View {
            return Err("view execution cannot commit state changes".into());
        }
        let mut candidate = self.values.clone();
        for (key, value) in overlay.writes {
            let composite = (overlay.contract_id.clone(), key);
            match value {
                Some(value) => { candidate.insert(composite, value); }
                None => { candidate.remove(&composite); }
            }
        }
        let total_bytes = candidate.iter().try_fold(0usize, |sum, ((contract, key), value)| {
            sum.checked_add(contract.len())
                .and_then(|n| n.checked_add(key.len()))
                .and_then(|n| n.checked_add(value.len()))
                .ok_or_else(|| "contract state size overflow".to_string())
        })?;
        if total_bytes > MAX_CONTRACT_STORAGE_BYTES {
            return Err("contract state size limit exceeded".into());
        }
        self.values = candidate;
        Ok(())
    }

    /// Canonical state root: sorted keys, length-prefixed bytes, and domain separation.
    pub fn state_root(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(b"ZENVORA_CONTRACT_STATE_V1\0");
        hasher.update((self.values.len() as u32).to_be_bytes());
        for ((contract_id, key), value) in &self.values {
            hasher.update((contract_id.len() as u32).to_be_bytes());
            hasher.update(contract_id.as_bytes());
            hasher.update((key.len() as u32).to_be_bytes());
            hasher.update(key.as_bytes());
            hasher.update((value.len() as u32).to_be_bytes());
            hasher.update(value);
        }
        hasher.finalize().into()
    }

    /// Encode a canonical, versioned snapshot. This does not itself write a file or
    /// establish that a snapshot is trusted; callers must verify it against replayed state.
    pub fn encode_snapshot(&self) -> Result<Vec<u8>, String> {
        let mut out = Vec::new();
        out.extend_from_slice(b"ZENVORA_CONTRACT_STATE_V1\\0");
        let count = u32::try_from(self.values.len()).map_err(|_| "too many contract storage entries")?;
        out.extend_from_slice(&count.to_be_bytes());
        for ((contract_id, key), value) in &self.values {
            validate_contract_id(contract_id)?;
            validate_key(key)?;
            if value.len() > MAX_STORAGE_VALUE_BYTES {
                return Err("contract storage value exceeds size limit".into());
            }
            put_bytes(&mut out, contract_id.as_bytes())?;
            put_bytes(&mut out, key.as_bytes())?;
            put_bytes(&mut out, value)?;
            if out.len() > MAX_CONTRACT_STORAGE_BYTES + 1024 * 1024 {
                return Err("encoded contract snapshot exceeds size limit".into());
            }
        }
        Ok(out)
    }

    /// Decode only canonical, strictly sorted snapshots with bounded lengths and no trailing data.
    pub fn decode_snapshot(bytes: &[u8]) -> Result<Self, String> {
        const DOMAIN: &[u8] = b"ZENVORA_CONTRACT_STATE_V1\\0";
        const MAX_SNAPSHOT_BYTES: usize = MAX_CONTRACT_STORAGE_BYTES + 1024 * 1024;
        if bytes.len() > MAX_SNAPSHOT_BYTES {
            return Err("encoded contract snapshot exceeds size limit".into());
        }
        let mut reader = SnapshotReader { bytes, pos: 0 };
        reader.expect(DOMAIN)?;
        let count = reader.read_u32()? as usize;
        if count > MAX_CONTRACT_STORAGE_BYTES / 1 {
            return Err("contract snapshot entry count exceeds limit".into());
        }
        let mut state = Self::new();
        let mut previous: Option<(String, String)> = None;
        for _ in 0..count {
            let contract_id = reader.read_string(MAX_CONTRACT_ID_BYTES)?;
            let key = reader.read_string(MAX_STORAGE_KEY_BYTES)?;
            validate_contract_id(&contract_id)?;
            validate_key(&key)?;
            let value = reader.read_bytes(MAX_STORAGE_VALUE_BYTES)?.to_vec();
            let identity = (contract_id.clone(), key.clone());
            if previous.as_ref().is_some_and(|old| old >= &identity) {
                return Err("contract snapshot keys are not strictly sorted".into());
            }
            previous = Some(identity.clone());
            state.values.insert(identity, value);
        }
        reader.finish()?;
        let canonical = state.encode_snapshot()?;
        if canonical != bytes {
            return Err("contract snapshot is not canonically encoded".into());
        }
        Ok(state)
    }

    pub fn len(&self) -> usize { self.values.len() }
    pub fn is_empty(&self) -> bool { self.values.is_empty() }
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), String> {
    let len = u32::try_from(bytes.len()).map_err(|_| "snapshot field exceeds encoding limit")?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

struct SnapshotReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl SnapshotReader<'_> {
    fn take(&mut self, len: usize) -> Result<&[u8], String> {
        let end = self.pos.checked_add(len).ok_or_else(|| "snapshot length overflow".to_string())?;
        if end > self.bytes.len() {
            return Err("truncated contract snapshot".into());
        }
        let slice = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    fn expect(&mut self, expected: &[u8]) -> Result<(), String> {
        if self.take(expected.len())? != expected {
            return Err("unsupported contract snapshot version or domain".into());
        }
        Ok(())
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| "truncated snapshot integer")?;
        Ok(u32::from_be_bytes(bytes))
    }

    fn read_bytes(&mut self, max: usize) -> Result<&[u8], String> {
        let len = self.read_u32()? as usize;
        if len > max {
            return Err("snapshot field exceeds size limit".into());
        }
        self.take(len)
    }

    fn read_string(&mut self, max: usize) -> Result<String, String> {
        String::from_utf8(self.read_bytes(max)?.to_vec())
            .map_err(|_| "snapshot string is not UTF-8".into())
    }

    fn finish(self) -> Result<(), String> {
        if self.pos != self.bytes.len() {
            return Err("trailing data after contract snapshot".into());
        }
        Ok(())
    }
}

impl StateOverlay {
    pub fn get<'a>(&'a self, base: &'a ContractState, key: &str) -> Result<Option<&'a [u8]>, String> {
        validate_key(key)?;
        if let Some(value) = self.writes.get(key) {
            return Ok(value.as_deref());
        }
        base.get(&self.contract_id, key)
    }

    pub fn set(&mut self, key: &str, value: Vec<u8>) -> Result<(), String> {
        if self.mode == AccessMode::View {
            return Err("view execution cannot write contract state".into());
        }
        validate_key(key)?;
        if value.len() > MAX_STORAGE_VALUE_BYTES {
            return Err("contract storage value exceeds size limit".into());
        }
        self.writes.insert(key.to_owned(), Some(value));
        Ok(())
    }

    pub fn delete(&mut self, key: &str) -> Result<(), String> {
        if self.mode == AccessMode::View {
            return Err("view execution cannot write contract state".into());
        }
        validate_key(key)?;
        self.writes.insert(key.to_owned(), None);
        Ok(())
    }

    pub fn contract_id(&self) -> &str { &self.contract_id }
    pub fn is_read_only(&self) -> bool { self.mode == AccessMode::View }
}

fn validate_contract_id(contract_id: &str) -> Result<(), String> {
    if contract_id.is_empty()
        || contract_id.len() > MAX_CONTRACT_ID_BYTES
        || !contract_id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        return Err("invalid contract ID".into());
    }
    Ok(())
}

fn validate_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > MAX_STORAGE_KEY_BYTES {
        return Err("contract storage key is empty or exceeds size limit".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{AccessMode, ContractState, MAX_STORAGE_VALUE_BYTES};

    #[test]
    fn writes_are_invisible_until_atomic_commit() {
        let mut state = ContractState::new();
        let mut action = state.begin("counter", AccessMode::Action).unwrap();
        action.set("value", b"1".to_vec()).unwrap();
        assert_eq!(state.get("counter", "value").unwrap(), None);
        state.commit(action).unwrap();
        assert_eq!(state.get("counter", "value").unwrap(), Some(&b"1"[..]));
    }

    #[test]
    fn discarded_overlay_rolls_back_all_writes() {
        let mut state = ContractState::new();
        let mut seed = state.begin("counter", AccessMode::Action).unwrap();
        seed.set("a", b"old".to_vec()).unwrap();
        state.commit(seed).unwrap();
        let root_before = state.state_root();
        let mut failing_call = state.begin("counter", AccessMode::Action).unwrap();
        failing_call.set("a", b"partial".to_vec()).unwrap();
        failing_call.set("b", b"partial".to_vec()).unwrap();
        drop(failing_call); // a reverted call discards its overlay
        assert_eq!(state.state_root(), root_before);
        assert_eq!(state.get("counter", "a").unwrap(), Some(&b"old"[..]));
        assert_eq!(state.get("counter", "b").unwrap(), None);
    }

    #[test]
    fn view_cannot_write_or_commit() {
        let mut state = ContractState::new();
        let mut view = state.begin("read_only", AccessMode::View).unwrap();
        assert!(view.set("x", b"no".to_vec()).is_err());
        assert!(state.commit(view).is_err());
        assert!(state.is_empty());
    }

    #[test]
    fn contract_namespaces_are_isolated() {
        let mut state = ContractState::new();
        let mut first = state.begin("alpha", AccessMode::Action).unwrap();
        first.set("balance", b"10".to_vec()).unwrap();
        state.commit(first).unwrap();
        let second = state.begin("beta", AccessMode::Action).unwrap();
        assert_eq!(second.get(&state, "balance").unwrap(), None);
        assert_eq!(state.get("alpha", "balance").unwrap(), Some(&b"10"[..]));
    }

    #[test]
    fn deletes_are_overlay_local_until_commit() {
        let mut state = ContractState::new();
        let mut seed = state.begin("c", AccessMode::Action).unwrap();
        seed.set("k", b"v".to_vec()).unwrap();
        state.commit(seed).unwrap();
        let mut action = state.begin("c", AccessMode::Action).unwrap();
        action.delete("k").unwrap();
        assert_eq!(action.get(&state, "k").unwrap(), None);
        assert_eq!(state.get("c", "k").unwrap(), Some(&b"v"[..]));
        state.commit(action).unwrap();
        assert_eq!(state.get("c", "k").unwrap(), None);
    }

    #[test]
    fn state_root_is_insertion_order_independent() {
        let mut a = ContractState::new();
        let mut b = ContractState::new();
        for (key, value) in [("z", "last"), ("a", "first")] {
            let mut tx = a.begin("c", AccessMode::Action).unwrap();
            tx.set(key, value.as_bytes().to_vec()).unwrap();
            a.commit(tx).unwrap();
        }
        for (key, value) in [("a", "first"), ("z", "last")] {
            let mut tx = b.begin("c", AccessMode::Action).unwrap();
            tx.set(key, value.as_bytes().to_vec()).unwrap();
            b.commit(tx).unwrap();
        }
        assert_eq!(a.state_root(), b.state_root());
    }

    #[test]
    fn oversized_value_is_rejected_without_mutation() {
        let mut state = ContractState::new();
        let mut action = state.begin("c", AccessMode::Action).unwrap();
        assert!(action.set("k", vec![0; MAX_STORAGE_VALUE_BYTES + 1]).is_err());
        assert!(state.is_empty());
    }

    #[test]
    fn canonical_snapshot_round_trips_and_preserves_root() {
        let mut state = ContractState::new();
        for (contract, key, value) in [("z", "b", "two"), ("a", "a", "one")] {
            let mut action = state.begin(contract, AccessMode::Action).unwrap();
            action.set(key, value.as_bytes().to_vec()).unwrap();
            state.commit(action).unwrap();
        }
        let encoded = state.encode_snapshot().unwrap();
        let restored = ContractState::decode_snapshot(&encoded).unwrap();
        assert_eq!(restored, state);
        assert_eq!(restored.state_root(), state.state_root());
    }

    #[test]
    fn snapshot_rejects_trailing_bytes_and_wrong_version() {
        let state = ContractState::new();
        let mut encoded = state.encode_snapshot().unwrap();
        encoded.push(0);
        assert!(ContractState::decode_snapshot(&encoded).is_err());
        assert!(ContractState::decode_snapshot(b"WRONG_VERSION").is_err());
    }
}
