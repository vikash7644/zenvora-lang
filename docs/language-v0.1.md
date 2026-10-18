# Zenvora Lang v0.2 — current implementation

Zenvora Lang is an independent language project for the Zenvora ecosystem. This milestone adds numeric comparisons and conditional execution to the Rust compiler and stack-based bytecode virtual machine. It remains an early prototype, not a production smart-contract language.

## Build and run

Requires Rust and Cargo (including in Termux where the Rust toolchain is installed).

```sh
cargo check
cargo test
cargo run -- examples/hello.zv
```

## Supported syntax

```zv
define age := 20

if age >= 18 {
    show 1
} otherwise {
    show 0
}
```

The program prints `1`. Set `age := 16` and it prints `0`.

- `define name := expression` declares an integer variable.
- `name := expression` updates or creates an integer variable.
- `show expression` prints an integer; `print(expression)` is also accepted.
- Integer arithmetic: unary minus, `+`, `-`, `*`, `/`, and parentheses.
- Comparisons: `==`, `!=`, `<`, `<=`, `>`, `>=`. Comparisons evaluate to `1` (true) or `0` (false).
- Conditional blocks use `if condition {`, followed by statements and a closing `}`; an optional else branch uses the exact line ` } otherwise {` without the leading space (normally written as `} otherwise {`).
- `#` starts a comment.
- Nested if/otherwise blocks are supported.
- Division by zero, undefined variables, invalid syntax, integer overflow, and unmatched conditional braces are reported as errors.

## How execution works

1. Source text is parsed into stack-machine instructions.
2. The compiler emits operations such as PUSH, LOAD, STORE, ADD, comparison operations and conditional jumps.
3. The VM executes the bytecode and stores integer variables in its runtime environment.

## Not implemented yet

Strings, native boolean values, loops, `break`/`continue`, functions, custom types, modules, bytecode file serialization, native machine-code generation, smart contracts and blockchain integration are future milestones. Comparison results are represented as integer `1` or `0` for now. Do not use this prototype for funds or production contracts.

## Roadmap

1. Stabilize syntax, diagnostics and automated tests.
2. Add loops with correct `break`/`continue` behavior.
3. Add functions, scopes and useful types.
4. Add serializable bytecode and a stable VM specification.
5. Explore WebAssembly output and a contract ABI only after the core language is reliable.
