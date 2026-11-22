#![forbid(unsafe_code)]

//! Standalone QORVYNT virtual-machine runtime crate.
//! This crate contains VM instructions, values, safety limits, built-ins,
//! gas accounting interfaces, the execution engine, and Bytecode v2 parsing
//! and structural/control-flow verification. Source-language compilation remains host-owned.

pub mod bytecode_v2;


/// An executable instruction stream that can only be constructed after
/// Bytecode v2 parsing and verifier checks succeed.
pub struct VerifiedProgram {
    instructions: Vec<qvm::bytecode::Op>,
}

impl VerifiedProgram {
    /// Parse and verify an untrusted QORVYNT Bytecode v2 artifact.
    pub fn from_v2_source(source: &str) -> Result<Self, String> {
        let parsed = bytecode_v2::parse_and_verify_v2_file(source)?;
        let instructions = bytecode_v2::decode_verified_v2_file(&parsed)?;
        Ok(Self { instructions })
    }

    /// Borrow the verifier-approved instruction stream.
    pub fn instructions(&self) -> &[qvm::bytecode::Op] {
        &self.instructions
    }

    /// Execute the verified program with a host-provided output sink.
    pub fn execute_with_output<F>(&self, mut output: F) -> Result<(), String>
    where
        F: FnMut(String),
    {
        qvm::runtime::run_with_output_metered(&self.instructions, &mut output, None)
    }
}

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
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 3\nCODE\nPUSH 42\nPRINT\nJUMP 3\n";
        let parsed = parse_and_verify_v2_file(source).unwrap();
        let program = decode_verified_v2_file(&parsed).unwrap();
        let mut output = Vec::new();
        run_with_output_metered(&program, &mut |line| output.push(line), None).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn standalone_decoder_preserves_utf8_string_constants() {
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 3\nCODE\nPUSH_STRING_HEX 6869\nPRINT\nJUMP 3\n";
        let parsed = parse_and_verify_v2_file(source).unwrap();
        let program = decode_verified_v2_file(&parsed).unwrap();
        let mut output = Vec::new();
        run_with_output_metered(&program, &mut |line| output.push(line), None).unwrap();
        assert_eq!(output, vec!["hi"]);
    }

    #[test]
    fn verified_program_api_loads_and_executes_v2_artifacts() {
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 3\nCODE\nPUSH 42\nPRINT\nJUMP 3\n";
        let program = crate::VerifiedProgram::from_v2_source(source).unwrap();
        let mut output = Vec::new();
        program.execute_with_output(|line| output.push(line)).unwrap();
        assert_eq!(output, vec!["42"]);
    }

    #[test]
    fn verified_program_executes_declared_function_calls() {
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 4\nFUNCTION 4 6 1 value amount\nCODE\nPUSH 9\nCALL 4 1\nPRINT\nJUMP 4\nPUSH 1\nRETURN_VALUE\n";
        let program = crate::VerifiedProgram::from_v2_source(source).unwrap();
        let mut output = Vec::new();
        program.execute_with_output(|line| output.push(line)).unwrap();
        assert_eq!(output, vec!["1"]);
    }

    #[test]
    fn verified_program_executes_supported_builtins() {
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 4\nCODE\nPUSH_STRING_HEX 6869\nBUILTIN len 1\nPRINT\nJUMP 4\n";
        let program = crate::VerifiedProgram::from_v2_source(source).unwrap();
        let mut output = Vec::new();
        program.execute_with_output(|line| output.push(line)).unwrap();
        assert_eq!(output, vec!["2"]);
    }

    #[test]
    fn verified_program_api_rejects_unverified_artifacts() {
        let source = "QORVYNT_BYTECODE 2\nPROGRAM 0 2\nCODE\nADD\nJUMP 2\n";
        assert!(crate::VerifiedProgram::from_v2_source(source).is_err());
    }

    #[test]
    fn verifier_builtin_allowlist_has_runtime_dispatch_for_every_name() {
        let names = [
            "array", "map", "get", "len", "contains", "push", "put", "keys", "values",
            "hex_encode", "type_of", "to_string", "abs", "min", "max", "range", "assert",
            "sum", "starts_with", "ends_with", "split", "join", "replace", "char_at",
            "first", "last", "reverse", "is_empty", "clamp", "trim", "lower", "upper",
            "char_count", "to_int", "index_of", "unique", "assert_eq", "gcd", "lcm",
            "factorial", "is_prime", "fibonacci", "pow_int", "mean", "speed", "density",
            "molarity", "force", "kinetic_energy", "bmi_x10", "molar_mass",
            "dna_complement", "gc_content_percent",
        ];

        for name in names {
            assert!(crate::bytecode_v2::is_supported_builtin(name), "{name} missing from verifier allowlist");
            if let Err(error) = crate::qvm::builtins::builtin_unchecked(name, Vec::new()) {
                assert!(!error.starts_with("unknown builtin"), "{name} is accepted by verifier but missing runtime dispatch");
            }
        }
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
