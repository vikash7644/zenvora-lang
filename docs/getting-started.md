# QORVYNT Lang — Developer Getting Started

QORVYNT Lang is a small, independent programming language for the QORVYNT ecosystem. Its current compiler and stack-based virtual machine are implemented in Rust. Programs use the `.qv` extension and run through the `qorvynt` command.

> **Prototype status:** this is an early developer preview. The contract DSL is not a persistent smart-contract runtime, the local chain code is not a public testnet, and Bytecode v2 is experimental. Do not use it for real funds or production contracts.

## 1. Get the project

Install a current stable Rust toolchain and Cargo from the official Rust installation instructions, then:

```sh
git clone https://github.com/vikash7644/zenvora-lang.git
cd zenvora-lang
cargo check --all-targets
cargo test --all-targets
```

To run the compiler directly from the repository, prefix commands with `cargo run --`. You do not need to build on an Android phone: the repository's pre-release page provides CI-built binaries for Linux x86-64, macOS x86-64/ARM64, and Windows x86-64.

## 2. Run your first program

Create `hello.qv` with this source:

```qorvynt
define name := "QORVYNT"
define answer := multiply(6, 7)

show "Hello, " + name
show answer

fn multiply(left, right) {
    return left * right
}
```

Run and compile-check it:

```sh
cargo run -- hello.qv
cargo run -- --check hello.qv
```

Expected output from the first command:

```text
Hello, QORVYNT
42
```

The `--check` command parses and compiles the file and its relative imports without executing the program. It is not a full static type checker; some errors can still occur at runtime.

## 3. Learn by executable examples

Start with these files in the repository:

- [Language tour](../examples/qorvynt-language-tour.qv) — variables, strings, arrays, conditionals, loops, and functions.
- [Hello world and arithmetic](../examples/hello.qv).
- [Loops and loop control](../examples/loops.qv).
- [Functions](../examples/functions.qv).
- [Booleans](../examples/booleans.qv).
- [Strings](../examples/strings.qv) and [string helpers](../examples/stdlib-strings.qv).
- [Arrays and maps](../examples/collections.qv) and [indexing](../examples/indexing.qv).
- [Standard library](../examples/stdlib.qv).
- [Integer-only science helpers](../examples/science.qv).
- [Relative imports](../examples/modules.qv).
- [Experimental contract syntax](../examples/contracts.qv) — syntax demonstration only, not persistent chain execution.

Run an example with `cargo run -- examples/functions.qv`. GitHub Actions runs the language tour and a wider set of examples as regression checks.

## 4. Core syntax at a glance

### Variables and expressions

```qorvynt
define count := 3
count := count + 1
show count
show (6 * 7) / 2
```

Values include signed 64-bit integers, booleans, strings, arrays, and maps. Integer arithmetic is checked; division truncates toward zero. Overflow and division by zero are errors.

### Conditions and loops

```qorvynt
define n := 0
repeat n < 4 {
    show n
    n := n + 1
}
```

Use `if condition { ... } otherwise { ... }` for branching. `break` exits the nearest loop and `continue` returns to its condition. Both are compile-time errors outside a loop.

### Functions

```qorvynt
fn square(value) {
    return value * value
}
show square(9)
```

Functions support parameters, expression calls, return values, and recursion. Each call has its own local environment; top-level bindings are visible to functions. A `define` inside a function creates a local binding.

### Collections

```qorvynt
define numbers := array(10, 20, 30)
show numbers[1]
show len(numbers)
```

Maps use string keys, for example `map("city", "Patna")`. See the collection examples for access, update, and helper functions.

For the broader feature inventory, limitations, and roadmap, read [Language implementation status](language-v0.1.md) and the repository [README](../README.md).

## 5. Modules

A source file can import a relative `.qv` module using `use "relative/path.qv"`. Imports resolve from the importing file. Duplicate imports are skipped, cycles are rejected, and import depth is bounded. A full package manager, namespace/visibility system, and deterministic package registry are not implemented yet.

See [the module example](../examples/modules.qv) and its imported files.

## 6. Bytecode

The default bytecode format is version 1:

```sh
cargo run -- --emit-bytecode examples/functions.qv functions.qorvyntb
cargo run -- --run-bytecode functions.qorvyntb
```

Experimental version 2 emission is opt-in:

```sh
cargo run -- --emit-bytecode-v2 examples/functions.qv functions.qorvynt2b
cargo run -- --run-bytecode functions.qorvynt2b
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
2. Add an executable `.qv` example if the change is user-facing.
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
