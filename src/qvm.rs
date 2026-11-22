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

use super::{deserialize_bytecode_v2, read_bytecode_source, run, write_bytecode_v2};

/// Compile QORVYNT source into a verified Bytecode v2 QVM artifact.
pub(super) fn compile(source_path: &str, output_path: &str) -> Result<(), String> {
    write_bytecode_v2(source_path, output_path)
}

/// Load only verified Bytecode v2 artifacts for the QVM-specific execution path.
/// Legacy ZENVORA headers remain readable during the migration window.
pub(super) fn load(path: &str) -> Result<Vec<Op>, String> {
    let serialized = read_bytecode_source(path)?;
    if !(serialized.starts_with("QORVYNT_BYTECODE 2\n")
        || serialized.starts_with("ZENVORA_BYTECODE 2\n"))
    {
        return Err("QVM requires a verified Bytecode v2 artifact".into());
    }
    deserialize_bytecode_v2(&serialized)
}

/// Execute a previously verified QVM artifact using the current host executor.
pub(super) fn execute(path: &str) -> Result<(), String> {
    let code = load(path)?;
    run(&code)
}
