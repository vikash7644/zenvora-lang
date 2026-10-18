# Zenvora Lang

Zenvora Lang is an independent programming-language project for the Zenvora ecosystem. The repository includes an early runnable compiler and stack-based bytecode virtual machine written in Rust.

## Repository contents

- `Cargo.toml` — Rust package manifest for the `zv` command.
- `src/main.rs` — lexer, expression parser, bytecode compiler, stack VM and automated regression tests.
- `examples/hello.zv` — arithmetic and variable example.
- `examples/loops.zv` — loop condition, `break`, and `continue` example.
- `examples/functions.zv` — function parameters, local variables, return values, and loops.
- `examples/booleans.zv` — native booleans and logical operators.
- `docs/language-v0.1.md` — current language feature status and roadmap.
- `docs/technical_spec.md` — draft architecture for the proposed Zenvora Chain; design only, not a completed chain.
- `examples/token.zvl` — illustrative smart-contract DSL draft; not executable by the current compiler.
- `website/` — static responsive ecosystem landing page.

## Build and run

Requires Rust and Cargo (including on Android through Termux if the Rust toolchain is installed).

```sh
cargo check
cargo test
cargo run -- examples/hello.zv
cargo run -- examples/loops.zv
cargo run -- examples/functions.zv
cargo run -- examples/booleans.zv
cargo run -- --help
```

Expected example output:

```text
30
60
20
35
```

## Automated regression tests

`cargo test` checks expression evaluation, booleans, nested conditions, `break`/`continue`, nested-loop break behavior, function calls and recursion, invalid function calls, runtime errors, and malformed braces. GitHub Actions runs `cargo check`, `cargo test`, and the example programs on pushes and pull requests.

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

See [`examples/loops.zv`](examples/loops.zv) for a runnable `repeat`/`break`/`continue` example and [`examples/functions.zv`](examples/functions.zv) for functions.

The current prototype supports integer and boolean variables, assignment, arithmetic (`+`, `-`, `*`, `/`), unary minus, parentheses, comments beginning with `#`, `show expression`, `print(expression)`, comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), nested `if/otherwise` blocks, `repeat condition { ... }` loops, and `break`/`continue` loop control. Boolean values `true` and `false` are supported, comparisons produce booleans, and `not`, `and`, and `or` are available. Logical operators currently evaluate both operands (they are not short-circuiting). `break` exits the nearest loop; `continue` jumps back to reevaluate that loop's condition. Using either outside a loop is a compile error. Functions support integer parameters with `fn add(left, right) { ... }`, calls such as `call add(2 + 3, 4)`, and expression calls such as `show add(2, 3)`. `return expression` returns an integer value; plain `return` exits without a value. Calls can be made before a function definition, functions can call other functions recursively, and each call gets a separate local variable environment. Access to caller/global variables from inside a function is not implemented yet. Arithmetic operators require integers; ordered comparisons require integers; equality compares values. The compiler reports undefined variables/functions, invalid syntax, division by zero and integer overflow.

## Roadmap

1. Expand regression coverage and stabilize syntax and diagnostics.
2. Add global/module scope rules, types and better diagnostics.
3. Define a stable bytecode format and bytecode-file support.
4. Strengthen runtime safety checks and add resource-aware execution limits.
5. Explore WebAssembly output and smart-contract integration after the core language is reliable.

## Status and safety

This is an early work in progress. Strings, short-circuit logical evaluation, global/module scope rules, advanced static type checking, native machine-code generation, smart-contract execution, a production blockchain, a wallet, a DEX and mainnet are **not implemented by this compiler yet**. Do not use it for funds or production contracts.

## License

MIT.
