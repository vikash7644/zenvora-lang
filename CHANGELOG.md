# Changelog

Notable changes to Zenvora Lang are recorded here.

## Unreleased

- Live variable bindings and saved call-frame environments now share a 64 MiB estimated-memory budget, with regression coverage that aliases a large string until the limit is reached.

- The VM now caps the combined estimated size of operand-stack values at 16 MiB, with accounting on stack pushes/pops and a regression test for oversized argument/expression stacks.

- Runtime collection values now have an estimated 16 MiB aggregate-size budget in addition to item-count and nesting limits, with regression coverage for oversized arrays.

- Runtime arrays/maps are now limited to 256 nested collection levels, preventing dynamically constructed nested values from exhausting the stack during validation or display. Added a loop-built nesting regression test.

- Bytecode execution now reads at most 16 MiB plus one detection byte before decoding, preventing oversized files from being fully allocated before the decoder rejects them. Added a file-reader regression test.

- Parser expression nesting is now capped at 256 levels, with regression tests for deeply nested parentheses and unary operators to prevent stack exhaustion.

- Module loading now caps each source file at 4 MiB and expanded imported source at 16 MiB, using bounded reads and checked expansion before appending. Added oversize-input regression tests.

- Bounded array and map growth to 100,000 items across `push`, `put`, and indexed map assignment; updates to existing map keys remain allowed at the limit. Added resource-boundary regression coverage.

- Compiler diagnostics now track lexer token columns and report source-relative columns for parser errors, including malformed expressions and block conditions. Added regression tests for indentation, function calls, trailing tokens, and `if`/`repeat` conditions.

- Bytecode v2 serialization rejects non-canonical leading/trailing instruction whitespace, with a regression test to preserve serializer/parser round-trip consistency.

- Hardened bytecode v1 function-call metadata validation: argument counts must match parameter lists, parameter names must be valid and unique, and call sites targeting the same function entry must agree on the exact parameter list. Added malformed-bytecode regression tests and updated the bytecode specification.

- The loopback JSON-RPC listener now logs per-client response write failures and continues serving instead of terminating the entire service when a client disconnects mid-response.

- Block-store and transaction-pool loaders now cap the bytes read from the opened file itself, not only its earlier metadata size, so concurrent file growth/replacement cannot bypass their configured input limits.

- Ledger loading now caps bytes read from the open file itself, in addition to checking metadata, preventing a file that grows or is replaced during the read from bypassing the configured 64 MiB allocation limit.

- Added signed-transaction admission regression coverage for wrong chain IDs, expired transactions, nonce gaps, and insufficient amount-plus-fee reserves; each rejection is checked for zero state, nonce, or receipt mutation.

- The source compiler now checks reachable function return-kind consistency before returning compiled instructions, so mixed void/value return paths are rejected during source compilation instead of only when serialized bytecode is loaded. Added a source-level regression test for a value-returning function with a reachable void path.

- Extended contract-view checks with conservative call-graph analysis: views now reject calls to helper functions that directly assign declared contract state or transitively call detected state-mutating functions. Regression coverage verifies a view cannot reach a mutating helper. This is not a complete effect system; complex dynamic or indirect behaviors remain outside the current language model.

- Contract view lowering now rejects direct calls to declared `action` functions, including calls used in expressions, while ignoring matching text inside string literals and comments. Added regression tests for both call forms and the string-literal case; indirect mutation through arbitrary helper functions remains a known limitation.

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
- Added a control-flow bytecode verifier for reachable stack underflow, inconsistent stack depths at branch joins, invalid region jumps, function return stack balance and reachable return-kind consistency, and main-program exit stack balance. Structural operand and jump checks still cover unreachable instructions.
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
