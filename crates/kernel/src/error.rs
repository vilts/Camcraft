//! Kernel seam errors - every engine failure becomes a located, typed value.
//! Provenance: our own error design (thiserror), no engine source consulted.

/// What went wrong, and in which engine/operation.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum KernelError {
    /// The seam refused the input before the engine saw it.
    #[error("{op}: invalid input: {detail}")]
    InvalidInput {
        /// Operation name.
        op: &'static str,
        /// What was wrong.
        detail: String,
    },
    /// The engine does not offer this operation.
    #[error("{kernel}: {op} is not offered by this engine")]
    Unsupported {
        /// Engine name.
        kernel: &'static str,
        /// Operation name.
        op: &'static str,
    },
    /// The engine ran and reported failure (its own diagnostic in `detail`).
    #[error("{kernel}: {op} failed: {detail}")]
    Failed {
        /// Engine name.
        kernel: &'static str,
        /// Operation name.
        op: &'static str,
        /// The engine's diagnostic.
        detail: String,
    },
    /// The caller's time bound ended the operation before it finished.
    #[error("{op}: stopped at its time bound ({stage})")]
    TimedOut {
        /// Operation name.
        op: &'static str,
        /// What was running when the bound was reached.
        stage: &'static str,
    },
    /// The engine produced a result that is not a usable closed solid.
    #[error("{kernel}: {op} produced an invalid result: {detail}")]
    InvalidResult {
        /// Engine name.
        kernel: &'static str,
        /// Operation name.
        op: &'static str,
        /// What check failed.
        detail: String,
    },
}
