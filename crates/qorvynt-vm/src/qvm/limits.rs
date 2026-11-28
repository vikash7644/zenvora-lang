//! QORVYNT VM-owned deterministic resource and input limits.
//!
//! These limits are shared by the verifier, bytecode loader, and runtime so
//! host integrations cannot accidentally drift from the VM safety contract.

pub const MAX_VALUE_DEPTH: usize = 256;
pub const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_STACK_VALUE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_RUNTIME_STATE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_COLLECTION_ITEMS: usize = 100_000;
pub const MAX_BYTECODE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_BYTECODE_INSTRUCTIONS: usize = 1_000_000;
pub const MAX_BYTECODE_STRING_BYTES: usize = 1024 * 1024;
pub const MAX_CALL_ARGUMENTS: usize = 1024;
pub const MAX_STACK_VALUES: usize = 65_536;
