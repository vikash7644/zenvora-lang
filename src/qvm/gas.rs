//! Host-independent gas charging interface used by the QORVYNT VM.

/// Versioned engineering gas schedule. Not activated chain parameters.
pub(crate) const GAS_SCHEDULE_VERSION: u16 = 1;
pub(crate) const BASE_EXECUTION_GAS: u64 = 21_000;
pub(crate) const INTEGER_OPERATION_GAS: u64 = 3;
pub(crate) const LOCAL_READ_GAS: u64 = 5;
pub(crate) const LOCAL_WRITE_GAS: u64 = 8;
pub(crate) const STORAGE_READ_GAS: u64 = 100;
pub(crate) const STORAGE_WRITE_GAS: u64 = 500;
pub(crate) const CONDITIONAL_BRANCH_GAS: u64 = 2;
pub(crate) const FUNCTION_CALL_GAS: u64 = 40;
pub(crate) const EVENT_BASE_GAS: u64 = 375;
pub(crate) const EVENT_BYTE_GAS: u64 = 8;

/// Deterministic gas charging contract required by the QVM executor.
pub(crate) trait GasMeter {
    /// Charge an integer amount; implementations must leave accounting unchanged on error.
    fn charge(&mut self, amount: u64) -> Result<(), String>;
}
