# Zenvora Lang v0.8.0 — implementation status

Zenvora Lang is an independent programming-language project for the Zenvora ecosystem. This branch contains a Rust source compiler, stack-based bytecode VM, versioned bytecode loader, relative source-module loader, standard-library helpers, an experimental contract DSL, and a local Zenvora Chain execution/persistence prototype. It is still an early prototype, not a production smart-contract language.

## Build, test, and run

Requires Rust and Cargo.

```sh
cargo check --all-targets
cargo clippy --all-targets
cargo test --all-targets

cargo run -- --new my-first-project
cargo run -- --check my-first-project/main.zv
cargo run -- my-first-project/main.zv

cargo run -- examples/hello.zv
cargo run -- examples/loops.zv
cargo run -- examples/functions.zv
cargo run -- examples/functions-basic.zv
cargo run -- examples/functions-summary.zv
cargo run -- examples/booleans.zv
cargo run -- examples/strings.zv
cargo run -- examples/collections.zv
cargo run -- examples/indexing.zv
cargo run -- examples/stdlib.zv
cargo run -- examples/stdlib-strings.zv
cargo run -- examples/science.zv
cargo run -- examples/contracts.zv
cargo run -- examples/chain_transfer.zv
cargo run -- examples/modules.zv

cargo run -- --emit-bytecode examples/functions.zv functions.zvb
cargo run -- --run-bytecode functions.zvb
cargo run -- --version
cargo run -- --help
```

Use `cargo test --all-targets` as the authoritative test result for the checked-out revision. GitHub Actions also runs Clippy, the Rust tests, runnable examples, bytecode round-trip checks, CLI checks, and the separate `compiler/` interpreter MVP suite. Green CI is useful evidence, not a proof of correctness or security.

## Language features

```zv
define age := 20

if age >= 18 {
    show "adult"
} otherwise {
    show "minor"
}
```

This program prints `adult`.

The current implementation supports:

- **Values and variables:** signed 64-bit integers, booleans, strings, arrays, and maps; `define name := expression` declares a value and `name := expression` updates an existing binding.
- **Expressions:** checked integer arithmetic, unary minus, parentheses, comparisons, equality, and boolean `not`, `and`, and `or`. Logical operators short-circuit.
- **Control flow:** nested `if`/`otherwise`, `repeat condition { ... }`, `break`, and `continue`.
- **Functions:** parameters, expression calls, return values, recursion, global reads/updates, and local variables that can shadow globals. Reachable return paths in bytecode v1 must agree on whether a value is returned.
- **Strings:** double-quoted literals, common escapes, equality, concatenation with `+`, and string arguments/returns.
- **Collections:** `array(...)`, `map(...)`, `get`, `len`, `contains`, `push`, `put`, `keys`, `values`, and postfix indexing such as `items[0]` or `profile["name"]`. Direct indexed assignment is supported; `push` and `put` return updated collections.
- **Standard library:** `type_of`, `to_string`, checked `abs`, `min`, `max`, bounded end-exclusive `range`, `assert`, and `assert_eq`; string helpers such as `starts_with`, `ends_with`, `replace`, `split`, `join`, `trim`, `lower`, `upper`, `char_count`, and `to_int`; collection helpers such as `first`, `last`, `reverse`, `is_empty`, `unique`, and `index_of`; checked numeric helpers including `sum`, `gcd`, `lcm`, `factorial`, `is_prime`, `fibonacci`, `pow_int`, `mean`, and integer-only introductory science helpers.
- **Modules:** relative `use "path.zv"` imports resolve from the importing file. Duplicate imports are skipped, cycles are rejected, and import depth is limited.
- **CLI:** `--new` creates a starter project without overwriting an existing path; `--check` validates source and imports without running the program; `--help` and `--version` report CLI information.
- **Bytecode:** source can be compiled to a versioned text bytecode file and later executed by the VM.

See `examples/` and the repository README for runnable programs and syntax examples.

## Bytecode v1 and verifier

A bytecode file begins with `ZENVORA_BYTECODE 1`. The loader checks the header/version, instruction syntax and operands, jump bounds, call arguments, and other structural constraints. The verifier checks stack underflow, inconsistent stack depths at control-flow joins, jumps that escape inferred code regions, function return stack balance, reachable return-kind consistency, stack limits, and main-program exit stack balance. It also inspects disconnected instruction islands inside known regions from an assumed empty stack. The VM retains runtime checks and limits for execution, call depth, operand stack, file size, instruction count, strings, and call arguments.

These checks are meaningful hardening, but **arbitrary bytecode is not production-safe or audited**. Bytecode v1 has no explicit function table or declared return types. Function regions are inferred from call targets, so uncalled function bodies and orphan instruction regions cannot be fully partitioned and validated. Type safety is not proven. A future bytecode version should carry explicit function boundaries, parameter counts, and return-kind metadata. See [Bytecode Format v1](bytecode-v1.md).

## Experimental contract DSL

A single `contract` block can declare `state`, `action`, and `view` members. The compiler lowers these to globals and ordinary functions for a local VM run. Direct assignments to declared contract state and calls to declared `action` functions inside a `view` body are rejected. A conservative call-graph pass also rejects calls to helper functions that directly mutate declared state or transitively call a detected mutating function. This is not a formal effect system and does not prove all possible view behavior pure.

This is **not yet a secure smart-contract runtime**. State is not persisted between program runs; indirect view mutation, action authorization, a stable contract ABI, deterministic gas accounting, deployment, consensus, and full chain integration are not delivered by the contract compiler. The separate local chain prototype is also not a public testnet or mainnet.

## Current limitations and next engineering milestones

1. **Bytecode format:** add explicit function metadata and declared return kinds in a new version, validate all instruction regions, and add adversarial tests and fuzzing.
2. **Language correctness:** build a formal parser/AST with source spans and clearer diagnostics, then introduce static type checking and a compatibility policy.
3. **Modules and packages:** add namespaces, visibility rules, package metadata, and deterministic package resolution.
4. **Runtime safety:** account for memory and collection growth, define resource budgets, and apply limits consistently across source and bytecode execution.
5. **Contract model:** define a typed ABI, persistent state, authorization, enforced action/view semantics, and deterministic gas before production-chain integration.
6. **Chain foundations:** finalize canonical protocol decisions, transaction/fee rules, multi-node networking, consensus/finality, and recovery tests before any public testnet decision.

Native machine-code generation, a production blockchain, a wallet, a DEX, and a mainnet are not delivered by this prototype. Do not use it to hold funds or deploy production contracts.
