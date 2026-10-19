# Zenvora Chain — Implementation Roadmap and Release Gates

**Status:** Planning document, 2026-10-18  
**Branch:** `chain-lang-integration`  
**Authority:** This document is a staged plan, not evidence that planned features exist. The protocol drafts remain unapproved until decisions are explicitly recorded and reviewed.

## Product direction

Build an independent Zenvora network with ZVR as its proposed native asset and Zenvora Lang as a first-class developer path. Independence means owning the protocol rules, genesis configuration, transaction/state model, P2P network, consensus implementation, and release process. It does not require inventing new cryptographic primitives. Use established, reviewed cryptographic algorithms and canonical encodings.

EVM compatibility is **undecided**. It may be evaluated for tooling reach, but must not be confused with consensus or network independence. The first implementation should not combine a custom VM, EVM compatibility, bridges, DeFi, and production consensus in one release.

## Current maturity

- **Zenvora Lang / VM:** early Rust-based compiler and runtime prototype; CI is available.
- **Zenvora Chain integration:** VM-backed local ledger/transfer harness with tests; this is not a full blockchain node.
- **Protocol:** specification drafts exist; consensus, finality, cryptographic formats, network parameters, and tokenomics are not locked.
- **Public testnet / mainnet:** not launched by this plan; neither should be claimed until implemented and independently verified.

Use these maturity labels in status updates: **Specified → Prototype → Tested → Reviewed → Production-ready**. A phase advances only when its exit criteria are met.

The current contract execution design proposal is documented in [`contract-execution-v1.md`](contract-execution-v1.md). It specifies intended state, authorization, gas, rollback, persistence, replay, and test gates; it is **not yet implemented protocol behavior**.

## Phase plan

### Phase 0 — Stabilize the language-to-chain boundary (current prerequisite)
- [ ] Keep the Zenvora Lang compiler/VM regression suite green.
- [ ] Make bytecode verification, resource limits, and deterministic execution release gates.
- [ ] Define a narrow, typed host interface for contract state reads/writes and transaction context.
- [ ] Ensure failed execution cannot partially mutate committed state.
- [ ] Add reproducible tests for replay, crash recovery, and state-root agreement.

**Exit gate:** CI passes; malformed programs/bytecode are rejected safely; deterministic results and rollback semantics are tested.

### Phase 1 — Core chain data model
- [ ] Define canonical encodings and versioning for genesis, block headers, transactions, receipts, events, and state commitments.
- [ ] Define transaction validation, nonce/replay protection, expiry, maximum sizes, resource limits, and failure semantics.
- [ ] Implement a deterministic state transition function independent of networking.
- [ ] Add a persistent block/state store with crash-safe writes and recovery.
- [ ] Add golden vectors and cross-process replay tests.

**Exit gate:** identical ordered inputs and parent state produce identical block/receipt/state commitments; invalid transactions are rejected without corrupting state.

### Phase 2 — P2P networking and synchronization
- [ ] Define protocol handshake with network ID and genesis-hash checks.
- [ ] Implement configurable seed nodes, peer discovery, peer scoring, bounded gossip, and block/transaction propagation.
- [ ] Add connection, message-size, rate, and resource limits; test malformed peers.
- [ ] Implement synchronization, fork handling, snapshots, and restart/recovery behavior.
- [ ] Add network metrics and repeatable multi-node tests.

**Exit gate:** independent local nodes join only the correct network, synchronize a known chain, and recover from restarts and bad peers in automated tests.

### Phase 3 — Consensus and finality
- [ ] Compare consensus candidates against explicit threat assumptions, validator count, network conditions, and operational cost.
- [ ] Specify validator registration/rotation, block production, finality, conflicting certificates, and validator-set changes.
- [ ] Specify rewards, commission, delegation, slashing, jailing, epochs, and unbonding only after economics and attack analysis.
- [ ] Implement consensus only after deterministic execution and block validation are stable.
- [ ] Test Byzantine/fault scenarios, partitions, restarts, and validator churn.

**Exit gate:** documented safety/liveness assumptions, automated adversarial tests, and an independent consensus review. Do not claim decentralization from a single-node or permissioned demo.

### Phase 4 — RPC, CLI, and node operations
- [ ] Versioned RPC for chain info, blocks, transactions, receipts, balances, and submission.
- [ ] WebSocket subscriptions only after event semantics are stable.
- [ ] CLI for key-free node operations, local devnet, genesis inspection, and diagnostics.
- [ ] Separate full, archive, RPC, and validator roles as justified by measured needs.
- [ ] Add health checks, metrics, logs, backups, snapshots, and disaster-recovery drills.

**Exit gate:** documented API conformance tests, bounded RPC resource use, and reproducible node setup/recovery.

### Phase 5 — Wallet and explorer
- [ ] Establish address/signature formats and wallet threat model first.
- [ ] Build key generation, encrypted keystore, recovery guidance, transaction review, and network separation.
- [ ] Add blocks, transactions, addresses, validators, events, and network statistics to the explorer.
- [ ] Test malicious RPC responses, wrong-network prompts, and transaction-display accuracy.
- [ ] Hardware wallet, multisig, QR payments, and dApp connections require separate design/review.

**Exit gate:** testnet-only wallet/explorer pass security and integration tests; never store recovery phrases in server logs or telemetry.

### Phase 6 — Smart contracts, tokens, and staking
- [ ] Decide first execution target: native Zenvora VM, EVM-compatible VM, or a staged design.
- [ ] Define a contract ABI, deployment format, deterministic gas/resource schedule, storage model, events, and verification process.
- [ ] Build local deployment, testing, debugging, and source-to-artifact verification tooling.
- [ ] Define token standards only after transaction and contract semantics are stable.
- [ ] Implement staking only after consensus and tokenomics are reviewed.

**Exit gate:** deterministic contract conformance suite, resource-exhaustion tests, reproducible builds, and security review. Solidity compatibility may only be claimed against a published passing compatibility suite.

### Phase 7 — Economic model and treasury
- [ ] Publish a supply/allocation model with genesis allocations, vesting, treasury, issuance, fees, staking rewards, and burn policy if any.
- [ ] Model validator costs, stake concentration, attack costs, participation incentives, and sensitivity to price/participation assumptions.
- [ ] Define parameter-change bounds, governance authority, timelocks, and emergency limits.
- [ ] Use valueless testnet units until a separate launch review is approved.
- [ ] Do not promise returns or choose token supply/reward numbers for marketing reasons.

**Exit gate:** transparent model, independent review, reconciled genesis allocation table, and tested parameter bounds.

### Phase 8 — DeFi and payments
- [ ] Start with audited, narrowly scoped token and payment primitives.
- [ ] DEX pools, lending/borrowing, stablecoin infrastructure, oracles, and launchpads each require independent threat models and audits.
- [ ] Add limits, circuit breakers, monitoring, oracle-failure behavior, and incident procedures.
- [ ] Test economic attacks and liquidity edge cases before any public deployment.

**Exit gate:** each protocol has a separate audit/review, adversarial testing, and an incident plan. Do not bundle unaudited financial features into core consensus.

### Phase 9 — Governance and interoperability
- [ ] Specify proposal, voting, delegation, quorum, execution, timelock, and emergency procedures.
- [ ] Keep emergency powers narrow, transparent, logged, and independently reviewable.
- [ ] Design bridges only with explicit trust assumptions, validator/relayer threat models, proof verification, and pause/recovery plans.
- [ ] Treat cross-chain messaging and external assets as high-risk, separate launches.

**Exit gate:** governance simulations and independent bridge review; no bridge launch based only on a successful demo transfer.

### Phase 10 — Developer ecosystem
- [ ] Versioned JavaScript/TypeScript SDK and CLI.
- [ ] Add Python/Rust/Go tooling when stable APIs and demand justify maintenance.
- [ ] Public docs, examples, local node, testnet faucet, contract templates, and developer portal.
- [ ] Add NFT, identity/name service, gaming, social, grants, and dApp directory only as separate products with owners and tests.
- [ ] Mobile SDK and AI-agent interfaces must use least-privilege signing and explicit transaction approval.

**Exit gate:** end-to-end examples work from a clean environment; API versions and support boundaries are documented.

### Phase 11 — Public testnet and security review
- [ ] Publish threat model, node runbook, genesis configuration, protocol versions, and known limitations.
- [ ] Run multi-operator validator trials, load tests, fuzzing, chaos tests, and recovery drills.
- [ ] Establish vulnerability reporting, incident response, patch/release process, and bug bounty only when triage is staffed.
- [ ] Resolve critical/high-severity findings and publish review scope/results.
- [ ] Make faucet tokens valueless and clearly label testnet status.

**Exit gate:** reproducible public testnet, independent security review, operational readiness, and published known issues.

### Phase 12 — Mainnet and long-term ecosystem
- [ ] Freeze reviewed protocol parameters and genesis allocation.
- [ ] Verify release artifacts and build provenance; rehearse upgrades and rollback/containment procedures.
- [ ] Require explicit launch approval after security, economics, legal, operational, and community review.
- [ ] Launch gradually with monitored limits and published incident channels.
- [ ] Expand toward Zenvora Pay, Identity, Storage integration, mobile SDK, and AI-agent infrastructure only when core reliability supports them.

**Exit gate:** signed launch checklist and accountable operators. Mainnet is not a date-based milestone; it is an evidence-based release decision.

## Cross-cutting security gates

Every phase should include relevant unit/integration tests, property or fuzz tests, resource limits, dependency review, error-path tests, and documented residual risks. Consensus-critical code and cryptographic designs require independent review. Never invent cryptography; use established libraries and canonical encodings.

## Measurement policy

Publish reproducible benchmarks rather than unsupported claims. Measure transaction throughput, latency/finality, resource cost, node sync/recovery, peer churn, RPC availability, state-root agreement, developer task completion, and security-test coverage. State hardware, network, dataset, configuration, and methodology. Set numerical targets only after a baseline and explicit use cases exist.

## Immediate next engineering order

1. Inspect and keep the current CI/regression suite green.
2. Harden the deterministic VM-backed state transition and persistent ledger recovery.
3. Specify canonical transaction/block/receipt/state formats and golden test vectors.
4. Add signed transactions, chain/network replay protection, and fee/resource accounting.
5. Implement a deterministic single-node block producer/validator as a test harness.
6. Only then connect multi-node P2P and select/implement consensus against a reviewed threat model.

**Non-goal for the immediate milestone:** do not start a DEX, bridge, stablecoin, mainnet, or token sale while the core transaction/state/consensus foundations remain prototypes.
