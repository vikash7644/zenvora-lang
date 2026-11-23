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
