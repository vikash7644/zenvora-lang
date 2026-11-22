//! Versioned deterministic gas-meter primitives for contract execution.
//!
//! These costs are engineering defaults only. They are not activated chain parameters until
//! transaction envelopes, fee semantics, opcode charging, receipts, and replay are integrated.
pub use crate::qvm::gas::{
    BASE_EXECUTION_GAS, CONDITIONAL_BRANCH_GAS, EVENT_BASE_GAS, EVENT_BYTE_GAS,
    FUNCTION_CALL_GAS, GAS_SCHEDULE_VERSION, INTEGER_OPERATION_GAS, LOCAL_READ_GAS,
    LOCAL_WRITE_GAS, STORAGE_READ_GAS, STORAGE_WRITE_GAS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GasMeter {
    limit: u64,
    used: u64,
}

impl GasMeter {
    pub fn new(limit: u64) -> Self { Self { limit, used: 0 } }

    pub fn limit(&self) -> u64 { self.limit }
    pub fn used(&self) -> u64 { self.used }
    pub fn remaining(&self) -> u64 { self.limit - self.used }

    /// Charge an integer amount deterministically; never consult wall-clock time.
    pub fn charge(&mut self, amount: u64) -> Result<(), String> {
        let next = self.used.checked_add(amount)
            .ok_or_else(|| "gas accounting overflow".to_string())?;
        if next > self.limit {
            return Err("out of gas".into());
        }
        self.used = next;
        Ok(())
    }

    pub fn charge_event(&mut self, payload_bytes: usize) -> Result<(), String> {
        let payload = u64::try_from(payload_bytes)
            .map_err(|_| "event payload size overflow".to_string())?;
        let variable = payload.checked_mul(EVENT_BYTE_GAS)
            .ok_or_else(|| "event gas calculation overflow".to_string())?;
        let total = EVENT_BASE_GAS.checked_add(variable)
            .ok_or_else(|| "event gas calculation overflow".to_string())?;
        self.charge(total)
    }
}


impl crate::qvm::gas::GasMeter for GasMeter {
    fn charge(&mut self, amount: u64) -> Result<(), String> {
        GasMeter::charge(self, amount)
    }
}

#[cfg(test)]
mod tests {
    use super::{GasMeter, BASE_EXECUTION_GAS, EVENT_BASE_GAS, EVENT_BYTE_GAS};

    #[test]
    fn charges_are_integer_and_deterministic() {
        let mut meter = GasMeter::new(100);
        meter.charge(21).unwrap();
        meter.charge(3).unwrap();
        assert_eq!(meter.used(), 24);
        assert_eq!(meter.remaining(), 76);
    }

    #[test]
    fn out_of_gas_does_not_overcharge_or_advance_meter() {
        let mut meter = GasMeter::new(10);
        meter.charge(7).unwrap();
        assert_eq!(meter.charge(4), Err("out of gas".into()));
        assert_eq!(meter.used(), 7);
        assert_eq!(meter.remaining(), 3);
    }

    #[test]
    fn event_cost_uses_checked_integer_arithmetic() {
        let mut meter = GasMeter::new(1_000);
        meter.charge_event(10).unwrap();
        assert_eq!(meter.used(), EVENT_BASE_GAS + 10 * EVENT_BYTE_GAS);
    }

    #[test]
    fn base_schedule_is_explicit_but_not_implicitly_charged() {
        let meter = GasMeter::new(100_000);
        assert_eq!(meter.used(), 0);
        assert_eq!(BASE_EXECUTION_GAS, 21_000);
    }
}
