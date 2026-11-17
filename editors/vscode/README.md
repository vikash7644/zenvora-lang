# QORVYNT editor support

This folder contains a small Visual Studio Code language extension for QORVYNT source files (`.qv`).

## Included

- Registers the **QORVYNT** language ID for `.qv` files.
- TextMate syntax highlighting for the current draft syntax: declarations, control flow, functions, strings and escapes, comments, booleans, integers, operators, punctuation, and common built-ins.
- Basic bracket matching, quote pairing, and `#` line comments.

## Try it locally

1. Open this repository in VS Code.
2. Open this folder as an extension project in VS Code.
3. Press `F5` if the VS Code Extension Development Host is available.
4. In the development host, open an `.qv` example from the repository.

This is editor syntax support, not a compiler or language server. Diagnostics, autocomplete, go-to-definition, formatting, and debugging are not provided yet. The grammar tracks the current draft syntax and may need updates as the language evolves.

## Install a packaged VSIX

The main GitHub Actions workflow builds an installable `.vsix` artifact after validating the extension. Open the latest successful [QORVYNT CI run](https://github.com/vikash7644/qorvynt-lang/actions/workflows/rust.yml), download the `qorvynt-vscode-extension` artifact, extract it, then install it on desktop VS Code:

```sh
code --install-extension qorvynt-lang.vsix
```

The workflow artifact is temporary (7-day retention). The extension is **not yet published to the VS Code Marketplace**; Marketplace publication needs a maintainer-controlled publisher token and release approval.

## GitHub language recognition

VS Code registration does **not** by itself change GitHub's repository language statistics. GitHub's Linguist language catalogue needs to recognize QORVYNT before the repository can display it as a separate language in the language bar. Until then, Rust remains the primary language because the compiler/VM implementation is written in Rust.
