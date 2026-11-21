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
