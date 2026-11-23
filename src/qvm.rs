//! QORVYNT Virtual Machine artifact boundary.
//!
//! This module owns the QVM-specific compile/load/run entry points. The current
//! instruction executor is still hosted by the Rust reference CLI; moving this
//! boundary out first gives the runtime a stable seam for later extraction.

mod bytecode;
mod gas;
mod value;
pub(crate) use bytecode::Op;
pub(crate) use gas::instruction_gas_cost;
pub(crate) use value::{check_map_capacity, set_indexed_value, binding_size, binding_map_size, runtime_state_bytes_after_sizes, runtime_state_bytes_after_update, ensure_value_depth, pop_stack_value, push_stack_value, value_size, value_to_string, Value, MAX_STACK_VALUE_BYTES, MAX_STACK_VALUES, MAX_VM_CALL_DEPTH, MAX_VM_EXECUTION_STEPS, MAX_VALUE_BYTES, MAX_VALUE_DEPTH, MAX_RUNTIME_STATE_BYTES};

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
