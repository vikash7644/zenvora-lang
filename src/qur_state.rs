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
