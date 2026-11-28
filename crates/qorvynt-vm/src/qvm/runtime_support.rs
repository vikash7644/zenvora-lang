//! Shared QVM value, collection, and resource-accounting helpers.
//!
//! Kept separate from the CLI so the execution engine can become a standalone
//! runtime without duplicating value-safety rules.

use crate::qvm::Value;
use crate::qvm::limits::{MAX_RUNTIME_STATE_BYTES, MAX_STACK_VALUE_BYTES, MAX_VALUE_BYTES, MAX_VALUE_DEPTH, MAX_COLLECTION_ITEMS};
use crate::qvm::builtins::builtin_unchecked;
use std::collections::HashMap;

pub fn check_map_capacity(items: &std::collections::BTreeMap<String, Value>, key: &str) -> Result<(), String> {
    if !items.contains_key(key) && items.len() >= MAX_COLLECTION_ITEMS {
        return Err(format!("map exceeds safety limit ({MAX_COLLECTION_ITEMS} items)"));
    }
    Ok(())
}

pub fn set_indexed_value(collection: &mut Value, index: Value, value: Value) -> Result<(), String> {
    match (collection, index) {
        (Value::Array(items), Value::Int(index)) if index >= 0 => {
            let slot = items.get_mut(index as usize).ok_or_else(|| format!("array index {index} out of bounds"))?;
            *slot = value;
            Ok(())
        }
        (Value::Array(_), Value::Int(index)) => Err(format!("array index {index} out of bounds")),
        (Value::Array(_), _) => Err("type error: array index must be an integer".into()),
        (Value::Map(items), Value::Str(key)) => {
            check_map_capacity(items, &key)?;
            items.insert(key, value);
            Ok(())
        }
        (Value::Map(_), _) => Err("type error: map index must be a string".into()),
        _ => Err("type error: indexed assignment expects an array or map".into()),
    }
}

pub fn value_to_string(value: &Value) -> String {
    match value {
        Value::Int(n) => n.to_string(),
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => format!("[{}]", items.iter().map(value_to_string).collect::<Vec<_>>().join(", ")),
        Value::Map(items) => format!("{{{}}}", items.iter().map(|(k, v)| format!("\"{}\": {}", k, value_to_string(v))).collect::<Vec<_>>().join(", ")),
    }
}

pub fn builtin(name: &str, args: Vec<Value>) -> Result<Value, String> {
    let value = builtin_unchecked(name, args)?;
    ensure_value_depth(&value)?;
    Ok(value)
}

pub fn value_depth(value: &Value) -> usize {
    match value {
        Value::Int(_) | Value::Str(_) | Value::Bool(_) => 0,
        Value::Array(items) => 1 + items.iter().map(value_depth).max().unwrap_or(0),
        Value::Map(items) => 1 + items.values().map(value_depth).max().unwrap_or(0),
    }
}

pub fn value_size(value: &Value) -> usize {
    match value {
        Value::Int(_) => std::mem::size_of::<i64>(),
        Value::Bool(_) => std::mem::size_of::<bool>(),
        Value::Str(text) => text.len(),
        Value::Array(items) => items.iter().map(value_size).fold(
            items.len().saturating_mul(std::mem::size_of::<Value>()),
            |total, size| total.saturating_add(size),
        ),
        Value::Map(items) => items.iter().map(|(key, value)| key.len().saturating_add(value_size(value))).fold(
            items.len().saturating_mul(64),
            |total, size| total.saturating_add(size),
        ),
    }
}

pub fn ensure_value_depth(value: &Value) -> Result<(), String> {
    let depth = value_depth(value);
    if depth > MAX_VALUE_DEPTH {
        return Err(format!("value nesting exceeds safety limit ({MAX_VALUE_DEPTH} levels)"));
    }
    let size = value_size(value);
    if size > MAX_VALUE_BYTES {
        return Err(format!("value exceeds estimated memory safety budget ({MAX_VALUE_BYTES} bytes)"));
    }
    Ok(())
}

pub fn push_stack_value(stack: &mut Vec<Value>, stack_bytes: &mut usize, value: Value) -> Result<(), String> {
    let size = value_size(&value);
    let next_size = stack_bytes.saturating_add(size);
    if next_size > MAX_STACK_VALUE_BYTES {
        return Err(format!("operand stack values exceed safety limit ({MAX_STACK_VALUE_BYTES} bytes)"));
    }
    stack.push(value);
    *stack_bytes = next_size;
    Ok(())
}

pub fn pop_stack_value(stack: &mut Vec<Value>, stack_bytes: &mut usize, underflow: &str) -> Result<Value, String> {
    let value = stack.pop().ok_or_else(|| underflow.to_string())?;
    *stack_bytes = stack_bytes.saturating_sub(value_size(&value));
    Ok(value)
}

pub fn binding_size(name: &str, value: &Value) -> usize {
    name.len().saturating_add(64).saturating_add(value_size(value))
}

pub fn binding_map_size(bindings: &HashMap<String, Value>) -> usize {
    bindings.iter().map(|(name, value)| binding_size(name, value))
        .fold(0usize, |total, size| total.saturating_add(size))
}

pub fn runtime_state_bytes_after_sizes(current: usize, old_size: usize, new_size: usize) -> Result<usize, String> {
    let next = current.saturating_sub(old_size).saturating_add(new_size);
    if next > MAX_RUNTIME_STATE_BYTES {
        return Err(format!("live variable/call-frame values exceed safety limit ({MAX_RUNTIME_STATE_BYTES} estimated bytes)"));
    }
    Ok(next)
}

pub fn runtime_state_bytes_after_update(current: usize, name: &str, old: Option<&Value>, new: &Value) -> Result<usize, String> {
    let old_size = old.map(|value| binding_size(name, value)).unwrap_or(0);
    runtime_state_bytes_after_sizes(current, old_size, binding_size(name, new))
}

