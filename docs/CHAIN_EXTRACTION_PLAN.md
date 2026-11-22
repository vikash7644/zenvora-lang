# Chain extraction inventory — Phase 1

Status: preparation only. No chain code has been moved or deleted.

QORVYNT Lang and the blockchain protocol are separate products. The language repository must retain a compiler-only build while the chain implementation is migrated to its own repository.

## Current chain-coupled modules

The root `src/main.rs` currently declares these chain modules:

- `chain_runtime.rs` — native account balances, nonces, and transaction execution.
- `chain_protocol.rs` — addresses, signed transaction and protocol types.
- `chain_consensus.rs` — consensus validation and validator logic.
- `chain_block.rs` — block structures and canonical encoding.
- `block_store.rs` — local block persistence and replay.
- `chain_rpc.rs` — local RPC surface.
- `chain_p2p.rs` — peer-to-peer networking prototype.
- `tx_pool.rs` — signed transaction pool.
- `qur.rs`, `qur_runtime.rs`, `qur_state.rs`, `qur_dao.rs` — QUR token, execution-state, and DAO prototype modules.

The same CLI entry point also contains chain-specific command dispatch. Removing module declarations alone would break compilation until those commands, imports, tests, and shared helpers are extracted.

## Safe migration sequence

1. Create and verify the destination chain repository and agree its canonical identity.
2. Inventory every chain module's imports, shared types, CLI flags, tests, fixtures, and CI references.
3. Move the chain modules and their tests as a coherent unit to the destination repository; preserve history where possible.
4. Replace language-side chain commands with a documented integration boundary or SDK dependency; do not leave dangling imports.
5. Run both repositories' full CI, examples, package builds, and security checks.
6. Only then remove the migrated chain code from this repository.

## Destination blocker

The expected destination repositories `vikash7644/fleur-network`, `vikash7644/fleur-chain`, and `vikash7644/bhyvorix-chain` were checked through GitHub and were not found. The available GitHub connection can read and modify repository contents and branches, but does not expose repository-creation administration. Therefore this branch deliberately does not delete or relocate chain code into an unverified destination.

## Completion criteria

- [ ] Destination repository exists and its default branch is confirmed.
- [ ] Chain modules, tests, fixtures, and workflows are migrated.
- [ ] QORVYNT Lang no longer compiles or ships chain-specific modules/CLI commands.
- [ ] Destination chain CI passes independently.
- [ ] QORVYNT Lang CI and release package pass independently.
