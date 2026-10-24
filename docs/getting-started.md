# Zenvora Lang — Developer Getting Started

Zenvora Lang is a small, independent programming language for the Zenvora ecosystem. Its current compiler and stack-based virtual machine are implemented in Rust. Programs use the `.zv` extension and run through the `zv` command.

> **Prototype status:** this is an early developer preview. The contract DSL is not a persistent smart-contract runtime, the local chain code is not a public testnet, and Bytecode v2 is experimental. Do not use it for real funds or production contracts.

## 1. Get the project

Install a current stable Rust toolchain and Cargo from the official Rust installation instructions, then:

```sh
git clone https://github.com/vikash7644/zenvora-lang.git
cd zenvora-lang
cargo check --all-targets
cargo test --all-targets
```

To run the compiler directly from the repository, prefix commands with `cargo run --`. If you do not want to install Rust/Cargo, use a prebuilt CLI asset from the [GitHub Releases page](https://github.com/vikash7644/zenvora-lang/releases) when an asset is available for your operating system and CPU architecture. Download the matching archive, extract it, and run the `zv` executable (on Linux/macOS, you may need to mark it executable with `chmod +x zv`). Do not download source code and expect it to run as a binary. If no matching release asset is published, the source-build path below currently requires Rust/Cargo; Rust-independent self-hosted installation is not complete yet.

## 2. Run your first program

Create `hello.zv` with this source:

```zv
define name := "Zenvora"
define answer := multiply(6, 7)

show "Hello, " + name
show answer

fn multiply(left, right) {
    return left * right
}
```

Run and compile-check it:

```sh
cargo run -- hello.zv
cargo run -- --check hello.zv
```

Expected output from the first command:

```text
Hello, Zenvora
42
```

The `--check` command parses and compiles the file and its relative imports without executing the program. It is not a full static type checker; some errors can still occur at runtime.

## 3. Learn by executable examples

Start with these files in the repository:

- [Language tour](../examples/zenvora-language-tour.zv) — variables, strings, arrays, conditionals, loops, and functions.
- [Hello world and arithmetic](../examples/hello.zv).
- [Loops and loop control](../examples/loops.zv).
- [Functions](../examples/functions.zv).
- [Booleans](../examples/booleans.zv).
- [Strings](../examples/strings.zv) and [string helpers](../examples/stdlib-strings.zv).
- [Arrays and maps](../examples/collections.zv) and [indexing](../examples/indexing.zv).
- [Standard library](../examples/stdlib.zv).
- [Integer-only science helpers](../examples/science.zv).
- [Relative imports](../examples/modules.zv).
- [Experimental contract syntax](../examples/contracts.zv) — syntax demonstration only, not persistent chain execution.

Run an example with `cargo run -- examples/functions.zv`. GitHub Actions runs the language tour and a wider set of examples as regression checks.

## 4. Core syntax at a glance

### Variables and expressions

```zv
define count := 3
count := count + 1
show count
show (6 * 7) / 2
```

Values include signed 64-bit integers, booleans, strings, arrays, and maps. Integer arithmetic is checked; division truncates toward zero. Overflow and division by zero are errors.

### Conditions and loops

```zv
define n := 0
repeat n < 4 {
    show n
    n := n + 1
}
```

Use `if condition { ... } otherwise { ... }` for branching. `break` exits the nearest loop and `continue` returns to its condition. Both are compile-time errors outside a loop.

### Functions

```zv
fn square(value) {
    return value * value
}
show square(9)
```

Functions support parameters, expression calls, return values, and recursion. Each call has its own local environment; top-level bindings are visible to functions. A `define` inside a function creates a local binding.

### Collections

```zv
define numbers := array(10, 20, 30)
show numbers[1]
show len(numbers)
```

Maps use string keys, for example `map("city", "Patna")`. See the collection examples for access, update, and helper functions.

For the broader feature inventory, limitations, and roadmap, read [Language implementation status](language-v0.1.md) and the repository [README](../README.md).

## 5. Modules

A source file can import a relative `.zv` module using `use "relative/path.zv"`. Imports resolve from the importing file. Duplicate imports are skipped, cycles are rejected, and import depth is bounded. A full package manager, namespace/visibility system, and deterministic package registry are not implemented yet.

See [the module example](../examples/modules.zv) and its imported files.

## 6. Bytecode

The default bytecode format is version 1:

```sh
cargo run -- --emit-bytecode examples/functions.zv functions.zvb
cargo run -- --run-bytecode functions.zvb
```

Experimental version 2 emission is opt-in:

```sh
cargo run -- --emit-bytecode-v2 examples/functions.zv functions.zv2b
cargo run -- --run-bytecode functions.zv2b
```

V2 adds explicit program/function metadata and structural, control-flow, and stack verification. The parser/verifier has deterministic adversarial tests, round-trip/property checks, and fuzz campaigns in CI; this is not an independent security audit. The grammar and opcode set remain provisional, and v1 remains the default. Read [Bytecode v1](bytecode-v1.md) and [Bytecode v2 design and status](bytecode-v2-design.md).

## 7. Test changes before proposing them

Run the same baseline locally (or use GitHub Actions if you are not building on your device):

```sh
cargo fmt -- --check
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

When changing behavior:
1. Add a focused regression test for the reported case.
2. Add an executable `.zv` example if the change is user-facing.
3. Run the relevant example and the full test suite.
4. Update the language status/bytecode documentation if behavior or compatibility changed.
5. Check the Actions run for the exact commit; a successful older run does not validate a newer commit.

See [Contributing](../CONTRIBUTING.md) for the change checklist.

## 8. Known boundaries

- The language is not statically typed; several invalid operations are reported at runtime.
- Module imports are relative and do not yet provide package namespaces or a package manager.
- The `contract/state/action/view` DSL lowers to globals/functions for one local VM run. State is not persisted between runs, and this is not a secure contract runtime.
- Bytecode v2 is experimental and not a stable long-term file format.
- The chain-related code is a local prototype; it is not a decentralized public testnet/mainnet and must not be exposed to the public internet.
- A production wallet, DEX, native machine-code backend, and mainnet are not delivered.

These boundaries are intentional release notes for an early preview, not promises that the listed future work is already implemented.
