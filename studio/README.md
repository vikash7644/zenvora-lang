# QORVYNT Studio for Windows

QORVYNT Studio is an early-access Windows desktop IDE for the QORVYNT
programming language. It uses Tauri 2 for the native desktop host and
CodeMirror 6 for the source editor.

## Working features

- Native Windows desktop shell.
- `.qv` source editor with QORVYNT syntax highlighting and basic completion.
- Open and save source files through the native file-system plugin.
- **Compile** calls the shared Rust compiler and emits verified Bytecode v2.
- **Run** calls the same in-process QORVYNT compiler/VM used by the CLI and
  displays program output or compiler/runtime errors.
- Compile and Run are native Tauri commands; they are not JavaScript simulations.

The shared API currently lives in the library crate at `src/lib.rs`:
`compile_source_to_bytecode_v2` and `run_source`. The CLI delegates to the
same library entry point. The next architecture step is to split compiler,
verifier and VM modules into smaller reusable modules.

## Not implemented yet

- Debugger, breakpoints and instruction stepping.
- Project/workspace creation and multi-file project explorer.
- Language server, semantic autocomplete and refactoring tools.
- Installer signing and a published Windows release.

## Windows prerequisites

- Windows 10 or 11
- Node.js 20+ and npm
- Rust stable toolchain
- Microsoft C++ Build Tools
- Microsoft Edge WebView2 Runtime

## Development

From this directory:

```powershell
npm install
npm run tauri:dev
```

## Build a Windows installer

```powershell
npm run tauri:build
```

The Tauri bundler is configured for NSIS and MSI. A successful local or CI
installer build is still required before calling a Windows installer release
ready.

## Validation

The repository's Windows Studio workflow builds the frontend and checks the
native Tauri host. The QORVYNT CI also runs compiler, VM and public API tests.
Check the latest GitHub Actions result before treating a build as verified.
