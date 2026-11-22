# QVM standalone extraction — dependency boundary

Status: engineering plan based on the current `qorvynt/phase1-chain-separation` branch. This document does not claim a standalone crate exists.

## Current module ownership

- `src/qvm/bytecode.rs`: instruction/opcode model.
- `src/qvm/value.rs`: runtime value model.
- `src/qvm/runtime.rs`: instruction execution loop.
- `src/qvm/runtime_support.rs`: stack, collection, value-depth, and runtime-state accounting helpers.
- `src/qvm/builtins.rs`: built-in dispatch implementation.
- `src/qvm/limits.rs`: QVM resource limits.
- `src/qvm/gas.rs`: host-neutral gas-meter trait and default engineering schedule.
- `src/contract_gas.rs`: contract-facing gas meter adapter.

## Remaining coupling to remove

1. `src/qvm.rs` imports bytecode serialization/deserialization, source loading, and the host `run` function through `super`; artifact verification/loading therefore remains attached to the compiler crate.
2. `runtime.rs`, `runtime_support.rs`, and `builtins.rs` refer to `crate::qvm::...`. Their types and helper visibility are crate-scoped rather than a published library API.
3. The VM runtime currently receives verified `Op` values from the root compiler/verifier. A standalone runner needs a public, validated artifact-loading boundary and must not accept unchecked instruction streams from callers.
4. CLI output is currently supplied as a callback. Preserve this host-I/O boundary; the VM core should not print directly or depend on a terminal.
5. Gas is now trait-based, but the standalone API must define whether metering is optional, how exhaustion is surfaced, and which schedule version is attached to an artifact/execution request.

## Safe extraction sequence

1. Make the QVM core compile as a library module with explicit public API types while preserving the existing CLI behavior.
2. Move bytecode-v2 decoding and verification behind a QVM-owned `VerifiedProgram` constructor. Keep raw opcode construction private to the verifier.
3. Extract the QVM core into a workspace crate (proposed package name: `qorvynt-vm`) only after the core no longer imports compiler/parser, chain, contract, or root-binary modules.
4. Add a runner entry point that loads a verified artifact, accepts an output sink and optional `GasMeter`, and returns structured execution errors.
5. Add integration tests that build and run the VM crate independently of the compiler binary, including malformed artifact rejection, resource limits, deterministic output, and gas exhaustion.
6. Add a clean-workspace build test and package the runner before marking the standalone milestone complete.

## Non-goals for this extraction

- Do not move chain consensus, RPC, P2P, transaction-pool, or contract deployment code into QVM.
- Do not silently freeze Bytecode v2; its format remains experimental until separately reviewed.
- Do not claim Rust independence: this milestone separates the VM as a Rust library, not a self-hosted compiler.

## Verifier-gated program boundary

The current extraction adds `qvm::VerifiedProgram`, whose instruction vector is private. It can only be created by `load_verified`, which checks the QORVYNT/legacy migration header and then calls the existing Bytecode v2 decoder/verifier. Existing CLI callers retain a compatibility `load` function, while the QVM execution path consumes the verified wrapper directly. This is an API boundary inside the current root crate, not yet a separately compiled VM package.
