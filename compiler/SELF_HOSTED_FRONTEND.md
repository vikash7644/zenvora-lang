# Self-hosted compiler: first executable source component

This directory begins the compiler frontend **in Zenvora source**, not Rust.
The first component is intentionally narrow: `token_stream.zv` accepts a
normalized source line where operators and punctuation are already separated
by spaces. It removes empty tokens, preserves token order, and returns an array.
The checked-in smoke fixture prints the resulting tokens and count.

Run from the repository root:

```sh
cargo run -- compiler/token_stream.zv
cargo run -- --check compiler/token_stream.zv
```

## Explicit limitation

This is **not yet a full lexer or compiler**, and Zenvora is not self-hosting
yet. The current language lacks a sufficiently capable character/string-slicing
API for a robust lexer written entirely in Zenvora. The next prerequisite is a
small deterministic string/character API (or equivalent language-level byte
iteration) with UTF-8-safe semantics; then implement real tokenization including
quoted/escaped strings, comments, source spans and malformed-input diagnostics.
After that, add AST parsing, semantic checks, bytecode emission and bootstrap
differential tests. Rust remains the temporary behavior oracle until parity and
reproducible self-compilation are proven.
