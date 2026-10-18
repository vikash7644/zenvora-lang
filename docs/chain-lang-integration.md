# Zenvora Lang ↔ Zenvora Chain integration

## Current milestone: VM-backed transaction runtime prototype

`src/chain_runtime.rs` adds a Rust transaction harness that calls the existing Zenvora Lang compiler and VM for every transfer. The transfer policy is written as Zenvora source; the Rust host passes in the current balances and amount, executes the program, checks the VM result, and only then commits updated balances. A versioned `ZENVORA_LEDGER_V3` ledger stores genesis allocations, transfer receipts, and signed-transaction nonce/ID metadata; the loader replays receipts from genesis and rejects inconsistent balance transitions, malformed records, duplicate allocations, non-contiguous signed nonces, duplicate transaction IDs, unsupported versions, and oversized files. V2 local ledgers remain readable and are rewritten as V3 when saved; V1 snapshots remain unsupported.

### Local CLI for the prototype

After building the `zv` binary, the current prototype supports:

```text
# Earlier VM-backed transfer ledger
zv --chain-init ledger.zvr alice 1000
zv --chain-transfer ledger.zvr alice bob 125
zv --chain-balance ledger.zvr alice
zv --chain-receipts ledger.zvr

# Signed local block-store prototype
zv --chain-keygen owner.zvk
zv --chain-store-init dev.zvb 71 owner.zvk 1000000
zv --chain-store-transfer dev.zvb 71 owner.zvk bob 125 3
zv --chain-store-balance dev.zvb 71 bob
zv --chain-store-info dev.zvb 71
```

`--chain-init` creates a new V2-compatible local transfer ledger and refuses to overwrite an existing file. The newer signed workflow creates an Ed25519 key file, initializes a height-zero genesis block, signs a transfer with the key, executes it through the Zenvora Lang VM, builds a block with transaction/receipt/state roots, appends it to the local block store, and verifies the store by replay. The CLI currently supports one genesis account per new store; multi-account genesis configuration is a later milestone.

**Key warning:** `owner.zvk` contains an unencrypted private seed. On Unix-like systems the CLI requests owner-only file permissions, but this is a development key format, not a production wallet or secure key-management system. Fees are checked as a reserve but are not charged. The block store is a single local file: this is not yet a networked testnet, consensus is not implemented, and no real funds should be used.

Automated tests cover:
- a valid transfer and subsequent transfer using the committed state;
- overspending, zero, and negative amounts;
- rejection of malformed addresses and self-transfers;
- VM arithmetic overflow without partial state mutation;
- transaction receipts and balance updates.

The CI workflow must pass before this prototype is considered verified. This is a real VM-backed transaction execution path inside the Zenvora Lang repository, but it is **not yet a full blockchain node**.

## Canonical protocol-format and hashing prototype

`src/chain_protocol.rs` defines versioned canonical encodings for transaction signing payloads, signed transaction envelopes, and block headers. Encodings use explicit big-endian integers, length-prefixed fields, and distinct domain tags. Bounded decoders reject truncated inputs, trailing bytes, invalid versions, malformed fields, and oversized inputs.

The module uses `sha2` for domain-separated SHA-256 transaction IDs, block-header hashes, and transaction Merkle roots (distinct leaf/node tags, a tagged empty-tree root, and duplicate-last handling for odd-width levels). It also uses `ed25519-dalek` to verify Ed25519 signatures against the canonical signing payload and bind the supplied 32-byte public key to a deterministic `zv` + 40-hex-character sender address. Domain tags use NUL terminators. `validate_signed_transfer` combines network/expiry validation, Ed25519 verification, exact expected-nonce comparison, overflow-safe amount-plus-fee-reserve arithmetic, and an available-balance check. It returns the required reserve without mutating state or consuming a nonce. Tests cover deterministic encoding/hashes, decoder round trips and malformed inputs, order-sensitive Merkle roots, signature success and tampering, wrong keys, malformed key sizes, address derivation, nonce mismatch, insufficient balance, wrong chain, expiry, and arithmetic overflow. The full CI checkpoint passed: [GitHub Actions run 39145856522](https://github.com/vikash7644/zenvora-lang/actions/runs/39145856522).

The runtime exposes deterministic state roots over sorted balances/nonces and a receipt Merkle root over canonical receipt fields, alongside the protocol module's transaction Merkle root and block-header hash. `src/chain_block.rs` adds an atomic block-execution prototype: it checks chain ID, genesis/parent linkage, contiguous heights, nondecreasing timestamps, transaction root, executed receipt root, and resulting state root. Transactions run against a cloned runtime; the live state is replaced only after every transaction and root check succeeds. Tests cover valid sequential blocks, bad roots, invalid transactions, parent linkage, and atomic rollback. CI passed: [GitHub Actions run 39146464770](https://github.com/vikash7644/zenvora-lang/actions/runs/39146464770).

`src/block_store.rs` adds an append-only local block store for genesis allocations, canonical headers, signed transaction envelopes, and public keys. Loading replays every stored block through the atomic execution path, so broken linkage, altered signatures/transactions, or mismatched transaction/receipt/state roots reject the store. The replay/tamper tests passed in [GitHub Actions run 39146669677](https://github.com/vikash7644/zenvora-lang/actions/runs/39146669677).

This is **not yet a complete transaction protocol or chain**. The V3 VM-backed ledger persists signed transaction IDs and per-account nonces and reconstructs them by replaying the receipt log; the earlier V2 format remains readable for local unsigned transfer history. The block store is local-file persistence, not a networked node or consensus engine; consensus proofs and proposer authorization remain opaque and unverified. The runtime does not yet charge fees or enforce a full gas schedule. P2P, fork choice, finality, snapshots, RPC, and production security review remain separate milestones. Hashing, signature verification, nonce persistence, root calculation, atomic block execution, and replay-verified local persistence are building blocks, not proof of consensus or production security.

## Intended architecture

1. **Zenvora Lang source** — contract logic and developer-facing syntax.
2. **Zenvora compiler** — parse and validate source, then emit versioned Zenvora bytecode.
3. **Zenvora VM/runtime** — deterministic execution with instruction limits and a narrowly defined host API.
4. **Zenvora Chain node** — validates signed transactions, executes contract calls against a transactional state overlay, commits state only on success, and records receipts/events.
5. **RPC, wallet, faucet and explorer** — client interfaces around the node.

## Next implementation gates

- Replace generated transfer-rule source with compiled contract artifacts and a typed contract ABI.
- Add transaction context and explicit state read/write host interfaces to the VM.
- Add persistent state storage and replay/recovery tests.
- Add signed transaction validation, nonce/replay protection, fee accounting and block/receipt encoding.
- Define and implement consensus only after deterministic execution and state transitions are stable.
- Add fuzzing, verifier tests, resource accounting, reproducible builds, and an external security review before any public testnet with value.

## Safety/status

The runtime has a replay-validated V2 local ledger format, but does not yet implement a block database, consensus, cryptographic signatures, a deployable ABI, production wallet security, or a public testnet. Do not use it for real funds. The JavaScript devnet branch remains a separate API/protocol prototype and must not be described as executing Zenvora Lang contracts.
