//! Deterministic, advisory-only governance intelligence for QORVYNT DAIO.
//! This module never mutates governance state or authorizes execution.
use super::qur_dao::{DaoProposal, ProposalStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindingSeverity {
    Info,
    Review,
    High,
}

impl FindingSeverity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Review => "review",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceFinding {
    pub code: &'static str,
    pub severity: FindingSeverity,
    pub message: &'static str,
    pub score: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GovernanceAssessment {
    pub proposal_id: String,
    pub risk_score: u8,
    pub risk_level: &'static str,
    pub findings: Vec<GovernanceFinding>,
    pub advisory_only: bool,
}

impl GovernanceAssessment {
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "proposal_id": self.proposal_id,
            "risk_score": self.risk_score,
            "risk_level": self.risk_level,
            "advisory_only": self.advisory_only,
            "findings": self.findings.iter().map(|finding| serde_json::json!({
                "code": finding.code,
                "severity": finding.severity.as_str(),
                "message": finding.message,
                "score": finding.score
            })).collect::<Vec<_>>()
        })
    }
}

/// Assess persisted proposal invariants without changing state.
/// Scores are deterministic triage hints, not a vote, audit, or execution decision.
pub fn assess_proposal(proposal: &DaoProposal) -> GovernanceAssessment {
    let mut findings = Vec::new();
    let mut score: u16 = 0;
    let mut add = |code, severity, message, points| {
        findings.push(GovernanceFinding { code, severity, message, score: points });
        score = score.saturating_add(points as u16);
    };

    if proposal.voting_token.is_none() {
        add(
            "UNWEIGHTED_VOTING_MODEL",
            FindingSeverity::Info,
            "This proposal uses equal unit voting rather than a token-holder snapshot.",
            5,
        );
    } else {
        if proposal.snapshot_total_weight == 0 {
            add(
                "EMPTY_WEIGHTED_SNAPSHOT",
                FindingSeverity::High,
                "Weighted proposal has no committed voting power.",
                35,
            );
        }
        if proposal.quorum > proposal.snapshot_total_weight {
            add(
                "QUORUM_EXCEEDS_SNAPSHOT",
                FindingSeverity::High,
                "Required quorum exceeds the recorded snapshot voting power.",
                30,
            );
        }
        if proposal.snapshot_ledger_root.as_deref().map_or(true, |root| !is_digest(root)) {
            add(
                "INVALID_LEDGER_COMMITMENT",
                FindingSeverity::High,
                "Weighted proposal is missing a valid ledger-root commitment.",
                25,
            );
        }
        if proposal.voting_snapshot_root.as_deref().map_or(true, |root| !is_digest(root)) {
            add(
                "INVALID_VOTING_SNAPSHOT_COMMITMENT",
                FindingSeverity::High,
                "Weighted proposal is missing a valid voting-snapshot commitment.",
                25,
            );
        }

        let anchor_parts = [
            proposal.snapshot_chain_id.is_some(),
            proposal.snapshot_block_height.is_some(),
            proposal.snapshot_block_hash.is_some(),
            proposal.snapshot_chain_state_root.is_some(),
        ];
        let anchor_count = anchor_parts.iter().filter(|present| **present).count();
        if anchor_count == 0 {
            add(
                "UNANCHORED_WEIGHTED_SNAPSHOT",
                FindingSeverity::Review,
                "Voting snapshot has no verified block-store context anchor.",
                15,
            );
        } else if anchor_count != anchor_parts.len()
            || proposal.snapshot_chain_id == Some(0)
            || proposal.snapshot_block_hash.as_deref().map_or(true, |hash| !is_digest(hash))
            || proposal.snapshot_chain_state_root.as_deref().map_or(true, |root| !is_digest(root))
        {
            add(
                "INCOMPLETE_SNAPSHOT_ANCHOR",
                FindingSeverity::High,
                "Weighted snapshot has incomplete or malformed chain-anchor metadata.",
                30,
            );
        }
        if proposal.total_votes() > proposal.snapshot_total_weight as u128 {
            add(
                "VOTES_EXCEED_SNAPSHOT_POWER",
                FindingSeverity::High,
                "Recorded vote weight exceeds the total voting power committed at proposal creation.",
                35,
            );
        }
    }

    if proposal.treasury_recipient.is_some() {
        // Legacy unit-vote treasury proposals also need an explicit risk signal when
        // they lack a replay anchor; weighted proposals are checked above.
        if proposal.voting_token.is_none() {
            let anchor_parts = [
                proposal.snapshot_chain_id.is_some(),
                proposal.snapshot_block_height.is_some(),
                proposal.snapshot_block_hash.is_some(),
                proposal.snapshot_chain_state_root.is_some(),
            ];
            let anchor_count = anchor_parts.iter().filter(|present| **present).count();
            if anchor_count == 0 {
                add(
                    "UNANCHORED_TREASURY_ACTION",
                    FindingSeverity::High,
                    "Treasury proposal has no replay-verified chain snapshot anchor.",
                    35,
                );
            } else if anchor_count != anchor_parts.len()
                || proposal.snapshot_chain_id == Some(0)
                || proposal.snapshot_block_hash.as_deref().map_or(true, |hash| !is_digest(hash))
                || proposal.snapshot_chain_state_root.as_deref().map_or(true, |root| !is_digest(root))
            {
                add(
                    "INCOMPLETE_TREASURY_ANCHOR",
                    FindingSeverity::High,
                    "Treasury proposal has incomplete or malformed chain-anchor metadata.",
                    35,
                );
            }
        }
        if proposal.treasury_amount == 0 {
            add(
                "ZERO_VALUE_TREASURY_ACTION",
                FindingSeverity::High,
                "Treasury proposal contains a zero-value transfer action.",
                20,
            );
        }
        if proposal.timelock_blocks == 0 {
            add(
                "MISSING_TREASURY_TIMELOCK",
                FindingSeverity::High,
                "Treasury proposal has no execution timelock.",
                25,
            );
        }
        if proposal.status == ProposalStatus::Approved && proposal.approved_at_height.is_none() {
            add(
                "MISSING_APPROVAL_HEIGHT",
                FindingSeverity::High,
                "Approved treasury proposal has no recorded approval height.",
                25,
            );
        }
    }

    match proposal.status {
        ProposalStatus::Approved
            if proposal.total_votes() < proposal.quorum as u128
                || proposal.yes_votes <= proposal.no_votes =>
        {
            add(
                "APPROVAL_TALLY_INCONSISTENT",
                FindingSeverity::High,
                "Approved status conflicts with the recorded quorum or vote tally.",
                35,
            );
        }
        ProposalStatus::Rejected if proposal.total_votes() >= proposal.quorum as u128
            && proposal.yes_votes > proposal.no_votes =>
        {
            add(
                "REJECTION_TALLY_INCONSISTENT",
                FindingSeverity::High,
                "Rejected status conflicts with the recorded quorum or vote tally.",
                35,
            );
        }
        ProposalStatus::QuorumNotMet if proposal.total_votes() >= proposal.quorum as u128 => {
            add(
                "QUORUM_STATUS_INCONSISTENT",
                FindingSeverity::High,
                "Quorum-not-met status conflicts with the recorded vote total.",
                35,
            );
        }
        _ => {}
    }

    if proposal.executed
        && (proposal.status != ProposalStatus::Approved || proposal.treasury_recipient.is_none())
    {
        add(
            "EXECUTION_STATE_INCONSISTENT",
            FindingSeverity::High,
            "Proposal is marked executed without an approved treasury action.",
            40,
        );
    }

    let risk_score = score.min(100) as u8;
    let risk_level = if risk_score >= 50 {
        "high"
    } else if risk_score >= 20 {
        "review"
    } else {
        "info"
    };

    GovernanceAssessment {
        proposal_id: proposal.id.clone(),
        risk_score,
        risk_level,
        findings,
        advisory_only: true,
    }
}

fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}


/// Maximum size of untrusted, off-chain advisor output accepted by the boundary.
pub const MAX_ADVISOR_SUMMARY_BYTES: usize = 1_024;
pub const MAX_ADVISOR_FINDINGS: usize = 16;
pub const MAX_ADVISOR_EVIDENCE_REFS: usize = 16;
pub const MAX_ADVISOR_FIELD_BYTES: usize = 256;

/// Read-only facts supplied to an optional advisor. Proposal text is untrusted input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorContext {
    pub proposal_id: String,
    pub title: String,
    pub status: &'static str,
    pub yes_votes: u64,
    pub no_votes: u64,
    pub quorum: u64,
    pub treasury_recipient: Option<String>,
    pub treasury_amount: u64,
    pub timelock_blocks: u64,
}

impl AdvisorContext {
    pub fn from_proposal(proposal: &DaoProposal) -> Self {
        let status = match proposal.status {
            ProposalStatus::Open => "open",
            ProposalStatus::Approved => "approved",
            ProposalStatus::Rejected => "rejected",
            ProposalStatus::QuorumNotMet => "quorum_not_met",
        };
        Self {
            proposal_id: proposal.id.clone(),
            title: proposal.title.clone(),
            status,
            yes_votes: proposal.yes_votes,
            no_votes: proposal.no_votes,
            quorum: proposal.quorum,
            treasury_recipient: proposal.treasury_recipient.clone(),
            treasury_amount: proposal.treasury_amount,
            timelock_blocks: proposal.timelock_blocks,
        }
    }
}

/// A model/provider observation. It is deliberately not a governance action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorObservation {
    pub code: String,
    pub severity: FindingSeverity,
    pub explanation: String,
    pub evidence_refs: Vec<String>,
}

/// Optional off-chain analysis, kept separate from consensus and proposal state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvisorAnnotation {
    pub provider: String,
    pub model: String,
    pub summary: String,
    pub observations: Vec<AdvisorObservation>,
}

impl AdvisorAnnotation {
    /// Reject oversized or malformed provider output before displaying or storing it.
    pub fn validate(&self) -> Result<(), String> {
        if !bounded_text(&self.provider, MAX_ADVISOR_FIELD_BYTES)
            || !bounded_text(&self.model, MAX_ADVISOR_FIELD_BYTES)
        {
            return Err("advisor provider and model identifiers must be non-empty, bounded, and contain no control characters".into());
        }
        if !bounded_text(&self.summary, MAX_ADVISOR_SUMMARY_BYTES) {
            return Err("advisor summary must be non-empty, bounded, and contain no control characters".into());
        }
        if self.observations.len() > MAX_ADVISOR_FINDINGS {
            return Err("advisor returned too many observations".into());
        }
        for observation in &self.observations {
            if !bounded_code(&observation.code)
                || !bounded_text(&observation.explanation, MAX_ADVISOR_FIELD_BYTES)
            {
                return Err("advisor observation contains an invalid code or explanation".into());
            }
            if observation.evidence_refs.len() > MAX_ADVISOR_EVIDENCE_REFS
                || observation.evidence_refs.iter().any(|reference|
                    !bounded_text(reference, MAX_ADVISOR_FIELD_BYTES))
            {
                return Err("advisor observation contains invalid or excessive evidence references".into());
            }
        }
        Ok(())
    }

    pub fn to_json(&self) -> Result<serde_json::Value, String> {
        self.validate()?;
        Ok(serde_json::json!({
            "provider": self.provider,
            "model": self.model,
            "summary": self.summary,
            "advisory_only": true,
            "observations": self.observations.iter().map(|observation| serde_json::json!({
                "code": observation.code,
                "severity": observation.severity.as_str(),
                "explanation": observation.explanation,
                "evidence_refs": observation.evidence_refs
            })).collect::<Vec<_>>()
        }))
    }
}

/// Implementations receive immutable proposal context and can return annotations only.
/// The trait exposes no state handle, wallet, signer, vote, or execution method.
pub trait GovernanceAdvisor: Send + Sync {
    fn annotate(&self, context: &AdvisorContext) -> Result<AdvisorAnnotation, String>;
}

fn bounded_text(value: &str, max_bytes: usize) -> bool {
    !value.trim().is_empty()
        && value.len() <= max_bytes
        && !value.chars().any(char::is_control)
}

fn bounded_code(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|byte|
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
}

#[cfg(test)]
mod tests {
    use super::{assess_proposal, AdvisorAnnotation, AdvisorContext, AdvisorObservation, FindingSeverity};
    use crate::qur_dao::{DaoProposal, ProposalStatus};

    fn proposal() -> DaoProposal {
        DaoProposal {
            id: "p1".into(),
            proposer: "alice".into(),
            title: "Test proposal".into(),
            quorum: 2,
            yes_votes: 2,
            no_votes: 0,
            status: ProposalStatus::Approved,
            treasury_recipient: None,
            treasury_amount: 0,
            timelock_blocks: 0,
            approved_at_height: None,
            executed: false,
            voting_token: None,
            snapshot_total_weight: 0,
            snapshot_ledger_root: None,
            voting_snapshot_root: None,
            snapshot_chain_id: None,
            snapshot_block_height: None,
            snapshot_block_hash: None,
            snapshot_chain_state_root: None,
        }
    }

    #[test]
    fn assessment_is_advisory_and_does_not_mutate_proposal() {
        let proposal = proposal();
        let before = proposal.clone();
        let assessment = assess_proposal(&proposal);
        assert!(assessment.advisory_only);
        assert_eq!(assessment.proposal_id, "p1");
        assert_eq!(assessment.risk_level, "info");
        assert_eq!(proposal, before);
    }

    #[test]
    fn weighted_snapshot_without_anchor_is_flagged_for_review() {
        let mut proposal = proposal();
        proposal.voting_token = Some("QUR".into());
        proposal.snapshot_total_weight = 10;
        proposal.snapshot_ledger_root = Some("a".repeat(64));
        proposal.voting_snapshot_root = Some("b".repeat(64));

        let assessment = assess_proposal(&proposal);
        assert!(assessment.findings.iter().any(|finding|
            finding.code == "UNANCHORED_WEIGHTED_SNAPSHOT"
                && finding.severity == FindingSeverity::Review
        ));
    }

    #[test]
    fn weighted_votes_cannot_exceed_committed_snapshot_power() {
        let mut proposal = proposal();
        proposal.voting_token = Some("QUR".into());
        proposal.snapshot_total_weight = 10;
        proposal.snapshot_ledger_root = Some("a".repeat(64));
        proposal.voting_snapshot_root = Some("b".repeat(64));
        proposal.yes_votes = 11;

        let assessment = assess_proposal(&proposal);
        assert!(assessment.findings.iter().any(|finding|
            finding.code == "VOTES_EXCEED_SNAPSHOT_POWER"
        ));
        assert_eq!(assessment.risk_level, "high");
    }

    #[test]
    fn treasury_proposal_without_timelock_requires_review() {
        let mut proposal = proposal();
        proposal.treasury_recipient = Some("bob".into());
        proposal.treasury_amount = 100;
        proposal.timelock_blocks = 0;
        proposal.approved_at_height = Some(10);

        let assessment = assess_proposal(&proposal);
        assert!(assessment.findings.iter().any(|finding|
            finding.code == "MISSING_TREASURY_TIMELOCK"
        ));
        assert_eq!(assessment.risk_level, "review");
    }

    #[test]
    fn unanchored_unit_vote_treasury_proposal_is_flagged_high_risk() {
        let mut proposal = proposal();
        proposal.treasury_recipient = Some("bob".into());
        proposal.treasury_amount = 100;
        proposal.timelock_blocks = 5;
        proposal.approved_at_height = Some(10);

        let assessment = assess_proposal(&proposal);
        assert!(assessment.findings.iter().any(|finding|
            finding.code == "UNANCHORED_TREASURY_ACTION"
                && finding.severity == FindingSeverity::High
        ));
        assert_eq!(assessment.risk_level, "high");
    }

    #[test]
    fn malformed_unit_vote_treasury_anchor_is_flagged_high_risk() {
        let mut proposal = proposal();
        proposal.treasury_recipient = Some("bob".into());
        proposal.treasury_amount = 100;
        proposal.timelock_blocks = 5;
        proposal.snapshot_chain_id = Some(71);
        proposal.snapshot_block_height = Some(12);
        proposal.snapshot_block_hash = Some("not-a-digest".into());

        let assessment = assess_proposal(&proposal);
        assert!(assessment.findings.iter().any(|finding|
            finding.code == "INCOMPLETE_TREASURY_ANCHOR"
                && finding.severity == FindingSeverity::High
        ));
    }

    #[test]
    fn assessment_json_explicitly_marks_advisory_only() {
        let assessment = assess_proposal(&proposal());
        let json = assessment.to_json();
        assert_eq!(json["advisory_only"], true);
        assert_eq!(json["proposal_id"], "p1");
        assert!(json["findings"].is_array());
    }

    #[test]
    fn advisor_context_contains_only_read_only_proposal_facts() {
        let source = proposal();
        let before = source.clone();
        let context = AdvisorContext::from_proposal(&source);
        assert_eq!(context.proposal_id, "p1");
        assert_eq!(context.status, "approved");
        assert_eq!(context.yes_votes, 2);
        assert_eq!(source, before);
    }

    #[test]
    fn bounded_advisor_annotation_serializes_as_advisory_only() {
        let annotation = AdvisorAnnotation {
            provider: "local-test".into(),
            model: "fixture-v1".into(),
            summary: "Review the treasury evidence.".into(),
            observations: vec![AdvisorObservation {
                code: "CHECK_TREASURY".into(),
                severity: FindingSeverity::Review,
                explanation: "Verify the recipient against the approved proposal.".into(),
                evidence_refs: vec!["sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()],
            }],
        };
        let json = annotation.to_json().unwrap();
        assert_eq!(json["advisory_only"], true);
        assert_eq!(json["observations"][0]["code"], "CHECK_TREASURY");
    }

    #[test]
    fn advisor_annotation_rejects_control_characters_and_oversized_output() {
        let mut annotation = AdvisorAnnotation {
            provider: "provider".into(),
            model: "model".into(),
            summary: "safe".into(),
            observations: Vec::new(),
        };
        annotation.summary = "unsafe\nsummary".into();
        assert!(annotation.validate().is_err());

        annotation.summary = "x".repeat(super::MAX_ADVISOR_SUMMARY_BYTES + 1);
        assert!(annotation.validate().is_err());
    }

    #[test]
    fn advisor_annotation_rejects_too_many_observations() {
        let annotation = AdvisorAnnotation {
            provider: "provider".into(),
            model: "model".into(),
            summary: "bounded".into(),
            observations: (0..=super::MAX_ADVISOR_FINDINGS).map(|_| AdvisorObservation {
                code: "CHECK".into(),
                severity: FindingSeverity::Info,
                explanation: "Observation".into(),
                evidence_refs: Vec::new(),
            }).collect(),
        };
        assert!(annotation.validate().is_err());
    }
}
