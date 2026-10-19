# Contract Execution V1 — Design and Implementation Gates

**Status:** Design proposal; not implemented protocol behavior.  
**Scope:** The first deterministic contract execution milestone for Zenvora Lang and the local chain harness.  
**Safety rule:** This document must not be interpreted as evidence that persistent contract storage, gas charging, or production authorization already exists.

## Goals

1. Make contract state part of the same committed state transition as account balances and nonces.
2. Ensure a failed transaction cannot leave partial writes behind.
3. Make execution results independent of wall-clock time, host filesystem, network access, randomized iteration order, and machine-specific behavior.
4. Charge a deterministic fee from a transaction's declared maximum fee reserve.
5. Make block replay recompute contract state, gas usage, fees, receipts, and state commitments from genesis.

## State model

The committed state is a versioned map of canonical keys to canonical values. V1 keys are:
- account balance: `account/<address>/balance`
- account nonce: `account/<address>/nonce`
- contract storage: `contract/<contract-id>/storage/<storage-key>`
- contract metadata: `contract/<contract-id>/code-hash` and `contract/<contract-id>/schema-version`

Addresses, contract IDs, storage keys, and values must have explicit byte-length limits and canonical encodings. A contract may read or write only its own storage namespace unless a future, separately reviewed capability explicitly grants access. State-root input must use lexicographically sorted canonical keys and values, length prefixes, a version/domain tag, and deterministic hashing.

The current transfer-only runtime is not yet this general state model. Existing balance/nonce roots and block-store replay remain compatibility inputs until a versioned migration is implemented.

## Transaction and authorization rules

- Authenticate the transaction signature and bind the public key to the sender address before execution.
- Validate chain ID, expiry, exact expected nonce, positive amount where applicable, and bounded resource limits.
- Reject duplicate transaction IDs.
- Contract deployment must bind the deployed code hash to the signed transaction and deterministic compiler/bytecode format version.
- A contract call must identify the target contract and entry point in canonical transaction data; it must not be inferred from display strings or logs.
- `action` may write state only through the host state interface. `view` is read-only and any attempted write fails the call.
- The transaction sender is the initial caller identity. Calls cannot forge another sender; future delegated permissions require explicit signed capabilities.
- A transaction's execution occurs on a temporary state overlay. Commit overlay, nonce, fee, and receipt together only after the defined transaction outcome has been determined. Block-level failure still rolls back the entire candidate block.

## Deterministic gas V1 proposal

Gas is an integer unit, not elapsed time. V1 must define constants in a versioned protocol table before activation. Proposed initial schedule for review:
- base transaction admission/execution: 21,000 gas
- integer arithmetic and boolean comparison: 3 gas each
- local variable read: 5 gas
- local variable write: 8 gas
- contract storage read: 100 gas
- contract storage write: 500 gas
- conditional branch: 2 gas
- direct function call: 40 gas
- emitted event with bounded payload: 375 gas plus 8 gas per payload byte

These are provisional engineering values, not activated network parameters. Before implementation is declared complete, the schedule must be tied to concrete VM opcodes and host operations, have overflow-checked arithmetic, and be covered by golden vectors. No host-language wall-clock measurement may influence gas.

Each transaction declares `gas_limit` and `max_fee`. V1 fee semantics must explicitly choose and encode a gas price or fee cap; it must not silently treat `max_fee` as both a per-gas price and a total reserve. The implementation must calculate a checked maximum reserve, reject insufficient funds before execution, stop execution deterministically when the gas limit is exhausted, and define whether a reverted transaction pays consumed gas. The receipt must include gas limit, gas used, effective gas price, total fee, execution status, and a stable error code. Fee debits and any fee destination/burn must be committed in state and included in the state root.

Until those fields and semantics are implemented and versioned, the current prototype's `max_fee` remains a reserve only; it is not a charged fee.

## Failure and rollback semantics

- Invalid signature, wrong chain ID, wrong nonce, expired transaction, duplicate transaction, or insufficient maximum reserve: reject before VM execution; do not consume nonce or charge fee.
- Runtime revert or out-of-gas: discard contract storage overlay; record a deterministic failed receipt and consume nonce/charge the protocol-defined consumed-gas fee only after fee semantics are activated.
- Invalid block commitment, invalid consensus proof, or any block-level invariant failure: discard the entire candidate block and leave the canonical runtime unchanged.
- Any integer overflow, non-canonical value, missing code, invalid bytecode, resource limit violation, or unexpected host error must fail closed rather than produce machine-dependent state.

## Persistence and replay

The persistent block store must include the versioned deployment/call transaction envelopes and any required public-key or code-artifact data. It must not trust a caller-supplied post-state. On load, replay every block from the genesis configuration and independently recompute:
1. transaction validation and authorization,
2. gas consumed and fee charged,
3. receipts and receipt root,
4. contract/account state and state root,
5. transaction root, parent linkage, height, and consensus proof.

A failed replay rejects the stored chain. A versioned snapshot may later accelerate startup, but snapshots must be checked against a trusted/replayed commitment and cannot replace replay correctness.

## Required tests before calling this milestone complete

- Persistent contract storage survives process restart and replay.
- Two independent runtimes given the same genesis and transaction sequence produce byte-identical receipts and identical state roots.
- Reordered map insertion does not change state roots.
- `view` cannot mutate state; unauthorized cross-contract reads/writes fail.
- Wrong signer, nonce gap, replayed transaction, wrong chain ID, and expired transaction are rejected.
- Arithmetic overflow, oversized keys/values, excessive call depth, instruction exhaustion, and out-of-gas fail deterministically.
- Failed calls leave no partial contract writes; fee/nonce behavior matches the versioned rule.
- Tampered receipt, gas usage, fee, code hash, state root, transaction root, or parent hash is rejected on reload.
- Two-node replay/sync tests agree on height, receipts, and state root after restart and malformed input.

## Release gate

This design is not a release. The implementation must land in small reviewed changes, with unit/integration/fuzz tests and green GitHub Actions. Contract deployment, general contract storage, charged gas, a public testnet, and mainnet must not be advertised until their respective gates pass.
