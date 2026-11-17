# QORVYNT compiler workspace

This directory currently contains two distinct development components:

1. **parser.qv** — the QORVYNT-language frontend. It parses source, builds and validates the compiler plan, executes plan-level regression cases, and exposes `compile_to_bytecode_v2(source)` to return canonical Bytecode v2 text for valid input. Invalid source returns an error stage and an empty bytecode string.
2. **The Rust interpreter MVP** — an isolated line-based interpreter used for syntax experiments and its own regression suite. It is not the canonical bytecode compiler or VM.

The canonical Bytecode v2 compiler, verifier, serializer, and VM implementation still live in the repository root under `src/`. GitHub Actions currently runs the QORVYNT frontend through the Rust reference host and checks its output. **This is not yet a Rust-independent bootstrap:** the QORVYNT frontend has not yet been proven to build and run its own compiler in a clean environment without the Rust host.

## QORVYNT frontend coverage

The frontend has regression coverage for:

- Source-to-plan compilation and plan verification
- Variables, expressions, control flow, loops, and collection operations
- Function declarations, recursive calls, local-name validation, duplicate parameters, and call arity
- Malformed instruction records and invalid jump targets
- Plan-to-Bytecode-v2 emission, including source containing function declarations
- The public `compile_to_bytecode_v2(source)` success and failure contract

The CI PASS marker for the public API is `QORVYNT_SOURCE_TO_BYTECODE_V2_API_PASS`. A passing marker confirms the tested source cases through the current reference-host pipeline; it does not establish native standalone execution or full language completeness.

## Rust interpreter MVP

The separate interpreter supports:

- `define name := expression` and reassignment with `name := expression`
- `show expression`
- Numbers, booleans, quoted strings, arithmetic, and comparisons
- `if { ... } otherwise { ... }` and `otherwise if`
- `repeat condition { ... }`, `break`, and `continue`
- Loop safety and nesting limits, with regression tests for nested loop control

GitHub Actions runs its regression suite independently from the canonical compiler/VM checks. No phone-local setup is required for these cloud checks.

## Remaining bootstrap work

Before calling QORVYNT self-hosted or Rust-independent, the project still needs to:

1. Remove the Rust reference host from the required source-compiler execution path.
2. Build a clean-environment bootstrap that can compile the QORVYNT compiler from QORVYNT source.
3. Verify stage-by-stage reproducibility and compare bootstrap outputs.
4. Run the complete compiler and Bytecode v2 regression suite through that bootstrapped executable.

The current project is under development. Do not treat it as a production smart-contract platform or use it to handle keys or funds.
