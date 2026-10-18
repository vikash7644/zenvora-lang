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
# Persisted transaction-pool flow
zv --chain-rpc-pool dev.zvb 71 pending.zvp 8545
zv --chain-store-produce dev.zvb 71 pending.zvp owner.zvk
```

`--chain-init` creates a new V2-compatible local transfer ledger and refuses to overwrite an existing file. The newer signed workflow creates an Ed25519 key file, initializes a height-zero genesis block, signs a transfer with the key, executes it through the Zenvora Lang VM, builds a block with transaction/receipt/state roots, appends it to the local block store, and verifies the store by replay. The single-account initializer remains available, and `--chain-store-init-multi` accepts a multi-account genesis config. Genesis headers use a deterministic timestamp so stores initialized from the same chain ID and allocations share the same genesis hash.

### Multi-validator genesis configuration

Create one key file per validator with `zv --chain-keygen validator-1.zvk`, then copy the printed addresses into a text file such as `genesis.txt`:

```text
ZENVORA_GENESIS_V1
chain_id=71
<address-printed-for-validator-1> 1000000
<address-printed-for-validator-2> 1000000
<address-printed-for-validator-3> 1000000
<address-printed-for-validator-4> 1000000
```

Replace each placeholder with the exact `zv...` address printed by key generation; do not include angle brackets. The address and balance are whitespace-separated. Replace each placeholder with the entire exact address printed by `--chain-keygen` (including its `zv` prefix). The config accepts up to 42 funded public-key-derived validator addresses, plus optional ordinary genesis allocations.

```text
zv --chain-store-init-multi dev.zvb 71 genesis.txt
# For four validators, provide at least three distinct authorized validator keys:
zv --chain-store-transfer-multi dev.zvb 71 validator-1.zvk bob 125 3 validator-1.zvk validator-2.zvk validator-3.zvk
```

The transaction key must control a funded account. The trailing validator key files must match genesis validators and meet the 2/3+1 quorum. For a four-validator set, two signatures are insufficient; three are required. The CLI does not coordinate these signatures over the network—you supply the validator key files locally.

**Key warning:** `owner.zvk` contains an unencrypted private seed. On Unix-like systems the CLI requests owner-only file permissions, but this is a development key format, not a production wallet or secure key-management system. Fees are checked as a reserve but are not charged. The block store is a local file and the CLI currently runs a single validator. A quorum-signature prototype exists, but full BFT consensus/finality and production key management are not implemented. No real funds should be used.

Automated tests cover:
- a valid transfer and subsequent transfer using the committed state;
- overspending, zero, and negative amounts;
- rejection of malformed addresses and self-transfers;
- VM arithmetic overflow without partial state mutation;
- transaction receipts and balance updates.

The CI workflow must pass before this prototype is considered verified. This is a real VM-backed transaction execution path inside the Zenvora Lang repository, but it is **not yet a full blockchain node**.

## Loopback read-only JSON-RPC

Run the local query server against a replay-verified block store:

```text
zv --chain-rpc dev.zvb 71
zv --chain-rpc dev.zvb 71 8545
```

The service binds only to `127.0.0.1` and supports JSON-RPC 2.0 methods:
- `chain_getInfo` — chain ID, block count, tip height/hash, state root, and maturity labels.
- `chain_getBalance` — account balance and next expected nonce.
- `chain_getBlock` — block header commitments and transaction summaries by height.
- `chain_getTransaction` — signed transaction details by transaction ID.

Example request body:

```json
{"jsonrpc":"2.0","id":1,"method":"chain_getInfo","params":[]}
```

With a persisted transaction pool enabled, use `zv --chain-rpc-pool dev.zvb 71 pending.zvp 8545`. This mode additionally supports:
- `chain_submitTransaction` — parameters `publicKey` and `transaction`, both lowercase hex; the transaction field is the canonical signed `TransactionV1` envelope.
- `chain_getPendingTransactions` — list pending transaction IDs and basic fields.

The pool verifies the transaction signature, chain ID, expiry, sender nonce, and balance/fee reserve against a cloned runtime with earlier pool entries replayed. It persists to a bounded local file scoped to the next block height. Produce a block with `zv --chain-store-produce dev.zvb 71 pending.zvp validator-1.zvk ...`; supply enough distinct genesis-authorized validator keys to meet the quorum. After a successful block commit, the producer resets the pool for the next height. This is manual local submission and block production; transaction gossip and automatic validator coordination are not implemented.

This is read-only and reloads/replays the local block store for each request. It is deliberately not a public RPC endpoint and has no authentication layer because it only binds loopback. Do not expose it through a tunnel or public interface. HTTP requests are size-bounded; chunked transfer encoding and non-POST requests are rejected. The RPC does not submit transactions or implement peer networking.

## TCP peer status and verified block synchronization

Two local nodes can exchange blocks over a length-prefixed TCP protocol:

```text
# Node A: create the genesis store and serve it on a trusted network interface
zv --chain-keygen owner.zvk
zv --chain-store-init node-a.zvb 71 owner.zvk 1000000
zv --chain-p2p-serve node-a.zvb 71 0.0.0.0 9000

# Node B: use the exact same genesis key/config, then sync from Node A
zv --chain-store-init node-b.zvb 71 owner.zvk 1000000
zv --chain-p2p-sync node-b.zvb 71 192.168.1.10:9000
zv --chain-store-info node-b.zvb 71
```

The protocol supports peer status and fetching a block by height. Sync verifies the chain ID, exact genesis header hash and state root, header linkage, canonical transaction encodings, signatures, nonce/balance rules, and transaction/receipt/state roots by replaying blocks before persisting them. Sync is append-only: it refuses a peer that is behind or has a different tip at the same height; fork choice and reorganization are not implemented.

**Network warning:** the peer service has no peer authentication, fork choice, or full consensus/finality. Block headers now require genesis-authorized validator quorum signatures, but the CLI currently supports only one genesis validator and does not implement a BFT round protocol. Binding to `0.0.0.0` exposes it on reachable interfaces; use only on a trusted private network and never on the public internet. This is a block-sync prototype, not a secure public testnet.

## Validator signatures and quorum prototype

`src/chain_consensus.rs` defines a domain-separated block-header signing payload that excludes the proof itself, avoiding a circular signature/hash dependency. Each proof record contains a 32-byte Ed25519 public key and 64-byte signature; records are sorted by the derived validator address and duplicates are rejected. Validators are derived from positive-balance genesis allocations whose addresses match the public-key-derived `zv...` address format. The proposer must sign, and the prototype requires a `floor(2n/3)+1` quorum, with a maximum of 42 validators under the current header-size limit. Genesis is trusted and has no consensus proof.

Block execution and block-store replay now verify this proof before accepting non-genesis blocks. Tests cover single-validator verification, a 3-of-4 quorum, duplicate signatures, unsigned blocks, non-validator signers, and signature invalidation after header mutation. CI passed: [GitHub Actions run 39148065358](https://github.com/vikash7644/zenvora-lang/actions/runs/39148065358).

**Important limitation:** this is validator authorization and quorum-signature verification, not a complete BFT consensus protocol. There is no leader election, round/view changes, timeout recovery, equivocation handling, slashing, fork choice, or finality protocol. The multi-validator CLI is a local block-production/testing tool; it does not provide validator networking, automated voting, or fault-tolerant consensus.

## Canonical protocol-format and hashing prototype

`src/chain_protocol.rs` defines versioned canonical encodings for transaction signing payloads, signed transaction envelopes, and block headers. Encodings use explicit big-endian integers, length-prefixed fields, and distinct domain tags. Bounded decoders reject truncated inputs, trailing bytes, invalid versions, malformed fields, and oversized inputs.

The module uses `sha2` for domain-separated SHA-256 transaction IDs, block-header hashes, and transaction Merkle roots (distinct leaf/node tags, a tagged empty-tree root, and duplicate-last handling for odd-width levels). It also uses `ed25519-dalek` to verify Ed25519 signatures against the canonical signing payload and bind the supplied 32-byte public key to a deterministic `zv` + 40-hex-character sender address. Domain tags use NUL terminators. `validate_signed_transfer` combines network/expiry validation, Ed25519 verification, exact expected-nonce comparison, overflow-safe amount-plus-fee-reserve arithmetic, and an available-balance check. It returns the required reserve without mutating state or consuming a nonce. Tests cover deterministic encoding/hashes, decoder round trips and malformed inputs, order-sensitive Merkle roots, signature success and tampering, wrong keys, malformed key sizes, address derivation, nonce mismatch, insufficient balance, wrong chain, expiry, and arithmetic overflow. The full CI checkpoint passed: [GitHub Actions run 39145856522](https://github.com/vikash7644/zenvora-lang/actions/runs/39145856522).

The runtime exposes deterministic state roots over sorted balances/nonces and a receipt Merkle root over canonical receipt fields, alongside the protocol module's transaction Merkle root and block-header hash. `src/chain_block.rs` adds an atomic block-execution prototype: it checks chain ID, genesis/parent linkage, contiguous heights, nondecreasing timestamps, transaction root, executed receipt root, and resulting state root. Transactions run against a cloned runtime; the live state is replaced only after every transaction and root check succeeds. Tests cover valid sequential blocks, bad roots, invalid transactions, parent linkage, and atomic rollback. CI passed: [GitHub Actions run 39146464770](https://github.com/vikash7644/zenvora-lang/actions/runs/39146464770).

`src/block_store.rs` adds an append-only local block store for genesis allocations, canonical headers, signed transaction envelopes, and public keys. Loading replays every stored block through the atomic execution path, so broken linkage, altered signatures/transactions, or mismatched transaction/receipt/state roots reject the store. The replay/tamper tests passed in [GitHub Actions run 39146669677](https://github.com/vikash7644/zenvora-lang/actions/runs/39146669677).

This is **not yet a complete transaction protocol or chain**. The V3 VM-backed ledger persists signed transaction IDs and per-account nonces and reconstructs them by replaying the receipt log; the earlier V2 format remains readable for local unsigned transfer history. The block store is local-file persistence, not a production node. Validator quorum signatures are verified, but full BFT consensus, peer authentication, fork choice, reorganization, finality, snapshots, fee charging, a full gas schedule, multi-validator genesis tooling, and production security review remain unfinished. Hashing, signature verification, nonce persistence, root calculation, atomic block execution, quorum verification, and replay-verified local persistence are building blocks, not proof of production security.

## Intended architecture

1. **Zenvora Lang source** — contract logic and developer-facing syntax.
2. **Zenvora compiler** — parse and validate source, then emit versioned Zenvora bytecode.
3. **Zenvora VM/runtime** — deterministic execution with instruction limits and a narrowly defined host API.
4. **Zenvora Chain node** — validates signed transactions, executes contract calls against a transactional state overlay, commits state only on success, and records receipts/events.
5. **RPC, wallet, faucet and explorer** — client interfaces around the node.

## Next implementation gates

- Add validator discovery/communication and an automated multi-validator round/vote workflow.
- Implement a full consensus round protocol: validator-set configuration, proposal/vote messages, timeouts, equivocation handling, fork choice, and explicit finality rules.
- Add transaction fee charging, gas metering, per-operation cost schedule, and deterministic execution limits.
- Add transaction gossip, authenticated RPC for non-loopback deployments, peer authentication, rate limits, and abuse controls.
- Add indexed block/receipt storage, snapshots, recovery tests, and state/receipt proof APIs.
- Replace the illustrative transfer rule with compiled Zenvora contract artifacts and a typed contract ABI/state host interface.
- Add fuzzing, property tests, multi-node adversarial tests, reproducible builds, and an external security review before any public testnet with value.

## Safety/status

The repository now has a local signed-transfer workflow, deterministic block commitments, atomic block execution, replay-verified local block storage, read-only loopback JSON-RPC, a TCP block-sync prototype, and genesis-authorized validator quorum-signature checks. It still does not have a secure decentralized consensus protocol, public-testnet readiness, production wallet/key management, or a deployable audited contract ABI. Do not use it for real funds. The JavaScript devnet branch remains a separate API/protocol prototype and must not be described as executing Zenvora Lang contracts.
