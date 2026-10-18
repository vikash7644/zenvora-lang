# Zenvora Lang

Zenvora Lang is an independent programming-language project for the Zenvora ecosystem. The repository includes an early runnable compiler and stack-based bytecode virtual machine written in Rust.

## Repository contents

- `Cargo.toml` — Rust package manifest for the `zv` command.
- `CHANGELOG.md` — v0.8.0 release notes and current feature status.
- `src/main.rs` — lexer, expression parser, bytecode compiler, stack VM and automated regression tests.
- `examples/hello.zv` — arithmetic and variable example.
- `examples/loops.zv` — loop condition, `break`, and `continue` example.
- `examples/functions.zv` — function parameters, local variables, return values, and loops.
- `examples/booleans.zv` — native booleans and logical operators.
- `examples/strings.zv` — string literals, escapes, concatenation, equality, and string function arguments.
- `examples/collections.zv` — arrays, maps, collection access, and immutable update helpers.
- `examples/indexing.zv` — direct array/map indexing, computed indexes, and nested collections.
- `examples/stdlib.zv` — safe range, type inspection, string conversion, integer helpers, and assertions.
- `examples/contracts.zv` — experimental `contract`, `state`, `action`, and `view` syntax.
- `examples/modules.zv` — relative module imports and reusable functions.
- `docs/language-v0.1.md` — current language feature status and roadmap.
- `docs/bytecode-v1.md` — version 1 bytecode instruction format and loader-validation limits.
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
cargo run -- examples/strings.zv
cargo run -- examples/collections.zv
cargo run -- examples/indexing.zv
cargo run -- examples/stdlib.zv
cargo run -- examples/stdlib-strings.zv
cargo run -- examples/contracts.zv
cargo run -- examples/modules.zv
cargo run -- --emit-bytecode examples/functions.zv functions.zvb
cargo run -- --run-bytecode functions.zvb
cargo run -- --version
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

`cargo test` runs the compiler/VM, CLI, and language regression suites across the compiler/VM, CLI, and language examples. Coverage includes arithmetic, booleans, Unicode strings and escapes, bytecode round-trips, malformed bytecode, verifier stack-underflow and control-flow-join checks, nested conditions and loops, `break`/`continue`, function calls and recursion, invalid names, runtime errors, recursion-depth limits, arrays/maps, direct indexing and indexed assignment, collection access/update helpers, standard-library validation, and malformed syntax. GitHub Actions runs `cargo check`, `cargo test`, and the example programs on pushes and pull requests.

## Bytecode files

The compiler can write and run versioned stack-machine bytecode files:

```sh
cargo run -- --emit-bytecode examples/functions.zv functions.zvb
cargo run -- --run-bytecode functions.zvb
```

The current text format starts with `ZENVORA_BYTECODE 1`. The loader checks the format version, instruction syntax, jump targets, and function-entry targets. It also limits bytecode files to 16 MiB, programs to 1,000,000 instructions, strings to 1 MiB, and call arguments to 1,024; VM execution has instruction, call-depth, and operand-stack limits. Version 1 is an early format and is not yet promised to remain compatible forever.

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

The current prototype supports an experimental single-contract-per-file DSL (`contract`, `state`, `action`, and `view`); it is lowered to globals and functions for one local VM run, not blockchain execution. It also supports integer, boolean, string, array, and map values, assignment, string concatenation with `+`, integer arithmetic (`+`, `-`, `*`, `/`), unary minus, parentheses, comments beginning with `#`, double-quoted strings with `\\`, `\"`, `\n`, `\r`, and `\t` escapes, `show expression`, `print(expression)`, comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), nested `if/otherwise` blocks, `repeat condition { ... }` loops, and `break`/`continue` loop control. Boolean values `true` and `false` are supported, comparisons produce booleans, and `not`, `and`, and `or` are available. `and` and `or` short-circuit: `false and ...` skips the right operand, and `true or ...` skips the right operand. `break` exits the nearest loop; `continue` jumps back to reevaluate that loop's condition. Using either outside a loop is a compile error. Functions support value parameters with `fn add(left, right) { ... }`, calls such as `call add(2 + 3, 4)`, and expression calls such as `show add(2, 3)`. Top-level `define` declarations are globals visible inside functions; a function can update an existing global by assignment, while `define` inside a function creates a local that shadows a global. Assigning to an undeclared name is an error. Collection helpers are built-ins: `array(...)` creates an array; `map("key", value, ...)` creates a string-keyed map; `get(collection, key)` reads an element; `len(collection)` returns array/map/string length; `contains(collection, value_or_key)` checks membership; `push(array, value)` returns a new array; `put(map, key, value)` returns a new map; `keys(map)` and `values(map)` return arrays. Standard-library helpers include `type_of(value)`, `to_string(value)`, checked `abs(integer)`, `min(a, b)`, `max(a, b)`, bounded `range(start, end)` (end-exclusive, up to 100,000 elements), and `assert(boolean[, message])`. String helpers include `starts_with(text, prefix)`, `ends_with(text, suffix)`, `replace(text, search, replacement)`, `split(text, separator)` (returns an array and caps output at 100,000 items), and `join(string_array, separator)`. `sum(integer_array)` performs checked integer aggregation; invalid element types and overflow are errors. `first(array)` and `last(array)` return endpoint elements and reject empty arrays; `reverse(array_or_string)` reverses arrays or Unicode text; `is_empty(array_or_map_or_string)` checks emptiness; and `clamp(value, minimum, maximum)` bounds an integer after validating the range. String replacement/join output is capped at 1 MiB. `push` and `put` are immutable-style helpers: they return updated collections and do not mutate the original. `return expression` returns an integer, boolean, or string value; plain `return` exits without a value. Calls can be made before a function definition, functions can call other functions recursively, and each call gets a separate local variable environment. Top-level declarations are global and visible inside functions; assignment updates an existing local or global, and `define` inside a function creates a local that shadows a global. Arithmetic operators other than string concatenation require integers; ordered comparisons require integers; equality compares values. The compiler reports undefined variables/functions, invalid syntax, division by zero and integer overflow.

## Roadmap

1. Add a real module namespace/package model and static type checking.
2. Strengthen bytecode verification, compatibility guarantees, and resource accounting.
3. Turn the experimental contract DSL into a typed contract ABI with enforced view/action rules and persistent state.
4. Explore WebAssembly output and native code generation only after the language and VM are stable.

## Status and safety

This is an early work in progress. The `contract/state/action/view` syntax is an experimental source-lowering prototype: state exists only for one program run, and authorization, persistent storage, indirect view-purity enforcement, and blockchain execution are not implemented. Direct assignments to declared contract state inside a `view` body are rejected by the source-lowering check. Advanced static type checking, native machine-code generation, a production blockchain, a wallet, a DEX and mainnet are **not implemented by this compiler yet**. Do not use it for funds or production contracts.

## License

MIT.
