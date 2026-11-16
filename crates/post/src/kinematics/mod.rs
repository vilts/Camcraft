// Provenance: clean-room implementation from public 5-axis XYZAC trunnion geometry and Rodrigues rotation math.

//! Pure-Rust inverse kinematics solver for table-table XYZAC milling machines.
//!
//! Shared between machine reachability validation and post processors so
//! simulated and posted `X/Y/Z/A/C` coordinates agree identically.

mod math;
mod path;
mod solve;
#[cfg(test)]
mod tests;
mod travel;
mod types;

pub use math::{forward_machine, forward_part, validate_trunnion};
pub use path::{solve_path, solve_toolpath};
pub use solve::{analytical_branches, AcSolver};
pub use types::{
    AcCandidate, AcTrunnionConfig, KinematicsError, MachinePose, PartPose, PathMove, PathOptions,
    RotaryAxisConfig, RotarySignConvention, SectionBoundary, SolvedMove, ValidatedTrunnion,
};
