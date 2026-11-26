//! The fine-bore shift's door at command creation (#1445, #1479): a fine
//! bore states its shift, the shift is more than zero and within the
//! generator's bound, and it is smaller than each hole's radius. Anything
//! else is refused by name when a command is made, not deep in a post.
//!
//! Provenance: our own validate-early law (AGENTS.md Prime Directive). The
//! cycle is the published G76 fine boring canned cycle (Haas Mill
//! Operator's Manual, G76): the shift lifts the stopped tip off the wall.

use crate::drilling::{CycleChoice, FINE_BORE_MAX_SHIFT_UM};
use crate::operation::types::params::OpParams;

impl OpParams {
    /// The by-name refusal of a fine-bore cycle whose shift is not usable.
    /// The commands ask it. The room for the bar is the generator's door:
    /// it needs the tool, and refuses there by name.
    #[must_use]
    pub fn fine_bore_refusal(&self) -> Option<String> {
        let OpParams::Drill { cycle, holes, .. } = self else {
            return None;
        };
        let CycleChoice::FineBore { shift_x_um, shift_y_um } = *cycle else {
            return None;
        };
        let [dx, dy] = cycle.fine_bore_shift()?;
        if shift_x_um == 0 && shift_y_um == 0 {
            return Some(
                "fine bore needs a shift: with no shift the stopped tip drags up the finished wall - give the shift along X and Y"
                    .into(),
            );
        }
        let max = FINE_BORE_MAX_SHIFT_UM;
        if shift_x_um.unsigned_abs() > max.unsigned_abs() || shift_y_um.unsigned_abs() > max.unsigned_abs() {
            return Some(format!(
                "fine bore shift ({dx}, {dy}) mm is not usable - give at most {} mm on each axis",
                f64::from(max) / 1000.0
            ));
        }
        let s = dx.hypot(dy);
        // A radius that is not a number is not "larger": refused too.
        let tight = holes.iter().find(|h| s.partial_cmp(&h.radius) != Some(std::cmp::Ordering::Less))?;
        Some(format!(
            "fine bore shift {s} mm is not smaller than the radius {} mm of the hole at ({}, {})",
            tight.radius, tight.xy.x, tight.xy.y
        ))
    }
}
