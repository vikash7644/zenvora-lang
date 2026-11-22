# QORVYNT DAIO Architecture

**DAIO** means **Decentralised Autonomous Intelligent Organization**.

This document defines the intended governance architecture for QORVYNT. It is a design boundary, not a claim that the current local DAO prototype is already decentralized, consensus-backed, or AI-enabled.

## 1. Core principles

- **Decentralised:** governance state and authority must be verifiable by independent network participants; no single operator is the ultimate authority.
- **Autonomous:** deterministic, versioned rules perform routine transitions without an operator manually approving every step.
- **Intelligent:** optional AI services can summarize proposals, identify risks, compare evidence, and explain trade-offs.
- **Member-governed:** only authenticated governance actions can approve proposals or authorize consequential execution.
- **Auditable:** proposals, votes, policy versions, assessments, and execution receipts must be attributable and reproducible.

Autonomy is not the same as intelligence. A deterministic state machine can be autonomous without AI. AI is an advisory capability, not a source of governance authority.

## 2. Authority and trust boundaries

| Component | May do | Must not do |
|---|---|---|
| Governance state machine | Validate rules, count authenticated votes, finalize proposals | Invent member identity or bypass quorum |
| Member wallet | Sign proposal votes and explicitly authorized actions | Delegate its private key to an AI service |
| Intelligence/advisor layer | Read proposal context, produce risk findings and explanations | Cast votes, change proposal state, sign transactions, or move treasury funds |
| Execution engine | Execute a finalized, eligible action after deterministic checks | Trust an AI score as approval |
| Network validators | Verify and replay deterministic state transitions | Depend on nondeterministic model output for consensus |

AI output is untrusted input. It must be treated as an annotation attached to a proposal, not as a command.

## 3. Proposal lifecycle

1. A member submits a proposal with a stable identifier and explicit action payload.
2. The protocol validates the proposal schema, proposer authorization, bounds, and policy version.
3. The intelligence layer may produce a read-only assessment with evidence references and uncertainty.
4. Members inspect the proposal and assessment, then submit wallet-signed votes.
5. The deterministic governance engine checks voter eligibility, duplicate-vote prevention, quorum, and voting rules.
6. Finalization records the result and, where applicable, the approval height.
7. A treasury action remains locked until its deterministic timelock expires.
8. Execution rechecks authorization, finality, replay protection, balances, and action bounds before applying state changes.
9. A receipt records the state transition and relevant state roots.

An AI assessment can never substitute for steps 4–6.

## 4. Intelligence assessment contract

The current src/qur_daio.rs module is a deterministic, read-only risk triage foundation. Its JSON report includes:

- proposal identifier;
- bounded risk score and coarse risk level;
- stable finding codes, severity, explanation, and point contribution;
- advisory_only: true.

The score is a prioritization hint. It is not a probability, security certification, vote recommendation, or automatic rejection rule.

Future AI adapters should implement a narrow read-only interface. Inputs should be bounded and include the proposal payload, applicable policy version, and verified evidence references. Outputs should be schema-validated, size-limited, attributable to a model/version, and stored separately from consensus state unless a deterministic protocol explicitly commits to a canonical representation.

## 5. Required security controls

- Wallet signatures bind the chain/network, DAO, proposal, action, and voting choice; replay across networks or proposals must fail.
- Weighted voting uses a fixed, verifiable snapshot and rejects duplicate voters and weight beyond committed power.
- Proposal execution is deterministic and revalidates eligibility at execution time.
- Treasury actions have explicit recipients, nonzero bounded amounts, quorum, timelocks, and replay protection.
- No model provider receives wallet private keys or signing capability.
- Model unavailability, malformed output, prompt injection, or contradictory analysis must not block ordinary member governance or change protocol state.
- Risk assessments must preserve evidence provenance and distinguish missing evidence from evidence of safety.
- Governance rule changes require their own authenticated proposal and a versioned activation boundary.

## 6. Implementation phases

1. **Read-only assessment:** deterministic checks and JSON findings in src/qur_daio.rs.
2. **Governance hardening:** signed member identity, weighted snapshot verification, deterministic finalization, treasury timelocks, and regression tests.
3. **Advisor interface:** pluggable AI providers behind a read-only contract; strict input/output validation and no signing permissions.
4. **Evidence and audit trail:** proposal-linked assessment versions, evidence hashes, reproducible deterministic findings, and optional model provenance.
5. **Network integration:** consensus-backed proposal/vote state, validator replay, canonical execution receipts, and multi-node tests.
6. **Operational decentralization:** independent infrastructure, transparent upgrades, monitoring, and recovery procedures.

## 7. Current limitations

The existing DAO runtime is a local single-process prototype. The new assessment module does not connect to an AI model, does not independently verify live chain state, and does not authorize execution. A successful unit test or CI run does not establish production readiness or decentralization. Those claims require network-level integration, independent review, and reproducible security testing.
