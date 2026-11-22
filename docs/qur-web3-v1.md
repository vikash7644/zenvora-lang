# QORVYNT Universal Runtime (QUR) — Web3 Manifest v1

QUR is the proposed unified runtime layer for QORVYNT projects. This first implementation milestone adds a **manifest parser and validator** for chain, token, DAO, and wallet declarations.

## Validate a manifest

```qorvynt
chain MyChain {
    consensus = "proof_of_stake"
    native_token = "QOR"
}

token GOLD {
    standard = "fungible"
    supply = 100000000
}

dao Community {
    governance = "on_chain"
}

wallet MyWallet {
    network = MyChain
}
```

Run `qorvynt --qur-check examples/qur/manifest.qv` for a human-readable validation summary, or `qorvynt --qur-json examples/qur/manifest.qv` for a stable JSON manifest suitable for editor integrations and future cloud tooling.

The validator checks declaration names, duplicates, allowed and required fields, supported consensus/governance/token modes, positive u64 token supply, ticker shape, and wallet-to-chain references. It enforces a 1 MiB source limit and a 256-declaration limit. The CI suite also checks a deliberately invalid wallet-to-chain reference and requires `--qur-json` to return `valid: false` with a non-zero exit status.

## Local token-ledger runtime prototype

After validating the manifest, initialize a deterministic local token ledger:

```sh
qorvynt --qur-token-init examples/qur/manifest.qv token-ledger.json
qorvynt --qur-token-transfer token-ledger.json GOLD genesis alice 25
qorvynt --qur-token-balance token-ledger.json GOLD alice
```

Genesis allocates each declared token's full configured supply to the reserved `genesis` account. Transfers use checked integer arithmetic, reject zero amounts, unknown tokens, invalid account identifiers, insufficient funds, and sender/recipient aliasing. Every mutation preserves the declared total supply and writes a canonical state root into the JSON snapshot; the loader verifies that root before accepting a snapshot. Failed transfers leave the ledger unchanged. Ledger loading also validates token identifiers (ASCII identifier syntax, at most 128 bytes), caps the ledger at 4,096 tokens and 1,000,000 balance entries, and rejects malformed or oversized collections before accepting the state.

This is a local single-process ledger prototype. It is not yet backed by QVM contract bytecode, chain transaction admission, consensus, validator signatures, or peer-to-peer replication. The next integration milestone is to expose these state transitions as deterministic, gas-metered QVM contract calls rather than treating this file ledger as a production token network.

### QVM-metered contract-storage transfer

The experimental CLI can also apply a transfer through the deterministic contract-storage layer:

```sh
qorvynt --qur-token-transfer-qvm token-ledger.json contract-state.bin qur_tokens GOLD genesis alice 25 30000
```

The command charges the versioned contract gas schedule, stages supply and account-balance keys in a write overlay, and commits the in-memory token ledger only after the contract-state overlay succeeds. It reports gas used, the contract storage root, and the unified execution-state root combining the updated QUR ledger and contract storage. This is a local integration prototype: it uses the repository's deterministic gas meter and contract storage module, but token transfer is not yet emitted as QVM bytecode/opcodes or admitted as a signed chain transaction. The two local snapshot files are not a cross-file atomic database transaction; production integration must move both records into one chain-state transaction.

### Unified QORVYNT execution-state commitment

The local runtime now exposes a deterministic composite commitment over the canonical QUR token-ledger root and contract-storage root. The versioned `QORVYNT_EXECUTION_STATE_V1` domain binds the two component roots in a fixed order, so a change to either component changes the combined commitment. This is a state-composition primitive for replay and receipt integration; it does not make the two existing snapshot files a single atomic database, and it is not yet a consensus-validated chain state root.

## Execution boundary

This milestone **validates configuration only**. It does not create a blockchain node, start Proof of Stake, mint tokens, deploy contracts, create wallets/private keys, run a DAO, or launch an exchange. A consensus label is not a consensus implementation.

The repository separately contains experimental QVM-backed contract execution, deterministic gas primitives, isolated contract state, and local chain transaction/block-store prototypes. Those remain prototypes, not a production network or complete BFT consensus system. QUR will integrate with them incrementally and retain their security gates.

## Manifest v1 constraints

- One global namespace for declaration names.
- Consensus: `proof_of_stake`, `proof_of_authority`, or `proof_of_work`.
- Native ticker: 2–12 uppercase ASCII letters/digits.
- Token standard: `fungible` or `non_fungible`; supply is a positive 64-bit integer.
- DAO governance: `on_chain`, `off_chain`, or `hybrid`.
- Wallet network must reference a chain in the same manifest.
- Quoted-string escapes and semicolons are not supported in v1.


### Deterministic transfer events

Gas-metered token transfer receipts now include a `TokenTransfer` event with token, sender, recipient, amount, and a domain-separated SHA-256 event hash. The event payload is encoded with length-prefixed fields and charged through the versioned event gas schedule. The CLI prints the event and hash; CI checks the receipt fields. This is a local receipt/event prototype, not yet a canonical block receipt or a QVM bytecode event opcode.


### DAO proposal and voting runtime (local prototype)

The experimental DAO runtime persists proposal records and individual voter markers in the contract-state snapshot. It supports:

```sh
qorvynt --qur-dao-propose dao-state.bin community proposal1 alice "Fund public goods" 2 30000
qorvynt --qur-dao-vote dao-state.bin community proposal1 alice yes 30000
qorvynt --qur-dao-vote dao-state.bin community proposal1 bob yes 30000
qorvynt --qur-dao-finalize dao-state.bin community proposal1 30000
qorvynt --qur-dao-show dao-state.bin community proposal1
```

Each operation uses the deterministic local gas meter and an atomic contract-state overlay, and emits a state root plus a domain-separated event hash. A voter can vote only once per proposal. Finalization requires the configured vote-count quorum; if quorum is met, yes votes must strictly exceed no votes for approval. A tie is rejected. Proposal status is terminal after finalization.

Important limitations: this basic proposal path is a local governance-engine prototype. It does not provide token-weighted voting, delegation, a voting deadline, chain transaction admission, or consensus-backed execution. Treasury actions use a separate anchored proposal path; an `Approved` status alone never authorizes a treasury transfer.


### Wallet-authenticated DAO voting

The signed-vote path uses Ed25519 keys from the existing QORVYNT key-file format. Ordinary local proposals use the legacy local-only domain-separated message. Replay-anchored weighted and treasury proposals use the V3 domain, binding the signature to chain ID, snapshot height, block hash, state root, DAO ID, proposal ID, and vote choice. The runtime verifies the signature before deriving the voter address from the public key; one wallet can vote only once per proposal.

```sh
qorvynt --chain-keygen alice.qvk
qorvynt --qur-dao-vote-signed dao-state.bin community proposal1 alice.qvk yes 30000
```

The unsigned `--qur-dao-vote` CLI route has been removed. The signed-vote CLI does not turn a local proposal into a consensus transaction: membership eligibility, hardware-wallet signing, transaction admission, and network finality remain separate requirements. Unanchored legacy treasury proposals reject signed voting.


### DAO treasury and block-height timelock (local accounting prototype)

Treasury proposals must carry a replay-verified snapshot anchor before they can accept signed votes. The anchored creation API stores the anchor's chain ID, height, block hash, and state root in the proposal. The signed vote is V3 and binds all of those fields. A legacy unanchored treasury proposal can still be loaded for compatibility, but signed voting rejects it without mutating state.

The block store exposes `verified_anchor_at(height)` for historical blocks accepted by its append/replay validation. The DAO library's verified-tip helpers re-fetch the proposal's stored snapshot height from the supplied store, compare the exact chain ID, block hash, and state root, require the current tip to be on the same chain and at or above the snapshot, and derive the finalization/execution height from that tip. Execution also checks that the recorded approval height lies between the snapshot and current verified tip.

The anchored proposal path is also available through the CLI. It loads and replay-verifies the supplied block store, retrieves the requested historical height, and persists that anchor before any signed treasury vote:

```sh
qorvynt --chain-keygen validator.qvk
qorvynt --chain-store-init dao-chain.qvb 98765 validator.qvk 100
qorvynt --qur-dao-propose-treasury-anchored dao-state.bin dao-chain.qvb 98765 0 community payout1 founder "Contributor payout" contributor 40 1 5 30000
qorvynt --qur-dao-vote-signed dao-state.bin community payout1 alice.qvk yes 30000
```

These are local replay-verified library/CLI helpers, not distributed finality or consensus execution. The caller must supply the correct canonical block store for the DAO's chain. Treasury finalization/execution CLI commands remain disabled; no caller-supplied height is accepted as authority.

### DAO treasury payouts using the QUR token ledger

The token-backed payout implementation remains covered by DAO module tests, but the legacy CLI payout routes are intentionally disabled. A local token ledger balance or a successful unit test does not prove that the DAO is integrated into chain consensus. The verified-tip library API rechecks the proposal anchor and timelock before staging a local QUR ledger transfer; it is not yet an on-chain custody or execution adapter.

Production use still requires DAO state and token-ledger state to be part of the same deterministic block transition, canonical chain selection/finality, replay protection across nodes, and an execution receipt committed by consensus. Do not use the local prototype as production treasury custody.

### Proposal-snapshot token-weighted wallet voting

Weighted proposals now capture the selected token's holder balances when the proposal is created. The snapshot is stored in DAO contract state with a 2,048-holder limit; later token transfers do not change that proposal's voting power. Signed votes derive the voter address from the verified Ed25519 key and read the fixed snapshot weight. The generic caller-supplied-weight API rejects weighted proposals, so it cannot be used to inflate voting power; weighted proposals accept only the authenticated snapshot-vote path. Zero-snapshot-balance wallets and duplicate votes are rejected. Each weighted proposal also stores the source ledger's canonical state root and a versioned SHA-256 commitment over the token identifier and the sorted `(wallet, balance)` snapshot entries. These commitments are included in the serialized proposal and therefore in DAO contract storage state roots; legacy proposals without these optional fields remain readable. Before every weighted vote, the runtime enumerates the bounded snapshot entries in deterministic key order, validates wallet identifiers and fixed-width balances, recomputes total voting power, and recomputes the snapshot digest. A missing, malformed, altered, or total-mismatched snapshot is rejected before the vote can mutate state. The prefix scan is bounded by the existing 2,048-holder limit and its reads are charged against the vote gas budget.

```sh
qorvynt --qur-dao-propose-weighted token-ledger.json dao-state.bin community proposal1 founder "Weighted governance vote" GOLD 100 50000
qorvynt --qur-dao-vote-weighted dao-state.bin community proposal1 alice.qvk GOLD yes 100000
```

A weighted proposal is rejected if its requested quorum exceeds the total eligible wallet voting power captured in that snapshot; this prevents creating a proposal that can never reach quorum. The exact-boundary case (quorum equal to total eligible voting power) is valid. The Rust DAO API now exposes `create_weighted_proposal_at_anchor`, which accepts a `VerifiedBlockAnchor` obtained from the replay-verified block store and persists its chain ID, tip height, header hash, and chain state root alongside the weighted token snapshot. These fields are included in the proposal serialization and DAO storage commitment. The CLI now also exposes `--qur-dao-propose-weighted-anchored <ledger.json> <state.bin> <block-store> <chain-id> <dao-id> <proposal-id> <proposer> <title> <token> <quorum> <gas-limit>`. It loads the block store through replay verification, rejects an empty store, and records the verified tip anchor on the proposal. The existing CLI `--qur-dao-propose-weighted` remains the unanchored local-ledger path; callers must not treat the new metadata as proof that token balances were part of the chain's replayed state. The anchor is contextual provenance only until the token ledger is incorporated into the chain runtime and replay transition. A verified-tip accessor is now available on the local append-only block store: `BlockStore::verified_tip_anchor()` returns the chain ID, tip height, canonical header hash, and committed chain state root only from the store's accepted block prefix. Store loading independently replays every block before exposing that tip. This is an integration primitive, not yet a DAO snapshot binding: the QUR token ledger remains separate from the chain runtime, so the returned chain state root does not prove that the weighted-voting token balances were included in that block. The next integration must carry this anchor into proposal creation and, before production use, make token-ledger state part of the same replayed consensus state transition. The local block-store anchor is not a distributed finality certificate. The snapshot is currently taken from the local QUR token ledger supplied at proposal creation. Only key-derived QORVYNT wallet addresses (`zv` plus 40 lowercase/uppercase hexadecimal characters) are eligible for voting weight; reserved/system accounts such as `genesis` and `dao_<id>` are excluded because they cannot authenticate as wallet signers. The proposal quorum and yes/no totals are measured in token units, not wallet count. It is not yet anchored to a canonical block height or consensus state root, and snapshot entries are not yet generated from chain replay. Production governance must authenticate the snapshot against a finalized chain block, and the wallet signature CLI must be replaced or complemented by externally signed transaction admission.

### Root-verified execution snapshots

The Rust runtime API now exposes `encode_execution_snapshot(&ledger, &contracts)` and `decode_execution_snapshot(&value)`. The versioned `qorvynt.execution-state.v1` JSON envelope stores the canonical token-ledger JSON, a bounded canonical contract-storage snapshot encoded as lowercase hex, and the composite execution root. Restore validates the ledger's own supply/root invariants, strictly decodes the contract snapshot, recomputes the composite root, and rejects mismatches before returning either component. This gives tests and future replay code one portable state handoff instead of trusting two unrelated serialized files. It is still a local QUR+contract snapshot API: it does not yet include native chain accounts, does not alter block encoding, and must not be described as consensus replay or a production database transaction. The separate `chain_execution_state_root` helper composes native-account, QUR-ledger, and contract-storage roots, but block execution and `BlockStore::load_from_file` still need to invoke and verify that three-component transition.
