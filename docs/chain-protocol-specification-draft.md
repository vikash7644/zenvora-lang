# Zenvora Chain Protocol Specification — Draft 0.1

**Status:** DESIGN DRAFT — NOT CONSENSUS-LOCKED  
**Purpose:** Establish protocol decisions before implementing consensus-critical chain code.  
**Scope:** Zenvora Chain / ZVR and its integration with Zenvora Lang.

> Nothing in this draft should be treated as a deployed network rule. Values marked **DECISION REQUIRED** must be approved and frozen in a versioned specification before mainnet or consensus implementation. Test-only prototypes must use a clearly separate chain ID and must not imply real economic value.

## 1. Design principles

1. Deterministic state transitions: the same valid parent state and ordered transactions must produce identical state, receipts, and roots on every honest node.
2. Explicit versioning and domain separation: chain ID, transaction version, signature domain, and protocol version must prevent cross-network and cross-version replay.
3. Verify before execute: validate encoding, signatures, nonce, limits, fees, and resource budgets before mutating state.
4. No unreviewed consensus changes: consensus rules require a documented upgrade path, compatibility tests, and a timelock/governance process.
5. Safety before throughput: prefer clear finality and bounded resource use over unmeasured performance claims.
6. Testnet first: test with valueless tokens; do not advertise security, decentralization, or production readiness before independent review.

## 2. Protocol decision register

These are proposals to evaluate, not final locked decisions.

| Decision | Proposed starting point | Status |
|---|---|---|
| Chain ID / network ID | Separate immutable numeric chain ID per network; distinct devnet/testnet/mainnet IDs | DECISION REQUIRED |
| Consensus | Compare a BFT proof-of-stake design with a simpler permissioned testnet first; define fault assumptions and validator set changes | DECISION REQUIRED |
| Finality | Explicit finalized checkpoint; document when a block is irreversible and how conflicting certificates are handled | DECISION REQUIRED |
| Native asset | ZVR as native fee/staking asset, subject to tokenomics approval | PROPOSED |
| Execution | Zenvora VM first; EVM compatibility is a separate, explicit decision | DECISION REQUIRED |
| Account model | Nonce-based accounts are a reasonable initial design; specify address derivation and signature algorithm | DECISION REQUIRED |
| Fees | Deterministic resource/gas metering, bounded minimum fee and explicit fee distribution | DECISION REQUIRED |
| Supply | Do not assume a supply cap, inflation, burn, or reward rate until tokenomics are modeled | NOT DECIDED |
| Block time / limits | Set only after consensus and benchmark tests; enforce maximum block bytes, tx count, and execution budget | NOT DECIDED |
| Hash/signature algorithms | Select reviewed, standard algorithms and canonical encodings; do not invent cryptography | DECISION REQUIRED |
| Upgrade policy | Versioned protocol upgrades with compatibility tests and a defined activation height | DECISION REQUIRED |

## 3. Blockchain protocol

### 3.1 Chain identity and genesis

Genesis configuration must be canonical, hashed, versioned, and committed to the repository. It should define:
- chain ID, network name, protocol version, genesis timestamp and genesis hash;
- initial validator set and consensus parameters;
- initial account balances, if any, with auditable allocations;
- native asset metadata and initial state;
- execution limits, fee parameters, and protocol feature flags.

Every node must reject a peer or block whose chain ID/genesis hash differs from its configured network. Genesis allocations must match the published tokenomics allocation table exactly.

### 3.2 Blocks and transactions

A block header should commit to at least: header version, chain ID, height, parent hash, timestamp policy, proposer/consensus proof, transaction root, receipt root, state root, and any consensus-required validator-set commitment. Specify canonical serialization and hash domain separation.

A signed transaction should include: transaction version, chain ID, sender, nonce, operation/payload, fee limits, resource limit, expiry or validity window where supported, and signature. Define exact bytes signed, signature validation, nonce advancement, and failure semantics. Reject unknown mandatory versions rather than interpreting them loosely.

### 3.3 State, receipts, events, and Merkle commitments

- **State root:** deterministic commitment to the post-block state using a specified authenticated data structure.
- **Transaction root:** commitment to the ordered transactions in the block.
- **Receipt root:** commitment to one receipt per included transaction, in transaction order.
- **Receipts:** include transaction hash, execution status, consumed resources, fee charged, and event/log commitment.
- **Events/logs:** deterministic, bounded, indexed by transaction and execution order; define whether failed execution preserves logs (recommended: revert execution logs on failure while still recording a failed receipt).
- **Merkle proofs:** specify tree construction, leaf/node domain separation, odd-leaf behavior, proof encoding, and verification test vectors.

Do not use the same root label for different data structures. All roots must be recomputable from canonical block data and execution results.

### 3.4 Reorganizations, forks, finality, and synchronization

The spec must distinguish fork choice from finality. Define:
- how competing blocks are selected before finality;
- what finalized means and whether finalized history can ever be reverted;
- behavior when a node sees conflicting finality proofs (halt safely and alert; never silently pick one);
- reorg limits and mempool reconciliation for non-finalized blocks;
- fork detection, peer evidence collection, and recovery;
- state sync checkpoints, snapshot format/version, snapshot integrity verification, and replay from a trusted finalized checkpoint;
- archive-node retention guarantees, light-client proof requirements, and their limitations.

Fast sync must verify authenticated commitments and consensus proofs; downloading a snapshot from a peer alone is not trustless verification.

## 4. P2P networking

Specify a versioned, authenticated handshake containing protocol version, chain ID, genesis hash, node identity, and supported capabilities. Define message framing, size limits, deadlines, and malformed-message handling.

Required subsystems:
- peer discovery and configurable bootstrap/seed nodes;
- bounded inbound/outbound connections and per-IP/resource limits;
- peer scoring based on reproducible behavior, with decay and appeal/recovery rules;
- temporary bans for abuse, with protections against trivial identity/IP rotation;
- gossip and deduplication for transactions and blocks;
- block/transaction propagation with backpressure and request timeouts;
- NAT traversal only where safe and practical; never make it a consensus dependency;
- network health metrics and safe shutdown/reconnect behavior.

P2P data is untrusted. Every block and transaction must be validated independently of the peer's reputation.

## 5. Developer infrastructure

Build in this order:
1. local single-node devnet and deterministic test fixtures;
2. `zenvora-cli` for init, key management, node run, status, account, tx, and logs;
3. local multi-node simulator and automated integration tests;
4. documented RPC API with authentication/rate limits for privileged methods;
5. contract compile/deploy/call tooling, ABI/interface versioning, and reproducible contract verification;
6. WebSocket subscriptions with reconnect semantics and bounded queues;
7. testnet deployment tools, SDKs, package/registry design, and developer documentation.

Contract verification must compare reproducible compiler inputs, compiler version, settings, and artifact hashes—not merely accept uploaded source code. SDK registry/package signing and dependency pinning need a separate threat model.

## 6. Wallet infrastructure

Wallet work must follow the transaction and signature specification. Required design areas:
- HD derivation standard, derivation paths, address format, and recovery compatibility;
- seed phrase generation/backup warnings and secure entropy source;
- encrypted keystore format, KDF parameters, versioning, and migration;
- multiple accounts and explicit network/chain ID display;
- transaction preview, nonce handling, fee/resource estimation, and simulation warnings;
- address book, QR encoding with checksum, and human-readable amount display;
- hardware-wallet signing interface and multisig policy format;
- dApp connection permissions, origin display, chain switching consent, and per-request signing approval.

Never transmit seed phrases or private keys to RPC servers. WalletConnect-style connectivity must not silently grant transaction-signing permission.

## 7. Explorer and public APIs

Index blocks, transactions, receipts, addresses, events, contracts, tokens, and validators from verified node data. Display finality status clearly. Support search, pagination, API keys/rate limits, and network statistics. Charts must state their data source and indexing delay.

Internal transactions should only be displayed if the execution model defines them unambiguously. Contract verification must be tied to reproducible build metadata. An explorer is an indexer/view, not the source of consensus truth.

## 8. DeFi and token applications

Do not implement production DeFi before the base chain, contract runtime, event semantics, and upgrade policy have been reviewed. Treat each as a separate audited protocol:
- DEX and liquidity pools; LP share accounting and manipulation risks;
- staking, validator rewards, unbonding, slashing (if any), and withdrawal delays;
- lending/borrowing, collateral parameters, liquidation, and bad-debt handling;
- stablecoins and price feeds, with explicit collateral/redemption model;
- oracles with source diversity, staleness checks, manipulation defenses, and circuit breakers;
- liquid staking, treasury, payment channels, and yield farming only after separate specifications.

Do not promise yields or price stability. Bridges and oracle-dependent contracts create additional trust assumptions.

## 9. Security requirements

Minimum requirements include replay protection, canonical signature verification, nonce and double-spend protection, mempool quotas, transaction size/resource limits, RPC authentication and rate limits where needed, validator key isolation/rotation, secure configuration, secret handling, dependency scanning, fuzzing, property tests, and deterministic replay tests.

Prepare incident procedures before public testnet: disclosure contacts, pause/containment powers and limits, key-compromise steps, rollback/forward-fix policy, snapshot recovery, backups, postmortems, and independent audit scope. Bug bounty terms must be published before inviting reports. “Emergency stop” must not create an undocumented centralized control over finalized state.

## 10. Governance and upgrades

Specify proposal creation, deposits/spam controls, voting period, quorum, approval threshold, delegation, validator participation, treasury custody, conflicts of interest, and proposal execution. Consensus-critical parameter changes require a delayed activation height and a compatibility window. Define emergency governance powers, scope, expiry, public notice, and safeguards against unilateral capture.

Governance must not be able to retroactively change transaction validity or rewrite finalized history without an explicitly documented social-consensus hard fork.

## 11. Interoperability

EVM compatibility is **not assumed**. Decide it before choosing the VM and contract tooling. If selected, define bytecode compatibility level, opcode/precompile scope, gas schedule, RPC compatibility, and test-suite targets.

Cross-chain bridges, relayers, token bridging, and cross-chain messaging are deferred until the base chain is stable. Every bridge design must state its trust model (multisig, light-client verification, or other), validator/relayer compromise consequences, replay protection, rate limits, and emergency procedures.

## 12. Operations and ecosystem

Operations baseline: structured logs, metrics API, Prometheus-compatible metrics, dashboard definitions, node/validator/RPC alerts, transaction monitoring, uptime probes, release checksums, signed releases, public status page, and documented incident communications.

Ecosystem work follows a stable testnet and usable documentation: tutorials, example projects, SDK docs, community portal, dApp directory, grants, hackathons, ambassador program, forum, and transparent grant review/reporting. Community programs do not substitute for technical security.

## 13. Tokenomics specification — mandatory pre-implementation gate

No supply, reward, inflation, fee split, or allocation number is approved by this draft. Publish a versioned tokenomics table with absolute units and percentages that reconcile exactly to 100% where allocations are percentages.

Must decide and model:
- maximum supply vs uncapped supply;
- initial circulating supply and genesis allocations;
- validator issuance, reward schedule, and security budget;
- staking eligibility, lock/unbonding period, reward calculation, and penalties/slashing (if any);
- transaction fee split: burn, proposer, validator pool, treasury, or other;
- treasury and ecosystem/developer grant allocations;
- community allocation and distribution method;
- vesting cliff, linear vesting interval, unlock schedules, and beneficiary transparency;
- optional burn rules and whether supply can ever increase after burns;
- inflation/deflation bounds and long-run validator economics;
- handling of lost keys, unclaimed rewards, and rounding/dust.

Required simulations: low/medium/high transaction demand, different token prices without assuming a price target, validator participation, reward dilution, treasury runway, attack cost assumptions, and stress scenarios. Publish assumptions and sensitivity analysis. Never design consensus security around a promised token price.

## 14. Required test gates

Before a public testnet:
- deterministic genesis and block replay tests;
- canonical serialization and hash test vectors;
- transaction signature, chain-ID replay, nonce, expiry, and fee tests;
- Merkle root/proof tests including empty/single/odd-sized trees;
- invalid block, invalid state root, invalid receipt root, and malformed encoding rejection;
- fork/finality conflict and reorg integration tests;
- peer abuse, oversized messages, disconnect/reconnect, and sync interruption tests;
- snapshot verification and state equivalence tests;
- fuzzing of transaction decoder, block decoder, VM boundary, and RPC parsers;
- resource exhaustion and adversarial load tests;
- tokenomics invariant tests and independent review.

Before mainnet: external consensus/cryptography review, contract-runtime audit, operational rehearsal, reproducible releases, documented threat model, and an explicit launch-readiness sign-off.

## 15. Implementation phases

**Phase A — Specification:** settle consensus/finality, account model, hash/signature formats, block/transaction schemas, execution model, limits, upgrade rules, and tokenomics. Produce a threat model and test vectors.

**Phase B — Deterministic single-node devnet:** genesis, state transition, signed transactions, receipts, roots, local persistence, replay, and comprehensive tests. No real-value token.

**Phase C — Multi-node testnet:** P2P, peer limits/scoring, consensus, finality, fork handling, state sync, observability, and fault-injection tests.

**Phase D — Developer experience:** CLI, RPC, SDK, contract tooling, explorer, wallet prototype, and local simulator.

**Phase E — Security and ecosystem:** external audits, bug bounty, governance, tokenomics finalization, bridge/DeFi decisions, and mainnet readiness review.

## 16. Sign-off checklist

Do not mark the protocol “locked” until each item has a named decision, rationale, testable rule, version number, and review:
- [ ] Consensus and fault assumptions
- [ ] Finality and fork choice
- [ ] Chain IDs and genesis schema
- [ ] Canonical block and transaction formats
- [ ] Hashes, signatures, and address derivation
- [ ] State model, roots, receipts, events, and proofs
- [ ] VM/contract compatibility choice
- [ ] Gas/resource limits and fee rules
- [ ] Validator economics and tokenomics
- [ ] Upgrade/governance policy
- [ ] P2P and RPC threat models
- [ ] Snapshot, archive, and light-client trust model
- [ ] Independent review plan and test gates

**Current status:** Draft only. No consensus, tokenomics, mainnet, audit, or production-readiness claim is made by this document.
