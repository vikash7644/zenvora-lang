# Zenvora Lang ↔ Zenvora Chain integration

## Current milestone: VM-backed transaction runtime prototype

`src/chain_runtime.rs` adds a Rust transaction harness that calls the existing Zenvora Lang compiler and VM for every transfer. The transfer policy is written as Zenvora source; the Rust host passes in the current balances and amount, executes the program, checks the VM result, and only then commits updated balances. A versioned `ZENVORA_LEDGER_V2` ledger stores genesis allocations and the receipt log; the loader replays every receipt from genesis and rejects inconsistent balance transitions, malformed records, duplicate allocations, unsupported versions, and oversized files. V1 snapshots are not accepted by this prototype loader; keep any old local snapshots backed up before testing this version.

### Local CLI for the prototype

After building the `zv` binary, the current prototype supports:

```text
zv --chain-init ledger.zvr alice 1000
zv --chain-transfer ledger.zvr alice bob 125
zv --chain-balance ledger.zvr alice
zv --chain-receipts ledger.zvr
```

`--chain-init` creates a new ledger and refuses to overwrite an existing file. To fund another address, use `--chain-init` only when creating a fresh ledger; genesis balances for multiple addresses should be added through a dedicated genesis/config format in a future milestone. Each transfer loads the snapshot, runs the transfer rule in the Zenvora VM, and saves the resulting balances and receipt. A rejected transfer is recorded as a rejected receipt without changing balances.

Automated tests cover:
- a valid transfer and subsequent transfer using the committed state;
- overspending, zero, and negative amounts;
- rejection of malformed addresses and self-transfers;
- VM arithmetic overflow without partial state mutation;
- transaction receipts and balance updates.

The CI workflow must pass before this prototype is considered verified. This is a real VM-backed transaction execution path inside the Zenvora Lang repository, but it is **not yet a full blockchain node**.

## Canonical protocol-format prototype

`src/chain_protocol.rs` now defines a versioned canonical encoding for a transaction signing payload and block header. It uses explicit big-endian integers, length-prefixed byte/string fields, and domain-separated format tags. Structural tests cover deterministic encoding, wrong-chain rejection, expiry, malformed fields, and genesis/parent-hash rules.

This is **not yet a complete transaction protocol**: signature bytes are opaque and not cryptographically verified; transaction nonces/balances are not checked by this module; roots are supplied rather than computed; there is no cryptographic transaction/block hash, block execution, persistent block database, P2P protocol, or consensus proof verification. Those are separate gates, not implied by the format structs.

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
