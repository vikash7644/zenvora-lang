#![forbid(unsafe_code)]

//! Standalone QORVYNT virtual-machine runtime crate.
//! This crate contains VM instructions, values, safety limits, built-ins,
//! gas accounting interfaces, the execution engine, and Bytecode v2 parsing
//! and structural/control-flow verification. Source-language compilation remains host-owned.

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
    use crate::bytecode_v2::{decode_verified_v2_file, parse_and_verify_v2_file, parse_v2_file, verify_v2_file};

    #[test]
    fn standalone_bytecode_v2_verifier_accepts_valid_program() {
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 3\nCODE\nPUSH 42\nPRINT\nJUMP 3\n";
        let parsed = parse_v2_file(source).unwrap();
        verify_v2_file(&parsed).unwrap();
    }

    #[test]
    fn standalone_bytecode_v2_verifier_rejects_stack_underflow() {
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 2\nCODE\nADD\nJUMP 2\n";
        let parsed = parse_v2_file(source).unwrap();
        assert!(verify_v2_file(&parsed).unwrap_err().contains("stack underflow"));
    }

    #[test]
    fn standalone_bytecode_v2_decoder_materializes_verified_program() {
        let source = "QORVYNT_BYTECODE 2\\nPROGRAM 0 3\\nCODE\\nPUSH 42\\nPRINT\\nJUMP 3\\n";
        let parsed = parse_and_verify_v2_file(source).unwrap();
        let program = decode_verified_v2_file(&parsed).unwrap();
        let mut output = Vec::new();
        run_with_output_metered(&program, &mut |line| output.push(line), None).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn standalone_decoder_preserves_utf8_string_constants() {
        let source = "QORVYNT_BYTECODE 2\\nPROGRAM 0 3\\nCODE\\nPUSH_STRING_HEX 6869\\nPRINT\\nJUMP 3\\n";
        let parsed = parse_and_verify_v2_file(source).unwrap();
        let program = decode_verified_v2_file(&parsed).unwrap();
        let mut output = Vec::new();
        run_with_output_metered(&program, &mut |line| output.push(line), None).unwrap();
        assert_eq!(output, vec!["hi"]);
    }

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
