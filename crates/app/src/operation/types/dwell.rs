//! The drilling dwell's door at command creation (#1445): a dwell at the
//! hole bottom is a time in seconds, from 0 to [`MAX_DWELL_S`]. Anything
//! else is refused by name when a command is made, not deep in a post.
//!
//! Provenance: our own validate-early law (AGENTS.md Prime Directive); the
//! bound is the dwell range our Heidenhain post already enforces, well
//! inside the P ranges of the RS-274 dwell (G4/G82) on the shipped posts.

use crate::operation::types::params::OpParams;

/// The longest dwell at a hole bottom a command takes, in seconds.
pub const MAX_DWELL_S: f64 = 3600.0;

impl OpParams {
    /// The by-name refusal of a drilling dwell that is not a time from 0
    /// to [`MAX_DWELL_S`] seconds. The commands ask it; a project file
    /// is not refused by it, so a project saved before #1445 still opens.
    #[must_use]
    pub fn dwell_refusal(&self) -> Option<String> {
        let OpParams::Drill { dwell, .. } = self else {
            return None;
        };
        if dwell.is_finite() && (0.0..=MAX_DWELL_S).contains(dwell) {
            return None;
        }
        Some(format!(
            "drilling dwell {dwell} s is not usable - give a dwell at the hole bottom from 0 to {MAX_DWELL_S} seconds"
        ))
    }
}
