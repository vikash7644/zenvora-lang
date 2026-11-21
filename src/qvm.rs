//! QORVYNT Virtual Machine artifact boundary.
//!
//! This module owns the QVM-specific compile/load/run entry points. The current
//! instruction executor is still hosted by the Rust reference CLI; moving this
//! boundary out first gives the runtime a stable seam for later extraction.

mod bytecode;
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


/// Execute verified in-memory bytecode through the QVM boundary.
///
/// Compiler services and deterministic runtimes should depend on this API rather
/// than reaching into the CLI module's host-executor implementation.
pub(super) fn execute_ops_with_output<F>(code: &[Op], output: &mut F) -> Result<(), String>
where
    F: FnMut(String),
{
    super::run_with_output(code, output)
}

#[cfg(test)]
mod tests {
    use super::{execute_ops_with_output, Op};

    #[test]
    fn execution_api_runs_in_memory_bytecode_and_captures_output() {
        let code = [Op::Push(42), Op::Print];
        let mut output = Vec::new();
        execute_ops_with_output(&code, &mut |line| output.push(line)).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn execution_api_propagates_runtime_errors() {
        let code = [Op::Div];
        let error = execute_ops_with_output(&code, &mut |_| {}).unwrap_err();
        assert!(error.contains("stack underflow"));
    }
}
