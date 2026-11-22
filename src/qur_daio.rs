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
        if proposal.snapshot_ledger_root.as_deref().is_none_or(|root| !is_digest(root)) {
            add(
                "INVALID_LEDGER_COMMITMENT",
                FindingSeverity::High,
                "Weighted proposal is missing a valid ledger-root commitment.",
                25,
            );
        }
        if proposal.voting_snapshot_root.as_deref().is_none_or(|root| !is_digest(root)) {
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
            || proposal.snapshot_block_hash.as_deref().is_none_or(|hash| !is_digest(hash))
            || proposal.snapshot_chain_state_root.as_deref().is_none_or(|root| !is_digest(root))
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

#[cfg(test)]
mod tests {
    use super::{assess_proposal, FindingSeverity};
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
    fn treasury_proposal_without_timelock_is_high_risk() {
        let mut proposal = proposal();
        proposal.treasury_recipient = Some("bob".into());
        proposal.treasury_amount = 100;
        proposal.timelock_blocks = 0;

        let assessment = assess_proposal(&proposal);
        assert!(assessment.findings.iter().any(|finding|
            finding.code == "MISSING_TREASURY_TIMELOCK"
        ));
        assert_eq!(assessment.risk_level, "review");
    }

    #[test]
    fn assessment_json_explicitly_marks_advisory_only() {
        let assessment = assess_proposal(&proposal());
        let json = assessment.to_json();
        assert_eq!(json["advisory_only"], true);
        assert_eq!(json["proposal_id"], "p1");
        assert!(json["findings"].is_array());
    }
}
