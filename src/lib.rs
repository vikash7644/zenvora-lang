//! Public, platform-neutral building blocks for the QORVYNT toolchain.
//!
//! The Bytecode v2 parser and verifier live in this library crate so the CLI,
//! browser/WASM adapters, and other hosts can share the same validation rules.
//! The current compiler and instruction executor remain in the Rust CLI binary.
pub mod bytecode_v2;
