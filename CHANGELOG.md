# Changelog

Notable changes to Zenvora Lang are recorded here.

## Unreleased

- Hardened block-store appends by independently executing each candidate against a verified tip-runtime cache before persistence; loading still rebuilds state by replaying the complete stored chain. Added regression coverage proving that state/receipt commitments from unrelated transactions are rejected without altering the stored tip. The verified cache avoids replaying the entire chain on every append.

- Added `zv --new <project-name>` to create a safe starter project with a runnable `main.zv` and project README; existing paths are never overwritten. Added CLI dispatch and regression coverage.
- Documented the beginner starter-project workflow in the README.
- Added DNA complement and GC-content helpers plus an integer atomic-mass summation helper, with input validation and regression tests.
- Added checked integer mathematics helpers (`gcd`, `lcm`, `factorial`, `is_prime`, `fibonacci`, `pow_int`, `mean`) and introductory integer-only science formula helpers (`speed`, `force`, `kinetic_energy`, `density`, `molarity`, `bmi_x10`), with regression tests and `examples/science.zv`.

- Added `zv --check <file.zv>` to compile source and relative imports without executing the program; added regression coverage for successful compile-only validation and syntax errors.

- Rejected language keywords as variable, function, parameter, and contract-state identifiers, with regression coverage for declaration and function headers.

- Added `unique(array)` with stable first-occurrence ordering and `assert_eq(actual, expected[, message])` for test and contract-invariant assertions.
- Added regression tests for mixed-value deduplication, empty arrays, equality, arity errors, and custom assertion messages.

- Added `starts_with`, `ends_with`, `replace`, `split`, and `join` string helpers, plus overflow-checked `sum` for integer arrays.
- Added `first`, `last`, and `reverse` collection/text helpers, `is_empty` checks for arrays/maps/strings, and range-validated integer `clamp`.
- Added whitespace trimming, Unicode-aware lowercase/uppercase conversion, Unicode scalar counting, checked string-to-`i64` conversion, and zero-based `index_of` for strings and arrays.
- Added argument/type validation, empty-separator/search rejection, and result-size/item-count limits for the new standard-library functions.
- Added runnable `examples/stdlib-strings.zv` and regression tests for successful operations, invalid input, and overflow.
- Hardened bytecode loading with file-size, instruction-count, decoded-string, and call-argument limits.
- Added a VM operand-stack limit alongside existing instruction and call-depth limits.
- Added regression assertions for oversized bytecode payloads and excessive argument counts.
- Added a control-flow bytecode verifier for reachable stack underflow, inconsistent stack depths at branch joins, invalid region jumps, and main-program exit stack balance.
- Documented the verifier's current v1 format limitations; arbitrary bytecode is still not considered production-safe or audited.

## v0.8.0 — Contract DSL prototype

- Added experimental `contract`, `state`, `action`, and `view` syntax lowered to global declarations and functions.
- Added a runnable local Counter contract example and contract parser regression tests.
- Direct assignments to declared contract state inside `view` bodies are rejected. State exists only for the current program run; persistent storage, authorization, indirect view-purity enforcement, and blockchain execution are not implemented.

## v0.7.0 — Standard library and modules

- Added relative `use "path.zv"` imports with duplicate-load prevention, cycle detection, and an import-depth limit.
- Added direct postfix array/map indexing, including nested indexing and computed index expressions.
- Added `type_of`, `to_string`, `abs`, `min`, `max`, bounded end-exclusive `range`, and `assert` built-ins.
- Added explicit declaration bytecode and global/local variable scope rules, including global reads/updates from functions and local shadowing.
- Added regression tests and runnable module, indexing, and standard-library examples.
- CI verifies compiler/VM tests, interpreter tests, bytecode round trips, and runnable examples.

## Unreleased

- Added relative use-path module imports for source files and bytecode builds.
- Module files resolve relative to their importer and are loaded once per program.
- Added cycle detection, a 64-file import-depth limit, and source-location context for import errors.
- Added module-loader regression tests and a runnable examples/modules.zv program.

## v0.6.1 — 2026-10-18

- Added first-class runtime array and map values.
- Added collection built-ins: `array`, `map`, `get`, `len`, `contains`, `push`, `put`, `keys`, and `values`.
- Added stable map display order, collection display output, and bytecode serialization support for built-in instructions.
- Added regression tests for collection operations, bytecode round-trips, invalid indexes/keys, and type errors.
- Collection updates are immutable-style: `push` and `put` return a new collection; index syntax and direct mutation are not implemented yet.
- GitHub Actions passed on commit `acbf28c`: `cargo check`, all 62 tests (23 compiler/VM, 5 CLI, 34 integration), and all six runnable examples including `examples/collections.zv`.

## v0.6.0 — 2026-10-18

- Added string literals with `\\`, `\"`, `\n`, `\r`, and `\t` escapes.
- Added string output, concatenation with `+`, equality comparisons, string function arguments/returns, and UTF-8-safe bytecode encoding.
- Implemented short-circuit semantics for `and` and `or`.
- Added versioned text bytecode files using the `ZENVORA_BYTECODE 1` header.
- Added CLI commands to emit and run bytecode files, plus `--version`.
- Added loader checks for bytecode version, instruction shape, identifiers, jump bounds, and function-entry bounds.
- Added a 1,024-frame VM function-call depth limit in addition to the instruction execution limit.
- Added regression coverage for strings, Unicode, malformed literals, short-circuit evaluation, bytecode round-trips, malformed bytecode, and CLI behavior.
- GitHub Actions verification for the v0.6 code commit: `cargo check` passed; `cargo test` passed with 21 compiler/VM unit tests, 5 CLI tests, and 34 language integration tests (60 total); all five runnable examples passed.

### Known limitations

- Bytecode v1 is still an early format and has no long-term compatibility guarantee.
- The bytecode loader is not a complete control-flow/stack verifier.
- Global/module scopes, a static type system, modules, native machine-code generation, smart contracts, and blockchain integration are not implemented.
- Logical operands are checked at runtime when evaluated; there is no full static type checker yet.
