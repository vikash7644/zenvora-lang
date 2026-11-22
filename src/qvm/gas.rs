//! Host-independent gas charging interface used by the QORVYNT VM.

/// Deterministic gas charging contract required by the QVM executor.
pub(crate) trait GasMeter {
    /// Charge an integer amount; implementations must leave accounting unchanged on error.
    fn charge(&mut self, amount: u64) -> Result<(), String>;
}
