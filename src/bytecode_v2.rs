//! Structural layout validation for the proposed Zenvora Bytecode v2 format.
//!
//! This module is an initial, independently testable foundation. It does not yet
//! parse v2 files, validate opcode control flow, or execute v2 bytecode. The v1
//! serializer/loader remain unchanged until those stages are implemented.

use std::collections::HashSet;

/// Maximum function parameter count, kept consistent with the VM safety limit.
pub const MAX_FUNCTION_PARAMETERS: usize = 1_024;

/// Declared return behavior for a v2 function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturnKind {
    Void,
    Value,
}

/// Explicit half-open instruction region for a function: [entry, end).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionRegion {
    pub entry: usize,
    pub end: usize,
    pub parameters: Vec<String>,
    pub return_kind: ReturnKind,
}

/// Complete layout for a v2 program. Instruction indices are absolute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgramLayout {
    pub instruction_count: usize,
    pub main_end: usize,
    pub functions: Vec<FunctionRegion>,
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Validate the region table before any opcode control-flow verification.
///
/// Main occupies [0, main_end). Function regions must then cover the remainder
/// of the instruction stream exactly, with no gaps, overlaps, or undeclared code.
pub fn validate_layout(layout: &ProgramLayout) -> Result<(), String> {
    if layout.instruction_count == 0 {
        return Err("bytecode v2: instruction stream must not be empty".into());
    }
    if layout.main_end == 0 || layout.main_end > layout.instruction_count {
        return Err("bytecode v2: invalid main-program region boundary".into());
    }

    let mut expected_entry = layout.main_end;
    let mut entries = HashSet::new();
    for (index, function) in layout.functions.iter().enumerate() {
        if function.entry != expected_entry {
            return Err(format!(
                "bytecode v2: function region {index} begins at {}, expected {expected_entry}; regions must be contiguous and ordered",
                function.entry
            ));
        }
        if function.entry >= function.end || function.end > layout.instruction_count {
            return Err(format!("bytecode v2: invalid function region [{}, {})", function.entry, function.end));
        }
        if !entries.insert(function.entry) {
            return Err(format!("bytecode v2: duplicate function entry {}", function.entry));
        }
        if function.parameters.len() > MAX_FUNCTION_PARAMETERS {
            return Err(format!(
                "bytecode v2: function at {} exceeds parameter safety limit ({MAX_FUNCTION_PARAMETERS})",
                function.entry
            ));
        }

        let mut names = HashSet::new();
        for name in &function.parameters {
            if !valid_identifier(name) {
                return Err(format!("bytecode v2: invalid parameter name in function at {}", function.entry));
            }
            if !names.insert(name) {
                return Err(format!("bytecode v2: duplicate parameter name in function at {}", function.entry));
            }
        }
        expected_entry = function.end;
    }

    if expected_entry != layout.instruction_count {
        return Err(format!(
            "bytecode v2: undeclared instruction region [{expected_entry}, {})",
            layout.instruction_count
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_layout, FunctionRegion, ProgramLayout, ReturnKind};

    fn function(entry: usize, end: usize, parameters: &[&str]) -> FunctionRegion {
        FunctionRegion {
            entry,
            end,
            parameters: parameters.iter().map(|name| (*name).to_string()).collect(),
            return_kind: ReturnKind::Value,
        }
    }

    fn layout(instruction_count: usize, main_end: usize, functions: Vec<FunctionRegion>) -> ProgramLayout {
        ProgramLayout { instruction_count, main_end, functions }
    }

    #[test]
    fn accepts_main_only_program() {
        assert!(validate_layout(&layout(3, 3, vec![])).is_ok());
    }

    #[test]
    fn accepts_contiguous_declared_functions() {
        let value = layout(8, 3, vec![function(3, 5, &["x"]), function(5, 8, &[])]);
        assert!(validate_layout(&value).is_ok());
    }

    #[test]
    fn rejects_empty_program_and_invalid_main_boundary() {
        assert!(validate_layout(&layout(0, 0, vec![])).unwrap_err().contains("must not be empty"));
        assert!(validate_layout(&layout(4, 0, vec![])).unwrap_err().contains("main-program region"));
        assert!(validate_layout(&layout(4, 5, vec![])).unwrap_err().contains("main-program region"));
    }

    #[test]
    fn rejects_gaps_and_overlapping_or_out_of_order_regions() {
        let gap = layout(8, 3, vec![function(4, 8, &[])]);
        assert!(validate_layout(&gap).unwrap_err().contains("contiguous and ordered"));

        let overlap = layout(8, 3, vec![function(3, 6, &[]), function(5, 8, &[])]);
        assert!(validate_layout(&overlap).unwrap_err().contains("contiguous and ordered"));

        let out_of_bounds = layout(8, 3, vec![function(3, 9, &[])]);
        assert!(validate_layout(&out_of_bounds).unwrap_err().contains("invalid function region"));
    }

    #[test]
    fn rejects_undeclared_trailing_instructions() {
        let value = layout(8, 3, vec![function(3, 6, &[])]);
        assert!(validate_layout(&value).unwrap_err().contains("undeclared instruction region"));
    }

    #[test]
    fn rejects_invalid_and_duplicate_parameter_names() {
        let invalid = layout(5, 2, vec![function(2, 5, &["not-valid"])]);
        assert!(validate_layout(&invalid).unwrap_err().contains("invalid parameter name"));

        let duplicate = layout(5, 2, vec![function(2, 5, &["x", "x"])]);
        assert!(validate_layout(&duplicate).unwrap_err().contains("duplicate parameter name"));
    }

    #[test]
    fn rejects_parameter_limit_excess() {
        let names: Vec<String> = (0..=super::MAX_FUNCTION_PARAMETERS).map(|i| format!("p{i}")).collect();
        let region = FunctionRegion {
            entry: 1,
            end: 3,
            parameters: names,
            return_kind: ReturnKind::Void,
        };
        let value = layout(3, 1, vec![region]);
        assert!(validate_layout(&value).unwrap_err().contains("parameter safety limit"));
    }
}
