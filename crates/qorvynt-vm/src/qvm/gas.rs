//! Host-independent gas charging interface used by the QORVYNT VM.

/// Versioned engineering gas schedule. Not activated chain parameters.
pub const GAS_SCHEDULE_VERSION: u16 = 1;
pub const BASE_EXECUTION_GAS: u64 = 21_000;
pub const INTEGER_OPERATION_GAS: u64 = 3;
pub const LOCAL_READ_GAS: u64 = 5;
pub const LOCAL_WRITE_GAS: u64 = 8;
pub const STORAGE_READ_GAS: u64 = 100;
pub const STORAGE_WRITE_GAS: u64 = 500;
pub const CONDITIONAL_BRANCH_GAS: u64 = 2;
pub const FUNCTION_CALL_GAS: u64 = 40;
pub const EVENT_BASE_GAS: u64 = 375;
pub const EVENT_BYTE_GAS: u64 = 8;

/// Deterministic gas charging contract required by the QVM executor.
pub trait GasMeter {
    /// Charge an integer amount; implementations must leave accounting unchanged on error.
    fn charge(&mut self, amount: u64) -> Result<(), String>;
}
