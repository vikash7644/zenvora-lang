# Zenvora Lang

Zenvora Lang is an independent programming-language project for the Zenvora ecosystem. The repository now includes the first runnable compiler/bytecode-VM milestone in Rust.

## Repository contents

- `Cargo.toml` — Rust package manifest for the `zv` command.
- `src/main.rs` — lexer, expression parser, bytecode compiler and stack VM.
- `examples/hello.zv` — runnable example program.
- `docs/language-v0.1.md` — current feature status and roadmap.
- `docs/technical_spec.md` — draft architecture for the proposed Zenvora Chain; this is a design document, not a completed chain implementation.
- `examples/token.zvl` — illustrative smart-contract DSL draft; it is not executable by the current compiler.
- `website/` — static responsive ecosystem landing page.

## Build and run

Requires Rust and Cargo (including on Android through Termux if the Rust toolchain is installed).

```sh
cargo check
cargo run -- examples/hello.zv
cargo run -- --help
```

Expected example output:

```text
30
60
20
35
```

## Current language syntax

```zv
define a := 10
define b := 20
define total := a + b
show total
show total * 2
total := total + 5
show total
```

The current milestone supports integer variables, assignment, arithmetic (`+`, `-`, `*`, `/`), unary minus, parentheses, comments beginning with `#`, `show expression`, and `print(expression)`. It reports undefined variables, invalid syntax, division by zero and integer overflow.

## Roadmap

1. Add automated tests and stabilize expression semantics.
2. Add comparisons, booleans and conditional branches.
3. Implement loops with reliable `break` and `continue` behavior.
4. Add functions, scopes, types, and better diagnostics.
5. Define a stable bytecode format and bytecode-file support.
6. Explore WebAssembly output and smart-contract integration after the core language is reliable.

## Status and safety

This is an early work in progress. Strings, conditionals, loops, functions, native machine-code generation, smart-contract execution, a production blockchain, a wallet, a DEX and mainnet are **not implemented by this compiler yet**. Do not use it for funds or production contracts.

## License

MIT.
