//! Experimental Zenvora Bytecode v2 framing, metadata, and control-flow verification.
//!
//! The v2 parser/verifier is called by the bytecode loader before instructions are passed
//! to the existing VM. The compiler can emit v2 text on an opt-in CLI path. V1 remains
//! the default format. V2 is experimental: bounded libFuzzer campaigns and deterministic
//! malformed-input tests exist, but no independent security audit has been completed.

use std::collections::{HashMap, HashSet};

/// Maximum function parameter count, kept consistent with the VM safety limit.
pub const MAX_FUNCTION_PARAMETERS: usize = 1_024;
/// Defensive limit used before allocating the decoded instruction vector.
pub const MAX_V2_INSTRUCTIONS: usize = 1_000_000;
/// Defensive input bound for this textual metadata parser (16 MiB).
pub const MAX_V2_FILE_BYTES: usize = 16 * 1024 * 1024;
/// Defensive decoded string limit, matching the current VM's string bound.
pub const MAX_V2_STRING_BYTES: usize = 1024 * 1024;

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

/// Parsed v2 framing and instruction records, ready for opcode verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedV2File {
    pub layout: ProgramLayout,
    pub instructions: Vec<String>,
}

fn valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let syntax_ok = matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    syntax_ok && !matches!(
        name,
        "true" | "false" | "and" | "or" | "not" |
        "define" | "show" | "print" | "if" | "otherwise" | "repeat" |
        "break" | "continue" | "return" | "call" | "fn" | "use" |
        "contract" | "state" | "action" | "view"
    )
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
        if instruction.trim().is_empty() || (instruction.contains('\n') || instruction.contains('\r')) {
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

/// Parsed opcode model used only by the v2 verifier. V2 calls refer to explicit
/// metadata entries as \`CALL <entry> <argument-count>\`; parameter names come
/// exclusively from the FUNCTION metadata record.
#[derive(Debug, Clone, PartialEq, Eq)]
enum V2Instruction {
    Push,
    PushString,
    PushBool,
    Load,
    Store,
    Declare,
    Add,
    Sub,
    Mul,
    Div,
    Neg,
    Not,
    And,
    Or,
    Print,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Index,
    SetIndex,
    JumpIfFalse(usize),
    JumpIfFalseKeep(usize),
    JumpIfTrueKeep(usize),
    Jump(usize),
    Call { target: usize, argument_count: usize },
    Builtin { argument_count: usize },
    Return,
    ReturnValue,
}

fn parse_operand_usize(value: &str, description: &str) -> Result<usize, String> {
    value.parse::<usize>().map_err(|_| format!("bytecode v2: invalid {description}"))
}

fn parse_v2_instruction(line: &str) -> Result<V2Instruction, String> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let invalid = || format!("bytecode v2: malformed instruction '{line}'");
    let instruction = match fields.as_slice() {
        ["PUSH", value] if value.parse::<i64>().is_ok() => V2Instruction::Push,
        ["PUSH_STRING_HEX", hex] => {
            if *hex != "-" && (hex.len() % 2 != 0 || hex.len() / 2 > MAX_V2_STRING_BYTES) {
                return Err(invalid());
            }
            if *hex != "-" && !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(invalid());
            }
            V2Instruction::PushString
        }
        ["PUSH_BOOL", "true" | "false"] => V2Instruction::PushBool,
        ["LOAD", name] if valid_identifier(name) => V2Instruction::Load,
        ["STORE", name] if valid_identifier(name) => V2Instruction::Store,
        ["DECLARE", name] if valid_identifier(name) => V2Instruction::Declare,
        ["ADD"] => V2Instruction::Add,
        ["SUB"] => V2Instruction::Sub,
        ["MUL"] => V2Instruction::Mul,
        ["DIV"] => V2Instruction::Div,
        ["NEG"] => V2Instruction::Neg,
        ["NOT"] => V2Instruction::Not,
        ["AND"] => V2Instruction::And,
        ["OR"] => V2Instruction::Or,
        ["PRINT"] => V2Instruction::Print,
        ["EQ"] => V2Instruction::Eq,
        ["NE"] => V2Instruction::Ne,
        ["LT"] => V2Instruction::Lt,
        ["LE"] => V2Instruction::Le,
        ["GT"] => V2Instruction::Gt,
        ["GE"] => V2Instruction::Ge,
        ["INDEX"] => V2Instruction::Index,
        ["SET_INDEX", name] if valid_identifier(name) => V2Instruction::SetIndex,
        ["JUMP", target] => V2Instruction::Jump(parse_operand_usize(target, "jump target")?),
        ["JUMP_IF_FALSE", target] => V2Instruction::JumpIfFalse(parse_operand_usize(target, "jump target")?),
        ["JUMP_IF_FALSE_KEEP", target] => V2Instruction::JumpIfFalseKeep(parse_operand_usize(target, "jump target")?),
        ["JUMP_IF_TRUE_KEEP", target] => V2Instruction::JumpIfTrueKeep(parse_operand_usize(target, "jump target")?),
        ["CALL", target, count] => {
            let target = parse_operand_usize(target, "call target")?;
            let argument_count = parse_operand_usize(count, "call argument count")?;
            if argument_count > MAX_FUNCTION_PARAMETERS {
                return Err(format!("bytecode v2: call argument count exceeds safety limit ({MAX_FUNCTION_PARAMETERS})"));
            }
            V2Instruction::Call { target, argument_count }
        }
        ["BUILTIN", name, count] if valid_identifier(name) => {
            let argument_count = parse_operand_usize(count, "builtin argument count")?;
            if argument_count > MAX_FUNCTION_PARAMETERS {
                return Err(format!("bytecode v2: builtin argument count exceeds safety limit ({MAX_FUNCTION_PARAMETERS})"));
            }
            V2Instruction::Builtin { argument_count }
        }
        ["RETURN"] => V2Instruction::Return,
        ["RETURN_VALUE"] => V2Instruction::ReturnValue,
        _ => return Err(invalid()),
    };
    Ok(instruction)
}

fn region_for_pc(layout: &ProgramLayout, pc: usize) -> Option<(usize, usize, Option<usize>)> {
    if pc < layout.main_end {
        return Some((0, layout.main_end, None));
    }

    // Regions are validated as contiguous and ordered, so binary search avoids
    // scanning the whole function table for every instruction in large programs.
    let insertion = layout.functions.partition_point(|function| function.entry <= pc);
    let function = layout.functions.get(insertion.checked_sub(1)?)?;
    (pc < function.end).then_some((function.entry, function.end, Some(function.entry)))
}

fn function_for_entry(layout: &ProgramLayout, entry: usize) -> Option<&FunctionRegion> {
    layout.functions
        .binary_search_by_key(&entry, |function| function.entry)
        .ok()
        .map(|index| &layout.functions[index])
}

/// Verify opcode syntax, every jump/call target (including unreachable code), and
/// reachable control-flow stack effects for each region independently.
///
/// Each disconnected unreachable component is conservatively analyzed from an empty
/// operand stack. This is a validation policy, not proof that the code is reachable.
pub fn verify_v2_file(file: &ParsedV2File) -> Result<(), String> {
    if file.instructions.len() != file.layout.instruction_count {
        return Err("bytecode v2: instruction vector length disagrees with layout".into());
    }
    validate_layout(&file.layout)?;
    if file.instructions.len() > MAX_V2_INSTRUCTIONS {
        return Err("bytecode v2: instruction count exceeds safety limit".into());
    }

    let parsed: Vec<V2Instruction> = file.instructions.iter()
        .map(|line| parse_v2_instruction(line))
        .collect::<Result<_, _>>()?;

    // Structural checks run on every instruction, even unreachable instructions.
    for (pc, instruction) in parsed.iter().enumerate() {
        let (start, end, _) = region_for_pc(&file.layout, pc)
            .ok_or_else(|| format!("bytecode v2: instruction {pc} is outside a declared region"))?;
        match instruction {
            V2Instruction::Jump(target)
            | V2Instruction::JumpIfFalse(target)
            | V2Instruction::JumpIfFalseKeep(target)
            | V2Instruction::JumpIfTrueKeep(target) => {
                let is_main_exit = start == 0
                    && pc + 1 == end
                    && matches!(instruction, V2Instruction::Jump(target) if *target == end);
                if is_main_exit {
                    continue;
                }
                if *target < start || *target >= end {
                    return Err(format!("bytecode v2: jump at instruction {pc} escapes its region to {target}"));
                }
            }
            V2Instruction::Call { target, argument_count } => {
                let function = function_for_entry(&file.layout, *target)
                    .ok_or_else(|| format!("bytecode v2: call at instruction {pc} targets undeclared function {target}"))?;
                if function.parameters.len() != *argument_count {
                    return Err(format!(
                        "bytecode v2: call at instruction {pc} passes {argument_count} argument(s), but function {target} declares {}",
                        function.parameters.len()
                    ));
                }
            }
            _ => {}
        }
    }

    for (start, end, is_main, return_kind) in std::iter::once((0, file.layout.main_end, true, None))
        .chain(file.layout.functions.iter().map(|f| (f.entry, f.end, false, Some(f.return_kind))))
    {
        let mut depths: HashMap<usize, usize> = HashMap::new();
        let mut pending = vec![(start, 0usize)];
        let mut reached_exit = false;
        let mut entry_walk = true;

        loop {
        while let Some((pc, incoming_depth)) = pending.pop() {
            if pc == end && is_main {
                if entry_walk { reached_exit = true; }
                if incoming_depth != 0 {
                    return Err(format!("bytecode v2: main exits with {incoming_depth} operand value(s)"));
                }
                continue;
            }
            if pc < start || pc >= end {
                return Err(format!("bytecode v2: control flow escapes region at instruction {pc}"));
            }
            if let Some(previous) = depths.get(&pc) {
                if *previous != incoming_depth {
                    return Err(format!("bytecode v2: inconsistent stack depth at instruction {pc} ({previous} vs {incoming_depth})"));
                }
                continue;
            }
            depths.insert(pc, incoming_depth);

            let instruction = &parsed[pc];
            let (required, delta): (usize, isize) = match instruction {
                V2Instruction::Push | V2Instruction::PushString | V2Instruction::PushBool | V2Instruction::Load => (0, 1),
                V2Instruction::Store | V2Instruction::Declare | V2Instruction::Print
                | V2Instruction::Neg | V2Instruction::Not | V2Instruction::JumpIfFalse(_) => (1, -1),
                V2Instruction::SetIndex => (2, -2),
                V2Instruction::Index => (2, -1),
                V2Instruction::Add | V2Instruction::Sub | V2Instruction::Mul | V2Instruction::Div
                | V2Instruction::And | V2Instruction::Or | V2Instruction::Eq | V2Instruction::Ne
                | V2Instruction::Lt | V2Instruction::Le | V2Instruction::Gt | V2Instruction::Ge => (2, -1),
                V2Instruction::JumpIfFalseKeep(_) | V2Instruction::JumpIfTrueKeep(_) => (1, 0),
                V2Instruction::Jump(_) | V2Instruction::Return => (0, 0),
                V2Instruction::Builtin { argument_count } => (*argument_count, 1 - *argument_count as isize),
                V2Instruction::Call { target, argument_count } => {
                    let function = function_for_entry(&file.layout, *target)
                        .ok_or_else(|| format!("bytecode v2: undeclared call target {target}"))?;
                    let result: isize = if function.return_kind == ReturnKind::Value { 1 } else { 0 };
                    (*argument_count, result - *argument_count as isize)
                }
                V2Instruction::ReturnValue => (1, -1),
            };
            if incoming_depth < required {
                return Err(format!("bytecode v2: operand stack underflow at instruction {pc}"));
            }
            let outgoing_depth = if delta < 0 {
                incoming_depth.checked_sub((-delta) as usize)
                    .ok_or_else(|| format!("bytecode v2: stack underflow at instruction {pc}"))?
            } else {
                incoming_depth.checked_add(delta as usize)
                    .ok_or_else(|| "bytecode v2: stack depth overflow".to_string())?
            };
            if outgoing_depth > 65_536 {
                return Err(format!("bytecode v2: operand stack exceeds safety limit at instruction {pc}"));
            }

            match instruction {
                V2Instruction::Jump(target) => {
                    if is_main && pc + 1 == end && *target == end {
                        if entry_walk { reached_exit = true; }
                        if outgoing_depth != 0 {
                            return Err(format!("bytecode v2: main exits with {outgoing_depth} operand value(s)"));
                        }
                    } else {
                        pending.push((*target, outgoing_depth));
                    }
                }
                V2Instruction::JumpIfFalse(target)
                | V2Instruction::JumpIfFalseKeep(target)
                | V2Instruction::JumpIfTrueKeep(target) => {
                    pending.push((*target, outgoing_depth));
                    if pc + 1 < end {
                        pending.push((pc + 1, outgoing_depth));
                    } else if !is_main {
                        return Err(format!("bytecode v2: function falls through at instruction {pc}"));
                    } else {
                        return Err(format!("bytecode v2: main falls through at instruction {pc}"));
                    }
                }
                V2Instruction::Return | V2Instruction::ReturnValue => {
                    if is_main {
                        return Err(format!("bytecode v2: return instruction outside a function at {pc}"));
                    }
                    let expected_kind = if matches!(instruction, V2Instruction::Return) {
                        ReturnKind::Void
                    } else {
                        ReturnKind::Value
                    };
                    if return_kind != Some(expected_kind) {
                        return Err(format!("bytecode v2: return kind does not match function metadata at instruction {pc}"));
                    }
                    let expected_depth = if expected_kind == ReturnKind::Void { 0 } else { 1 };
                    if incoming_depth != expected_depth {
                        return Err(format!("bytecode v2: return at instruction {pc} has {incoming_depth} operand value(s), expected {expected_depth}"));
                    }
                }
                _ => {
                    if pc + 1 < end {
                        pending.push((pc + 1, outgoing_depth));
                    } else if !is_main {
                        return Err(format!("bytecode v2: function falls through at instruction {pc}"));
                    } else {
                        return Err(format!("bytecode v2: main falls through at instruction {pc}"));
                    }
                }
            }
        }
        if is_main && entry_walk && !reached_exit {
            return Err("bytecode v2: main region has no reachable exit".into());
        }
        entry_walk = false;
        if let Some(unreachable_entry) = (start..end).find(|pc| !depths.contains_key(pc)) {
            // Explicit policy: each disconnected unreachable component starts with an
            // empty operand stack. This rejects dead blocks that rely on imaginary
            // incoming values while still checking their stack/control-flow safety.
            pending.push((unreachable_entry, 0));
        } else {
            break;
        }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{parse_v2_file, serialize_v2_file, validate_layout, verify_v2_file, FunctionRegion, ParsedV2File, ProgramLayout, ReturnKind};

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
        let reserved = layout(5, 2, vec![function(2, 5, &["true"])]);
        assert!(validate_layout(&reserved).unwrap_err().contains("invalid parameter name"));
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
        let source = "ZENVORA_BYTECODE 2\nPROGRAM 0 2\nFUNCTION 2 4 1 value amount\nFUNCTION 4 6 0 void -\nCODE\nPUSH 7\nHALT\nRET\nNOP\nRET\nNOP\n";
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
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nFUNCTION 1 2 2 value only\nCODE\nHALT\n").unwrap_err().contains("parameter count"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nFUNCTION 1 2 0 maybe -\nCODE\nHALT\n").unwrap_err().contains("return kind"));
    }

    #[test]
    fn rejects_blank_code_and_uncovered_regions() {
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nCODE\nHALT\n\n").unwrap_err().contains("blank instruction"));
        assert!(parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 1\nCODE\nHALT\nNOP\n").unwrap_err().contains("undeclared instruction region"));
    }

    #[test]
    fn rejects_inconsistent_layout_before_serializing() {
        let file = ParsedV2File {
            layout: layout(2, 1, vec![]),
            instructions: vec!["HALT".into()],
        };
        assert!(serialize_v2_file(&file).unwrap_err().contains("vector length"));
    }

    #[test]
    fn verifier_accepts_metadata_driven_function_call_and_return() {
        let file = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 4\nFUNCTION 4 6 1 value amount\nCODE\nPUSH 9\nCALL 4 1\nPRINT\nJUMP 4\nPUSH 1\nRETURN_VALUE\n",
        ).unwrap();
        assert!(verify_v2_file(&file).is_ok());
    }

    #[test]
    fn verifier_rejects_unknown_opcode_and_bad_call_target() {
        let unknown = parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 3\nCODE\nJUMP 2\nMAGIC\nJUMP 3\n").unwrap();
        assert!(verify_v2_file(&unknown).unwrap_err().contains("malformed instruction"));

        let bad_call = parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 2\nCODE\nCALL 2 0\nJUMP 2\n").unwrap();
        assert!(verify_v2_file(&bad_call).unwrap_err().contains("undeclared function"));
    }

    #[test]
    fn verifier_rejects_cross_region_jumps_and_wrong_argument_count() {
        let cross_jump = parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 3\nCODE\nJUMP 3\nPUSH 1\nJUMP 3\n").unwrap();
        assert!(verify_v2_file(&cross_jump).unwrap_err().contains("escapes its region"));

        let wrong_args = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 2\nFUNCTION 2 4 1 value item\nCODE\nCALL 2 0\nJUMP 2\nPUSH 1\nRETURN_VALUE\n",
        ).unwrap();
        assert!(verify_v2_file(&wrong_args).unwrap_err().contains("passes 0 argument(s)"));
    }

    #[test]
    fn verifier_rejects_stack_underflow_inconsistent_joins_and_main_residue() {
        let underflow = parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 2\nCODE\nPRINT\nJUMP 2\n").unwrap();
        assert!(verify_v2_file(&underflow).unwrap_err().contains("stack underflow"));

        let join = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 7\nCODE\nPUSH_BOOL true\nJUMP_IF_FALSE 5\nPUSH 1\nJUMP 5\nPUSH 2\nPRINT\nJUMP 7\n",
        ).unwrap();
        assert!(verify_v2_file(&join).unwrap_err().contains("inconsistent stack depth"));

        let residue = parse_v2_file("ZENVORA_BYTECODE 2\nPROGRAM 0 2\nCODE\nPUSH 1\nJUMP 2\n").unwrap();
        assert!(verify_v2_file(&residue).unwrap_err().contains("main exits with 1 operand value(s)"));
    }

    #[test]
    fn verifier_rejects_function_return_kind_mismatch() {
        let file = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 1\nFUNCTION 1 3 0 void -\nCODE\nJUMP 1\nPUSH 1\nRETURN_VALUE\n",
        ).unwrap();
        assert!(verify_v2_file(&file).unwrap_err().contains("return kind does not match"));
    }


    #[test]
    fn verifier_checks_disconnected_unreachable_components_with_empty_stack_policy() {
        let unreachable_underflow = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 3\nCODE\nJUMP 2\nPRINT\nJUMP 3\n",
        ).unwrap();
        assert!(verify_v2_file(&unreachable_underflow).unwrap_err().contains("stack underflow"));

        let unreachable_loop = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 3\nCODE\nJUMP 2\nJUMP 1\nJUMP 3\n",
        ).unwrap();
        assert!(verify_v2_file(&unreachable_loop).is_ok());
    }


    #[test]
    fn verifier_checks_declared_but_uncalled_function_regions() {
        let file = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 2\nFUNCTION 2 4 0 void -\nCODE\nJUMP 1\nJUMP 2\nPRINT\nRETURN\n",
        ).unwrap();
        assert!(verify_v2_file(&file).unwrap_err().contains("stack underflow"));
    }

    #[test]
    fn parser_survives_deterministic_bounded_fuzz_smoke_inputs() {
        // Deterministic, dependency-free parser fuzz smoke test for CI. This is
        // not a substitute for longer-running cargo-fuzz/property-based campaigns.
        const ALPHABET: &[u8] = b"ZENVORA_BYTECODE 2\nPROGRAM FUNCTION CODE PUSH CALL JUMP RETURN_VALUE true false -_0123456789";
        let mut state = 0x5eed_u64;
        for case_index in 0..4_096usize {
            state ^= (case_index as u64).wrapping_mul(0x9e37_79b9);
            let length = (state as usize) % 192;
            let mut input = String::with_capacity(length + 32);
            if case_index % 3 == 0 {
                input.push_str("ZENVORA_BYTECODE 2\n");
            }
            for _ in 0..length {
                state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                input.push(ALPHABET[(state as usize) % ALPHABET.len()] as char);
            }
            let _ = super::parse_v2_file(&input);
        }
    }

    #[test]
    fn rejects_integer_boundaries_and_malformed_instruction_operands() {
        let too_large = "ZENVORA_BYTECODE 2\nPROGRAM 0 184467440737095516160\nCODE\nJUMP 1\n";
        assert!(parse_v2_file(too_large).unwrap_err().contains("invalid main end"));

        let malformed = [
            "PUSH nope",
            "PUSH_STRING_HEX abc",
            "PUSH_STRING_HEX gg",
            "CALL 184467440737095516160 0",
            "CALL 1 1025",
            "BUILTIN array 1025",
            "JUMP nope",
            "LOAD bad-name",
            "PUSH_BOOL maybe",
        ];
        for instruction in malformed {
            assert!(
                super::parse_v2_instruction(instruction).is_err(),
                "expected malformed instruction to be rejected: {instruction}"
            );
        }
    }

    #[test]
    fn verifier_rejects_invalid_jump_in_unreachable_code() {
        let file = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 3\nCODE\nJUMP 2\nJUMP 99\nJUMP 3\n",
        ).unwrap();
        assert!(verify_v2_file(&file).unwrap_err().contains("escapes its region"));
    }


    #[test]
    fn rejects_file_and_decoded_string_size_limits_before_use() {
        let oversized_file = "x".repeat(super::MAX_V2_FILE_BYTES + 1);
        assert!(parse_v2_file(&oversized_file).unwrap_err().contains("file exceeds size limit"));

        let oversized_hex = "a".repeat(super::MAX_V2_STRING_BYTES * 2 + 2);
        assert!(super::parse_v2_instruction(&format!("PUSH_STRING_HEX {oversized_hex}"))
            .unwrap_err().contains("malformed instruction"));
    }

    #[test]
    fn rejects_branch_targets_at_region_end_except_main_exit_jump() {
        let main_conditional_exit = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 1\nCODE\nJUMP_IF_FALSE 1\n",
        ).unwrap();
        assert!(verify_v2_file(&main_conditional_exit).unwrap_err().contains("escapes its region"));

        let function_end_jump = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 1\nFUNCTION 1 3 0 void -\nCODE\nJUMP 1\nJUMP 3\nRETURN\n",
        ).unwrap();
        assert!(verify_v2_file(&function_end_jump).unwrap_err().contains("escapes its region"));
    }

    #[test]
    fn rejects_extreme_branch_and_call_operands_without_panicking() {
        let extreme_jump = parse_v2_file(
            "ZENVORA_BYTECODE 2\nPROGRAM 0 2\nCODE\nJUMP 18446744073709551615\nJUMP 2\n",
        ).unwrap();
        assert!(verify_v2_file(&extreme_jump).unwrap_err().contains("escapes its region"));

        let overflow_call = "CALL 184467440737095516160 0";
        assert!(super::parse_v2_instruction(overflow_call).is_err());
    }

    #[test]
    fn serializer_rejects_embedded_instruction_line_breaks() {
        let file = ParsedV2File {
            layout: layout(1, 1, vec![]),
            instructions: vec!["JUMP 1\nPUSH 99".into()],
        };
        assert!(serialize_v2_file(&file).unwrap_err().contains("invalid instruction record"));
    }

    #[test]
    fn region_lookup_respects_half_open_boundaries_with_binary_search() {
        let value = layout(7, 2, vec![function(2, 4, &[]), function(4, 7, &[])]);
        assert_eq!(super::region_for_pc(&value, 0), Some((0, 2, None)));
        assert_eq!(super::region_for_pc(&value, 2), Some((2, 4, Some(2))));
        assert_eq!(super::region_for_pc(&value, 3), Some((2, 4, Some(2))));
        assert_eq!(super::region_for_pc(&value, 4), Some((4, 7, Some(4))));
        assert_eq!(super::region_for_pc(&value, 6), Some((4, 7, Some(4))));
        assert_eq!(super::region_for_pc(&value, 7), None);
        assert_eq!(super::function_for_entry(&value, 4).map(|f| f.end), Some(7));
        assert_eq!(super::function_for_entry(&value, 3), None);
    }


}
