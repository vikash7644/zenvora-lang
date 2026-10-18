# Zenvora Lang v0.8.0 — current implementation

Zenvora Lang is an independent programming-language project for the Zenvora ecosystem. The current repository contains a Rust source compiler, stack-based bytecode VM, versioned bytecode loader, relative source-module loader, runnable examples, and automated regression tests. It is an early prototype, not a production smart-contract language.

## Build and run

Requires Rust and Cargo (including in Termux when the Rust toolchain is installed).

```sh
cargo check --all-targets
cargo clippy --all-targets
cargo test --all-targets
cargo run -- examples/hello.zv
cargo run -- examples/loops.zv
cargo run -- examples/functions.zv
cargo run -- examples/booleans.zv
cargo run -- examples/strings.zv
cargo run -- examples/collections.zv
cargo run -- examples/indexing.zv
cargo run -- examples/stdlib.zv
cargo run -- examples/contracts.zv
cargo run -- examples/modules.zv
cargo run -- --emit-bytecode examples/functions.zv functions.zvb
cargo run -- --run-bytecode functions.zvb
cargo run -- --version
cargo run -- --help
```

## Automated tests and CI

The repository README currently documents 81 automated tests for the compiler/VM, CLI, module loader, bytecode validation, and language behavior. The exact test count can change as tests are added; use `cargo test --all-targets` for the authoritative count and result on the checked-out revision.

GitHub Actions in `.github/workflows/rust.yml` checks compilation, runs Clippy and all-target tests, executes the runnable language examples, round-trips a bytecode program, checks CLI output, and tests the separate `compiler/` interpreter MVP. A green CI run is useful evidence, but is not a formal proof of correctness or security.

## Supported language features

```zv
define age := 20

if age >= 18 {
    show "adult"
} otherwise {
    show "minor"
}
```

The program prints `adult`.

The current prototype supports:

- **Values:** signed 64-bit integers, booleans, strings, arrays, and maps.
- **Variables:** `define name := expression` declares a value; `name := expression` updates an existing binding.
- **Expressions:** integer arithmetic, unary minus, parentheses, comparisons, equality, and boolean `not`, `and`, and `or`. Logical operators short-circuit.
- **Control flow:** nested `if`/`otherwise`, `repeat condition { ... }`, `break`, and `continue`.
- **Functions:** parameters, expression calls, return values, recursion, global reads/updates, and local variables that can shadow globals.
- **Strings:** double-quoted literals, common escapes, output, equality, concatenation with `+`, and string arguments/returns.
- **Collections:** `array(...)`, `map(...)`, `get`, `len`, `contains`, `push`, `put`, `keys`, `values`, and postfix indexing such as `items[0]` or `profile["name"]`. `push` and `put` return updated collections; direct indexed assignment is also supported.
- **Standard library:** `type_of`, `to_string`, checked `abs`, `min`, `max`, bounded end-exclusive `range`, and `assert`.
- **Modules:** relative `use "path.zv"` imports resolve from the importing file. Duplicate imports are skipped, cycles are rejected, and import depth is limited.
- **CLI and bytecode:** source execution, `--help`, `--version`, bytecode emission, and execution of versioned bytecode files.

See the `examples/` directory for runnable programs and the README for commands and more syntax details.

## Bytecode v1 and verifier

Bytecode v1 begins with the header `ZENVORA_BYTECODE 1`. The loader validates the header/version, instruction syntax, operand shapes, jump bounds, function-entry targets, and other structural constraints. A control-flow verifier checks reachable stack underflow, inconsistent stack depths at branch joins, invalid region transitions, stack limits, and main-program exit stack balance. The VM also has execution, call-depth, operand-stack, file-size, instruction-count, string-size, and call-argument limits.

These checks are meaningful hardening, but **they do not make arbitrary bytecode production-safe**. The current format does not carry a complete explicit function table or return-type metadata; unreachable instructions and uncalled function bodies are not fully validated, and static type safety is not proven. Runtime checks remain necessary. See [Bytecode Format v1](bytecode-v1.md) for the instruction set, validation rules, and known limitations.

## Experimental contract DSL

A single `contract` block can declare `state`, `action`, and `view` members. The current implementation lowers this syntax to globals and ordinary functions for a local VM run. Direct assignments to declared contract state inside a `view` body are rejected by the source-lowering check.

This is **not yet blockchain execution or a secure smart-contract runtime**. State is not persisted across program runs; authorization, complete view-purity enforcement, a stable contract ABI, deployment, consensus, and chain integration are not implemented by this compiler.

## Current limitations and next milestones

1. **Verifier hardening:** validate all instruction regions, including unreachable code and uncalled functions; define explicit function metadata and return-kind checks in a future bytecode version; add adversarial tests and fuzzing.
2. **Language correctness:** add a real parser/AST with source spans and clearer diagnostics, then introduce static type checking and a documented compatibility policy.
3. **Modules and packages:** add namespaces, visibility rules, package metadata, deterministic import behavior, and tests for path edge cases.
4. **Runtime safety:** account for memory and collection growth, define resource budgets, and test limits consistently across source and bytecode execution.
5. **Contract model:** define a typed ABI, persistent state model, authorization rules, and enforceable action/view semantics before connecting contracts to a chain.
6. **Ecosystem integration:** explore WebAssembly and chain integration only after language semantics, verifier behavior, and interfaces are stable.

Native machine-code generation, a production blockchain, a wallet, a DEX, and a mainnet are not delivered by this language prototype. Do not use it to hold funds or deploy production contracts.
