# Zenvora Self-Hosted Compiler — Engineering Gate

Status: **design baseline; implementation not yet complete**  
Target: Zenvora Lang v0.9 compiler/VM foundation. This document does not claim self-hosting has been achieved.

## Non-negotiable architecture

- Zenvora Lang remains its own project. BHYVORIX Chain remains a separate repository and consumes a versioned Zenvora compiler/VM interface.
- The final compiler implementation is written in Zenvora. Rust may only be used as a temporary bootstrap/reference implementation during the transition; it must not be represented as the final Zenvora compiler.
- The existing Rust implementation is the behavioral oracle until the Zenvora implementation passes differential tests. Do not delete the oracle before parity is demonstrated.
- The browser Playground must execute the same compiler/VM semantics; a JavaScript-only demo is not equivalent to the native engine.

## Compiler pipeline to implement

1. **Lexer** — source spans, escaped strings, comments, bounded token count, deterministic errors.
2. **Parser and AST** — expressions, declarations, conditions, loops, functions, collections, modules, and contract declarations.
3. **Name and type analysis** — scopes, duplicate/undefined names, function arity, type diagnostics, module-cycle checks.
4. **Intermediate representation** — explicit control-flow blocks and typed operands; validation before code generation.
5. **Code generation** — versioned Zenvora bytecode; stable instruction semantics and source-location mapping.
6. **Verifier** — valid jump/function targets, stack-height agreement at joins, call/return consistency, resource limits.
7. **VM** — deterministic execution, bounded resources, structured errors, standard-library calls, and metered contract execution.
8. **CLI** — `zv check`, `zv compile`, `zv run`, `zv test`, `zv fmt` (only expose commands once implemented and tested).

## Bootstrap and self-hosting stages

- **Stage A — Freeze behavior:** turn the current compiler/VM behavior into executable language fixtures and regression tests.
- **Stage B — Zenvora-written frontend:** implement the lexer/parser/AST and diagnostics in `.zv` source. Keep feature parity explicit; unsupported syntax must fail clearly.
- **Stage C — Zenvora-written compiler backend:** emit verified bytecode from the Zenvora AST/IR. Add source-vs-bytecode differential tests.
- **Stage D — Bootstrap:** compile the Zenvora compiler source with the temporary reference compiler, then run that generated compiler to compile itself again.
- **Stage E — Reproducibility:** compile twice and compare normalized artifacts; compare behavior, diagnostics, and bytecode verifier results across bootstrap stages.
- **Stage F — Remove build dependency:** demonstrate a clean build and tests using the Zenvora-produced compiler artifact without invoking the Rust compiler for language compilation. Only then can self-hosting be claimed.

## Stage-D/E bootstrap gate (current blocker)

The frontend smoke workflow is not, by itself, a self-hosting proof: `compiler/parser.zv` currently runs a fixed regression harness and emits a plan for a hard-coded `runnable_source`. The native Rust CLI still parses and executes that `.zv` file. Converting the emitted sample plan to Bytecode v2 proves the plan adapter/VM path, not that the Zenvora compiler compiled itself.

A reusable `compile_source(source)` entry point now exists in `compiler/parser.zv`. It lexes, parses, validates, lowers, emits a bytecode plan, and runs the plan verifier for caller-provided source text; CI asserts two distinct source programs produce executable output and malformed source fails during parsing. This is an incremental generic source-to-plan function, **not yet** a standalone CLI/compiler artifact: the parser file still contains a regression harness, the reference Rust runtime executes that file, and the emitted plan still requires the Rust CLI to serialize and execute canonical Bytecode v2. A self-hosting release must turn this function into a standalone compiler driver that accepts source/path input, emits a complete executable artifact for arbitrary supported programs, and reports diagnostics without the Rust frontend. CI must then:

1. Build the reference `zv` once and use it only for bootstrap stage 1.
2. Use stage 1 to compile the Zenvora-written compiler driver into a runnable artifact.
3. Run that artifact to compile the same compiler source again (stage 2), without invoking Cargo/Rust for language compilation.
4. Compile a separate fixture with both stages; compare normalized artifacts and stdout/stderr/exit status.
5. Start from a clean runner and prove the stage-2 compiler can build and execute the fixture using only the documented runtime/artifact dependencies.

Until those commands exist and pass in CI, Stage D/E/F status is **blocked / not achieved**. Re-running the current parser harness or comparing its hard-coded sample plan must not be reported as self-hosting.

## Required release gates

- Every checked-in `.zv` example executes in CI.
- Parser/compiler/VM and bytecode tests pass on the same commit; fuzz tests remain a separate signal and are not substituted for semantic tests.
- Differential tests cover expressions, branches, loops, functions/recursion, collections, modules, runtime errors, and contract state/action/view.
- Malformed inputs return bounded diagnostics rather than panic or unbounded execution.
- Compiler and VM versions, bytecode format versions, and artifact metadata are explicit.
- Release documentation states what is implemented, experimental, and not yet supported.

## Contract and ecosystem boundary

The language repository owns the contract syntax, compiler, VM interface, ABI/artifact tooling, and local deterministic contract-runtime tests. Signed chain transactions, chain consensus/finality, BVRX fees, wallet, explorer, P2P network, and testnet operations belong to the separate BHYVORIX Chain repository. Contract execution must not be described as production chain deployment until the signed transaction, authorization, fee settlement, persistent replay, and chain integration gates are tested.

## Current truth

This document establishes the engineering contract only. It does not mean the Zenvora-written compiler, self-hosted build, real browser VM, production smart-contract runtime, or v0.9.0 release has already been completed. Track implementation through source files, tests, and CI results—not workflow counts or version labels alone.
