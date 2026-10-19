# Persistent contract runtime: implementation gate

Status: planned work, not implemented by this document.

## Current boundary

The first-generation `contract/state/action/view` syntax is lowered into ordinary compiler declarations and functions. The current `ChainRuntime` transfer path executes a built-in `Transfer` source program against balances supplied by the Rust host. This is useful prototype coverage, but it is not general contract deployment or per-contract persistent storage.

## Required execution model

1. **Canonical identity:** derive a contract ID from chain ID, sender address, sender nonce, and canonical deployment bytecode/source hash using a domain-separated hash. Reject malformed or non-canonical input.
2. **Stored code:** store immutable deployed bytecode by contract ID; verify it with the bytecode verifier before accepting deployment.
3. **Isolated storage:** key state by (contract ID, storage key), never by process-global variable names. Define canonical value encoding and bounded storage reads/writes.
4. **Invocation context:** expose caller, contract ID, chain ID, transaction ID, and execution limits as explicit host inputs. Do not allow contracts to choose or spoof caller identity.
5. **Atomic execution:** execute against a staged state overlay. Commit code/storage, nonce, balances, receipts and roots only if the whole transaction succeeds; on trap/revert, discard staged changes while recording a deterministic failure receipt according to the protocol.
6. **Authorization and value safety:** validate caller signatures/nonces before VM entry; use checked arithmetic; define whether nested calls and value transfers exist before implementing them.
7. **Determinism:** prohibit filesystem, network, clock, random, environment, and host-dependent behavior inside contract execution. Define gas/instruction accounting and deterministic failure semantics.
8. **Versioning:** bind contract bytecode format and runtime semantics to explicit versions; reject unsupported versions rather than silently reinterpret code.

## Acceptance tests

- Deploy valid bytecode; verify its ID and code survive process restart and replay.
- Reject malformed, oversized, unverifiable, and unsupported-version bytecode without changing state.
- Invoke an action; verify only that contract's storage changes and its state root is reproducible.
- Run a view; verify it cannot mutate storage, balances, nonces, or receipts.
- Fail after one or more staged writes; verify no partial writes survive.
- Reject unsigned, wrong-chain, wrong-nonce, replayed, unauthorized, and malformed calls.
- Verify deterministic results and identical state/receipt roots across two fresh nodes replaying the same genesis and transactions.
- Fuzz deployment decoding, storage encoding, invocation inputs, and failure/rollback boundaries.
- Run the complete compiler/VM, chain, fuzz, formatting, and lint CI suites before merge.

## Explicit non-goals for the first milestone

No claim of public testnet readiness, production wallet security, full BFT consensus, fee charging, nested contract calls, or mainnet safety until separately implemented and verified. Development key files currently contain an unencrypted private seed; do not use real funds.
