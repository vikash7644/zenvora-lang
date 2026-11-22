//! QORVYNT VM instruction execution loop.
//!
//! This module owns instruction dispatch and execution limits. Language
//! builtins and compiler-side helpers remain in the host module for now.

use super::super::*;

pub(super) fn run(code: &[Op]) -> Result<(), String> {
    run_with_output(code, &mut |line| println!("{line}"))
}

pub(super) fn run_with_output<F>(code: &[Op], output: &mut F) -> Result<(), String>
where
    F: FnMut(String),
{
    run_with_output_metered(code, output, None)
}

pub(super) fn run_with_gas<F>(code: &[Op], output: &mut F, meter: &mut contract_gas::GasMeter) -> Result<(), String>
where
    F: FnMut(String),
{
    run_with_output_metered(code, output, Some(meter))
}

pub(super) fn run_with_output_metered<F>(
    code: &[Op],
    output: &mut F,
    mut gas_meter: Option<&mut contract_gas::GasMeter>,
) -> Result<(), String>
where
    F: FnMut(String),
{
    if let Some(meter) = gas_meter.as_deref_mut() {
        meter.charge(contract_gas::BASE_EXECUTION_GAS)?;
    }
    let mut stack: Vec<Value> = Vec::new();
    let mut stack_value_bytes = 0usize;
    let mut runtime_state_bytes = 0usize;
    let mut vars: HashMap<String, Value> = HashMap::new();
    let mut globals: HashMap<String, Value> = HashMap::new();
    let mut pc = 0usize;
    let mut call_stack: Vec<(usize, HashMap<String, Value>)> = Vec::new();
    let mut steps = 0usize;
    const MAX_STEPS: usize = 1_000_000;
    const MAX_CALL_DEPTH: usize = 1_024;
    if code.len() > MAX_BYTECODE_INSTRUCTIONS {
        return Err(format!("program instruction count exceeds safety limit ({MAX_BYTECODE_INSTRUCTIONS})"));
    }
    while pc < code.len() {
        steps += 1;
        if steps > MAX_STEPS {
            return Err("execution exceeded safety limit (1,000,000 instructions)".into());
        }
        if let Some(meter) = gas_meter.as_deref_mut() {
            let cost = match code.get(pc) {
                Some(Op::Load(_)) => contract_gas::LOCAL_READ_GAS,
                Some(Op::Store(_) | Op::Declare(_) | Op::SetIndex(_)) => contract_gas::LOCAL_WRITE_GAS,
                Some(Op::JumpIfFalse(_) | Op::JumpIfFalseKeep(_) | Op::JumpIfTrueKeep(_)) => contract_gas::CONDITIONAL_BRANCH_GAS,
                Some(Op::Call(_, _) | Op::CallTarget { .. }) => contract_gas::FUNCTION_CALL_GAS,
                Some(Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Neg | Op::Not | Op::And | Op::Or
                    | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge) => contract_gas::INTEGER_OPERATION_GAS,
                Some(_) => 1,
                None => return Err("instruction pointer out of range".into()),
            };
            meter.charge(cost)?;
        }
        match &code[pc] {
            Op::Push(n) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Int(*n))?,
            Op::PushString(value) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Str(value.clone()))?,
            Op::PushBool(value) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Bool(*value))?,
            Op::Load(name) => { let value = vars.get(name).or_else(|| globals.get(name)).cloned().ok_or_else(|| format!("undefined variable '{name}'"))?; push_stack_value(&mut stack, &mut stack_value_bytes, value)?; },
            Op::Store(name) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                if let Some(old) = vars.get(name) {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, Some(old), &value)?;
                    vars.insert(name.clone(), value);
                    runtime_state_bytes = next;
                } else if let Some(old) = globals.get(name) {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, Some(old), &value)?;
                    globals.insert(name.clone(), value);
                    runtime_state_bytes = next;
                } else {
                    return Err(format!("assignment to undefined variable '{name}'; use 'define' first"));
                }
            }
            Op::Declare(name) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                if call_stack.is_empty() {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, globals.get(name), &value)?;
                    globals.insert(name.clone(), value);
                    runtime_state_bytes = next;
                } else {
                    let next = runtime_state_bytes_after_update(runtime_state_bytes, name, vars.get(name), &value)?;
                    vars.insert(name.clone(), value);
                    runtime_state_bytes = next;
                }
            }
            Op::Print => match pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")? {
                Value::Int(n) => output(format!("{n}")),
                Value::Str(value) => output(value),
                Value::Bool(value) => output(format!("{value}")),
                value @ (Value::Array(_) | Value::Map(_)) => output(value_to_string(&value)),
            },
            Op::SetIndex(name) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading indexed value")?;
                let index = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading collection index")?;
                let old_binding = vars.get(name).or_else(|| globals.get(name))
                    .ok_or_else(|| format!("undefined variable '{name}'"))?;
                let old_binding_size = binding_size(name, old_binding);
                if vars.contains_key(name) {
                    let collection = vars.get_mut(name).ok_or_else(|| format!("undefined variable '{name}'"))?;
                    set_indexed_value(collection, index, value)?;
                } else {
                    let collection = globals.get_mut(name).ok_or_else(|| format!("undefined variable '{name}'"))?;
                    set_indexed_value(collection, index, value)?;
                }
                let updated = vars.get(name).or_else(|| globals.get(name))
                    .ok_or_else(|| format!("undefined variable '{name}'"))?;
                ensure_value_depth(updated)?;
                let next = runtime_state_bytes_after_sizes(runtime_state_bytes, old_binding_size, binding_size(name, updated))?;
                runtime_state_bytes = next;
            }
            Op::Index => {
                let index = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading collection index")?;
                let collection = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading collection")?;
                match (collection, index) {
                    (Value::Array(items), Value::Int(index)) if index >= 0 => push_stack_value(&mut stack, &mut stack_value_bytes, items.get(index as usize).cloned().ok_or_else(|| format!("array index {index} out of bounds"))?)?,
                    (Value::Array(_), Value::Int(index)) => return Err(format!("array index {index} out of bounds")),
                    (Value::Array(_), _) => return Err("type error: array index must be an integer".into()),
                    (Value::Map(items), Value::Str(key)) => push_stack_value(&mut stack, &mut stack_value_bytes, items.get(&key).cloned().ok_or_else(|| format!("map key '{key}' not found"))?)?,
                    (Value::Map(_), _) => return Err("type error: map index must be a string".into()),
                    _ => return Err("type error: indexing expects an array or map".into()),
                }
            }
            Op::Neg => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                match value {
                    Value::Int(n) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Int(n.checked_neg().ok_or("integer overflow")?))?,
                    Value::Bool(_) | Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: unary '-' expects an integer".into()),
                }
            }
            Op::Not => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                match value {
                    Value::Bool(value) => push_stack_value(&mut stack, &mut stack_value_bytes, Value::Bool(!value))?,
                    Value::Int(_) | Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: 'not' expects a boolean".into()),
                }
            }
            Op::JumpIfFalse(target) => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                let is_false = match value {
                    Value::Bool(value) => !value,
                    Value::Int(n) => n == 0,
                    Value::Str(_) | Value::Array(_) | Value::Map(_) => return Err("type error: condition expects a boolean or integer".into()),
                };
                if is_false { pc = *target; continue; }
            }
            Op::JumpIfFalseKeep(target) => match stack.last() {
                Some(Value::Bool(false)) => { pc = *target; continue; }
                Some(Value::Bool(true)) => {},
                Some(_) => return Err("type error: 'and' expects booleans".into()),
                None => return Err("VM stack underflow".into()),
            },
            Op::JumpIfTrueKeep(target) => match stack.last() {
                Some(Value::Bool(true)) => { pc = *target; continue; }
                Some(Value::Bool(false)) => {},
                Some(_) => return Err("type error: 'or' expects booleans".into()),
                None => return Err("VM stack underflow".into()),
            },
            Op::Jump(target) => { pc = *target; continue; }
            Op::Call(name, _) => return Err(format!("unresolved function call '{name}'")),
            Op::Builtin(name, arg_count) => {
                if *arg_count > MAX_CALL_ARGUMENTS {
                    return Err(format!("builtin argument count exceeds safety limit ({MAX_CALL_ARGUMENTS})"));
                }
                let mut values = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count { values.push(pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading builtin arguments")?); }
                values.reverse();
                push_stack_value(&mut stack, &mut stack_value_bytes, builtin(name, values)?)?;
            }
            Op::CallTarget { target, params, arg_count } => {
                if *arg_count > MAX_CALL_ARGUMENTS || params.len() > MAX_CALL_ARGUMENTS {
                    return Err(format!("function argument count exceeds safety limit ({MAX_CALL_ARGUMENTS})"));
                }
                if *arg_count != params.len() {
                    return Err("invalid function call argument count".into());
                }
                if call_stack.len() >= MAX_CALL_DEPTH {
                    return Err(format!("maximum function call depth exceeded ({MAX_CALL_DEPTH})"));
                }
                let mut values = Vec::with_capacity(*arg_count);
                for _ in 0..*arg_count { values.push(pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow while reading function arguments")?); }
                values.reverse();
                let added_bytes = params.iter().zip(values.iter()).map(|(name, value)| binding_size(name, value))
                    .fold(0usize, |total, size| total.saturating_add(size));
                let next_state_bytes = runtime_state_bytes.saturating_add(added_bytes);
                if next_state_bytes > MAX_RUNTIME_STATE_BYTES {
                    return Err(format!("live variable/call-frame values exceed safety limit ({MAX_RUNTIME_STATE_BYTES} estimated bytes)"));
                }
                let mut local_vars = HashMap::new();
                for (name, value) in params.iter().zip(values) { local_vars.insert(name.clone(), value); }
                let caller_vars = std::mem::replace(&mut vars, local_vars);
                call_stack.push((pc + 1, caller_vars));
                runtime_state_bytes = next_state_bytes;
                pc = *target;
                continue;
            }
            Op::Return => {
                let local_bytes = binding_map_size(&vars);
                let (return_pc, caller_vars) = call_stack.pop().ok_or("return used outside a function call")?;
                runtime_state_bytes = runtime_state_bytes.saturating_sub(local_bytes);
                vars = caller_vars;
                pc = return_pc;
                continue;
            }
            Op::ReturnValue => {
                let value = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow in return expression")?;
                let local_bytes = binding_map_size(&vars);
                let (return_pc, caller_vars) = call_stack.pop().ok_or("return used outside a function call")?;
                runtime_state_bytes = runtime_state_bytes.saturating_sub(local_bytes);
                vars = caller_vars;
                push_stack_value(&mut stack, &mut stack_value_bytes, value)?;
                pc = return_pc;
                continue;
            }
            Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge | Op::And | Op::Or => {
                let b = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                let a = pop_stack_value(&mut stack, &mut stack_value_bytes, "VM stack underflow")?;
                let value = match (&code[pc], a, b) {
                    (Op::Add, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_add(b).ok_or("integer overflow")?),
                    (Op::Add, Value::Str(a), Value::Str(b)) => {
                        let combined_len = a.len().checked_add(b.len()).ok_or("string length overflow")?;
                        if combined_len > MAX_BYTECODE_STRING_BYTES {
                            return Err(format!("string exceeds safety limit ({} bytes)", MAX_BYTECODE_STRING_BYTES));
                        }
                        Value::Str(format!("{a}{b}"))
                    },
                    (Op::Add, Value::Str(_), Value::Int(_)) | (Op::Add, Value::Int(_), Value::Str(_)) => return Err("type error: '+' requires two integers or two strings".into()),
                    (Op::Sub, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_sub(b).ok_or("integer overflow")?),
                    (Op::Mul, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_mul(b).ok_or("integer overflow")?),
                    (Op::Div, Value::Int(_), Value::Int(0)) => return Err("division by zero".into()),
                    (Op::Div, Value::Int(a), Value::Int(b)) => Value::Int(a.checked_div(b).ok_or("integer overflow")?),
                    (Op::Eq, a, b) => Value::Bool(a == b),
                    (Op::Ne, a, b) => Value::Bool(a != b),
                    (Op::And, Value::Bool(a), Value::Bool(b)) => Value::Bool(a && b),
                    (Op::Or, Value::Bool(a), Value::Bool(b)) => Value::Bool(a || b),
                    (Op::And, _, _) => return Err("type error: 'and' expects booleans".into()),
                    (Op::Or, _, _) => return Err("type error: 'or' expects booleans".into()),
                    (Op::Lt, Value::Int(a), Value::Int(b)) => Value::Bool(a < b),
                    (Op::Le, Value::Int(a), Value::Int(b)) => Value::Bool(a <= b),
                    (Op::Gt, Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
                    (Op::Ge, Value::Int(a), Value::Int(b)) => Value::Bool(a >= b),
                    (Op::Add, _, _) => return Err("type error: '+' expects integers".into()),
                    (Op::Sub, _, _) => return Err("type error: '-' expects integers".into()),
                    (Op::Mul, _, _) => return Err("type error: '*' expects integers".into()),
                    (Op::Div, _, _) => return Err("type error: '/' expects integers".into()),
                    (Op::Lt | Op::Le | Op::Gt | Op::Ge, _, _) => return Err("type error: ordered comparisons expect integers".into()),
                    _ => unreachable!(),
                };
                push_stack_value(&mut stack, &mut stack_value_bytes, value)?;
            }
        }
        if stack.len() > MAX_STACK_VALUES {
            return Err(format!("operand stack exceeds safety limit ({MAX_STACK_VALUES} values)"));
        }
        pc += 1;
    }
    Ok(())
}
