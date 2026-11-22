# QORVYNT Studio (Windows)

QORVYNT Studio is the planned native Windows desktop IDE. This directory is the first desktop-shell scaffold, not yet a working compiler integration.

## Current scaffold

- Tauri 2 desktop host (Windows WebView2)
- Vite-powered editor interface
- .qv editing surface, project outline placeholder and output panel
- Compile and Run are intentionally absent until they can invoke the real Rust compiler and QVM

## Prerequisites

Windows 10/11, Node.js 20+, npm, Rust stable, Microsoft C++ Build Tools and Microsoft Edge WebView2 Runtime.

## Development

From this directory run: npm install, then npm run tauri:dev.

## Windows installer

Run npm run tauri:build. Tauri can produce NSIS and MSI installers when Windows build prerequisites are installed.

## Integration milestones

1. Extract compiler, bytecode verifier and QVM into a reusable Rust library.
2. Add typed Tauri commands for compile, run, diagnostics and project files.
3. Map VM instruction locations back to .qv source files.
4. Enable Run/Build only after end-to-end compiler tests pass.
5. Add debugger controls after the VM exposes safe stepping and breakpoints.

The desktop shell is separated from compiler internals so the IDE cannot silently become a second JavaScript-only implementation.
