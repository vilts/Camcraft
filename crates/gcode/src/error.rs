//! Line-numbered error types for RS-274/ISO 6983 parsing and validation.
//! Provenance: clean-room refusal taxonomy derived from RS-274/NGC (NISTIR 6556) and CamCraft's named-refusal contract.

/// Line-numbered parser and validation errors.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum GcodeError {
    #[error("line {line}: syntax error: {message}")]
    Syntax { line: usize, message: String },

    #[error("line {line}: unknown word '{word}'")]
    UnknownWord { line: usize, word: String },

    #[error("line {line}: missing unit mode (G20 or G21 must precede motion coordinates)")]
    MissingUnitMode { line: usize },

    #[error("line {line}: unsupported {code}: {reason}")]
    UnsupportedCode {
        line: usize,
        code: String,
        reason: String,
    },

    #[error("line {line}: {code} requires a configured machine reference position")]
    MissingReferencePosition { line: usize, code: String },

    #[error("line {line}: invalid arc: {message}")]
    InvalidArc { line: usize, message: String },

    #[error("line {line}: invalid canned cycle: {message}")]
    InvalidCycle { line: usize, message: String },

    #[error("line {line}: invalid modal state: {message}")]
    InvalidModal { line: usize, message: String },

    #[error("line {line}: value out of range: {message}")]
    OutOfRange { line: usize, message: String },

    #[error("invalid parse option {field}: {message}")]
    InvalidOptions { field: String, message: String },
}

impl GcodeError {
    /// 1-based NC line number where the error occurred; 0 for a caller option
    /// ([`GcodeError::InvalidOptions`]), which no source line holds.
    #[must_use]
    pub const fn line(&self) -> usize {
        match self {
            GcodeError::InvalidOptions { .. } => 0,
            GcodeError::Syntax { line, .. }
            | GcodeError::UnknownWord { line, .. }
            | GcodeError::MissingUnitMode { line }
            | GcodeError::UnsupportedCode { line, .. }
            | GcodeError::MissingReferencePosition { line, .. }
            | GcodeError::InvalidArc { line, .. }
            | GcodeError::InvalidCycle { line, .. }
            | GcodeError::InvalidModal { line, .. }
            | GcodeError::OutOfRange { line, .. } => *line,
        }
    }
}
