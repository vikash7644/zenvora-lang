//! Instruction set shared by the QORVYNT compiler and virtual machine.

/// Verified QORVYNT VM instruction representation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Op {
    Push(i64), PushString(String), PushBool(bool), Load(String), Store(String), Declare(String), Add, Sub, Mul, Div, Neg, Not, And, Or, Print,
    Eq, Ne, Lt, Le, Gt, Ge, Index, SetIndex(String), JumpIfFalse(usize), JumpIfFalseKeep(usize), JumpIfTrueKeep(usize), Jump(usize), Call(String, usize), Builtin(String, usize), CallTarget { target: usize, params: Vec<String>, arg_count: usize }, Return, ReturnValue,
}

/// Decode a hexadecimal string payload from a verified QORVYNT bytecode file.
pub(super) fn hex_decode(input: &str) -> Result<String, String> {
    if input == "-" {
        return Ok(String::new());
    }
    if input.len() / 2 > super::MAX_BYTECODE_STRING_BYTES {
        return Err(format!(
            "bytecode string exceeds safety limit ({} bytes)",
            super::MAX_BYTECODE_STRING_BYTES
        ));
    }
    if input.len() % 2 != 0 {
        return Err("invalid hex string in bytecode".into());
    }
    let mut bytes = Vec::with_capacity(input.len() / 2);
    for chunk in input.as_bytes().chunks_exact(2) {
        let digits = std::str::from_utf8(chunk)
            .map_err(|_| "invalid hex string in bytecode")?;
        bytes.push(
            u8::from_str_radix(digits, 16)
                .map_err(|_| "invalid hex string in bytecode")?,
        );
    }
    String::from_utf8(bytes).map_err(|_| "bytecode string is not valid UTF-8".into())
}

/// Parse and verify a QORVYNT Bytecode v2 artifact into executable QVM ops.
pub(super) fn deserialize_v2(source: &str) -> Result<Vec<Op>, String> {
    let file = crate::bytecode_v2::parse_v2_file(source)?;
    crate::bytecode_v2::verify_v2_file(&file)?;
    let mut code = Vec::with_capacity(file.instructions.len());

    for (pc, line) in file.instructions.iter().enumerate() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        let invalid = || format!("bytecode v2: malformed instruction at index {pc}");
        let op = match parts.as_slice() {
            ["PUSH", value] => Op::Push(value.parse().map_err(|_| invalid())?),
            ["PUSH_STRING_HEX", value] => Op::PushString(hex_decode(value)?),
            ["PUSH_BOOL", "true"] => Op::PushBool(true),
            ["PUSH_BOOL", "false"] => Op::PushBool(false),
            ["LOAD", name] if super::super::valid_identifier(name) => Op::Load((*name).into()),
            ["STORE", name] if super::super::valid_identifier(name) => Op::Store((*name).into()),
            ["DECLARE", name] if super::super::valid_identifier(name) => Op::Declare((*name).into()),
            ["ADD"] => Op::Add,
            ["SUB"] => Op::Sub,
            ["MUL"] => Op::Mul,
            ["DIV"] => Op::Div,
            ["NEG"] => Op::Neg,
            ["NOT"] => Op::Not,
            ["AND"] => Op::And,
            ["OR"] => Op::Or,
            ["PRINT"] => Op::Print,
            ["EQ"] => Op::Eq,
            ["NE"] => Op::Ne,
            ["LT"] => Op::Lt,
            ["LE"] => Op::Le,
            ["GT"] => Op::Gt,
            ["GE"] => Op::Ge,
            ["INDEX"] => Op::Index,
            ["SET_INDEX", name] if super::super::valid_identifier(name) => {
                Op::SetIndex((*name).into())
            }
            ["JUMP_IF_FALSE", target] => {
                Op::JumpIfFalse(target.parse().map_err(|_| invalid())?)
            }
            ["JUMP_IF_FALSE_KEEP", target] => {
                Op::JumpIfFalseKeep(target.parse().map_err(|_| invalid())?)
            }
            ["JUMP_IF_TRUE_KEEP", target] => {
                Op::JumpIfTrueKeep(target.parse().map_err(|_| invalid())?)
            }
            ["JUMP", target] => {
                let target: usize = target.parse().map_err(|_| invalid())?;
                // The main exit must skip the function regions in the shared VM.
                if pc + 1 == file.layout.main_end && target == file.layout.main_end {
                    Op::Jump(file.layout.instruction_count)
                } else {
                    Op::Jump(target)
                }
            }
            ["CALL", target, count] => {
                let target: usize = target.parse().map_err(|_| invalid())?;
                let arg_count: usize = count.parse().map_err(|_| invalid())?;
                let function = file
                    .layout
                    .functions
                    .iter()
                    .find(|function| function.entry == target)
                    .ok_or_else(|| {
                        format!(
                            "bytecode v2: call at instruction {pc} targets undeclared function {target}"
                        )
                    })?;
                Op::CallTarget {
                    target,
                    params: function.parameters.clone(),
                    arg_count,
                }
            }
            ["BUILTIN", name, count]
                if super::super::valid_identifier(name) && super::super::is_builtin(name) =>
            {
                Op::Builtin((*name).into(), count.parse().map_err(|_| invalid())?)
            }
            ["RETURN"] => Op::Return,
            ["RETURN_VALUE"] => Op::ReturnValue,
            _ => return Err(invalid()),
        };
        code.push(op);
    }
    Ok(code)
}

