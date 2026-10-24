# Self-hosted compiler frontend

Status: **incremental implementation; not yet a complete compiler or self-hosting build**.

## Zenvora-written stages now present

- `compiler/token_stream.zv`: character-level lexer for identifiers, exact keywords, booleans, decimal integers, compact operators, punctuation, comments, quoted strings/common escapes, and Unicode string content. Tokens carry kind, value, line, and column; malformed input produces an error field.
- `compiler/parser.zv`: recursive-descent expression and statement parser for literals, names, calls, indexing, declarations, assignments, `show`, `return`, functions, nested `if/otherwise`, `repeat`, `break`, and `continue`. It now lowers the parsed AST into a structured Zenvora IR, then emits a Zenvora-authored bytecode *plan* with stack operations, branch/loop jump targets, and separate function records. The IR, plan, and a basic semantic pass are exercised by the self-hosted frontend CI. The semantic pass currently checks supported AST shapes, duplicate top-level function names/parameters, and whether `return`, `break`, and `continue` appear in valid contexts.

## Verify locally or in CI

```sh
cargo run --locked -- compiler/token_stream.zv
cargo run --locked -- compiler/parser.zv
```

The `Zenvora self-hosted frontend smoke` workflow runs both components and checks token kinds, AST node kinds, precedence, and malformed-expression diagnostics. The Rust runtime is currently only the bootstrap/reference executor for these Zenvora source files.

## Remaining work — not yet a standalone/self-hosted compiler

1. The bytecode plan is not yet the VM's canonical executable bytecode format; map every emitted instruction/operand to the VM specification, then serialize and verify it.
2. Expand semantic/name/type checks to resolve names and scopes, validate function arity/types, and produce deterministic source-span diagnostics.
3. Differential-test supported programs against the reference VM, including nested branches/loops, break/continue, recursion, collections, malformed input, and runtime errors.
4. Implement the bootstrap path so the Zenvora-written compiler can compile its own source and reproduce equivalent artifacts/behavior without the Rust compiler being required for normal installation.
5. Make the installed CLI and Playground use the Zenvora compiler/VM path; the current browser JavaScript demo is not equivalent.
6. Keep Rust only as a temporary reference/bootstrap implementation until parity and reproducible self-compilation are proven. Do not call this compiler standalone or complete before those gates pass.

## Architecture boundary

Zenvora Lang remains separate from BHYVORIX Chain. The language repository owns compiler, VM interface, ABI/artifact tooling, and deterministic local contract tests. Chain consensus, signed transactions, native BVRX fee settlement, wallet, explorer, P2P network, and testnet operations belong in the separate BHYVORIX Chain repository.

Do not declare v0.9.0 release-ready or claim self-hosting until the stages above are implemented and verified on a single reproducible commit.
