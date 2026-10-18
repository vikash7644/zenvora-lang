# Changelog

Notable changes to Zenvora Lang are recorded here.

## v0.6.0 — 2026-10-18

- Added string literals with `\\`, `\"`, `\n`, `\r`, and `\t` escapes.
- Added string output, concatenation with `+`, equality comparisons, string function arguments/returns, and UTF-8-safe bytecode encoding.
- Implemented short-circuit semantics for `and` and `or`.
- Added versioned text bytecode files using the `ZENVORA_BYTECODE 1` header.
- Added CLI commands to emit and run bytecode files, plus `--version`.
- Added loader checks for bytecode version, instruction shape, identifiers, jump bounds, and function-entry bounds.
- Added a 1,024-frame VM function-call depth limit in addition to the instruction execution limit.
- Added regression coverage for strings, Unicode, malformed literals, short-circuit evaluation, bytecode round-trips, malformed bytecode, and CLI behavior.
- GitHub Actions verification for the v0.6 code commit: `cargo check` passed; `cargo test` passed with 21 compiler/VM unit tests, 5 CLI tests, and 34 language integration tests (60 total); all four runnable examples passed.

### Known limitations

- Bytecode v1 is still an early format and has no long-term compatibility guarantee.
- The bytecode loader is not a complete control-flow/stack verifier.
- Global/module scopes, a static type system, modules, native machine-code generation, smart contracts, and blockchain integration are not implemented.
- Logical operands are checked at runtime when evaluated; there is no full static type checker yet.
