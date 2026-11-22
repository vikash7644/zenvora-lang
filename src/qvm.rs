//! QORVYNT Virtual Machine artifact boundary.
//!
//! This module owns the QVM-specific compile/load/run entry points. The current
//! instruction executor and shared value-safety helpers now live in dedicated
//! modules; compiler integration and host built-ins remain in the reference crate.

mod bytecode;
pub(crate) mod gas;
pub(crate) mod limits;
pub(crate) mod runtime_support;
pub(crate) mod builtins;
pub(super) mod runtime;
mod value;
pub(crate) use bytecode::Op;
pub(crate) use value::Value;

use super::{deserialize_bytecode_v2, read_bytecode_source, write_bytecode_v2};

/// Compile QORVYNT source into a verified Bytecode v2 QVM artifact.
pub(super) fn compile(source_path: &str, output_path: &str) -> Result<(), String> {
    write_bytecode_v2(source_path, output_path)
}

/// An instruction stream that has passed the Bytecode v2 decoder and verifier.
///
/// Its private field prevents callers from wrapping unchecked opcodes. This is
/// the invariant-preserving boundary intended for the standalone VM loader.
pub(crate) struct VerifiedProgram {
    instructions: Vec<Op>,
}

impl VerifiedProgram {
    /// Borrow the verifier-approved instruction stream for execution.
    pub(crate) fn instructions(&self) -> &[Op] {
        &self.instructions
    }

    /// Consume the wrapper when transferring ownership to an executor.
    pub(crate) fn into_instructions(self) -> Vec<Op> {
        self.instructions
    }
}

/// Load only verified Bytecode v2 artifacts for the QVM-specific execution path.
/// Legacy ZENVORA headers remain readable during the migration window.
pub(crate) fn load_verified(path: &str) -> Result<VerifiedProgram, String> {
    let serialized = read_bytecode_source(path)?;
    if !(serialized.starts_with("QORVYNT_BYTECODE 2\n")
        || serialized.starts_with("ZENVORA_BYTECODE 2\n"))
    {
        return Err("QVM requires a verified Bytecode v2 artifact".into());
    }
    let instructions = deserialize_bytecode_v2(&serialized)?;
    Ok(VerifiedProgram { instructions })
}

/// Compatibility loader for existing CLI call sites.
pub(super) fn load(path: &str) -> Result<Vec<Op>, String> {
    Ok(load_verified(path)?.into_instructions())
}

/// Execute a verified QVM artifact through the QVM runtime API.
/// Output is supplied by the host, so the VM core does not own terminal I/O.
pub(crate) fn execute_with_output<F>(path: &str, mut output: F) -> Result<(), String>
where
    F: FnMut(String),
{
    let program = load_verified(path)?;
    runtime::run_with_output_metered(program.instructions(), &mut output, None)
}

/// CLI convenience wrapper; the execution engine itself remains output-sink based.
pub(super) fn execute(path: &str) -> Result<(), String> {
    execute_with_output(path, |line| println!("{line}"))
}
