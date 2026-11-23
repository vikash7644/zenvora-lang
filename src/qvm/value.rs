//! Runtime value representation owned by the QORVYNT virtual machine.

/// Values carried by the QORVYNT VM operand stack and variable environment.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    Int(i64),
    Str(String),
    Bool(bool),
    Array(Vec<Value>),
    Map(std::collections::BTreeMap<String, Value>),
}


/// Render a runtime value using the language's stable human-readable format.
pub(crate) fn value_to_string(value: &Value) -> String {
    match value {
        Value::Int(n) => n.to_string(),
        Value::Str(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Array(items) => format!("[{}]", items.iter().map(value_to_string).collect::<Vec<_>>().join(", ")),
        Value::Map(items) => format!("{{{}}}", items.iter().map(|(k, v)| format!("\"{}\": {}", k, value_to_string(v))).collect::<Vec<_>>().join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::{value_to_string, Value};
    use std::collections::BTreeMap;

    #[test]
    fn formats_scalar_values() {
        assert_eq!(value_to_string(&Value::Int(-12)), "-12");
        assert_eq!(value_to_string(&Value::Str("qorvynt".into())), "qorvynt");
        assert_eq!(value_to_string(&Value::Bool(true)), "true");
    }

    #[test]
    fn formats_nested_collections_deterministically() {
        let mut map = BTreeMap::new();
        map.insert("answer".to_string(), Value::Int(42));
        map.insert("items".to_string(), Value::Array(vec![Value::Bool(false), Value::Str("ok".into())]));
        let value = Value::Array(vec![Value::Map(map)]);
        assert_eq!(value_to_string(&value), "[{\"answer\": 42, \"items\": [false, ok]}]");
    }
}


pub(crate) const MAX_VALUE_DEPTH: usize = 256;
pub(crate) const MAX_VALUE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_STACK_VALUE_BYTES: usize = 16 * 1024 * 1024;
pub(crate) const MAX_STACK_VALUES: usize = 65_536;
pub(crate) const MAX_VM_EXECUTION_STEPS: usize = 1_000_000;
pub(crate) const MAX_VM_CALL_DEPTH: usize = 1_024;

/// Return the maximum collection nesting depth within a runtime value.
pub(crate) fn value_depth(value: &Value) -> usize {
    match value {
        Value::Int(_) | Value::Str(_) | Value::Bool(_) => 0,
        Value::Array(items) => 1 + items.iter().map(value_depth).max().unwrap_or(0),
        Value::Map(items) => 1 + items.values().map(value_depth).max().unwrap_or(0),
    }
}

/// Estimate the heap payload held by a runtime value.
pub(crate) fn value_size(value: &Value) -> usize {
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

/// Enforce nesting and estimated-size budgets before a value enters the VM.
pub(crate) fn ensure_value_depth(value: &Value) -> Result<(), String> {
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

/// Push a value while accounting for the VM operand-stack memory budget.
pub(crate) fn push_stack_value(stack: &mut Vec<Value>, stack_bytes: &mut usize, value: Value) -> Result<(), String> {
    let size = value_size(&value);
    let next_size = stack_bytes.saturating_add(size);
    if next_size > MAX_STACK_VALUE_BYTES {
        return Err(format!("operand stack values exceed safety limit ({MAX_STACK_VALUE_BYTES} bytes)"));
    }
    stack.push(value);
    *stack_bytes = next_size;
    Ok(())
}

/// Pop a value and release its accounted operand-stack bytes.
pub(crate) fn pop_stack_value(stack: &mut Vec<Value>, stack_bytes: &mut usize, underflow: &str) -> Result<Value, String> {
    let value = stack.pop().ok_or_else(|| underflow.to_string())?;
    *stack_bytes = stack_bytes.saturating_sub(value_size(&value));
    Ok(value)
}
