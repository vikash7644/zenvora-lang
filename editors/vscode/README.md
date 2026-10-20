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

## Install a packaged VSIX

The main GitHub Actions workflow builds an installable `.vsix` artifact after validating the extension. Open the latest successful [Zenvora Lang CI run](https://github.com/vikash7644/zenvora-lang/actions/workflows/rust.yml), download the `zenvora-vscode-extension` artifact, extract it, then install it on desktop VS Code:

```sh
code --install-extension zenvora-lang.vsix
```

The workflow artifact is temporary (7-day retention). The extension is **not yet published to the VS Code Marketplace**; Marketplace publication needs a maintainer-controlled publisher token and release approval.

## GitHub language recognition

VS Code registration does **not** by itself change GitHub's repository language statistics. GitHub's Linguist language catalogue needs to recognize Zenvora Lang before the repository can display it as a separate language in the language bar. Until then, Rust remains the primary language because the compiler/VM implementation is written in Rust.
