# Zenvora Lang

Zenvora Lang is an independent programming-language project for the Zenvora ecosystem. The repository includes an early runnable compiler and stack-based bytecode virtual machine written in Rust.

## Zenvora Chain design documents

- [Zenvora Technical Specification v1 — working draft](docs/zenvora-technical-spec-v1.md): ecosystem scope, proposed differentiator, measurable goals, phase gates, and unresolved decisions.
- [Chain protocol specification draft](docs/chain-protocol-specification-draft.md): consensus-critical protocol, networking, tokenomics, security, and test requirements.
- [Chain implementation roadmap](docs/chain-implementation-roadmap.md): 12 staged phases, engineering order, explicit exit gates, and security release criteria.

These are design drafts, not proof that listed features are implemented. Consensus and tokenomics are not locked, and Zenvora Chain remains a prototype/local-development project.

## Repository contents

- `Cargo.toml` — Rust package manifest for the `zv` command.
- `CHANGELOG.md` — v0.8.0 release notes and current feature status.
- `src/main.rs` — lexer, expression parser, bytecode compiler, stack VM, CLI, and automated regression tests.
- `src/chain_protocol.rs` — canonical transaction/header encodings, SHA-256 IDs, Merkle roots, and Ed25519 signature verification.
- `src/chain_runtime.rs` — Zenvora-VM transfer execution, V3 ledger persistence, signed nonce tracking, and state/receipt roots.
- `src/chain_block.rs` — atomic candidate-block execution and transaction/receipt/state-root validation.
- `src/block_store.rs` — append-only local block storage, independent candidate execution before persistence, and full replay verification.
- `examples/hello.zv` — arithmetic and variable example.
- `examples/loops.zv` — loop condition, `break`, and `continue` example.
- `examples/functions.zv` — function parameters, local variables, return values, and loops.
- `examples/booleans.zv` — native booleans and logical operators.
- `examples/strings.zv` — string literals, escapes, concatenation, equality, and string function arguments.
- `examples/collections.zv` — arrays, maps, collection access, and immutable update helpers.
- `examples/indexing.zv` — direct array/map indexing, computed indexes, and nested collections.
- `examples/stdlib.zv` — safe range, type inspection, string conversion, integer helpers, and assertions.
- `examples/science.zv` — checked integer mathematics and introductory physics/science helper functions.
- `examples/contracts.zv` — experimental `contract`, `state`, `action`, and `view` syntax.
- `examples/modules.zv` — relative module imports and reusable functions.
- `docs/language-v0.1.md` — current language feature status and roadmap.
- `docs/bytecode-v1.md` — version 1 bytecode instruction format and loader-validation limits.
- `docs/technical_spec.md` — draft architecture for the proposed Zenvora Chain; design only, not a completed chain.
- `examples/token.zvl` — illustrative smart-contract DSL draft; not executable by the current compiler.
- `website/` — static responsive ecosystem landing page.

## Latest prototype release

The latest cross-platform CLI build is published as a **pre-release**: [Zenvora Lang v0.8.0 Prototype 1](https://github.com/vikash7644/zenvora-lang/releases/tag/v0.8.0-prototype.1).

- Linux x86-64: [tar.gz](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-linux-x86_64.tar.gz)
- macOS Intel x86-64: [tar.gz](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-macos-x86_64.tar.gz)
- macOS Apple Silicon ARM64: [tar.gz](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-macos-aarch64.tar.gz)
- Windows x86-64: [ZIP](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-windows-x86_64.zip)

Each archive includes the `zv` CLI, README, and MIT license. These binaries were built by GitHub Actions for the listed platforms; they do not require compiling the project on your phone.

**Prototype warning:** this is not a production smart-contract toolchain. Do not use real funds or deploy production contracts with it.

## Build and run

Requires Rust and Cargo (including on Android through Termux if the Rust toolchain is installed).

```sh
cargo check
cargo test
cargo run -- --new my-first-project
cargo run -- my-first-project/main.zv
cargo run -- --check my-first-project/main.zv
cargo run -- examples/hello.zv
cargo run -- examples/loops.zv
cargo run -- examples/functions.zv
cargo run -- examples/booleans.zv
cargo run -- examples/strings.zv
cargo run -- examples/collections.zv
cargo run -- examples/indexing.zv
cargo run -- examples/stdlib.zv
cargo run -- examples/science.zv
cargo run -- examples/stdlib-strings.zv
cargo run -- examples/contracts.zv
cargo run -- examples/modules.zv
cargo run -- --check examples/functions.zv
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

## Create your first project

The starter command creates a new folder with `main.zv` and a project `README.md`. It refuses to overwrite an existing path and validates the project name.

```sh
cargo run -- --new my-first-project
cargo run -- my-first-project/main.zv
cargo run -- --check my-first-project/main.zv
```

The generated program demonstrates variables, strings, arithmetic, arrays, and `sum`. When using a built release binary, the same commands are `zv --new my-first-project`, `zv my-first-project/main.zv`, and `zv --check my-first-project/main.zv`.

## Beginner workflow: from idea to runnable project

1. Create a project: `zv --new my-project`.
2. Open `my-project/main.zv` in any text editor and change the example.
3. Compile-check it: `zv --check my-project/main.zv`.
4. Run it: `zv my-project/main.zv`.
5. Keep a small runnable example for each new feature and add a regression test when changing the compiler or VM.

During development from this repository, prefix each command with `cargo run --`. For example, `cargo run -- --new my-project`. A successful `--check` means the current compiler accepted the syntax; it does not guarantee that runtime operations will succeed. The current CLI is not yet a full package manager, IDE, debugger, or cross-platform installer.

## Automated regression tests

`cargo test` runs the compiler/VM, CLI, and language regression suites across the compiler/VM, CLI, and language examples. Coverage includes arithmetic, booleans, Unicode strings and escapes, bytecode round-trips, malformed bytecode, verifier stack-underflow and control-flow-join checks, nested conditions and loops, `break`/`continue`, function calls and recursion, invalid names, runtime errors, recursion-depth limits, arrays/maps, direct indexing and indexed assignment, collection access/update helpers, standard-library validation, and malformed syntax. GitHub Actions runs `cargo check`, `cargo test`, and the example programs on pushes and pull requests.

## Compile-only validation

Use `zv --check <file.zv>` to load relative imports and compile the program without executing it. This catches syntax and compile-time errors without running application logic or its side effects. It is not a static type checker: many value-type errors are still detected at runtime.

```sh
cargo run -- --check examples/functions.zv
```

## Local signed-chain development prototype

A local-only signed transaction workflow is available in the `zv` CLI:

```sh
zv --chain-keygen owner.zvk
zv --chain-store-init dev.zvb 71 owner.zvk 1000000
zv --chain-store-transfer dev.zvb 71 owner.zvk bob 125 3
zv --chain-store-balance dev.zvb 71 bob
zv --chain-store-info dev.zvb 71
zv --chain-rpc dev.zvb 71 8545
# Signed transaction pool + manual validator block production:
zv --chain-rpc-pool dev.zvb 71 pending.zvp 8545
zv --chain-store-produce dev.zvb 71 pending.zvp owner.zvk
# On a trusted private network only:
zv --chain-p2p-serve dev.zvb 71 0.0.0.0 9000
# On another node initialized from the exact same genesis:
zv --chain-p2p-sync node-b.zvb 71 192.168.1.10:9000
# Optional manual transaction gossip (pool-enabled peer):
zv --chain-p2p-serve-pool dev.zvb 71 pending.zvp 0.0.0.0 9000
zv --chain-p2p-gossip node-b.zvb 71 pending-b.zvp 192.168.1.10:9000
```

The prototype signs Ed25519 transactions, verifies genesis-authorized validator quorum signatures, executes transfers through the Zenvora Lang VM, checks block commitments, persists a local block store and signed transaction pool, replays stored blocks to verify the resulting state, and exposes loopback JSON-RPC (read-only or pool-enabled) plus TCP block sync. The CLI supports multi-validator genesis configs and manually supplied quorum signers; see [the chain integration guide](docs/chain-lang-integration.md) for the exact format and limits.

**Development-only warning:** key files contain an unencrypted private seed. Fees are currently reserved for admission but are not charged; the TCP peer prototype supports block sync and manual transaction gossip but has no peer authentication, automatic discovery/gossip, automated voting, full BFT consensus/finality, or production wallet/security review. Quorum signatures alone do not provide decentralized consensus. Never expose it to the public internet. Do not use real funds or treat the local file as a decentralized blockchain.

## Scientific computing (early integer-only helpers)

The runtime now includes checked integer helpers: `gcd`, `lcm`, `factorial` (0–20), `is_prime`, `fibonacci` (0–92), `pow_int`, and integer-array `mean`. Introductory formula helpers include `speed`, `force`, `kinetic_energy`, `density`, `molarity`, and `bmi_x10` (BMI multiplied by 10), `molar_mass` (sum of caller-supplied integer atomic masses), `dna_complement`, and `gc_content_percent` (integer DNA GC percentage). See [`examples/science.zv`](examples/science.zv).

**Important limits:** the language currently uses signed 64-bit integers only; division truncates, and the science helpers do not track physical units or provide floating-point precision. These are starter utilities, not validated scientific, medical, chemistry-lab, or engineering software. Next steps include floating-point/decimal support, dimensions and units, and domain libraries with independent tests.

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

The current prototype supports an experimental single-contract-per-file DSL (`contract`, `state`, `action`, and `view`); it is lowered to globals and functions for one local VM run, not blockchain execution. It also supports integer, boolean, string, array, and map values, assignment, string concatenation with `+`, integer arithmetic (`+`, `-`, `*`, `/`), unary minus, parentheses, comments beginning with `#`, double-quoted strings with `\\`, `\"`, `\n`, `\r`, and `\t` escapes, `show expression`, `print(expression)`, comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), nested `if/otherwise` blocks, `repeat condition { ... }` loops, and `break`/`continue` loop control. Boolean values `true` and `false` are supported, comparisons produce booleans, and `not`, `and`, and `or` are available. `and` and `or` short-circuit: `false and ...` skips the right operand, and `true or ...` skips the right operand. `break` exits the nearest loop; `continue` jumps back to reevaluate that loop's condition. Using either outside a loop is a compile error. Functions support value parameters with `fn add(left, right) { ... }`, calls such as `call add(2 + 3, 4)`, and expression calls such as `show add(2, 3)`. Top-level `define` declarations are globals visible inside functions; a function can update an existing global by assignment, while `define` inside a function creates a local that shadows a global. Assigning to an undeclared name is an error. Collection helpers are built-ins: `array(...)` creates an array; `map("key", value, ...)` creates a string-keyed map; `get(collection, key)` reads an element; `len(collection)` returns array/map/string length; `contains(collection, value_or_key)` checks membership; `push(array, value)` returns a new array; `put(map, key, value)` returns a new map; `keys(map)` and `values(map)` return arrays. Standard-library helpers include `type_of(value)`, `to_string(value)`, checked `abs(integer)`, `min(a, b)`, `max(a, b)`, bounded `range(start, end)` (end-exclusive, up to 100,000 elements), and `assert(boolean[, message])`. String helpers include `starts_with(text, prefix)`, `ends_with(text, suffix)`, `replace(text, search, replacement)`, `split(text, separator)` (returns an array and caps output at 100,000 items), and `join(string_array, separator)`. `sum(integer_array)` performs checked integer aggregation; invalid element types and overflow are errors. `first(array)` and `last(array)` return endpoint elements and reject empty arrays; `reverse(array_or_string)` reverses arrays or Unicode text; `is_empty(array_or_map_or_string)` checks emptiness; and `clamp(value, minimum, maximum)` bounds an integer after validating the range. Text helpers include `trim(text)`, Unicode-aware `lower(text)`/`upper(text)`, Unicode scalar `char_count(text)`, and `to_int(value)` for checked 64-bit integer parsing, and `index_of(text_or_array, needle)` for zero-based lookup (`-1` when absent; string results count Unicode scalar values), `unique(array)` to keep the first occurrence of each value, and `assert_eq(actual, expected[, message])` for value assertions. String replacement/join output is capped at 1 MiB. `push` and `put` are immutable-style helpers: they return updated collections and do not mutate the original. `return expression` returns an integer, boolean, or string value; plain `return` exits without a value. Calls can be made before a function definition, functions can call other functions recursively, and each call gets a separate local variable environment. Top-level declarations are global and visible inside functions; assignment updates an existing local or global, and `define` inside a function creates a local that shadows a global. Arithmetic operators other than string concatenation require integers; ordered comparisons require integers; equality compares values. The compiler reports undefined variables/functions, reserved-keyword identifiers, invalid syntax, division by zero and integer overflow.

## Roadmap

1. Add a real module namespace/package model and static type checking.
2. Strengthen bytecode verification, compatibility guarantees, and resource accounting.
3. Turn the experimental contract DSL into a typed contract ABI with enforced view/action rules and persistent state.
4. Explore WebAssembly output and native code generation only after the language and VM are stable.

## Status and safety

This is an early work in progress. The `contract/state/action/view` syntax is an experimental source-lowering prototype: state exists only for one program run, and authorization, persistent storage, indirect view-purity enforcement, and blockchain execution are not implemented. Direct assignments to declared contract state inside a `view` body are rejected by the source-lowering check. Advanced static type checking, native machine-code generation, a production blockchain, a wallet, a DEX and mainnet are **not implemented by this compiler yet**. Do not use it for funds or production contracts.

## License

MIT.
