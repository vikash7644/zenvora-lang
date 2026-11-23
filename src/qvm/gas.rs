//! Deterministic per-instruction gas schedule for QORVYNT VM execution.

use super::Op;

/// Return the gas cost for the instruction at the current program counter.
pub(crate) fn instruction_gas_cost(op: Option<&Op>) -> Result<u64, String> {
    match op {
        Some(Op::Load(_)) => Ok(super::super::contract_gas::LOCAL_READ_GAS),
        Some(Op::Store(_) | Op::Declare(_) | Op::SetIndex(_)) => Ok(super::super::contract_gas::LOCAL_WRITE_GAS),
        Some(Op::JumpIfFalse(_) | Op::JumpIfFalseKeep(_) | Op::JumpIfTrueKeep(_)) => Ok(super::super::contract_gas::CONDITIONAL_BRANCH_GAS),
        Some(Op::Call(_, _) | Op::CallTarget { .. }) => Ok(super::super::contract_gas::FUNCTION_CALL_GAS),
        Some(Op::Add | Op::Sub | Op::Mul | Op::Div | Op::Neg | Op::Not | Op::And | Op::Or
            | Op::Eq | Op::Ne | Op::Lt | Op::Le | Op::Gt | Op::Ge) => Ok(super::super::contract_gas::INTEGER_OPERATION_GAS),
        Some(_) => Ok(1),
        None => Err("instruction pointer out of range".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{instruction_gas_cost, Op};
    use crate::contract_gas;

    #[test]
    fn assigns_deterministic_costs_to_instruction_categories() {
        assert_eq!(instruction_gas_cost(Some(&Op::Load("x".into()))).unwrap(), contract_gas::LOCAL_READ_GAS);
        assert_eq!(instruction_gas_cost(Some(&Op::Store("x".into()))).unwrap(), contract_gas::LOCAL_WRITE_GAS);
        assert_eq!(instruction_gas_cost(Some(&Op::Jump(0))).unwrap(), 1);
        assert_eq!(instruction_gas_cost(Some(&Op::Add)).unwrap(), contract_gas::INTEGER_OPERATION_GAS);
    }

    #[test]
    fn rejects_missing_instruction() {
        assert_eq!(instruction_gas_cost(None), Err("instruction pointer out of range".into()));
    }
}
