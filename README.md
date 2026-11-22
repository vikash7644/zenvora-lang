# QORVYNT

## Official website

- **QORVYNT website:** https://vikash7644.github.io/zenvora-lang/
- **Browser playground:** https://vikash7644.github.io/zenvora-lang/playground.html
- **Source repository:** https://github.com/vikash7644/zenvora-lang

The static site is in `website/` and is deployed by the GitHub Pages workflow when website files change on `main`. If the links do not open yet, check the latest [Pages deployment workflow](https://github.com/vikash7644/zenvora-lang/actions/workflows/pages.yml).

QORVYNT is an actively developed programming language. The canonical and only supported source-file extension is `.qv`. This repository contains a working Rust compiler/interpreter, a stack-based bytecode virtual machine, a command-line tool, executable language examples, regression tests, and CI workflows. The underlying Rust-hosted implementation is still in its v0.8.0-era state; the QORVYNT rename and Rust-independent bootstrap are not yet complete. It is real runnable software—not only a blueprint—but it is not yet a stable v1.0 language or a production smart-contract toolchain. Experimental areas and remaining release gates are listed in [the project status checklist](docs/PROJECT_STATUS.md).

## Legacy blockchain material — migration pending

A prior integration effort left chain-specific code, workflows, and design drafts in this repository. They are **not part of the QORVYNT Lang release scope** and must be moved to a separate, independently maintained blockchain repository only after that destination and its tests are verified.

- [Archived legacy chain technical specification draft](docs/legacy-chain-technical-spec-v1.md)
- [Legacy chain protocol specification draft](docs/chain-protocol-specification-draft.md)
- [Legacy chain implementation roadmap](docs/chain-implementation-roadmap.md)

These files describe proposals and a local-development prototype, not a production network, mainnet, wallet, or native coin. This language repository is being migrated to QORVYNT; chain development is separate and is not part of this language release.

## Project status and completion checklist

The live implementation checklist, known limitations, and CI completion gates are tracked in [docs/PROJECT_STATUS.md](docs/PROJECT_STATUS.md). A queued workflow is not considered a passing check.

## Repository contents

- `Cargo.toml` and `Cargo.lock` — Rust package manifest and locked dependency versions for the `qorvynt` command.
- `CHANGELOG.md` — v0.8.0 release notes and current feature status.
- `docs/language-grammar.md` — current parser grammar draft and unresolved v1.0 semantics.
- `src/lib.rs` — language lexer/parser, compiler, stack VM, CLI, and regression tests; `src/main.rs` is the thin binary entrypoint. Legacy chain CLI entrypoints are still present pending migration.
- `src/chain_*.rs`, `src/block_store.rs`, and `src/tx_pool.rs` — legacy chain integration files temporarily retained until the separate BHYVORIX Chain repository and its tests are verified; not part of the QORVYNT Lang release scope.
- `examples/hello.qv` — arithmetic and variable example.
- `examples/qorvynt-icon.qv` — QORVYNT source that emits a scalable SVG icon using the language's string and output features.
- `examples/qorvynt-language-tour.qv` — an executable tour of variables, strings, arrays, conditionals, loops, and functions.
- `examples/loops.qv` — loop condition, `break`, and `continue` example.
- `examples/functions.qv` — function parameters, local variables, return values, and loops.
- `examples/booleans.qv` — native booleans and logical operators.
- `examples/strings.qv` — string literals, escapes, concatenation, equality, and string function arguments.
- `examples/collections.qv` — arrays, maps, collection access, and immutable update helpers.
- `examples/indexing.qv` — direct array/map indexing, computed indexes, and nested collections.
- `examples/stdlib.qv` — safe range, type inspection, string conversion, integer helpers, and assertions.
- `examples/science.qv` — checked integer mathematics and introductory physics/science helper functions.
- `examples/contracts.qv` — experimental `contract`, `state`, `action`, and `view` syntax.
- `examples/modules.qv` — relative module imports and reusable functions.
- `docs/getting-started.md` — developer setup, first program, syntax tour, bytecode workflow, and testing guide.
- `CONTRIBUTING.md` — contributor checklist and validation commands.
- `docs/language-v0.1.md` — current language feature status and roadmap.
- `docs/bytecode-v1.md` — version 1 bytecode instruction format and loader-validation limits.
- `docs/technical_spec.md` — legacy draft architecture for the former chain prototype; planned migration to the separate BHYVORIX Chain repository.
- `examples/token.qvt` — illustrative smart-contract DSL draft; not executable by the current compiler.
- `website/` — static responsive ecosystem landing page.

## See QORVYNT code in action

The `.qv` files in [`examples/`](examples/) are programs written in **QORVYNT**. Start with the [executable language tour](examples/qorvynt-language-tour.qv): it covers variables, strings, arrays, conditionals, loops, and functions in one small program. The CI workflow runs this file with the Rust compiler/VM on every push and pull request, so the showcase is checked as executable code rather than being a screenshot or mockup.

```qorvynt
define name := "QORVYNT"
define answer := multiply(6, 7)
show "Hello, " + name
show answer

fn multiply(left, right) {
    return left * right
}
```

GitHub's repository language meter may still show **Rust** because the compiler and virtual machine are implemented in Rust; that meter does not mean the `.qv` programs are Rust. QORVYNT Lang is currently a custom language and GitHub does not automatically provide its own syntax-highlighting grammar from a file extension alone.

## Editor support

The `.qv` extension is the sole QORVYNT source format. All examples, editor metadata, compiler commands, and module imports use `.qv`.

A starter VS Code language extension is available in [`editors/vscode/`](editors/vscode/). It registers `.qv` files as **QORVYNT**, provides TextMate syntax highlighting, and configures basic brackets and `#` comments. Its metadata and grammar regexes are checked by the main CI workflow.

This is syntax highlighting only—not a language server, compiler, debugger, or VS Code Marketplace release. See the [editor support guide](editors/vscode/README.md) for setup. GitHub's language bar still requires upstream Linguist recognition; adding an editor grammar does not change that.

## QORVYNT Studio for Windows

An early-access Tauri desktop IDE is being developed in [`studio/`](studio/). It includes a `.qv` editor, native Open/Save, and Compile/Run actions connected to the shared QORVYNT compiler and VM. The Windows workflow now builds MSI and NSIS installers and uploads them as CI artifacts; this is not yet a signed public release. See the [Studio guide](studio/README.md) and [latest Windows build](https://github.com/vikash7644/zenvora-lang/actions/workflows/studio-windows.yml).

## Latest prototype release

The latest cross-platform CLI build is published as a **pre-release**: [QORVYNT Lang v0.8.0 Prototype 1](https://github.com/vikash7644/zenvora-lang/releases/tag/v0.8.0-prototype.1).

- Linux x86-64: [tar.gz](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-linux-x86_64.tar.gz)
- macOS Intel x86-64: [tar.gz](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-macos-x86_64.tar.gz)
- macOS Apple Silicon ARM64: [tar.gz](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-macos-aarch64.tar.gz)
- Windows x86-64: [ZIP](https://github.com/vikash7644/zenvora-lang/releases/download/v0.8.0-prototype.1/zenvora-lang-windows-x86_64.zip)

Each archive includes the `qorvynt` CLI, README, and MIT license. These binaries were built by GitHub Actions for the listed platforms; they do not require compiling the project on your phone.

**CLI transition note:** the published v0.8.0 Prototype 1 binaries predate the new `run`, `check`, `new`, and `compile` verbs. Until the next release is published, use the compatible forms `qorvynt file.qv`, `qorvynt --check file.qv`, and `qorvynt --new project-name`. The new verbs are implemented and tested on [PR #83](https://github.com/vikash7644/zenvora-lang/pull/83); they will be available in the next release after its checks and merge.

**Prototype warning:** this is not a production smart-contract toolchain. Do not use real funds or deploy production contracts with it.

## Local Rust compiler HTTP API (development preview)

The CLI can start a loopback-only HTTP service that uses the actual Rust compiler and VM (not the browser demo interpreter):

```sh
qorvynt --compiler-api
# listens on http://127.0.0.1:8787
```

- `GET /health` reports the service identity.
- `POST /compile-run` accepts JSON such as `{"source":"define a := 10\\nshow a * 3"}` and returns `ok`, output lines, or a compile/runtime error.
- Requests are limited to 64 KiB and source programs to 48 KiB.
- The service binds to `127.0.0.1` only. It is a local development API, **not a public hosted endpoint**, and should not be exposed to the internet. The GitHub Pages Playground now has an expanded JavaScript demo runner for variables, strings, expressions, branching, loops, functions, arrays/maps, and selected standard-library helpers. It still does not execute the actual Rust compiler/VM and does not yet support module imports, the contract DSL, file/network APIs, or full language parity. A secure hosted compiler service or a browser-compatible build of the real compiler is required before claiming that every `.qv` program can run on the website.

Example request:

```sh
curl -sS http://127.0.0.1:8787/compile-run \
  -H 'Content-Type: application/json' \
  --data '{"source":"define a := 10\\ndefine b := 20\\nshow a + b"}'
```

## Build and run

End users do **not** need Rust, Cargo, or Termux. Download the prebuilt CLI for your operating system from the [QORVYNT pre-release](https://github.com/vikash7644/zenvora-lang/releases/tag/v0.8.0-prototype.1), extract it, and place the `qorvynt` executable on your `PATH`. Open a terminal and verify it with `qorvynt --version`. The commands below assume that the executable is on your `PATH`.

```sh
qorvynt new my-first-project
qorvynt run my-first-project/main.qv
qorvynt check my-first-project/main.qv
qorvynt --render examples/qorvynt-icon.qv qorvynt-icon.svg
qorvynt run examples/hello.qv
qorvynt run examples/loops.qv
qorvynt run examples/functions.qv
qorvynt run examples/booleans.qv
qorvynt run examples/strings.qv
qorvynt run examples/collections.qv
qorvynt run examples/indexing.qv
qorvynt run examples/stdlib.qv
qorvynt run examples/science.qv
qorvynt run examples/stdlib-strings.qv
qorvynt run examples/contracts.qv
qorvynt run examples/modules.qv
qorvynt check examples/functions.qv
qorvynt compile examples/functions.qv functions.qv2b
qorvynt --run-bytecode functions.qv2b
qorvynt --version
qorvynt --help
```

Expected example output:

```text
30
60
20
35
```

## Create your first project

The starter command creates a new folder with `main.qv` and a project `README.md`. It refuses to overwrite an existing path and validates the project name.

```sh
qorvynt new my-first-project
qorvynt run my-first-project/main.qv
qorvynt check my-first-project/main.qv
```

The generated program demonstrates variables, strings, arithmetic, arrays, and `sum`. The first-class commands are `qorvynt new`, `qorvynt run`, `qorvynt check`, and `qorvynt compile`.

## Beginner workflow: from idea to runnable project

1. Create a project: `qorvynt new my-project`.
2. Open `my-project/main.qv` in any text editor and change the example.
3. Compile-check it: `qorvynt check my-project/main.qv`.
4. Run it: `qorvynt run my-project/main.qv`.
5. Keep a small runnable example for each new feature and add a regression test when changing the compiler or VM.

When developing from source, build once with `cargo build --release --bin qorvynt`, then use `./target/release/qorvynt` (or `target\\release\\qorvynt.exe` on Windows). Run `cargo check` and `cargo test` only when contributing to the Rust-hosted compiler. Cargo is only the current compiler-development/build tool; it is not part of the QORVYNT user command. A successful `--check` means the current compiler accepted the syntax; it does not guarantee that runtime operations will succeed. The CLI is not yet a full package manager. QORVYNT Studio is an early Windows IDE preview; project workspaces, debugger support, and a signed public installer are still pending.

## Automated regression tests

`cargo test` runs the compiler/VM, CLI, and language regression suites across the compiler/VM, CLI, and language examples. Coverage includes arithmetic, booleans, Unicode strings and escapes, bytecode round-trips, malformed bytecode, verifier stack-underflow and control-flow-join checks, nested conditions and loops, `break`/`continue`, function calls and recursion, invalid names, runtime errors, recursion-depth limits, arrays/maps, direct indexing and indexed assignment, collection access/update helpers, standard-library validation, and malformed syntax. GitHub Actions runs `cargo check`, `cargo test`, and the example programs on pushes and pull requests.

## Compile-only validation

Use `qorvynt --check <file.qv>` to load relative imports and compile the program without executing it. Use `qorvynt --render <file.qv> <output.txt>` to execute a source program and save its printed lines as a text asset (for example, SVG). This is a host CLI operation; it is not available inside the deterministic contract VM. This catches syntax and compile-time errors without running application logic or its side effects. It is not a static type checker: many value-type errors are still detected at runtime.

```sh
qorvynt check examples/functions.qv
```

## Local signed-chain development prototype

A local-only signed transaction workflow is available in the `qorvynt` CLI:

```sh
qorvynt --chain-keygen owner.qvk
qorvynt --chain-store-init dev.qvstore 71 owner.qvk 1000000
qorvynt --chain-store-transfer dev.qvstore 71 owner.qvk bob 125 3
qorvynt --chain-store-balance dev.qvstore 71 bob
qorvynt --chain-store-info dev.qvstore 71
qorvynt --chain-rpc dev.qvstore 71 8545
# Signed transaction pool + manual validator block production:
qorvynt --chain-rpc-pool dev.qvstore 71 pending.qvp 8545
qorvynt --chain-store-produce dev.qvstore 71 pending.qvp owner.qvk
# On a trusted private network only:
qorvynt --chain-p2p-serve dev.qvstore 71 0.0.0.0 9000
# On another node initialized from the exact same genesis:
qorvynt --chain-p2p-sync node-b.qvstore 71 192.168.1.10:9000
# Optional manual transaction gossip (pool-enabled peer):
qorvynt --chain-p2p-serve-pool dev.qvstore 71 pending.qvp 0.0.0.0 9000
qorvynt --chain-p2p-gossip node-b.qvstore 71 pending-b.qvp 192.168.1.10:9000
```

The prototype signs Ed25519 transactions, verifies genesis-authorized validator quorum signatures, executes transfers through the QORVYNT Lang VM, checks block commitments, persists a local block store and signed transaction pool, replays stored blocks to verify the resulting state, and exposes loopback JSON-RPC (read-only or pool-enabled) plus TCP block sync. The CLI supports multi-validator genesis configs and manually supplied quorum signers; see [the chain integration guide](docs/chain-lang-integration.md) for the exact format and limits.

**Development-only warning:** key files contain an unencrypted private seed. Fees are currently reserved for admission but are not charged; the TCP peer prototype supports block sync and manual transaction gossip but has no peer authentication, automatic discovery/gossip, automated voting, full BFT consensus/finality, or production wallet/security review. Quorum signatures alone do not provide decentralized consensus. Never expose it to the public internet. Do not use real funds or treat the local file as a decentralized blockchain.

## Scientific computing (early integer-only helpers)

The runtime now includes checked integer helpers: `gcd`, `lcm`, `factorial` (0–20), `is_prime`, `fibonacci` (0–92), `pow_int`, and integer-array `mean`. Introductory formula helpers include `speed`, `force`, `kinetic_energy`, `density`, `molarity`, and `bmi_x10` (BMI multiplied by 10), `molar_mass` (sum of caller-supplied integer atomic masses), `dna_complement`, and `gc_content_percent` (integer DNA GC percentage). See [`examples/science.qv`](examples/science.qv).

**Important limits:** the language currently uses signed 64-bit integers only; division truncates, and the science helpers do not track physical units or provide floating-point precision. These are starter utilities, not validated scientific, medical, chemistry-lab, or engineering software. Next steps include floating-point/decimal support, dimensions and units, and domain libraries with independent tests.

## Bytecode files

The compiler can write and run versioned stack-machine bytecode files:

```sh
qorvynt --emit-bytecode examples/functions.qv functions.qvb
qorvynt --run-bytecode functions.qvb

# Experimental opt-in v2 output:
qorvynt --emit-bytecode-v2 examples/functions.qv functions.qv2b
qorvynt --run-bytecode functions.qv2b
```

Version 1 remains the default and its serializer/loader behavior is unchanged. The experimental v2 format starts with `QORVYNT_BYTECODE 2` and includes explicit main/function regions, parameter names, and return-kind metadata. The v2 compiler emission path verifies the generated file before writing it; the loader verifies a v2 file again before passing its instructions to the VM. The verifier checks region boundaries, opcodes/operands, jumps, calls/argument counts, return kinds, stack underflow/joins, and main exit stack cleanliness. Both versions are bounded by bytecode-size, instruction-count, string-size, argument-count, call-depth, and operand-stack limits.

**V2 stability warning:** v2 is experimental and its text grammar/opcode set may change. Deterministic adversarial tests, round-trip/property checks, and dedicated fuzz campaigns are covered by CI; independent security review remains pending. This is not formal verification or an audit; do not use v2 for real funds or production contracts.

## Current language syntax

```qorvynt
define age := 20

if age >= 18 {
    show 1
} otherwise {
    show 0
}
```

This prints `1`. Change the age to `16` and it prints `0`.

See [`examples/loops.qv`](examples/loops.qv) for a runnable `repeat`/`break`/`continue` example and [`examples/functions.qv`](examples/functions.qv) for functions.

The current prototype supports an experimental single-contract-per-file DSL (`contract`, `state`, `action`, and `view`); it is lowered to globals and functions for one local VM run, not blockchain execution. It also supports integer, boolean, string, array, and map values, assignment, string concatenation with `+`, integer arithmetic (`+`, `-`, `*`, `/`), unary minus, parentheses, comments beginning with `#`, double-quoted strings with `\\`, `\"`, `\n`, `\r`, and `\t` escapes, `show expression`, `print(expression)`, comparisons (`==`, `!=`, `<`, `<=`, `>`, `>=`), nested `if/otherwise` blocks, `repeat condition { ... }` loops, and `break`/`continue` loop control. Boolean values `true` and `false` are supported, comparisons produce booleans, and `not`, `and`, and `or` are available. `and` and `or` short-circuit: `false and ...` skips the right operand, and `true or ...` skips the right operand. `break` exits the nearest loop; `continue` jumps back to reevaluate that loop's condition. Using either outside a loop is a compile error. Functions support value parameters with `fn add(left, right) { ... }`, calls such as `call add(2 + 3, 4)`, and expression calls such as `show add(2, 3)`. Top-level `define` declarations are globals visible inside functions; a function can update an existing global by assignment, while `define` inside a function creates a local that shadows a global. Assigning to an undeclared name is an error. Collection helpers are built-ins: `array(...)` creates an array; `map("key", value, ...)` creates a string-keyed map; `get(collection, key)` reads an element; `len(collection)` returns array/map/string length; `contains(collection, value_or_key)` checks membership; `push(array, value)` returns a new array; `put(map, key, value)` returns a new map; `keys(map)` and `values(map)` return arrays. Standard-library helpers include `type_of(value)`, `to_string(value)`, checked `abs(integer)`, `min(a, b)`, `max(a, b)`, bounded `range(start, end)` (end-exclusive, up to 100,000 elements), and `assert(boolean[, message])`. String helpers include `starts_with(text, prefix)`, `ends_with(text, suffix)`, `replace(text, search, replacement)`, `split(text, separator)` (returns an array and caps output at 100,000 items), and `join(string_array, separator)`. `sum(integer_array)` performs checked integer aggregation; invalid element types and overflow are errors. `first(array)` and `last(array)` return endpoint elements and reject empty arrays; `reverse(array_or_string)` reverses arrays or Unicode text; `is_empty(array_or_map_or_string)` checks emptiness; and `clamp(value, minimum, maximum)` bounds an integer after validating the range. Text helpers include `trim(text)`, Unicode-aware `lower(text)`/`upper(text)`, Unicode scalar `char_count(text)`, and `to_int(value)` for checked 64-bit integer parsing, and `index_of(text_or_array, needle)` for zero-based lookup (`-1` when absent; string results count Unicode scalar values), `unique(array)` to keep the first occurrence of each value, and `assert_eq(actual, expected[, message])` for value assertions. String replacement/join output is capped at 1 MiB. `push` and `put` are immutable-style helpers: they return updated collections and do not mutate the original. `return expression` returns an integer, boolean, or string value; plain `return` exits without a value. Calls can be made before a function definition, functions can call other functions recursively, and each call gets a separate local variable environment. Top-level declarations are global and visible inside functions; assignment updates an existing local or global, and `define` inside a function creates a local that shadows a global. Arithmetic operators other than string concatenation require integers; ordered comparisons require integers; equality compares values. The compiler reports undefined variables/functions, reserved-keyword identifiers, invalid syntax, division by zero and integer overflow.

## Roadmap

1. Add a real module namespace/package model and static type checking.
2. Strengthen bytecode verification, compatibility guarantees, and resource accounting.
3. Turn the experimental contract DSL into a typed contract ABI with enforced view/action rules and persistent state.
4. Explore WebAssembly output and native code generation only after the language and VM are stable.

## Status and safety

This is an early work in progress. The `contract/state/action/view` syntax is an experimental source-lowering prototype: state exists only for one program run, and authorization, persistent storage, indirect view-purity enforcement, and blockchain execution are not implemented. Direct assignments to declared contract state inside a `view` body are rejected by the source-lowering check. Advanced static type checking, native machine-code generation, a production blockchain, a wallet, a DEX and mainnet are **not implemented by this compiler yet**. Do not use it for funds or production contracts.

## Support QORVYNT

QORVYNT is developed in the open. If the language, compiler, VM, documentation, or examples are useful to you, you can help sustain the work:

- **GitHub Sponsors:** [View sponsorship options](https://github.com/sponsors/vikash7644). The Sponsor page and its available payment methods depend on GitHub account eligibility and the creator's configured tiers.
- **Contribute code:** see [CONTRIBUTING.md](CONTRIBUTING.md) for tests and review expectations.
- **Report a reproducible issue:** use [GitHub Issues](https://github.com/vikash7644/zenvora-lang/issues).

Suggested voluntary support levels, if enabled on the GitHub Sponsors profile, are **$1, $5, $10, $25, or $100**. These are suggestions only—not configured or guaranteed payment tiers. A small contribution is appreciated, but never required.

Sponsorship supports ongoing language engineering, compiler/VM testing, documentation, examples, and release infrastructure. It does not buy equity, ownership, tokens, investment returns, or guaranteed features. No tax-deductible or charitable status is claimed. See [SUPPORT.md](docs/SUPPORT.md) for details and transparency principles.

## License

MIT.
