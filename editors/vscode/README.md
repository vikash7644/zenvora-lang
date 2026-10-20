# Zenvora Lang editor support

This folder contains a small Visual Studio Code language extension for Zenvora Lang source files (`.zv`).

## Included

- Registers the **Zenvora Lang** language ID for `.zv` files.
- TextMate syntax highlighting for the current draft syntax: declarations, control flow, functions, strings and escapes, comments, booleans, integers, operators, punctuation, and common built-ins.
- Basic bracket matching, quote pairing, and `#` line comments.

## Try it locally

1. Open this repository in VS Code.
2. Open this folder as an extension project in VS Code.
3. Press `F5` if the VS Code Extension Development Host is available.
4. In the development host, open an `.zv` example from the repository.

This is editor syntax support, not a compiler or language server. Diagnostics, autocomplete, go-to-definition, formatting, and debugging are not provided yet. The grammar tracks the current draft syntax and may need updates as the language evolves.

## GitHub language recognition

VS Code registration does **not** by itself change GitHub's repository language statistics. GitHub's Linguist language catalogue needs to recognize Zenvora Lang before the repository can display it as a separate language in the language bar. Until then, Rust remains the primary language because the compiler/VM implementation is written in Rust.
