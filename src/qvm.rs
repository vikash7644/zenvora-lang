//! QORVYNT compiler-facing adapter for the standalone VM crate.
//!
//! The VM instruction set, values, built-ins, safety helpers, gas interface,
//! and executor are owned by the independent `qorvynt-vm` package. This module
//! retains compiler-specific bytecode serialization and verification glue.

pub(crate) use qorvynt_vm::qvm::{Op, Value};
pub(crate) use qorvynt_vm::qvm::{builtins, gas, limits, runtime, runtime_support};

use super::{deserialize_bytecode_v2, read_bytecode_source, write_bytecode_v2};

/// Compile QORVYNT source into a verified Bytecode v2 QVM artifact.
pub(super) fn compile(source_path: &str, output_path: &str) -> Result<(), String> {
    write_bytecode_v2(source_path, output_path)
}

/// An instruction stream that has passed the host compiler's Bytecode v2 verifier.
pub(crate) struct VerifiedProgram {
    instructions: Vec<Op>,
}

impl VerifiedProgram {
    pub(crate) fn instructions(&self) -> &[Op] { &self.instructions }
    pub(crate) fn into_instructions(self) -> Vec<Op> { self.instructions }
}

/// Load only verified Bytecode v2 artifacts. Legacy ZENVORA headers remain readable.
pub(crate) fn load_verified(path: &str) -> Result<VerifiedProgram, String> {
    let serialized = read_bytecode_source(path)?;
    if !(serialized.starts_with("QORVYNT_BYTECODE 2\\n")
        || serialized.starts_with("ZENVORA_BYTECODE 2\\n"))
    {
        return Err("QVM requires a verified Bytecode v2 artifact".into());
    }
    let instructions = deserialize_bytecode_v2(&serialized)?;
    Ok(VerifiedProgram { instructions })
}

/// Compatibility loader for existing compiler CLI and tests.
pub(super) fn load(path: &str) -> Result<Vec<Op>, String> {
    Ok(load_verified(path)?.into_instructions())
}

/// Execute a verified artifact through the standalone runtime API.
pub(crate) fn execute_with_output<F>(path: &str, mut output: F) -> Result<(), String>
where F: FnMut(String),
{
    let program = load_verified(path)?;
    runtime::run_with_output_metered(program.instructions(), &mut output, None)
}

/// CLI convenience wrapper.
pub(super) fn execute(path: &str) -> Result<(), String> {
    execute_with_output(path, |line| println!("{line}"))
}
