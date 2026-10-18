# Zenvora Lang — interpreter MVP

This folder contains the first repository-hosted Rust interpreter prototype for the `.zv` language. It is separate from the Termux working copy, so it does not overwrite any files on your phone.

## Implemented in this MVP

- `define name := expression` and reassignment with `name := expression`
- `show expression`
- Numbers, booleans, quoted strings, arithmetic and comparisons
- `if { ... } otherwise { ... }` and `otherwise if`
- `repeat condition { ... }`, `break`, and `continue`
- A one-million-iteration loop safety cap and nesting limit
- `break` and `continue` are rejected outside loops with clear errors
- Tests for expression precedence, booleans, strings, loop control, nested loops, and invalid loop-control statements

## Run on Android with Termux

Install Rust if needed, then:

```sh
cd ~/zenvora-lang
mkdir -p compiler/src
```

The repo-hosted version lives in `compiler/`. To run it from this repository root:

```sh
cd compiler
cargo test
cargo run -- ../examples/hello.zv
```

If you already have a separate local crate at `~/zenvora-lang/zenvora`, keep it intact for now. Compare/test this isolated MVP first rather than overwriting your working copy.

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
