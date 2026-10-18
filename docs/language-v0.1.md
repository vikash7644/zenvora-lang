# Zenvora Lang v0.1 — current implementation

Zenvora Lang is an independent language project for the Zenvora ecosystem. This first runnable milestone is a small compiler and stack-based bytecode virtual machine written in Rust. It is a foundation, not yet a production language or smart-contract compiler.

## Build and run

Requires Rust/Cargo (works in Termux where the Rust toolchain is installed).

```sh
cargo check
cargo run -- examples/hello.zv
cargo run -- --help
```

## Syntax supported now

```zv
define a := 10
define b := 20
define total := a + b
show total
show total * 2
total := total + 1
show (total - 5) / 2
```

- `define name := expression` declares an integer variable.
- `name := expression` updates/creates an integer variable.
- `show expression` prints an integer.
- `print(expression)` is also accepted.
- Integer operators: unary minus, `+`, `-`, `*`, `/`, and parentheses.
- `#` starts a comment.
- Division by zero, undefined variables, invalid syntax and integer overflow are reported as errors.

## How execution works

1. Source text is parsed into stack-machine instructions.
2. The VM executes instructions such as PUSH, LOAD, STORE, ADD, MUL and PRINT.
3. Variables live in the VM's runtime environment.

## Not implemented yet

Strings, booleans, if/otherwise, loops, break/continue, functions, types, modules, bytecode files, native machine-code generation, smart contracts and blockchain integration are future milestones. Do not describe these as working until implemented and tested.

## Roadmap

1. Add automated tests and lock down expression semantics.
2. Add comparisons, booleans and conditional branches.
3. Add loops with correct break/continue flow.
4. Add functions, scopes and useful diagnostics.
5. Add serializable bytecode and a stable VM specification.
6. Explore WASM output and a contract ABI only after the core language is reliable.
