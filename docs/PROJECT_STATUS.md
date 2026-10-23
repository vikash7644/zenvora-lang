# Zenvora Lang — Mission Status and Completion Gate

Last audited: 2026-10-23  
Scope: `vikash7644/zenvora-lang` on `main`. This ledger is a working checklist, not a claim that the project is complete.

## Status meanings

- **Implemented, needs current CI proof**: code and tests exist; the current revision still needs green CI.
- **Prototype / partial**: useful functionality exists, but known limitations remain.
- **Pending**: not complete enough to claim release-ready.
- **Out of scope for Zenvora Lang**: chain-specific work must be handled in the separate BHYVORIX Chain repository after that repository is verified.

## Current gate: Bytecode v2

- [x] Add regression tests for malformed instruction records, invalid jump operands, builtin stack underflow, `RETURN` in main, function fallthrough, invalid parameter identifiers, and parser/serializer round trips.
- [x] Add source-vs-bytecode-v2 execution comparison tests for collections/builtins/functions, branch/loop output, `break`/`continue`, recursive returns, and runtime-error parity.
- [x] Fix escaped newline sequences in the regression fixtures (commit `b340e2a02240943836d4e2325edb7ea96ade5445`).
- [x] Correct both source-vs-v2 regression tests to call the v2 decoder, not the legacy v1 decoder (commit `38b701ad9758f9a5fd85dc2a6ee20ed56ce1d67e`).
- [x] Historical baseline: main compiler/VM CI passed on commit `ef45ff34dc05003d58db468c194b4f31413babec`; newer verification is recorded below.
- [x] Historical baseline: bounded parser/verifier/serializer fuzz campaign passed on commit `ef45ff34dc05003d58db468c194b4f31413babec`; newer fuzz verification is tracked below.
- [x] No compile, Clippy, unit-test, fuzz, or example failures were reported by these two workflows on that commit.
- [x] Verify verifier coverage for jump/function boundaries, stack underflow, stack depth at control-flow joins, call target/argument counts, return-kind consistency, unreachable regions, and resource limits (regression cases are in `src/bytecode_v2.rs`).
- [x] Review/add differential coverage for nested calls, strings, collections, short-circuit branches, loops, `break`/`continue`, recursive returns, and runtime-error parity.
- [x] Historical bounded Bytecode v2 fuzz campaign passed on source commit `d61a4a37d71e8a2d3fde8dc0b7fb5e33b93fa95b` (run [39365146054](https://github.com/vikash7644/zenvora-lang/actions/runs/39365146054)).
- [x] Bounded Bytecode v2 fuzz campaign passed on compiler-source SHA `38a3afe778aeb3e68693813b448a4b2d52375473` (run [39368477353](https://github.com/vikash7644/zenvora-lang/actions/runs/39368477353)). Later changes only updated test scripts/workflow/docs, not compiler source.
- [ ] Do not call Bytecode v2 stable or production-safe; its format is experimental.

Main CI passed on commit `b818e1dabd7b1ebfb2e0be4fcc8900fd5077e939` (run [39368838438](https://github.com/vikash7644/zenvora-lang/actions/runs/39368838438)). It passed compiler/VM checks, Clippy, all tests including contract runtime and artifact ABI tests, optimized CLI build, `cargo package --locked`, compiler API smoke tests, editor validation, VSIX packaging/upload, all checked-in `.zv` examples, persistent contract CLI rollback/gas tests, and contract ABI/bytecode artifact CLI integration. The bounded Bytecode v2 fuzz campaign passed on compiler-source SHA `38a3afe778aeb3e68693813b448a4b2d52375473` (run [39368477353](https://github.com/vikash7644/zenvora-lang/actions/runs/39368477353)); subsequent changes only touched scripts, workflow wiring, and documentation.

## Contract runtime implementation slice (2026-10-20)

- Added `src/contract_runtime.rs`: the Rust compiler/VM now executes contract `action` and `view` calls against persisted integer state. Actions commit through isolated state overlays; failed execution/out-of-gas leaves prior state unchanged; views do not commit and cannot invoke actions.
- The VM has an opt-in deterministic gas-metered execution path: base gas is charged once, then each executed opcode is charged using the versioned integer schedule. This is an experimental local schedule, not an activated blockchain protocol parameter.
- Added `zv --contract-run <file.zv> <contract-id> <snapshot.bin> <action|view> <method(args)> [gas-limit]` with versioned atomic snapshots. CI verifies persistent actions, view results, gas reporting, and no snapshot change on out-of-gas.
- Added `replay_block` to replay ordered contract calls on a candidate state and publish it only when every call succeeds and the final state root matches the supplied commitment. This is a local contract-call replay harness, not full signed block/consensus replay.
- Added `zv --contract-artifact <contract.zv> <artifact.json>`: emits ABI metadata, source SHA-256, compiler version, and verifier-checked Bytecode v2. ABI output is experimental and currently reports state as integer and method types as dynamic; it is not a chain deployment format.
- Storage/gas tests, contract action/view tests, artifact tests, and the CLI integration test passed in main CI run [39368613130](https://github.com/vikash7644/zenvora-lang/actions/runs/39368613130). Bytecode v2 fuzz is still running on matching compiler-source revision [39368477353](https://github.com/vikash7644/zenvora-lang/actions/runs/39368477353).
- Still pending: signed contract-call transaction envelope and signer authorization, fee reservation/settlement, chain-specific transaction receipts and replay integration, broad typed state (arrays/maps/strings), real-compiler browser playground backend, deployment ABI freeze and on-chain deployment/verification, independent security review. BHYVORIX Chain must live in its own repository; `vikash7644/bhyvorix-chain` has not been created/verified yet.

## Language feature and release checklist

| Area | Current assessment | Remaining completion gate |
|---|---|---|
| Variables, integer values, booleans, strings, operators | Implemented in compiler/VM; current revision still requires CI proof | Confirm all operators, overflow/division errors, diagnostics, and edge cases in tests |
| if/otherwise, repeat, break/continue | Implemented | Verify nested control flow, early exits, loop safety, and VM parity |
| Functions and return values | Implemented | Verify recursion, argument errors, local/global binding rules, and all return paths |
| Arrays, maps, indexing, mutation helpers | Implemented | Verify bounds/type errors, nested values, size limits, and source/VM equivalence |
| Standard library | Broad set implemented | Inventory every builtin against tests and docs; check errors, overflow, and bounds |
| Relative modules | Implemented with regression tests | Relative paths, absolute/drive-prefixed path rejection, duplicate imports, cycles, missing modules, malformed directives, UTF-8, per-file/depth limits, and expanded-size limits are tested |
| Bytecode v1 | Existing format/loader | Keep compatibility tests and document its limits; do not confuse it with v2 |
| Bytecode v2 | Experimental; current main CI passes regression tests; bounded fuzz run still in progress | Keep differential/property coverage growing; format freeze and independent security review remain required before stability claims |
| Contract/state/action/view syntax | Integer state/actions/views now execute through Rust VM with atomic local snapshots and deterministic gas; local replay-root checks and experimental ABI artifact exist | Signed transaction authorization, fee settlement, chain replay integration, typed state, deployable ABI freeze, on-chain deployment and independent review remain pending |
| CLI/project scaffolding | Implemented with regression tests | `--new` creates runnable files, validates names, and refuses to overwrite an existing project path |
| Browser playground | Separate JavaScript demo with regression-tested variables, expressions, branching, loops, functions/recursion, collections, and selected builtins; not the Rust compiler/VM | Keep the limitations visible; build a real WASM or secure backend integration and differential-test it against the compiler |
| Websites, games, AI agents | Not supported as full first-class capabilities yet | Needs standard libraries/runtime APIs, project templates, rendering/game support, and safe tool/network integration |
| Documentation and examples | Existing docs/examples; CI now executes every checked-in `.zv` example | Continue reconciling feature claims with executable tests and clearly label prototype limitations |
| Editor support | VS Code `.zv` language registration, TextMate grammar, bracket/comment configuration, and automated metadata/regex validation implemented; CI passed on `bc9bda48c013183424e98c6d695a590264ced6cc` | Language server, diagnostics, completion, debugging, Marketplace packaging/publishing, and upstream GitHub Linguist recognition are still pending |
| Release readiness | Not complete | All required CI green on one commit, examples verified, security/limitations reviewed, release checklist completed |

## Latest verification (2026-10-20)

- Main CI passed on commit `b818e1dabd7b1ebfb2e0be4fcc8900fd5077e939` (run [39368838438](https://github.com/vikash7644/zenvora-lang/actions/runs/39368838438)). This includes compiler/VM check, Clippy, all tests, optimized CLI build, full `cargo package --locked` verification, every checked-in `.zv` example executed through the optimized release CLI, bytecode v1/v2 smoke tests, compiler API smoke tests, editor-support validation, VSIX packaging/upload, contract CLI persistence/gas/rollback, and ABI/bytecode artifact CLI integration.
- Interpreter MVP and editor-validation jobs passed on the same commit. The compiler change removes an unnecessary unconditional jump after a terminating `if` branch and computes function fallthrough before relocating local jump targets. This fixed a Bytecode v2 rejection of recursive functions with value returns in both `if/otherwise` branches.
- Added differential Bytecode v2 tests for nested calls, map/array built-ins, runtime-error parity, `break`/`continue`, and recursive returns. Main CI passes on the current source revision.
- VS Code editor source is in `editors/vscode/`; the extension is not yet published to the Marketplace, and GitHub Linguist recognition remains an upstream task.
- Bounded Bytecode v2 fuzz passed on compiler-source SHA `38a3afe778aeb3e68693813b448a4b2d52375473` (run [39368477353](https://github.com/vikash7644/zenvora-lang/actions/runs/39368477353)). CI caught and helped fix a contract artifact test-fixture newline bug and an incorrect persistent CLI expected value. Stable v1 remains blocked by signed contract transactions/fees, chain replay integration, real-compiler playground, production ABI/deployment, and independent security review.

## Latest verification (2026-10-23)

- Commit `fc3ea24e7b96ed7b62a5c44b29b7b986a37de173` added a reserved-parameter-identifier regression test but its first CI run failed because the test referenced the private helper without the `super::` module path.
- Fixed the test scope in commit `ee833344bbb4a327532b8b2cceee9c283fcba841`.
- Main CI passed on `ee833344bbb4a327532b8b2cceee9c283fcba841` (run [39933749386](https://github.com/vikash7644/zenvora-lang/actions/runs/39933749386)).
- The bounded Bytecode v2 fuzz workflow for that same SHA is still in progress (run [39933749441](https://github.com/vikash7644/zenvora-lang/actions/runs/39933749441)); do not mark the fuzz gate green until its conclusion is successful.

- Browser playground map handling was hardened so keys such as `__proto__` and `constructor` are stored as ordinary map keys and inherited object properties are not mistaken for user keys. Regression coverage added in commit `c4434455c810e99e44f23eeb99f37c2c51db218b`; playground CI passed on that commit (run [39934086610](https://github.com/vikash7644/zenvora-lang/actions/runs/39934086610)). This remains a browser syntax demo, not the real Rust compiler backend.

- Added a contract replay regression test proving that if a later call fails, earlier candidate writes are not published; commit `c5a909b578b11cf0ba5e0e504aecb4493d3a1344`. CI for this new commit is pending, so this test is not yet recorded as passing.

## Required CI evidence before calling the current milestone complete

1. Main workflow is green on the latest `main` SHA, including `cargo check --locked --all-targets`, Clippy, all tests, optimized release build, `cargo package --locked`, examples, CLI checks, bytecode v1/v2 smoke tests, compiler API smoke tests, and editor metadata/regex validation.
2. Bytecode v2 fuzz workflow is green on that same SHA (or its explicitly documented matching source SHA). Historical run passed on `d61a4a37d71e8a2d3fde8dc0b7fb5e33b93fa95b`; current source SHA `ee833344bbb4a327532b8b2cceee9c283fcba841` is still pending.
3. Any red check has a root cause, a committed fix, and a rerun showing green.
4. No claim of production readiness based only on unit tests or a queued workflow.

## Scope order

1. Keep the Bytecode v2 regression and bounded-fuzz gate green; the format remains experimental.
2. Continue the compiler/VM feature-by-feature audit and fix gaps found by tests.
3. Close module, standard-library, CLI, docs, and example mismatches.
4. Replace the demo-only playground execution path with a secure real-compiler integration and test parity.
5. Implement and independently test contract state persistence, atomic rollback, authorization, deterministic gas, ABI, and deployment before describing contracts as production-ready.
6. Finish editor/LSP and release packaging, then complete the release checklist.
7. Only after Zenvora Lang's release gate is met, move chain work to the separate BHYVORIX Chain repository. Do not mix chain branding into the Zenvora Lang name.

## Continuity note

At each work session, read this file and inspect the latest Actions runs before choosing the next unfinished task. Update this ledger only when there is concrete evidence of progress. A queued run is not a passing run, and a code commit is not the same as a verified feature.
