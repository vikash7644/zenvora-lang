# Self-hosted compiler frontend

`compiler/token_stream.zv` now implements character-level tokenization in Zenvora source. It scans compact operators, identifiers, keywords, booleans, decimal integers, punctuation, `#` line comments, and quoted strings with common escapes. Each token contains kind, value, line, and column. The result contains tokens plus an error string for malformed strings or unexpected characters.

Run `cargo run -- compiler/token_stream.zv` and `cargo run -- --check compiler/token_stream.zv` from the repository root.

## Remaining work

This is a lexer stage, not a complete self-hosted compiler. Remaining stages include dedicated lexer fixtures, a Zenvora-written parser and AST, semantic validation, bytecode emission, and reproducible bootstrap differential tests. Rust remains the reference runtime until parity and self-compilation are demonstrated. This PR does not declare v0.9.0 release-ready.
