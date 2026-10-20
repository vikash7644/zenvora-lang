//! Zenvora Engine: one entry point for source checking, compilation, and VM execution.
//!
//! This facade intentionally delegates to the same Rust compiler and verified stack VM used
//! by the CLI. It is not a separate interpreter and does not claim full smart-contract support.
use std::path::Path;

/// Output from executing one Zenvora Lang program with the native compiler/VM.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    /// Lines emitted by `show` / `print` in program order.
    pub output: Vec<String>,
    /// Number of compiler instructions produced for the program.
    pub instruction_count: usize,
}

/// Entry point for embedding the current Zenvora compiler/VM inside this crate.
#[derive(Debug, Default, Clone, Copy)]
pub struct Engine;

impl Engine {
    /// Version of the Zenvora Lang package that contains this engine.
    pub fn version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    /// Compile and verify source without executing it. Returns instruction count.
    pub fn check(source: &str) -> Result<usize, String> {
        let code = super::compile(source)?;
        super::verify_bytecode(&code)?;
        Ok(code.len())
    }

    /// Compile source and execute it with the native Rust VM.
    pub fn run(source: &str) -> Result<ExecutionResult, String> {
        let code = super::compile(source)?;
        super::verify_bytecode(&code)?;
        let instruction_count = code.len();
        let mut output = Vec::new();
        super::run_with_output(&code, &mut |line| output.push(line))?;
        Ok(ExecutionResult { output, instruction_count })
    }

    /// Load a file and its relative imports, compile it, and verify bytecode without execution.
    pub fn check_file(path: impl AsRef<Path>) -> Result<usize, String> {
        let source = super::module_loader::load(path.as_ref())?;
        Self::check(&source)
    }

    /// Load a file and its relative `use "module.zv"` imports, then run the expanded program.
    pub fn run_file(path: impl AsRef<Path>) -> Result<ExecutionResult, String> {
        let source = super::module_loader::load(path.as_ref())?;
        Self::run(&source)
    }
}

#[cfg(test)]
mod tests {
    use super::Engine;

    #[test]
    fn engine_runs_source_through_compiler_and_native_vm() {
        let source = r#"
define start := 4
fn twice(value) {
    return value * 2
}
show twice(start + 3)
"#;
        let result = Engine::run(source).expect("program should compile and run");
        assert_eq!(result.output, vec!["14"]);
        assert!(result.instruction_count > 0);
    }

    #[test]
    fn engine_check_rejects_invalid_source_without_running_it() {
        let error = Engine::check("show @").unwrap_err();
        assert!(error.contains("unexpected character"), "unexpected diagnostic: {error}");
    }

    #[test]
    fn engine_returns_runtime_errors_instead_of_silently_succeeding() {
        let error = Engine::run("show 1 / 0").unwrap_err();
        assert!(error.contains("division by zero"), "unexpected error: {error}");
    }

    #[test]
    fn engine_version_matches_package_metadata() {
        assert_eq!(Engine::version(), env!("CARGO_PKG_VERSION"));
    }
}
