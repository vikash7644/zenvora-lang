# Zenvora Lang

Zenvora Lang is an independent programming-language project for the Zenvora ecosystem. The repository includes an early runnable compiler and stack-based bytecode virtual machine written in Rust.

## Repository contents

- `Cargo.toml` — Rust package manifest for the `zv` command.
- `src/main.rs` — lexer, expression parser, bytecode compiler and stack VM.
- `examples/hello.zv` — runnable example program.
- `docs/language-v0.1.md` — current language feature status and roadmap (updated for v0.2 features).
- `docs/technical_spec.md` — draft architecture for the proposed Zenvora Chain; design only, not a completed chain.
- `examples/token.zvl` — illustrative smart-contract DSL draft; not executable by the current compiler.
- `website/` — static responsive ecosystem landing page.

## Build and run

Requires Rust and Cargo (including on Android through Termux if the Rust toolchain is installed).

```sh
cargo check
cargo test
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
define age := 20

if age >= 18 {
    show 1
} otherwise {
    show 0
}
```

This prints `1`. Change the age to `16` and it prints `0`.

The current prototype supports integer variables, assignment, arithmetic (`+`, `-`, `*`, `/`), unary minus, parentheses, comments beginning with `#`, `show expression`, `print(expression)`, comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), nested `if/otherwise` blocks, `repeat condition { ... }` loops, and `break`/`continue` loop control. Comparison results are represented as integer `1` (true) and `0` (false). `break` exits the nearest loop; `continue` jumps back to reevaluate that loop's condition. Using either outside a loop is a compile error. It reports undefined variables, invalid syntax, division by zero and integer overflow.

## Roadmap

1. Continue stabilizing syntax, diagnostics and automated regression tests.
2. Add functions, scopes, types and better diagnostics.
3. Define a stable bytecode format and bytecode-file support.
4. Add execution limits and more robust runtime safety checks.
5. Explore WebAssembly output and smart-contract integration after the core language is reliable.

## Status and safety

This is an early work in progress. Strings, loops, functions, native machine-code generation, smart-contract execution, a production blockchain, a wallet, a DEX and mainnet are **not implemented by this compiler yet**. Do not use it for funds or production contracts.

## License

MIT.
