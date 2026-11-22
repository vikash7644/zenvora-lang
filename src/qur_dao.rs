//! Deterministic local DAO proposal and voting runtime.
//! This module is a single-process governance prototype, not consensus-backed execution.
use sha2::{Digest, Sha256};
use ed25519_dalek::{Signature, VerifyingKey};
use super::contract_gas::{GasMeter, BASE_EXECUTION_GAS, STORAGE_READ_GAS, STORAGE_WRITE_GAS};
use super::contract_state::{AccessMode, ContractState};

const MAX_ID_BYTES: usize = 64;
const MAX_TITLE_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoteChoice { Yes, No }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProposalStatus { Open, Approved, Rejected, QuorumNotMet }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaoProposal {
    pub id: String,
    pub proposer: String,
    pub title: String,
    pub quorum: u64,
    pub yes_votes: u64,
    pub no_votes: u64,
    pub status: ProposalStatus,
    pub treasury_recipient: Option<String>,
    pub treasury_amount: u64,
    pub timelock_blocks: u64,
    pub approved_at_height: Option<u64>,
    pub executed: bool,
}

impl DaoProposal {
    pub fn total_votes(&self) -> u128 { self.yes_votes as u128 + self.no_votes as u128 }
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "id": self.id,
            "proposer": self.proposer,
            "title": self.title,
            "quorum": self.quorum,
            "yes_votes": self.yes_votes,
            "no_votes": self.no_votes,
            "status": status_name(self.status)
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaoReceipt {
    pub operation: &'static str,
    pub dao_id: String,
    pub proposal_id: String,
    pub gas_used: u64,
    pub state_root: [u8; 32],
    pub event_hash: [u8; 32],
}

fn valid_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_ID_BYTES
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn proposal_key(id: &str) -> String { format!("proposal/{id}") }

fn status_name(status: ProposalStatus) -> &'static str {
    match status {
        ProposalStatus::Open => "open",
        ProposalStatus::Approved => "approved",
        ProposalStatus::Rejected => "rejected",
        ProposalStatus::QuorumNotMet => "quorum_not_met",
    }
}

fn proposal_bytes(proposal: &DaoProposal) -> Result<Vec<u8>, String> {
    serde_json::to_vec(&serde_json::json!({
        "id": proposal.id,
        "proposer": proposal.proposer,
        "title": proposal.title,
        "quorum": proposal.quorum,
        "yes_votes": proposal.yes_votes,
        "no_votes": proposal.no_votes,
        "status": status_name(proposal.status),
        "treasury_recipient": proposal.treasury_recipient,
        "treasury_amount": proposal.treasury_amount,
        "timelock_blocks": proposal.timelock_blocks,
        "approved_at_height": proposal.approved_at_height,
        "executed": proposal.executed
    })).map_err(|e| e.to_string())
}

fn parse_proposal(bytes: &[u8]) -> Result<DaoProposal, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| "stored DAO proposal is corrupt".to_string())?;
    let status = match value.get("status").and_then(|v| v.as_str()) {
        Some("open") => ProposalStatus::Open,
        Some("approved") => ProposalStatus::Approved,
        Some("rejected") => ProposalStatus::Rejected,
        Some("quorum_not_met") => ProposalStatus::QuorumNotMet,
        _ => return Err("stored DAO proposal has an invalid status".into()),
    };
    Ok(DaoProposal {
        id: value.get("id").and_then(|v| v.as_str()).ok_or("stored DAO proposal ID is missing")?.to_owned(),
        proposer: value.get("proposer").and_then(|v| v.as_str()).ok_or("stored DAO proposer is missing")?.to_owned(),
        title: value.get("title").and_then(|v| v.as_str()).ok_or("stored DAO title is missing")?.to_owned(),
        quorum: value.get("quorum").and_then(|v| v.as_u64()).ok_or("stored DAO quorum is invalid")?,
        yes_votes: value.get("yes_votes").and_then(|v| v.as_u64()).ok_or("stored DAO yes vote count is invalid")?,
        no_votes: value.get("no_votes").and_then(|v| v.as_u64()).ok_or("stored DAO no vote count is invalid")?,
        status,
        treasury_recipient: value.get("treasury_recipient").and_then(|v| v.as_str()).map(str::to_owned),
        treasury_amount: value.get("treasury_amount").and_then(|v| v.as_u64()).unwrap_or(0),
        timelock_blocks: value.get("timelock_blocks").and_then(|v| v.as_u64()).unwrap_or(0),
        approved_at_height: value.get("approved_at_height").and_then(|v| v.as_u64()),
        executed: value.get("executed").and_then(|v| v.as_bool()).unwrap_or(false),
    })
}
fn vote_key(id: &str, voter: &str) -> String { format!("vote/{id}/{voter}") }

fn load_proposal(state: &ContractState, overlay: &super::contract_state::StateOverlay, id: &str) -> Result<DaoProposal, String> {
    let bytes = overlay.get(state, &proposal_key(id))?.ok_or_else(|| format!("unknown DAO proposal '{id}'"))?;
    parse_proposal(bytes)
}

fn append_field(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    out.extend_from_slice(bytes);
}

fn receipt(operation: &'static str, dao_id: &str, proposal_id: &str,
    payload: &[u8], gas: &GasMeter, state: &ContractState) -> DaoReceipt {
    let mut h = Sha256::new();
    h.update(b"QORVYNT_DAO_EVENT_V1\0");
    h.update((operation.len() as u32).to_be_bytes());
    h.update(operation.as_bytes());
    h.update((dao_id.len() as u32).to_be_bytes());
    h.update(dao_id.as_bytes());
    h.update((proposal_id.len() as u32).to_be_bytes());
    h.update(proposal_id.as_bytes());
    h.update((payload.len() as u32).to_be_bytes());
    h.update(payload);
    DaoReceipt { operation, dao_id: dao_id.into(), proposal_id: proposal_id.into(), gas_used: gas.used(),
        state_root: state.state_root(), event_hash: h.finalize().into() }
}

pub fn create_proposal(state: &mut ContractState, dao_id: &str, id: &str, proposer: &str,
    title: &str, quorum: u64, gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) || !valid_id(proposer) { return Err("invalid DAO, proposal, or proposer identifier".into()); }
    if title.trim().is_empty() || title.len() > MAX_TITLE_BYTES { return Err("proposal title must contain 1-256 bytes".into()); }
    if quorum == 0 { return Err("proposal quorum must be greater than zero".into()); }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS)?;
    gas.charge(STORAGE_WRITE_GAS)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    if overlay.get(state, &proposal_key(id))?.is_some() { return Err(format!("DAO proposal '{id}' already exists")); }
    let proposal = DaoProposal { id: id.into(), proposer: proposer.into(), title: title.into(),
        quorum, yes_votes: 0, no_votes: 0, status: ProposalStatus::Open,
        treasury_recipient: None, treasury_amount: 0, timelock_blocks: 0,
        approved_at_height: None, executed: false };
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let mut event_payload = Vec::new();
    append_field(&mut event_payload, proposer.as_bytes());
    append_field(&mut event_payload, title.as_bytes());
    event_payload.extend_from_slice(&quorum.to_be_bytes());
    gas.charge_event(event_payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("ProposalCreated", dao_id, id, &event_payload, &gas, state))
}

/// Canonical message signed by the wallet for a DAO vote.
pub fn vote_signing_message(dao_id: &str, proposal_id: &str, choice: VoteChoice) -> Result<Vec<u8>, String> {
    if !valid_id(dao_id) || !valid_id(proposal_id) { return Err("invalid DAO or proposal identifier".into()); }
    let mut message = b"QORVYNT_DAO_VOTE_V1\0".to_vec();
    append_field(&mut message, dao_id.as_bytes());
    append_field(&mut message, proposal_id.as_bytes());
    message.push(match choice { VoteChoice::Yes => 1, VoteChoice::No => 0 });
    Ok(message)
}

/// Apply a vote only after verifying the Ed25519 signature against the wallet public key.
pub fn cast_vote_signed(state: &mut ContractState, dao_id: &str, id: &str,
    public_key: &VerifyingKey, signature: &Signature, choice: VoteChoice,
    gas_limit: u64) -> Result<DaoReceipt, String> {
    let message = vote_signing_message(dao_id, id, choice)?;
    public_key.verify_strict(&message, signature)
        .map_err(|_| "invalid DAO vote signature".to_string())?;
    let voter = crate::chain_protocol::address_from_public_key(&public_key.to_bytes())?;
    cast_vote(state, dao_id, id, &voter, choice, gas_limit)
}

pub fn cast_vote(state: &mut ContractState, dao_id: &str, id: &str, voter: &str,
    choice: VoteChoice, gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) || !valid_id(voter) { return Err("invalid DAO, proposal, or voter identifier".into()); }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS.checked_mul(2).ok_or("gas calculation overflow")?)?;
    gas.charge(STORAGE_WRITE_GAS.checked_mul(2).ok_or("gas calculation overflow")?)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    let mut proposal = load_proposal(state, &overlay, id)?;
    if proposal.status != ProposalStatus::Open { return Err("proposal is not open for voting".into()); }
    let key = vote_key(id, voter);
    if overlay.get(state, &key)?.is_some() { return Err("voter has already voted on this proposal".into()); }
    match choice {
        VoteChoice::Yes => proposal.yes_votes = proposal.yes_votes.checked_add(1).ok_or("yes vote counter overflow")?,
        VoteChoice::No => proposal.no_votes = proposal.no_votes.checked_add(1).ok_or("no vote counter overflow")?,
    }
    overlay.set(&key, match choice { VoteChoice::Yes => b"yes".to_vec(), VoteChoice::No => b"no".to_vec() })?;
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let mut event_payload = Vec::new();
    append_field(&mut event_payload, voter.as_bytes());
    event_payload.push(match choice { VoteChoice::Yes => 1, VoteChoice::No => 0 });
    gas.charge_event(event_payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("VoteCast", dao_id, id, &event_payload, &gas, state))
}

pub fn finalize_proposal(state: &mut ContractState, dao_id: &str, id: &str,
    gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) { return Err("invalid DAO or proposal identifier".into()); }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS)?;
    gas.charge(STORAGE_WRITE_GAS)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    let mut proposal = load_proposal(state, &overlay, id)?;
    if proposal.status != ProposalStatus::Open { return Err("proposal has already been finalized".into()); }
    proposal.status = if proposal.total_votes() < proposal.quorum as u128 { ProposalStatus::QuorumNotMet }
        else if proposal.yes_votes > proposal.no_votes { ProposalStatus::Approved }
        else { ProposalStatus::Rejected };
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let event_payload = status_name(proposal.status).as_bytes();
    gas.charge_event(event_payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("ProposalFinalized", dao_id, id, event_payload, &gas, state))
}

pub fn get_proposal(state: &ContractState, dao_id: &str, id: &str) -> Result<DaoProposal, String> {
    if !valid_id(dao_id) || !valid_id(id) { return Err("invalid DAO or proposal identifier".into()); }
    let overlay = state.begin(dao_id, AccessMode::View)?;
    load_proposal(state, &overlay, id)
}

#[cfg(test)]
mod tests {
    use super::{cast_vote, cast_vote_signed, create_proposal, finalize_proposal, get_proposal, vote_signing_message, ProposalStatus, VoteChoice};
    use super::super::contract_state::ContractState;
    use ed25519_dalek::{Signer, SigningKey, Signature};

    #[test]
    fn wallet_signed_vote_authenticates_identity_and_prevents_replay() {
        let mut state = ContractState::new();
        create_proposal(&mut state, "community", "signed1", "founder", "Wallet vote", 1, 30_000).unwrap();
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let message = vote_signing_message("community", "signed1", VoteChoice::Yes).unwrap();
        let signature = key.sign(&message);
        let receipt = cast_vote_signed(&mut state, "community", "signed1",
            &key.verifying_key(), &signature, VoteChoice::Yes, 30_000).unwrap();
        assert_eq!(receipt.operation, "VoteCast");
        let proposal = get_proposal(&state, "community", "signed1").unwrap();
        assert_eq!(proposal.yes_votes, 1);
        assert!(cast_vote_signed(&mut state, "community", "signed1",
            &key.verifying_key(), &signature, VoteChoice::Yes, 30_000).is_err());
    }

    #[test]
    fn invalid_or_wrong_choice_signature_cannot_cast_vote() {
        let mut state = ContractState::new();
        create_proposal(&mut state, "community", "signed2", "founder", "Signature check", 1, 30_000).unwrap();
        let key = SigningKey::from_bytes(&[9u8; 32]);
        let message = vote_signing_message("community", "signed2", VoteChoice::Yes).unwrap();
        let signature = key.sign(&message);
        let root = state.state_root();
        assert!(cast_vote_signed(&mut state, "community", "signed2",
            &key.verifying_key(), &signature, VoteChoice::No, 30_000).is_err());
        let other = SigningKey::from_bytes(&[10u8; 32]);
        let forged = Signature::from_bytes(&signature.to_bytes());
        assert!(cast_vote_signed(&mut state, "community", "signed2",
            &other.verifying_key(), &forged, VoteChoice::Yes, 30_000).is_err());
        assert_eq!(state.state_root(), root);
    }

    #[test]
    fn proposal_votes_and_quorum_finalize_deterministically() {
        let mut state = ContractState::new();
        create_proposal(&mut state, "community", "p1", "alice", "Fund public goods", 2, 30_000).unwrap();
        cast_vote(&mut state, "community", "p1", "alice", VoteChoice::Yes, 30_000).unwrap();
        cast_vote(&mut state, "community", "p1", "bob", VoteChoice::Yes, 30_000).unwrap();
        let receipt = finalize_proposal(&mut state, "community", "p1", 30_000).unwrap();
        let proposal = get_proposal(&state, "community", "p1").unwrap();
        assert_eq!(proposal.status, ProposalStatus::Approved);
        assert_eq!(proposal.total_votes(), 2);
        assert_eq!(receipt.operation, "ProposalFinalized");
    }

    #[test]
    fn duplicate_votes_and_failed_calls_do_not_mutate_state() {
        let mut state = ContractState::new();
        create_proposal(&mut state, "community", "p2", "alice", "Upgrade", 1, 30_000).unwrap();
        cast_vote(&mut state, "community", "p2", "bob", VoteChoice::No, 30_000).unwrap();
        let root = state.state_root();
        assert!(cast_vote(&mut state, "community", "p2", "bob", VoteChoice::Yes, 30_000).is_err());
        assert_eq!(state.state_root(), root);
        assert!(finalize_proposal(&mut state, "community", "p2", 1).is_err());
        assert_eq!(state.state_root(), root);
    }

    #[test]
    fn quorum_failure_and_tie_are_rejected() {
        let mut state = ContractState::new();
        create_proposal(&mut state, "community", "p3", "alice", "Treasury policy", 3, 30_000).unwrap();
        cast_vote(&mut state, "community", "p3", "bob", VoteChoice::Yes, 30_000).unwrap();
        finalize_proposal(&mut state, "community", "p3", 30_000).unwrap();
        assert_eq!(get_proposal(&state, "community", "p3").unwrap().status, ProposalStatus::QuorumNotMet);

        create_proposal(&mut state, "community", "p4", "alice", "Protocol change", 2, 30_000).unwrap();
        cast_vote(&mut state, "community", "p4", "bob", VoteChoice::Yes, 30_000).unwrap();
        cast_vote(&mut state, "community", "p4", "carol", VoteChoice::No, 30_000).unwrap();
        finalize_proposal(&mut state, "community", "p4", 30_000).unwrap();
        assert_eq!(get_proposal(&state, "community", "p4").unwrap().status, ProposalStatus::Rejected);
    }

    #[test]
    fn out_of_gas_proposal_creation_is_atomic() {
        let mut state = ContractState::new();
        let root = state.state_root();
        assert!(create_proposal(&mut state, "community", "p5", "alice", "No gas", 1, 21_000).is_err());
        assert_eq!(state.state_root(), root);
    }
}
