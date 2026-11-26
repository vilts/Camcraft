//! Pure-Rust RS-274/NGC and ISO 6983 3-axis milling G-code parser into `cl::Toolpath` sections.
//! Provenance: clean-room implementation from public RS-274/NGC (NISTIR 6556) and ISO 6983 standards, plus CamCraft's own CL/post contracts (`crates/cl`, `assets/posts/*.cps`).

mod arc;
mod block;
mod block_codes;
mod cycle;
mod cycle_expand;
mod dialect;
mod domain;
mod error;
mod frame;
mod interp;
mod lexer;
mod motion;
mod section;
mod tooling;
pub mod types;

pub use domain::{MAX_ABS_MM, MAX_DWELL_S, MAX_FEED_MM_MIN};
pub use types::{
    ArcCenterMode, CoolantState, CycleReturnMode, CycleType, Dialect, DistanceMode, DwellEvent,
    DwellUnit, FeedMode, GcodeError, LineMap, ModalState, MotionMode, MoveLine, ParseOptions,
    ParsedProgram, ParsedSection, PathControlMode, ProgramEnd, ReferencePositions, SpindleDir,
    StopEvent, StopKind, WorkOffset,
};

/// Parse an RS-274/NGC or ISO 6983 3-axis milling program with default options.
///
/// # Errors
/// Returns a line-numbered [`GcodeError`] if the program uses unsupported words/codes,
/// omits unit mode (`G20`/`G21`) before motion coordinates, or violates arc/cycle rules.
pub fn parse(program: &str) -> Result<ParsedProgram, GcodeError> {
    parse_with(program, &ParseOptions::default())
}

/// Parse an RS-274/NGC or ISO 6983 3-axis milling program with caller dialect and reference positions.
///
/// # Errors
/// Returns a line-numbered [`GcodeError`] on syntax, modal, unit, reference, arc, or cycle failures.
pub fn parse_with(program: &str, options: &ParseOptions) -> Result<ParsedProgram, GcodeError> {
    interp::interpret(program, options)
}
