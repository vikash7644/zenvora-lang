# QORVYNT compiler and interpreter

This folder contains an isolated line-based Rust interpreter used for syntax experiments and regression tests. The reference Rust VM lives at the repository root (`src/main.rs`). The self-hosted frontend and bootstrap sources use the canonical `.qv` extension: `parser.qv`, `bootstrap.qv`, and `fixtures/runnable_sample.qv`. GitHub Actions verifies source-to-Bytecode-v2 parity and the Stage 1 → Stage 2 → Stage 3 bootstrap; no phone-local setup is required.

## Implemented in this MVP

- `define name := expression` and reassignment with `name := expression`
- `show expression`
- Numbers, booleans, quoted strings, arithmetic and comparisons
- `if { ... } otherwise { ... }` and `otherwise if`
- `repeat condition { ... }`, `break`, and `continue`
- A one-million-iteration loop safety cap and nesting limit
- `break` and `continue` are rejected outside loops with clear errors
- Tests for expression precedence, booleans, strings, loop control, nested loops, and invalid loop-control statements

## Automated verification

GitHub Actions runs `cargo test` in this folder and executes the shared hello example on every relevant push. The main bytecode VM has its own root-level test suite and also runs `examples/loops.qv` in CI. You do not need to run commands on your phone to keep these cloud checks working.

## Syntax

```qv
define count := 1
repeat count <= 5 {
    show count
    if count == 3 {
        break
    }
    count := count + 1
}
```

## Loop-control behavior

- `break` exits only the nearest enclosing `repeat` loop.
- `continue` skips the rest of the current loop body and reevaluates that loop's condition.
- Using either statement outside a loop exits with a clear `QORVYNT error`.
- Nested loops are tested to ensure an inner `break` does not terminate the outer loop.

Run the regression suite with:

```sh
cd compiler
cargo test
```

## Current limitations

The folder's line-based interpreter remains a reference MVP; it is not the self-hosted compiler or a production smart-contract runtime. The QORVYNT-written frontend in `parser.qv` emits a compiler plan and canonical Bytecode v2, while `bootstrap.qv` is exercised through three compiler generations in CI. The Rust VM remains a reference host during bootstrap validation. Use test values only; do not use it to handle keys or funds.
