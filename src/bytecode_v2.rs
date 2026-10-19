//! Structural validation and metadata framing for the proposed Zenvora Bytecode v2 format.
//!
//! This module deliberately does not decode opcodes, verify control flow, or execute v2.
//! The v1 serializer/loader remain unchanged. V2 text parsing here only frames raw
//! instruction records and validates metadata/layout; it is not an execution-ready loader.

use std::collections::HashSet;

/// Maximum function parameter count, kept consistent with the VM safety limit.
pub const MAX_FUNCTION_PARAMETERS: usize = 1_024;
/// Defensive limit used before allocating the decoded instruction vector.
pub const MAX_V2_INSTRUCTIONS: usize = 1_000_000;
/// Defensive input bound for this textual metadata parser (16 MiB).
pub const MAX_V2_FILE_BYTES: usize = 16 * 1024 * 1024;

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

/// Parsed v2 framing. Instruction lines remain opaque until the opcode decoder
/// and control-flow verifier are implemented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedV2File {
    pub layout: ProgramLayout,
    pub instructions: Vec<String>,
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

/// Parse the provisional canonical v2 framing:
///
/// `ZENVORA_BYTECODE 2`
/// `PROGRAM 0 <main-end>`
/// `FUNCTION <entry> <end> <parameter-count> <void|value> <names...>`
/// `CODE`
/// `<one opaque instruction record per line>`
///
/// Zero-parameter functions use a single `-` placeholder. This parser validates
/// framing and metadata only; it does not make the instruction records executable.
pub fn parse_v2_file(input: &str) -> Result<ParsedV2File, String> {
    if input.len() > MAX_V2_FILE_BYTES {
        return Err("bytecode v2: file exceeds size limit".into());
    }

    let mut lines = input.lines();
    if lines.next().map(str::trim) != Some("ZENVORA_BYTECODE 2") {
        return Err("bytecode v2: missing or unsupported header".into());
    }

    let program_line = lines.next().ok_or_else(|| "bytecode v2: missing PROGRAM record".to_string())?;
    let program: Vec<&str> = program_line.split_whitespace().collect();
    if program.len() != 3 || program[0] != "PROGRAM" {
        return Err("bytecode v2: malformed PROGRAM record".into());
    }
    let program_start = parse_usize(program[1], "program start")?;
    let main_end = parse_usize(program[2], "main end")?;
    if program_start != 0 {
        return Err("bytecode v2: main program must start at instruction zero".into());
    }

    let mut functions = Vec::new();
    let mut found_code = false;
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err("bytecode v2: blank metadata line is not allowed".into());
        }
        if trimmed == "CODE" {
            found_code = true;
            break;
        }
        let fields: Vec<&str> = trimmed.split_whitespace().collect();
        if fields.first().copied() != Some("FUNCTION") || fields.len() < 6 {
            return Err("bytecode v2: expected FUNCTION record or CODE terminator".into());
        }
        let entry = parse_usize(fields[1], "function entry")?;
        let end = parse_usize(fields[2], "function end")?;
        let parameter_count = parse_usize(fields[3], "parameter count")?;
        if parameter_count > MAX_FUNCTION_PARAMETERS {
            return Err(format!("bytecode v2: parameter count exceeds safety limit ({MAX_FUNCTION_PARAMETERS})"));
        }
        let return_kind = match fields[4] {
            "void" => ReturnKind::Void,
            "value" => ReturnKind::Value,
            _ => return Err("bytecode v2: return kind must be 'void' or 'value'".into()),
        };
        let parameters = if parameter_count == 0 && fields.len() == 6 && fields[5] == "-" {
            Vec::new()
        } else {
            if fields.len() != 5 + parameter_count {
                return Err("bytecode v2: parameter count does not match FUNCTION record".into());
            }
            fields[5..].iter().map(|name| (*name).to_string()).collect()
        };
        functions.push(FunctionRegion { entry, end, parameters, return_kind });
    }

    if !found_code {
        return Err("bytecode v2: missing CODE terminator".into());
    }

    let mut instructions = Vec::new();
    for line in lines {
        let instruction = line.trim();
        if instruction.is_empty() {
            return Err("bytecode v2: blank instruction record is not allowed".into());
        }
        if instructions.len() >= MAX_V2_INSTRUCTIONS {
            return Err("bytecode v2: instruction count exceeds safety limit".into());
        }
        instructions.push(instruction.to_string());
    }

    let layout = ProgramLayout { instruction_count: instructions.len(), main_end, functions };
    validate_layout(&layout)?;
    Ok(ParsedV2File { layout, instructions })
}

/// Serialize a structurally valid layout and its opaque instruction records.
/// This is a framing round-trip helper, not an opcode serializer.
pub fn serialize_v2_file(file: &ParsedV2File) -> Result<String, String> {
    if file.instructions.len() != file.layout.instruction_count {
        return Err("bytecode v2: instruction vector length disagrees with layout".into());
    }
    validate_layout(&file.layout)?;

    let mut output = String::from("ZENVORA_BYTECODE 2\n");
    output.push_str(&format!("PROGRAM 0 {}\n", file.layout.main_end));
    for function in &file.layout.functions {
        let kind = match function.return_kind {
            ReturnKind::Void => "void",
            ReturnKind::Value => "value",
        };
        output.push_str(&format!(
            "FUNCTION {} {} {} {}",
            function.entry, function.end, function.parameters.len(), kind
        ));
        if function.parameters.is_empty() {
            output.push_str(" -");
        } else {
            for parameter in &function.parameters {
                if !valid_identifier(parameter) {
                    return Err("bytecode v2: invalid parameter name".into());
                }
                output.push(' ');
                output.push_str(parameter);
            }
        }
        output.push('\n');
    }
    output.push_str("CODE\n");
    for instruction in &file.instructions {
        if instruction.trim().is_empty() || instruction.contains(['\n', '\r']) {
            return Err("bytecode v2: invalid instruction record".into());
        }
        output.push_str(instruction);
        output.push('\n');
    }
    if output.len() > MAX_V2_FILE_BYTES {
        return Err("bytecode v2: serialized file exceeds size limit".into());
    }
    Ok(output)
}

fn parse_usize(value: &str, field: &str) -> Result<usize, String> {
    value.parse::<usize>().map_err(|_| format!("bytecode v2: invalid {field}"))
}

#[cfg(test)]
mod tests {
    use super::{parse_v2_file, serialize_v2_file, validate_layout, FunctionRegion, ParsedV2File, ProgramLayout, ReturnKind};

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
        let region = FunctionRegion { entry: 1, end: 3, parameters: names, return_kind: ReturnKind::Void };
        let value = layout(3, 1, vec![region]);
        assert!(validate_layout(&value).unwrap_err().contains("parameter safety limit"));
    }

    #[test]
    fn parses_and_round_trips_metadata_and_opaque_code() {
        let source = "ZENVORA_BYTECODE 2\nPROGRAM 0 2\nFUNCTION 2 4 1 value amount\nFUNCTION 4 6 0 void -\nCODE\nPUSH 7\nHALT\nRET\nNOP\n";
        let parsed = parse_v2_file(source).unwrap();
        assert_eq!(parsed.layout.main_end, 2);
        assert_eq!(parsed.layout.functions[0].parameters, vec!["amount"]);
        assert_eq!(parsed.layout.functions[1].return_kind, ReturnKind::Void);
        assert_eq!(serialize_v2_file(&parsed).unwrap(), source);
    }

    #[test]
    fn rejects_missing_or_unknown_headers_and_missing_code() {
        assert!(parse_v2_file("PROGRAM 0 1\nCODE\nHALT\n").unwrap_err().contains("header"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 9\nPROGRAM 0 1\nCODE\nHALT\n").unwrap_err().contains("header"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nHALT\n").unwrap_err().contains("FUNCTION record or CODE"));
    }

    #[test]
    fn rejects_malformed_metadata_and_parameter_mismatch() {
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 1 2\nCODE\nHALT\n").unwrap_err().contains("start at instruction zero"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 x\nCODE\nHALT\n").unwrap_err().contains("main end"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nFUNCTION 1 2 1 value -\nCODE\nHALT\n").unwrap_err().contains("parameter count"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nFUNCTION 1 2 0 maybe -\nCODE\nHALT\n").unwrap_err().contains("return kind"));
    }

    #[test]
    fn rejects_blank_code_and_uncovered_regions() {
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nCODE\nHALT\n\n").unwrap_err().contains("blank instruction"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nFUNCTION 1 2 0 void -\nCODE\nHALT\n").unwrap_err().contains("undeclared instruction region"));
    }

    #[test]
    fn rejects_inconsistent_layout_before_serializing() {
        let file = ParsedV2File {
            layout: layout(2, 1, vec![]),
            instructions: vec!["HALT".into()],
        };
        assert!(serialize_v2_file(&file).unwrap_err().contains("vector length"));
    }
}
