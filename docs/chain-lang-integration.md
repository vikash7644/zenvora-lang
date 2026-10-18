# Zenvora Lang ↔ Zenvora Chain integration

## Current milestone: VM-backed transaction runtime prototype

`src/chain_runtime.rs` adds a Rust transaction harness that calls the existing Zenvora Lang compiler and VM for every transfer. The transfer policy is written as Zenvora source; the Rust host passes in the current balances and amount, executes the program, checks the VM result, and only then commits updated balances.

Automated tests cover:
- a valid transfer and subsequent transfer using the committed state;
- overspending, zero, and negative amounts;
- rejection of malformed addresses and self-transfers;
- VM arithmetic overflow without partial state mutation;
- transaction receipts and balance updates.

The CI workflow must pass before this prototype is considered verified. This is a real VM-backed transaction execution path inside the Zenvora Lang repository, but it is **not yet a full blockchain node**.

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

The runtime currently models an in-memory ledger and receipt list. It does not implement consensus, cryptographic signatures, persistent chain state, a deployable ABI, production wallet security, or a public testnet. Do not use it for real funds. The JavaScript devnet branch remains a separate API/protocol prototype and must not be described as executing Zenvora Lang contracts.
