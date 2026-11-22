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

Important limitations: voter identifiers are currently caller-supplied labels, not authenticated wallet identities; quorum counts votes, not token-weighted voting power; there is no voting deadline, delegation, treasury execution, timelock, chain transaction admission, or consensus replay. An `Approved` status records the result only and does not execute arbitrary actions. Treat this as a local governance-engine prototype, not a production DAO.


### Wallet-authenticated DAO voting

The signed-vote path uses Ed25519 keys from the existing QORVYNT key-file format. The wallet signs a domain-separated message containing the DAO ID, proposal ID, and vote choice. The runtime verifies the signature before deriving the voter address from the public key and recording the vote. The voter address is the replay-protection key, so one wallet can vote only once per proposal.

```sh
qorvynt --chain-keygen alice.qvk
qorvynt --qur-dao-vote-signed dao-state.bin community proposal1 alice.qvk yes 30000
```

The older `--qur-dao-vote` command remains only as a local test/development path and accepts an unauthenticated label; do not use it for real governance. Signed voting authenticates key ownership but does not yet establish token-weighted voting power, membership eligibility, chain-level transaction finality, hardware-wallet signing, or a consensus-enforced DAO.


### DAO treasury and block-height timelock (local accounting prototype)

DAO proposals may carry a single treasury-transfer action. The local contract-state ledger supports funding the DAO treasury, creating a transfer proposal with a positive quorum and block delay, finalizing it at an explicit block height, and executing it only after the delay has elapsed. Execution atomically debits the treasury, stores a transfer record, and marks the proposal executed. Re-execution and insufficient funds are rejected without partial state changes.

Example CLI flow:

```sh
qorvynt --qur-dao-treasury-fund dao-state.bin community 100 30000
qorvynt --qur-dao-propose-treasury dao-state.bin community payout1 founder "Contributor payout" contributor 40 1 5 30000
qorvynt --qur-dao-vote-signed dao-state.bin community payout1 alice.qvk yes 30000
# Height-based finalization and treasury execution are intentionally disabled.
# The current local DAO prototype does not yet bind treasury proposals to a
# replay-verified chain anchor and cannot safely derive a consensus execution height.
qorvynt --qur-dao-treasury-balance dao-state.bin community
```

This is not yet a transfer of native tokens or chain assets: treasury funding is local accounting state, the recipient is an identifier, and the supplied block height is not authenticated against a canonical chain header. Production use requires chain transaction admission, authenticated block context, token custody/escrow, consensus replay, and an on-chain execution adapter.


### DAO treasury payouts using the QUR token ledger

The DAO runtime now has a token-backed treasury execution path. Fund the DAO's token account by transferring tokens to the deterministic account `dao_<dao-id>`, then create and approve a treasury proposal with a signed wallet vote. The local CLI accepts a caller-supplied block height; the transfer is rejected before the timelock expires, but that height is not authenticated against a canonical chain header. On success, the QUR token ledger moves the proposal amount from `dao_<dao-id>` to the proposal recipient, preserves total token supply, stores the DAO execution marker and token balances in the contract-state overlay, and marks the proposal executed.

```sh
qorvynt --qur-token-init examples/qur/manifest.qv token-ledger.json
qorvynt --qur-token-transfer token-ledger.json GOLD genesis dao_community 100
qorvynt --qur-dao-propose-treasury dao-state.bin community payout1 founder "Contributor payout" contributor 40 1 5 30000
qorvynt --qur-dao-vote-signed dao-state.bin community payout1 alice.qvk yes 30000
qorvynt --qur-dao-finalize-height dao-state.bin community payout1 100 30000
qorvynt --qur-dao-treasury-token-execute token-ledger.json dao-state.bin community payout1 GOLD 105 50000
```

The execution API clones the token ledger, validates the transfer, stages DAO proposal and token-balance writes in one contract-state overlay, commits that overlay, and only then publishes the in-memory ledger candidate. Its receipt now includes a versioned unified `execution_state_root` combining the committed QUR ledger root and contract-storage root; the CLI emits this 32-byte commitment and CI checks its 64-character lowercase-hex encoding. The CLI now writes a bounded (64 MiB) write-ahead journal containing both outputs before publishing either file. Each staged file is synced before rename; on Unix, the containing directory is also synced after rename. Staging files are created with exclusive-create semantics (`create_new`) rather than truncating an existing path, and cleanup removes only a staging file created by the current writer; a stale or competing staging file therefore causes a safe failure instead of being overwritten. A new transaction first recovers any earlier journal, and the treasury execution command replays a pending journal before reading either file. Corrupt, truncated, oversized, or path-mismatched journals are rejected without applying their payload. This provides recoverable two-file publication, not simultaneous multi-file visibility or a substitute for a transactional database; filesystem and platform durability semantics still apply, and the journal must be preserved until recovery completes. A state-side recovery index is written before the ledger-side journal. DAO commands that receive only `dao-state.bin` (including weighted voting, proposal inspection, and finalization) consult this index and replay the paired ledger/state payload before loading the snapshot. If a crash occurs after the index but before the journal, the next state-only command safely removes the stale index. The index is path-checked against the journal before replay; older journals created before this index existed still require a command that supplies both ledger and state paths to recover them. This remains a local QUR runtime integration: the CLI-supplied height is not checked against a canonical block header, token balances are not yet part of consensus replay, and wallet signatures authenticate votes but not a network transaction envelope. Do not use it as production chain treasury custody yet.


### Proposal-snapshot token-weighted wallet voting

Weighted proposals now capture the selected token's holder balances when the proposal is created. The snapshot is stored in DAO contract state with a 2,048-holder limit; later token transfers do not change that proposal's voting power. Signed votes derive the voter address from the verified Ed25519 key and read the fixed snapshot weight. The generic caller-supplied-weight API rejects weighted proposals, so it cannot be used to inflate voting power; weighted proposals accept only the authenticated snapshot-vote path. Zero-snapshot-balance wallets and duplicate votes are rejected. Each weighted proposal also stores the source ledger's canonical state root and a versioned SHA-256 commitment over the token identifier and the sorted `(wallet, balance)` snapshot entries. These commitments are included in the serialized proposal and therefore in DAO contract storage state roots; legacy proposals without these optional fields remain readable. Before every weighted vote, the runtime enumerates the bounded snapshot entries in deterministic key order, validates wallet identifiers and fixed-width balances, recomputes total voting power, and recomputes the snapshot digest. A missing, malformed, altered, or total-mismatched snapshot is rejected before the vote can mutate state. The prefix scan is bounded by the existing 2,048-holder limit and its reads are charged against the vote gas budget.

```sh
qorvynt --qur-dao-propose-weighted token-ledger.json dao-state.bin community proposal1 founder "Weighted governance vote" GOLD 100 50000
qorvynt --qur-dao-vote-weighted dao-state.bin community proposal1 alice.qvk GOLD yes 100000
```

A weighted proposal is rejected if its requested quorum exceeds the total eligible wallet voting power captured in that snapshot; this prevents creating a proposal that can never reach quorum. The exact-boundary case (quorum equal to total eligible voting power) is valid. The Rust DAO API now exposes `create_weighted_proposal_at_anchor`, which accepts a `VerifiedBlockAnchor` obtained from the replay-verified block store and persists its chain ID, tip height, header hash, and chain state root alongside the weighted token snapshot. These fields are included in the proposal serialization and DAO storage commitment. The CLI now also exposes `--qur-dao-propose-weighted-anchored <ledger.json> <state.bin> <block-store> <chain-id> <dao-id> <proposal-id> <proposer> <title> <token> <quorum> <gas-limit>`. It loads the block store through replay verification, rejects an empty store, and records the verified tip anchor on the proposal. The existing CLI `--qur-dao-propose-weighted` remains the unanchored local-ledger path; callers must not treat the new metadata as proof that token balances were part of the chain's replayed state. The anchor is contextual provenance only until the token ledger is incorporated into the chain runtime and replay transition. A verified-tip accessor is now available on the local append-only block store: `BlockStore::verified_tip_anchor()` returns the chain ID, tip height, canonical header hash, and committed chain state root only from the store's accepted block prefix. Store loading independently replays every block before exposing that tip. This is an integration primitive, not yet a DAO snapshot binding: the QUR token ledger remains separate from the chain runtime, so the returned chain state root does not prove that the weighted-voting token balances were included in that block. The next integration must carry this anchor into proposal creation and, before production use, make token-ledger state part of the same replayed consensus state transition. The local block-store anchor is not a distributed finality certificate. The snapshot is currently taken from the local QUR token ledger supplied at proposal creation. Only key-derived QORVYNT wallet addresses (`zv` plus 40 lowercase/uppercase hexadecimal characters) are eligible for voting weight; reserved/system accounts such as `genesis` and `dao_<id>` are excluded because they cannot authenticate as wallet signers. The proposal quorum and yes/no totals are measured in token units, not wallet count. It is not yet anchored to a canonical block height or consensus state root, and snapshot entries are not yet generated from chain replay. Production governance must authenticate the snapshot against a finalized chain block, and the wallet signature CLI must be replaced or complemented by externally signed transaction admission.
 `BlockStore::verified_tip_anchor()` returns the chain ID, tip height, canonical header hash, and committed chain state root only from the store's accepted block prefix. Store loading independently replays every block before exposing that tip. This is an integration primitive, not yet a DAO snapshot binding: the QUR token ledger remains separate from the chain runtime, so the returned chain state root does not prove that the weighted-voting token balances were included in that block. The next integration must carry this anchor into proposal creation and, before production use, make token-ledger state part of the same replayed consensus state transition. The local block-store anchor is not a distributed finality certificate. The snapshot is currently taken from the local QUR token ledger supplied at proposal creation. Only key-derived QORVYNT wallet addresses (`zv` plus 40 lowercase/uppercase hexadecimal characters) are eligible for voting weight; reserved/system accounts such as `genesis` and `dao_<id>` are excluded because they cannot authenticate as wallet signers. The proposal quorum and yes/no totals are measured in token units, not wallet count. It is not yet anchored to a canonical block height or consensus state root, and snapshot entries are not yet generated from chain replay. Production governance must authenticate the snapshot against a finalized chain block, and the wallet signature CLI must be replaced or complemented by externally signed transaction admission.


### Root-verified execution snapshots

The Rust runtime API now exposes `encode_execution_snapshot(&ledger, &contracts)` and `decode_execution_snapshot(&value)`. The versioned `qorvynt.execution-state.v1` JSON envelope stores the canonical token-ledger JSON, a bounded canonical contract-storage snapshot encoded as lowercase hex, and the composite execution root. Restore validates the ledger's own supply/root invariants, strictly decodes the contract snapshot, recomputes the composite root, and rejects mismatches before returning either component. This gives tests and future replay code one portable state handoff instead of trusting two unrelated serialized files. It is still a local QUR+contract snapshot API: it does not yet include native chain accounts, does not alter block encoding, and must not be described as consensus replay or a production database transaction. The separate `chain_execution_state_root` helper composes native-account, QUR-ledger, and contract-storage roots, but block execution and `BlockStore::load_from_file` still need to invoke and verify that three-component transition.
