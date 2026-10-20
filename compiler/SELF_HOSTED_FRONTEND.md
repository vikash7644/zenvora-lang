# Self-hosted compiler frontend

Status: **incremental implementation; not yet a complete compiler or self-hosting build**.

## Zenvora-written stages now present

- `compiler/token_stream.zv`: character-level lexer for identifiers, exact keywords, booleans, decimal integers, compact operators, punctuation, comments, quoted strings/common escapes, and Unicode string content. Tokens carry kind, value, line, and column; malformed input produces an error field.
- `compiler/parser.zv`: recursive-descent expression parser with precedence layers for multiplication/division, addition/subtraction, comparisons, and logical `and`/`or`. It emits AST maps for `define` and `show` statements and rejects unsupported statements/invalid expressions.

## Verify locally or in CI

```sh
cargo run --locked -- compiler/token_stream.zv
cargo run --locked -- compiler/parser.zv
```

The `Zenvora self-hosted frontend smoke` workflow runs both components and checks token kinds, AST node kinds, precedence, and malformed-expression diagnostics. The Rust runtime is currently only the bootstrap/reference executor for these Zenvora source files.

## Remaining work

1. Expand parser statement support to assignments, branches, loops, functions/calls/returns, arrays/maps, modules, and contract declarations.
2. Add unary operators, indexing, function-call expressions, and full source-span diagnostics.
3. Add semantic/name/type validation and deterministic diagnostics.
4. Add an IR and a Zenvora-written bytecode backend with verifier checks.
5. Differential-test every language fixture against the Rust reference implementation.
6. Demonstrate reproducible bootstrap: compiler source compiled by the reference runtime, then compiled again by the resulting compiler with equivalent artifacts/behavior.
7. Make the compiler/VM path used by the Playground match the language engine; a JavaScript-only browser interpreter is not equivalent.

## Architecture boundary

Zenvora Lang remains separate from BHYVORIX Chain. The language repository owns compiler, VM interface, ABI/artifact tooling, and deterministic local contract tests. Chain consensus, signed transactions, native BVRX fee settlement, wallet, explorer, P2P network, and testnet operations belong in the separate BHYVORIX Chain repository.

Do not declare v0.9.0 release-ready or claim self-hosting until the stages above are implemented and verified on a single reproducible commit.
