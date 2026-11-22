//! Standalone QORVYNT virtual-machine runtime crate.
//! This crate contains VM instructions, values, safety limits, built-ins,
//! gas accounting interfaces, and the execution engine. Compiler parsing,
//! source loading, and bytecode verification remain in the host compiler.

pub mod bytecode_v2;

pub mod qvm {
    pub mod bytecode;
    pub mod builtins;
    pub mod gas;
    pub mod limits;
    pub mod runtime;
    pub mod runtime_support;
    pub mod value;

    pub use bytecode::Op;
    pub use value::Value;
}

#[cfg(test)]
mod tests {
    use crate::qvm::{runtime::run_with_output_metered, Op};

    #[test]
    fn standalone_runtime_executes_arithmetic_and_emits_output() {
        let program = [Op::Push(40), Op::Push(2), Op::Add, Op::Print];
        let mut output = Vec::new();
        run_with_output_metered(&program, &mut |line| output.push(line), None).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn standalone_runtime_rejects_stack_underflow() {
        let program = [Op::Add];
        let error = run_with_output_metered(&program, &mut |_| {}, None).unwrap_err();
        assert!(error.contains("stack underflow"));
    }
}
