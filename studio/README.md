# QORVYNT Studio for Windows

This is the first desktop-shell scaffold for the QORVYNT language IDE. It is not yet a complete IDE and does not compile or run QORVYNT programs.

## Stack
- Tauri 2 desktop host
- Vite frontend
- `.qv` source editing surface
- Output/status panel

## Windows prerequisites
Windows 10/11, Node.js 20+, npm, Rust stable, Microsoft C++ Build Tools and WebView2 Runtime.

## Run in development
From this directory: `npm install` then `npm run tauri:dev`.

## Build installer
Run `npm run tauri:build` on Windows. Tauri is configured for NSIS and MSI bundles.

## Next integration work
1. Extract compiler, verifier and QVM into a reusable Rust library.
2. Expose compile/run/diagnostics and file operations through typed Tauri commands.
3. Add source spans and VM instruction-to-source mapping.
4. Add debugger only after VM stepping and breakpoints are supported.

Compile/Run controls are intentionally not faked or enabled before the real compiler bridge exists.
