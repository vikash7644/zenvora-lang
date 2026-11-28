//! Instruction set shared by the QORVYNT compiler and virtual machine.

/// Verified QORVYNT VM instruction representation.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Push(i64), PushString(String), PushBool(bool), Load(String), Store(String), Declare(String), Add, Sub, Mul, Div, Neg, Not, And, Or, Print,
    Eq, Ne, Lt, Le, Gt, Ge, Index, SetIndex(String), JumpIfFalse(usize), JumpIfFalseKeep(usize), JumpIfTrueKeep(usize), Jump(usize), Call(String, usize), Builtin(String, usize), CallTarget { target: usize, params: Vec<String>, arg_count: usize }, Return, ReturnValue,
}
