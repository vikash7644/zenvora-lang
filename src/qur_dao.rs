//! Deterministic local DAO proposal and voting runtime.
//! This module is a single-process governance prototype, not consensus-backed execution.
use sha2::{Digest, Sha256};
use ed25519_dalek::{Signature, VerifyingKey};
use super::contract_gas::{GasMeter, BASE_EXECUTION_GAS, STORAGE_READ_GAS, STORAGE_WRITE_GAS};
use super::contract_state::{AccessMode, ContractState};

const MAX_ID_BYTES: usize = 64;
const MAX_TITLE_BYTES: usize = 256;
const MAX_DAO_SNAPSHOT_VOTERS: usize = 2048;

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
    pub voting_token: Option<String>,
    pub snapshot_total_weight: u64,
    pub snapshot_ledger_root: Option<String>,
    pub voting_snapshot_root: Option<String>,
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
            "status": status_name(self.status),
            "treasury_recipient": self.treasury_recipient,
            "treasury_amount": self.treasury_amount,
            "timelock_blocks": self.timelock_blocks,
            "approved_at_height": self.approved_at_height,
            "executed": self.executed,
            "voting_token": self.voting_token,
            "snapshot_total_weight": self.snapshot_total_weight,
            "snapshot_ledger_root": self.snapshot_ledger_root,
            "voting_snapshot_root": self.voting_snapshot_root
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
    /// Present only when the receipt commits both DAO storage and the QUR token ledger.
    pub execution_state_root: Option<[u8; 32]>,
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(64);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn weighted_snapshot_digest(token: &str, holders: &[(String, u64)]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"QORVYNT_DAO_WEIGHTED_SNAPSHOT_V1\0");
    h.update((token.len() as u32).to_be_bytes());
    h.update(token.as_bytes());
    h.update((holders.len() as u64).to_be_bytes());
    for (account, balance) in holders {
        h.update((account.len() as u32).to_be_bytes());
        h.update(account.as_bytes());
        h.update(balance.to_be_bytes());
    }
    h.finalize().into()
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
        "executed": proposal.executed,
        "voting_token": proposal.voting_token,
        "snapshot_total_weight": proposal.snapshot_total_weight,
        "snapshot_ledger_root": proposal.snapshot_ledger_root,
        "voting_snapshot_root": proposal.voting_snapshot_root
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
        voting_token: value.get("voting_token").and_then(|v| v.as_str()).map(str::to_owned),
        snapshot_total_weight: value.get("snapshot_total_weight").and_then(|v| v.as_u64()).unwrap_or(0),
        snapshot_ledger_root: value.get("snapshot_ledger_root").and_then(|v| v.as_str()).map(str::to_owned),
        voting_snapshot_root: value.get("voting_snapshot_root").and_then(|v| v.as_str()).map(str::to_owned),
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
        state_root: state.state_root(), event_hash: h.finalize().into(), execution_state_root: None }
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
        approved_at_height: None, executed: false, voting_token: None, snapshot_total_weight: 0, snapshot_ledger_root: None, voting_snapshot_root: None };
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let mut event_payload = Vec::new();
    append_field(&mut event_payload, proposer.as_bytes());
    append_field(&mut event_payload, title.as_bytes());
    event_payload.extend_from_slice(&quorum.to_be_bytes());
    gas.charge_event(event_payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("ProposalCreated", dao_id, id, &event_payload, &gas, state))
}

/// Create a proposal with a fixed token-holder snapshot captured at proposal creation.
pub fn create_weighted_proposal(state: &mut ContractState, ledger: &super::qur_runtime::QurLedger,
    dao_id: &str, id: &str, proposer: &str, title: &str, token: &str,
    quorum: u64, gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) || !valid_id(proposer) || !valid_id(token) {
        return Err("invalid DAO, proposal, proposer, or voting token identifier".into());
    }
    if title.trim().is_empty() || title.len() > MAX_TITLE_BYTES {
        return Err("proposal title must contain 1-256 bytes".into());
    }
    if quorum == 0 { return Err("proposal quorum must be greater than zero".into()); }
    // Only key-derived QORVYNT wallet addresses can cast signed votes. Exclude
    // reserved/system ledger accounts such as "genesis" and "dao_*" from quorum weight.
    let holders: Vec<(String, u64)> = ledger.token_holders(token)?
        .into_iter()
        .filter(|(account, _)| {
            account.len() == 42
                && account.starts_with("zv")
                && account[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        .collect();
    if holders.is_empty() { return Err("voting token has no eligible wallet holders".into()); }
    if holders.len() > MAX_DAO_SNAPSHOT_VOTERS {
        return Err(format!("voting snapshot exceeds holder limit ({MAX_DAO_SNAPSHOT_VOTERS})"));
    }
    let total_weight = holders.iter().try_fold(0u64, |sum, (_, balance)| {
        sum.checked_add(*balance).ok_or_else(|| "voting snapshot total overflow".to_string())
    })?;
    if quorum > total_weight {
        return Err(format!("weighted DAO quorum ({quorum}) exceeds eligible snapshot voting power ({total_weight})"));
    }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    let writes = u64::try_from(holders.len()).map_err(|_| "too many token holders")?
        .checked_add(1).ok_or("gas calculation overflow")?;
    gas.charge(STORAGE_WRITE_GAS.checked_mul(writes).ok_or("gas calculation overflow")?)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    if overlay.get(state, &proposal_key(id))?.is_some() {
        return Err(format!("DAO proposal '{id}' already exists"));
    }
    let proposal = DaoProposal {
        id: id.into(), proposer: proposer.into(), title: title.into(), quorum,
        yes_votes: 0, no_votes: 0, status: ProposalStatus::Open,
        treasury_recipient: None, treasury_amount: 0, timelock_blocks: 0,
        approved_at_height: None, executed: false,
        voting_token: Some(token.into()), snapshot_total_weight: total_weight,
        snapshot_ledger_root: Some(hex_digest(&ledger.state_root())),
        voting_snapshot_root: Some(hex_digest(&weighted_snapshot_digest(token, &holders))),
    };
    for (account, balance) in &holders {
        overlay.set(&format!("voting_power/{id}/{account}"), balance.to_be_bytes().to_vec())?;
    }
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let mut event_payload = Vec::new();
    append_field(&mut event_payload, proposer.as_bytes());
    append_field(&mut event_payload, title.as_bytes());
    append_field(&mut event_payload, token.as_bytes());
    event_payload.extend_from_slice(&quorum.to_be_bytes());
    event_payload.extend_from_slice(&total_weight.to_be_bytes());
    event_payload.extend_from_slice(&(holders.len() as u64).to_be_bytes());
    gas.charge_event(event_payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("WeightedProposalCreated", dao_id, id, &event_payload, &gas, state))
}

/// Create a DAO proposal that can execute one treasury transfer after approval and a block-height timelock.
pub fn create_treasury_proposal(state: &mut ContractState, dao_id: &str, id: &str,
    proposer: &str, title: &str, recipient: &str, amount: u64, quorum: u64,
    timelock_blocks: u64, gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) || !valid_id(proposer) || !valid_id(recipient) {
        return Err("invalid DAO, proposal, proposer, or treasury recipient identifier".into());
    }
    if title.trim().is_empty() || title.len() > MAX_TITLE_BYTES { return Err("proposal title must contain 1-256 bytes".into()); }
    if amount == 0 || quorum == 0 || timelock_blocks == 0 {
        return Err("treasury amount, quorum, and timelock must be greater than zero".into());
    }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS)?;
    gas.charge(STORAGE_WRITE_GAS)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    if overlay.get(state, &proposal_key(id))?.is_some() { return Err(format!("DAO proposal '{id}' already exists")); }
    let proposal = DaoProposal {
        id: id.into(), proposer: proposer.into(), title: title.into(), quorum,
        yes_votes: 0, no_votes: 0, status: ProposalStatus::Open,
        treasury_recipient: Some(recipient.into()), treasury_amount: amount,
        timelock_blocks, approved_at_height: None, executed: false,
        voting_token: None, snapshot_total_weight: 0, snapshot_ledger_root: None, voting_snapshot_root: None,
    };
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let mut payload = Vec::new();
    append_field(&mut payload, proposer.as_bytes());
    append_field(&mut payload, title.as_bytes());
    append_field(&mut payload, recipient.as_bytes());
    payload.extend_from_slice(&amount.to_be_bytes());
    payload.extend_from_slice(&quorum.to_be_bytes());
    payload.extend_from_slice(&timelock_blocks.to_be_bytes());
    gas.charge_event(payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("TreasuryProposalCreated", dao_id, id, &payload, &gas, state))
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
    cast_vote_with_weight(state, dao_id, id, voter, choice, 1, gas_limit)
}

/// Record a vote with a positive, caller-validated voting weight.
pub fn cast_vote_with_weight(state: &mut ContractState, dao_id: &str, id: &str, voter: &str,
    choice: VoteChoice, weight: u64, gas_limit: u64) -> Result<DaoReceipt, String> {
    cast_vote_with_weight_inner(state, dao_id, id, voter, choice, weight, gas_limit, false)
}

fn cast_vote_with_weight_inner(state: &mut ContractState, dao_id: &str, id: &str, voter: &str,
    choice: VoteChoice, weight: u64, gas_limit: u64, allow_weighted_snapshot: bool) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) || !valid_id(voter) { return Err("invalid DAO, proposal, or voter identifier".into()); }
    if weight == 0 { return Err("voting weight must be greater than zero".into()); }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS.checked_mul(2).ok_or("gas calculation overflow")?)?;
    gas.charge(STORAGE_WRITE_GAS.checked_mul(2).ok_or("gas calculation overflow")?)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    let mut proposal = load_proposal(state, &overlay, id)?;
    if proposal.voting_token.is_some() && !allow_weighted_snapshot {
        return Err("weighted DAO proposals require the authenticated snapshot-vote path".into());
    }
    if proposal.status != ProposalStatus::Open { return Err("proposal is not open for voting".into()); }
    let key = vote_key(id, voter);
    if overlay.get(state, &key)?.is_some() { return Err("voter has already voted on this proposal".into()); }
    match choice {
        VoteChoice::Yes => proposal.yes_votes = proposal.yes_votes.checked_add(weight).ok_or("yes vote counter overflow")?,
        VoteChoice::No => proposal.no_votes = proposal.no_votes.checked_add(weight).ok_or("no vote counter overflow")?,
    }
    let mut vote_record = Vec::with_capacity(9);
    vote_record.push(match choice { VoteChoice::Yes => 1, VoteChoice::No => 0 });
    vote_record.extend_from_slice(&weight.to_be_bytes());
    overlay.set(&key, vote_record)?;
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let mut event_payload = Vec::new();
    append_field(&mut event_payload, voter.as_bytes());
    event_payload.push(match choice { VoteChoice::Yes => 1, VoteChoice::No => 0 });
    event_payload.extend_from_slice(&weight.to_be_bytes());
    gas.charge_event(event_payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("VoteCast", dao_id, id, &event_payload, &gas, state))
}

/// Verify a wallet signature and weight its vote by the proposal's fixed token-holder snapshot.
pub fn cast_vote_weighted_signed(state: &mut ContractState, dao_id: &str, id: &str,
    token: &str, public_key: &VerifyingKey, signature: &Signature,
    choice: VoteChoice, gas_limit: u64) -> Result<DaoReceipt, String> {
    let message = vote_signing_message(dao_id, id, choice)?;
    public_key.verify_strict(&message, signature)
        .map_err(|_| "invalid DAO vote signature".to_string())?;
    let voter = crate::chain_protocol::address_from_public_key(&public_key.to_bytes())?;
    let proposal = get_proposal(state, dao_id, id)?;
    if proposal.voting_token.as_deref() != Some(token) {
        return Err("proposal voting token does not match the selected token".into());
    }
    let prefix = format!("voting_power/{id}/");
    let snapshot_entries = state.entries_with_prefix(dao_id, &prefix)?;
    if snapshot_entries.is_empty() || snapshot_entries.len() > MAX_DAO_SNAPSHOT_VOTERS {
        return Err("weighted DAO snapshot entry count is invalid".into());
    }
    let mut holders = Vec::with_capacity(snapshot_entries.len());
    let mut total_weight = 0u64;
    for (key, bytes) in snapshot_entries {
        let account = key.strip_prefix(&prefix).ok_or("corrupt DAO voting snapshot key")?;
        if account.len() != 42 || !account.starts_with("zv")
            || !account[2..].bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("corrupt DAO voting snapshot wallet address".into());
        }
        let balance = u64::from_be_bytes(bytes.as_slice().try_into()
            .map_err(|_| "corrupt DAO voting snapshot balance")?);
        if balance == 0 { return Err("corrupt DAO voting snapshot contains zero weight".into()); }
        total_weight = total_weight.checked_add(balance)
            .ok_or("DAO voting snapshot total weight overflow")?;
        holders.push((account.to_owned(), balance));
    }
    if total_weight != proposal.snapshot_total_weight {
        return Err("DAO voting snapshot total does not match its proposal commitment".into());
    }
    let expected_root = proposal.voting_snapshot_root.as_deref()
        .ok_or("weighted DAO proposal is missing its snapshot commitment")?;
    if expected_root != hex_digest(&weighted_snapshot_digest(token, &holders)) {
        return Err("DAO voting snapshot commitment mismatch".into());
    }
    let weight = holders.iter().find(|(account, _)| account == &voter)
        .map(|(_, balance)| *balance)
        .ok_or("wallet had no voting power at proposal snapshot")?;
    let read_count = u64::try_from(holders.len()).map_err(|_| "too many DAO snapshot entries")?
        .checked_add(2).ok_or("gas calculation overflow")?;
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS.checked_mul(read_count).ok_or("gas calculation overflow")?)?;
    cast_vote_with_weight_inner(state, dao_id, id, &voter, choice, weight,
        gas_limit.saturating_sub(gas.used()), true)
}

pub fn finalize_proposal(state: &mut ContractState, dao_id: &str, id: &str,
    gas_limit: u64) -> Result<DaoReceipt, String> {
    let proposal = get_proposal(state, dao_id, id)?;
    if proposal.treasury_recipient.is_some() {
        return Err("treasury proposals require an explicit block height from the caller".into());
    }
    finalize_proposal_at_height(state, dao_id, id, 0, gas_limit)
}

pub fn finalize_proposal_at_height(state: &mut ContractState, dao_id: &str, id: &str,
    current_height: u64, gas_limit: u64) -> Result<DaoReceipt, String> {
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
    if proposal.status == ProposalStatus::Approved && proposal.treasury_recipient.is_some() {
        proposal.approved_at_height = Some(current_height);
    }
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    let mut event_payload = Vec::new();
    append_field(&mut event_payload, status_name(proposal.status).as_bytes());
    event_payload.extend_from_slice(&current_height.to_be_bytes());
    gas.charge_event(event_payload.len())?;
    state.commit(overlay)?;
    Ok(receipt("ProposalFinalized", dao_id, id, &event_payload, &gas, state))
}

/// Fund the local DAO treasury ledger. This is accounting state, not a transfer of real chain assets.
pub fn fund_treasury(state: &mut ContractState, dao_id: &str, amount: u64,
    gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || amount == 0 { return Err("invalid DAO ID or zero treasury funding amount".into()); }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS)?;
    gas.charge(STORAGE_WRITE_GAS)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    let current = match overlay.get(state, "treasury/balance")? {
        Some(bytes) => u64::from_be_bytes(bytes.try_into().map_err(|_| "corrupt DAO treasury balance")?),
        None => 0,
    };
    let next = current.checked_add(amount).ok_or("DAO treasury balance overflow")?;
    overlay.set("treasury/balance", next.to_be_bytes().to_vec())?;
    gas.charge_event(8)?;
    state.commit(overlay)?;
    Ok(receipt("TreasuryFunded", dao_id, "treasury", &amount.to_be_bytes(), &gas, state))
}

pub fn treasury_balance(state: &ContractState, dao_id: &str) -> Result<u64, String> {
    if !valid_id(dao_id) { return Err("invalid DAO ID".into()); }
    let overlay = state.begin(dao_id, AccessMode::View)?;
    match overlay.get(state, "treasury/balance")? {
        Some(bytes) => Ok(u64::from_be_bytes(bytes.try_into().map_err(|_| "corrupt DAO treasury balance")?)),
        None => Ok(0),
    }
}

/// Execute an approved treasury proposal only after its deterministic block-height timelock.
pub fn execute_treasury_proposal(state: &mut ContractState, dao_id: &str, id: &str,
    current_height: u64, gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) { return Err("invalid DAO or proposal identifier".into()); }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS.checked_mul(2).ok_or("gas calculation overflow")?)?;
    gas.charge(STORAGE_WRITE_GAS.checked_mul(3).ok_or("gas calculation overflow")?)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    let mut proposal = load_proposal(state, &overlay, id)?;
    if proposal.status != ProposalStatus::Approved { return Err("treasury proposal is not approved".into()); }
    if proposal.executed { return Err("treasury proposal has already executed".into()); }
    let recipient = proposal.treasury_recipient.clone().ok_or("proposal has no treasury transfer action")?;
    let approved_at = proposal.approved_at_height.ok_or("proposal approval height is missing")?;
    let execute_after = approved_at.checked_add(proposal.timelock_blocks).ok_or("DAO timelock height overflow")?;
    if current_height < execute_after { return Err(format!("DAO timelock active until block {execute_after}")); }
    let current = match overlay.get(state, "treasury/balance")? {
        Some(bytes) => u64::from_be_bytes(bytes.try_into().map_err(|_| "corrupt DAO treasury balance")?),
        None => 0,
    };
    let remaining = current.checked_sub(proposal.treasury_amount).ok_or("insufficient DAO treasury balance")?;
    overlay.set("treasury/balance", remaining.to_be_bytes().to_vec())?;
    let mut transfer = Vec::new();
    append_field(&mut transfer, recipient.as_bytes());
    transfer.extend_from_slice(&proposal.treasury_amount.to_be_bytes());
    transfer.extend_from_slice(&current_height.to_be_bytes());
    overlay.set(&format!("treasury/transfer/{id}"), transfer.clone())?;
    proposal.executed = true;
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    gas.charge_event(transfer.len())?;
    state.commit(overlay)?;
    Ok(receipt("TreasuryTransferExecuted", dao_id, id, &transfer, &gas, state))
}

/// Execute an approved DAO treasury payout as an actual QUR token-ledger transfer.
/// The DAO storage overlay and candidate token ledger are committed together in memory.
pub fn execute_treasury_token_proposal(state: &mut ContractState, ledger: &mut super::qur_runtime::QurLedger,
    dao_id: &str, id: &str, token: &str, current_height: u64, gas_limit: u64) -> Result<DaoReceipt, String> {
    if !valid_id(dao_id) || !valid_id(id) || !valid_id(token) {
        return Err("invalid DAO, proposal, or token identifier".into());
    }
    let mut gas = GasMeter::new(gas_limit);
    gas.charge(BASE_EXECUTION_GAS)?;
    gas.charge(STORAGE_READ_GAS.checked_mul(3).ok_or("gas calculation overflow")?)?;
    gas.charge(STORAGE_WRITE_GAS.checked_mul(4).ok_or("gas calculation overflow")?)?;
    let mut overlay = state.begin(dao_id, AccessMode::Action)?;
    let mut proposal = load_proposal(state, &overlay, id)?;
    if proposal.status != ProposalStatus::Approved { return Err("treasury proposal is not approved".into()); }
    if proposal.executed { return Err("treasury proposal has already executed".into()); }
    let recipient = proposal.treasury_recipient.clone().ok_or("proposal has no treasury transfer action")?;
    let approved_at = proposal.approved_at_height.ok_or("proposal approval height is missing")?;
    let execute_after = approved_at.checked_add(proposal.timelock_blocks).ok_or("DAO timelock height overflow")?;
    if current_height < execute_after { return Err(format!("DAO timelock active until block {execute_after}")); }
    let treasury_account = format!("dao_{dao_id}");
    let mut candidate = ledger.clone();
    candidate.transfer(token, &treasury_account, &recipient, proposal.treasury_amount)?;
    let supply = candidate.total_supply(token).ok_or_else(|| format!("unknown token '{token}'"))?;
    overlay.set(&format!("token/{token}/supply"), supply.to_string().into_bytes())?;
    for account in [treasury_account.as_str(), recipient.as_str()] {
        let key = format!("token/{token}/balance/{account}");
        let balance = candidate.balance(token, account);
        if balance == 0 { overlay.delete(&key)?; }
        else { overlay.set(&key, balance.to_string().into_bytes())?; }
    }
    let mut event = Vec::new();
    append_field(&mut event, token.as_bytes());
    append_field(&mut event, treasury_account.as_bytes());
    append_field(&mut event, recipient.as_bytes());
    event.extend_from_slice(&proposal.treasury_amount.to_be_bytes());
    event.extend_from_slice(&current_height.to_be_bytes());
    proposal.executed = true;
    overlay.set(&proposal_key(id), proposal_bytes(&proposal)?)?;
    overlay.set(&format!("treasury/token-transfer/{id}"), event.clone())?;
    gas.charge_event(event.len())?;
    state.commit(overlay)?;
    *ledger = candidate;
    let mut result = receipt("TreasuryTokenTransferExecuted", dao_id, id, &event, &gas, state);
    result.execution_state_root = Some(super::qur_state::execution_state_root(ledger, state));
    Ok(result)
}

pub fn get_proposal(state: &ContractState, dao_id: &str, id: &str) -> Result<DaoProposal, String> {
    if !valid_id(dao_id) || !valid_id(id) { return Err("invalid DAO or proposal identifier".into()); }
    let overlay = state.begin(dao_id, AccessMode::View)?;
    load_proposal(state, &overlay, id)
}

#[cfg(test)]
mod tests {
    use super::{cast_vote, cast_vote_signed, cast_vote_weighted_signed, cast_vote_with_weight, create_proposal, create_weighted_proposal, create_treasury_proposal, execute_treasury_proposal, execute_treasury_token_proposal, finalize_proposal, finalize_proposal_at_height, fund_treasury, get_proposal, treasury_balance, vote_signing_message, ProposalStatus, VoteChoice};
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
    fn treasury_proposal_initializes_weighted_voting_fields() {
        let mut state = ContractState::new();
        create_treasury_proposal(&mut state, "community", "pay-metadata", "founder",
            "Metadata defaults", "recipient", 5, 1, 2, 30_000).unwrap();
        let proposal = get_proposal(&state, "community", "pay-metadata").unwrap();
        assert_eq!(proposal.treasury_recipient.as_deref(), Some("recipient"));
        assert_eq!(proposal.treasury_amount, 5);
        assert_eq!(proposal.voting_token, None);
        assert_eq!(proposal.snapshot_total_weight, 0);
        assert_eq!(proposal.approved_at_height, None);
        assert!(!proposal.executed);
    }

    #[test]
    fn treasury_transfer_waits_for_timelock_and_executes_once_atomically() {
        let mut state = ContractState::new();
        fund_treasury(&mut state, "community", 100, 30_000).unwrap();
        create_treasury_proposal(&mut state, "community", "pay1", "founder",
            "Pay contributor", "contributor", 40, 1, 10, 30_000).unwrap();
        let key = SigningKey::from_bytes(&[11u8; 32]);
        let msg = vote_signing_message("community", "pay1", VoteChoice::Yes).unwrap();
        let sig = key.sign(&msg);
        cast_vote_signed(&mut state, "community", "pay1", &key.verifying_key(), &sig,
            VoteChoice::Yes, 30_000).unwrap();
        assert!(finalize_proposal(&mut state, "community", "pay1", 30_000).is_err());
        finalize_proposal_at_height(&mut state, "community", "pay1", 100, 30_000).unwrap();
        let root = state.state_root();
        assert!(execute_treasury_proposal(&mut state, "community", "pay1", 109, 30_000).is_err());
        assert_eq!(state.state_root(), root);
        let receipt = execute_treasury_proposal(&mut state, "community", "pay1", 110, 30_000).unwrap();
        assert_eq!(receipt.operation, "TreasuryTransferExecuted");
        assert_eq!(treasury_balance(&state, "community").unwrap(), 60);
        assert!(get_proposal(&state, "community", "pay1").unwrap().executed);
        let committed_root = state.state_root();
        assert!(execute_treasury_proposal(&mut state, "community", "pay1", 120, 30_000).is_err());
        assert_eq!(state.state_root(), committed_root);
    }

    #[test]
    fn treasury_execution_rejects_insufficient_funds_without_partial_writes() {
        let mut state = ContractState::new();
        create_treasury_proposal(&mut state, "community", "pay2", "founder",
            "Pay contributor", "contributor", 40, 1, 1, 30_000).unwrap();
        let key = SigningKey::from_bytes(&[12u8; 32]);
        let msg = vote_signing_message("community", "pay2", VoteChoice::Yes).unwrap();
        let sig = key.sign(&msg);
        cast_vote_signed(&mut state, "community", "pay2", &key.verifying_key(), &sig,
            VoteChoice::Yes, 30_000).unwrap();
        finalize_proposal_at_height(&mut state, "community", "pay2", 5, 30_000).unwrap();
        let root = state.state_root();
        assert!(execute_treasury_proposal(&mut state, "community", "pay2", 6, 30_000).is_err());
        assert_eq!(state.state_root(), root);
        assert_eq!(treasury_balance(&state, "community").unwrap(), 0);
    }

    #[test]
    fn dao_treasury_payout_moves_real_qur_token_balances_after_timelock() {
        let manifest = super::super::qur::parse(r#"
chain MyChain {
 consensus = "proof_of_stake"
 native_token = "GOLD"
}
token GOLD {
 standard = "fungible"
 supply = 1000
}
"#).unwrap();
        let mut ledger = super::super::qur_runtime::QurLedger::from_manifest(&manifest).unwrap();
        ledger.transfer("GOLD", "genesis", "dao_community", 100).unwrap();
        let mut state = ContractState::new();
        create_treasury_proposal(&mut state, "community", "tokenpay1", "founder",
            "Contributor payout", "contributor", 40, 1, 5, 30_000).unwrap();
        let key = SigningKey::from_bytes(&[21u8; 32]);
        let message = vote_signing_message("community", "tokenpay1", VoteChoice::Yes).unwrap();
        let signature = key.sign(&message);
        cast_vote_signed(&mut state, "community", "tokenpay1", &key.verifying_key(), &signature,
            VoteChoice::Yes, 30_000).unwrap();
        finalize_proposal_at_height(&mut state, "community", "tokenpay1", 100, 30_000).unwrap();
        let before_state = state.state_root();
        let before_ledger = ledger.state_root();
        assert!(execute_treasury_token_proposal(&mut state, &mut ledger, "community", "tokenpay1",
            "GOLD", 104, 50_000).is_err());
        assert_eq!(state.state_root(), before_state);
        assert_eq!(ledger.state_root(), before_ledger);
        let receipt = execute_treasury_token_proposal(&mut state, &mut ledger, "community", "tokenpay1",
            "GOLD", 105, 50_000).unwrap();
        assert_eq!(receipt.operation, "TreasuryTokenTransferExecuted");
        assert_eq!(receipt.execution_state_root, Some(super::super::qur_state::execution_state_root(&ledger, &state)));
        assert_eq!(ledger.balance("GOLD", "dao_community"), 60);
        assert_eq!(ledger.balance("GOLD", "contributor"), 40);
        assert_eq!(ledger.total_supply("GOLD"), Some(1000));
        let committed_state = state.state_root();
        let committed_ledger = ledger.state_root();
        assert!(execute_treasury_token_proposal(&mut state, &mut ledger, "community", "tokenpay1",
            "GOLD", 106, 50_000).is_err());
        assert_eq!(state.state_root(), committed_state);
        assert_eq!(ledger.state_root(), committed_ledger);
    }

    #[test]
    fn signed_dao_votes_use_proposal_creation_token_snapshot() {
        let manifest = super::super::qur::parse(r#"
chain MyChain {
 consensus = "proof_of_stake"
 native_token = "GOLD"
}
token GOLD {
 standard = "fungible"
 supply = 1000
}
"#).unwrap();
        let mut ledger = super::super::qur_runtime::QurLedger::from_manifest(&manifest).unwrap();
        let yes_key = SigningKey::from_bytes(&[31u8; 32]);
        let no_key = SigningKey::from_bytes(&[32u8; 32]);
        let yes_address = super::super::chain_protocol::address_from_public_key(&yes_key.verifying_key().to_bytes()).unwrap();
        let no_address = super::super::chain_protocol::address_from_public_key(&no_key.verifying_key().to_bytes()).unwrap();
        ledger.transfer("GOLD", "genesis", &yes_address, 70).unwrap();
        ledger.transfer("GOLD", "genesis", &no_address, 30).unwrap();
        let mut state = ContractState::new();
        create_weighted_proposal(&mut state, &ledger, "community", "weighted1", "founder",
            "Weighted vote", "GOLD", 100, 50_000).unwrap();
        let snapshot = get_proposal(&state, "community", "weighted1").unwrap();
        assert_eq!(snapshot.snapshot_ledger_root, Some(super::hex_digest(&ledger.state_root())));
        assert_eq!(snapshot.voting_snapshot_root.as_ref().map(String::len), Some(64));
        let snapshot_json = super::proposal_bytes(&snapshot).unwrap();
        let restored = super::parse_proposal(&snapshot_json).unwrap();
        assert_eq!(restored.snapshot_ledger_root, snapshot.snapshot_ledger_root);
        assert_eq!(restored.voting_snapshot_root, snapshot.voting_snapshot_root);
        // Public caller-supplied weights must not bypass wallet-authenticated snapshot voting.
        let before_untrusted_weight = state.state_root();
        assert!(cast_vote_with_weight(&mut state, "community", "weighted1", &yes_address,
            VoteChoice::Yes, 1000, 30_000).is_err());
        assert_eq!(state.state_root(), before_untrusted_weight);
        // A modified persisted weight must fail the snapshot commitment check without voting.
        let mut tampered_state = state.clone();
        let mut tamper = tampered_state.begin("community", super::super::contract_state::AccessMode::Action).unwrap();
        tamper.set(&format!("voting_power/weighted1/{yes_address}"), 71u64.to_be_bytes().to_vec()).unwrap();
        tampered_state.commit(tamper).unwrap();
        let tampered_root = tampered_state.state_root();
        let tampered_message = vote_signing_message("community", "weighted1", VoteChoice::Yes).unwrap();
        let tampered_signature = yes_key.sign(&tampered_message);
        assert!(cast_vote_weighted_signed(&mut tampered_state, "community", "weighted1", "GOLD",
            &yes_key.verifying_key(), &tampered_signature, VoteChoice::Yes, 50_000).is_err());
        assert_eq!(tampered_state.state_root(), tampered_root);
        // Moving tokens after proposal creation cannot change the stored voting power.
        ledger.transfer("GOLD", &yes_address, &no_address, 70).unwrap();
        let yes_message = vote_signing_message("community", "weighted1", VoteChoice::Yes).unwrap();
        let yes_signature = yes_key.sign(&yes_message);
        cast_vote_weighted_signed(&mut state, "community", "weighted1", "GOLD",
            &yes_key.verifying_key(), &yes_signature, VoteChoice::Yes, 100_000).unwrap();
        let no_message = vote_signing_message("community", "weighted1", VoteChoice::No).unwrap();
        let no_signature = no_key.sign(&no_message);
        cast_vote_weighted_signed(&mut state, "community", "weighted1", "GOLD",
            &no_key.verifying_key(), &no_signature, VoteChoice::No, 100_000).unwrap();
        let proposal = get_proposal(&state, "community", "weighted1").unwrap();
        assert_eq!(proposal.yes_votes, 70);
        assert_eq!(proposal.no_votes, 30);
        assert_eq!(proposal.total_votes(), 100);
        assert_eq!(proposal.snapshot_total_weight, 100);
        assert_eq!(proposal.voting_token.as_deref(), Some("GOLD"));
        assert!(cast_vote_weighted_signed(&mut state, "community", "weighted1", "GOLD",
            &yes_key.verifying_key(), &yes_signature, VoteChoice::Yes, 30_000).is_err());
        let empty_key = SigningKey::from_bytes(&[33u8; 32]);
        let empty_message = vote_signing_message("community", "weighted1", VoteChoice::Yes).unwrap();
        let empty_signature = empty_key.sign(&empty_message);
        assert!(cast_vote_weighted_signed(&mut state, "community", "weighted1", "GOLD",
            &empty_key.verifying_key(), &empty_signature, VoteChoice::Yes, 30_000).is_err());
        let unchanged = get_proposal(&state, "community", "weighted1").unwrap();
        assert_eq!(unchanged.total_votes(), 100);
    }

    #[test]
    fn weighted_proposal_rejects_quorum_above_eligible_snapshot_power() {
        let manifest = super::super::qur::parse(r#"
chain MyChain {
 consensus = "proof_of_stake"
 native_token = "GOLD"
}
token GOLD {
 standard = "fungible"
 supply = 1000
}
"#).unwrap();
        let mut ledger = super::super::qur_runtime::QurLedger::from_manifest(&manifest).unwrap();
        let key = SigningKey::from_bytes(&[41u8; 32]);
        let address = super::super::chain_protocol::address_from_public_key(&key.verifying_key().to_bytes()).unwrap();
        ledger.transfer("GOLD", "genesis", &address, 100).unwrap();
        let mut state = ContractState::new();
        let root = state.state_root();
        let error = create_weighted_proposal(&mut state, &ledger, "community", "weighted-too-high",
            "founder", "Impossible quorum", "GOLD", 101, 50_000).unwrap_err();
        assert!(error.contains("exceeds eligible snapshot voting power"));
        assert_eq!(state.state_root(), root);
        assert!(create_weighted_proposal(&mut state, &ledger, "community", "weighted-exact",
            "founder", "Reachable quorum", "GOLD", 100, 50_000).is_ok());
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
