# Zenvora Lang ↔ Zenvora Chain integration plan

## Current milestone: language-level transaction rules

`examples/chain_transfer.zv` runs in the existing Rust Zenvora compiler/VM. It checks a tiny ledger model:
- a valid transfer debits the sender and credits the recipient;
- overspending is rejected without changing balances;
- zero and negative transfer amounts are rejected.

The example is an executable language regression test, **not yet a blockchain transaction executor**. It has in-memory example balances and does not call the JavaScript devnet or persist chain state.

## Intended architecture

1. **Zenvora Lang source** — contract logic and developer-facing syntax.
2. **Zenvora compiler** — parse and validate source, then emit versioned Zenvora bytecode (or later a defined WebAssembly target).
3. **Zenvora VM/runtime** — deterministic execution with instruction/gas limits and a narrowly defined host API.
4. **Zenvora Chain node** — validates signed transactions, executes contract calls against a transactional state overlay, commits state only on success, and records receipts/events.
5. **RPC, wallet, faucet and explorer** — client interfaces around the node.

## Next implementation gates

- Add a typed contract ABI and compile contract source into a deployable artifact.
- Define deterministic host calls for storage, caller, block context, events, and gas metering.
- Add transaction context and state read/write interfaces to the Rust VM; reject nondeterministic capabilities.
- Write Rust integration tests that deploy a contract, invoke actions, inspect receipts, and prove failed calls roll back state.
- Choose a chain-node architecture and replace the JavaScript prototype only after the Rust execution path is tested.
- Add fuzzing, verifier tests, resource accounting, reproducible builds, and an external security review before any public testnet with value.

## Safety/status

This milestone does not implement consensus, a deployable contract ABI, persistent contract storage, production wallet security, or a public testnet. Do not use it for real funds. The JavaScript devnet branch remains a separate API/protocol prototype and must not be described as executing Zenvora Lang contracts.
