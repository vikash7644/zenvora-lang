# Zenvora Lang — interpreter MVP

This folder contains an isolated line-based Rust interpreter used for syntax experiments and regression tests. The canonical bytecode compiler and VM live at the repository root (`src/main.rs`). GitHub Actions tests both implementations separately; no phone-local setup is required.

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

GitHub Actions runs `cargo test` in this folder and executes the shared hello example on every relevant push. The main bytecode VM has its own root-level test suite and also runs `examples/loops.zv` in CI. You do not need to run commands on your phone to keep these cloud checks working.

## Syntax

```zv
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
- Using either statement outside a loop exits with a clear `Zenvora error`.
- Nested loops are tested to ensure an inner `break` does not terminate the outer loop.

Run the regression suite with:

```sh
cd compiler
cargo test
```

## Current limitations

This is an early interpreter, not a native-code compiler, bytecode VM, blockchain runtime, or production-ready smart-contract language. Functions, collections, modules, static type checking, a formal lexer/parser AST, and bytecode are future work. Use test values only; do not use it to handle keys or funds.
