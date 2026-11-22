//! QORVYNT compiler-facing adapter for the standalone VM crate.
//!
//! The VM instruction set, values, built-ins, safety helpers, gas interface,
//! and executor are owned by the independent `qorvynt-vm` package. This module
//! retains compiler-specific bytecode serialization and verification glue.

pub(crate) use qorvynt_vm::qvm::{Op, Value};
pub(crate) use qorvynt_vm::VerifiedProgram;
pub(crate) use qorvynt_vm::qvm::{builtins, gas, limits, runtime, runtime_support};

use super::{deserialize_bytecode_v2, read_bytecode_source, write_bytecode_v2};

/// Compile QORVYNT source into a verified Bytecode v2 QVM artifact.
pub(super) fn compile(source_path: &str, output_path: &str) -> Result<(), String> {
    write_bytecode_v2(source_path, output_path)
}

/// Load only verified Bytecode v2 artifacts. Legacy ZENVORA headers remain readable.
pub(crate) fn load_verified(path: &str) -> Result<VerifiedProgram, String> {
    let serialized = read_bytecode_source(path)?;
    if !matches!(serialized.lines().next().map(str::trim), Some("QORVYNT_BYTECODE 2" | "ZENVORA_BYTECODE 2")) {
        return Err("QVM requires a verified Bytecode v2 artifact".into());
    }
    VerifiedProgram::from_v2_source(&serialized)
}

/// Compatibility loader for existing compiler CLI and tests.
pub(super) fn load(path: &str) -> Result<Vec<Op>, String> {
    Ok(load_verified(path)?.instructions().to_vec())
}

/// Execute a verified artifact through the standalone runtime API.
pub(crate) fn execute_with_output<F>(path: &str, mut output: F) -> Result<(), String>
where F: FnMut(String),
{
    let program = load_verified(path)?;
    program.execute_with_output(output)
}

/// CLI convenience wrapper.
pub(super) fn execute(path: &str) -> Result<(), String> {
    execute_with_output(path, |line| println!("{line}"))
}
