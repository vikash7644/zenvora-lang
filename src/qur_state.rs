//! Canonical composition root for the local QORVYNT execution state.
//!
//! This binds the token ledger and contract storage roots into one deterministic
//! commitment. It is a composition primitive, not yet a consensus state transition.
use sha2::{Digest, Sha256};
use super::contract_state::ContractState;
use super::qur_runtime::QurLedger;

/// Combine independently canonicalized runtime components under a versioned domain.
pub fn execution_state_root(ledger: &QurLedger, contracts: &ContractState) -> [u8; 32] {
    let ledger_root = ledger.state_root();
    let contract_root = contracts.state_root();
    let mut hasher = Sha256::new();
    hasher.update(b"QORVYNT_EXECUTION_STATE_V1\0");
    hasher.update(1u16.to_be_bytes());
    hasher.update(b"QUR_TOKEN_LEDGER\0");
    hasher.update(ledger_root);
    hasher.update(b"CONTRACT_STORAGE\0");
    hasher.update(contract_root);
    hasher.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::execution_state_root;
    use super::super::contract_state::{AccessMode, ContractState};
    use super::super::qur;
    use super::super::qur_runtime::QurLedger;

    const MANIFEST: &str = r#"chain MyChain {
consensus = "proof_of_stake"
native_token = "QOR"
}
token GOLD {
standard = "fungible"
supply = 100000000
}
"#;

    fn genesis() -> QurLedger {
        QurLedger::from_manifest(&qur::parse(MANIFEST).unwrap()).unwrap()
    }

    #[test]
    fn chain_execution_root_commits_native_tokens_and_contract_storage() {
        use super::chain_execution_state_root;
        use super::super::chain_runtime::ChainRuntime;

        let mut native_a = ChainRuntime::new();
        native_a.set_genesis_balance("alice", 100).unwrap();
        let mut native_b = native_a.clone();
        let ledger_a = genesis();
        let mut ledger_b = genesis();
        let contracts_a = ContractState::new();
        let mut contracts_b = ContractState::new();

        let root = chain_execution_state_root(&native_a, &ledger_a, &contracts_a);
        assert_eq!(root, chain_execution_state_root(&native_a, &ledger_a, &contracts_a));

        native_b.transfer("alice", "bob", 1).unwrap();
        assert_ne!(root, chain_execution_state_root(&native_b, &ledger_a, &contracts_a));

        ledger_b.transfer("GOLD", "genesis", "alice", 1).unwrap();
        assert_ne!(root, chain_execution_state_root(&native_a, &ledger_b, &contracts_a));

        let mut action = contracts_b.begin("counter", AccessMode::Action).unwrap();
        action.set("value", b"1".to_vec()).unwrap();
        contracts_b.commit(action).unwrap();
        assert_ne!(root, chain_execution_state_root(&native_a, &ledger_a, &contracts_b));
    }

    #[test]
    fn execution_root_is_deterministic_for_identical_components() {
        let ledger_a = genesis();
        let ledger_b = genesis();
        let contracts_a = ContractState::new();
        let contracts_b = ContractState::new();
        assert_eq!(
            execution_state_root(&ledger_a, &contracts_a),
            execution_state_root(&ledger_b, &contracts_b)
        );
    }

    #[test]
    fn either_component_change_changes_composite_root() {
        let ledger = genesis();
        let mut changed_ledger = genesis();
        changed_ledger.transfer("GOLD", "genesis", "alice", 1).unwrap();
        let contracts = ContractState::new();
        let baseline = execution_state_root(&ledger, &contracts);
        assert_ne!(baseline, execution_state_root(&changed_ledger, &contracts));

        let mut changed_contracts = ContractState::new();
        let mut action = changed_contracts.begin("counter", AccessMode::Action).unwrap();
        action.set("value", b"1".to_vec()).unwrap();
        changed_contracts.commit(action).unwrap();
        assert_ne!(baseline, execution_state_root(&ledger, &changed_contracts));
    }

    #[test]
    fn execution_snapshot_round_trip_and_rejects_tampering() {
        use super::{decode_execution_snapshot, encode_execution_snapshot};
        let mut ledger = genesis();
        ledger.transfer("GOLD", "genesis", "alice", 42).unwrap();
        let mut contracts = ContractState::new();
        let mut action = contracts.begin("vault", AccessMode::Action).unwrap();
        action.set("owner", b"alice".to_vec()).unwrap();
        contracts.commit(action).unwrap();

        let snapshot = encode_execution_snapshot(&ledger, &contracts).unwrap();
        let (restored_ledger, restored_contracts) = decode_execution_snapshot(&snapshot).unwrap();
        assert_eq!(restored_ledger, ledger);
        assert_eq!(restored_contracts, contracts);
        assert_eq!(execution_state_root(&ledger, &contracts),
            execution_state_root(&restored_ledger, &restored_contracts));

        let mut tampered = snapshot.clone();
        tampered["contract_snapshot_hex"] = serde_json::Value::String("00".repeat(8));
        assert!(decode_execution_snapshot(&tampered).is_err());
        let mut wrong_root = snapshot;
        wrong_root["execution_state_root"] = serde_json::Value::String("00".repeat(32));
        assert!(decode_execution_snapshot(&wrong_root).unwrap_err().contains("root mismatch"));
    }

    #[test]
    fn component_order_is_fixed_and_roots_are_not_interchangeable() {
        let ledger = genesis();
        let contracts = ContractState::new();
        let first = execution_state_root(&ledger, &contracts);
        let second = execution_state_root(&ledger, &contracts);
        assert_eq!(first, second);
        assert_ne!(first, ledger.state_root());
        assert_ne!(first, contracts.state_root());
    }
}


/// Versioned commitment spanning native chain accounts, QUR tokens, and contract storage.
/// This is a deterministic composition primitive; block execution/replay must still
/// compute and verify it before it can be treated as a consensus state root.
pub fn chain_execution_state_root(
    chain: &super::chain_runtime::ChainRuntime,
    ledger: &QurLedger,
    contracts: &ContractState,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"QORVYNT_CHAIN_EXECUTION_STATE_V1\0");
    hasher.update(1u16.to_be_bytes());
    hasher.update(b"NATIVE_CHAIN_ACCOUNTS\0");
    hasher.update(chain.state_root());
    hasher.update(b"QUR_TOKEN_LEDGER\0");
    hasher.update(ledger.state_root());
    hasher.update(b"CONTRACT_STORAGE\0");
    hasher.update(contracts.state_root());
    hasher.finalize().into()
}


/// Build a portable, root-checked snapshot for the QUR ledger + contract storage pair.
/// This is suitable as an input to deterministic replay; it is not a consensus block format.
pub fn encode_execution_snapshot(
    ledger: &QurLedger,
    contracts: &ContractState,
) -> Result<serde_json::Value, String> {
    let contract_bytes = contracts.encode_snapshot()?;
    Ok(serde_json::json!({
        "schema": "qorvynt.execution-state.v1",
        "ledger": ledger.to_json(),
        "contract_snapshot_hex": encode_hex(&contract_bytes),
        "execution_state_root": encode_hex(&execution_state_root(ledger, contracts)),
    }))
}

/// Restore a bounded snapshot and verify its component-derived root before returning state.
pub fn decode_execution_snapshot(
    value: &serde_json::Value,
) -> Result<(QurLedger, ContractState), String> {
    if value.get("schema").and_then(|v| v.as_str()) != Some("qorvynt.execution-state.v1") {
        return Err("unsupported QORVYNT execution snapshot schema".into());
    }
    let ledger_value = value.get("ledger").ok_or("execution snapshot ledger is missing")?;
    let ledger = QurLedger::from_json(ledger_value)?;
    let encoded = value.get("contract_snapshot_hex").and_then(|v| v.as_str())
        .ok_or("execution snapshot contract bytes are missing")?;
    if encoded.len() > 2 * (super::contract_state::MAX_CONTRACT_STORAGE_BYTES + 1024 * 1024) {
        return Err("execution snapshot contract bytes exceed size limit".into());
    }
    let contract_bytes = decode_hex(encoded)?;
    let contracts = ContractState::decode_snapshot(&contract_bytes)?;
    let expected = value.get("execution_state_root").and_then(|v| v.as_str())
        .ok_or("execution snapshot root is missing")?;
    if expected != encode_hex(&execution_state_root(&ledger, &contracts)) {
        return Err("QORVYNT execution snapshot root mismatch".into());
    }
    Ok((ledger, contracts))
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_hex(value: &str) -> Result<Vec<u8>, String> {
    if value.len() % 2 != 0 || !value.is_ascii() {
        return Err("invalid execution snapshot hex encoding".into());
    }
    value.as_bytes().chunks_exact(2).map(|pair| {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        };
        let high = digit(pair[0]).ok_or("invalid execution snapshot hex encoding")?;
        let low = digit(pair[1]).ok_or("invalid execution snapshot hex encoding")?;
        Ok((high << 4) | low)
    }).collect()
}
