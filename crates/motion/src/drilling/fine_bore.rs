//! Fine boring (#1445, #1479): the shift that a fine-bore cycle makes at
//! the bottom of the hole, and the doors that it answers to.
//!
//! Provenance: the cycle's shape (feed in, dwell, oriented spindle stop,
//! shift off the wall, rapid out, shift back) is the published G76 fine
//! boring canned cycle (Haas Mill Operator's Manual, G76 Fine Boring
//! Canned Cycle, words I and J). The room rule is the plane geometry of a
//! round bar in a round bore, our own. No vendor code consulted.

use super::hole::CycleChoice;
use crate::geometry::Point3;
use crate::machining2d::GenError;
use crate::tools::Tool;

/// The largest shift a fine-bore choice accepts on one axis, micrometres
/// (5 mm). A shift is tenths of a millimetre in practice; a larger number
/// is a typing error, refused by name.
pub const FINE_BORE_MAX_SHIFT_UM: i32 = 5_000;

impl CycleChoice {
    /// The fine-bore choice for a shift of (`dx`, `dy`) millimetres, in
    /// the X and Y of the hole list. The shift has no default (#1479): the
    /// direction away from the wall depends on how the bar is clamped at
    /// the spindle's orientation angle, and only the user knows it. A
    /// non-finite, zero or oversized shift is refused by name.
    pub fn fine_bore(dx: f64, dy: f64) -> Result<Self, String> {
        if !(dx.is_finite() && dy.is_finite()) {
            return Err("fine bore shift must be two finite numbers (X and Y)".into());
        }
        let max = f64::from(FINE_BORE_MAX_SHIFT_UM);
        let (ux, uy) = ((dx * 1000.0).round(), (dy * 1000.0).round());
        if ux.abs() > max || uy.abs() > max {
            return Err(format!(
                "fine bore shift ({dx}, {dy}) is larger than {} on one axis - a shift only lifts the tip off the wall",
                max / 1000.0
            ));
        }
        if ux == 0.0 && uy == 0.0 {
            return Err(
                "fine bore needs a shift: with no shift the stopped tip drags up the finished wall - state X and Y".into(),
            );
        }
        // In range by the checks above, so the casts are exact.
        Ok(CycleChoice::FineBore { shift_x_um: ux as i32, shift_y_um: uy as i32 })
    }

    /// The shift of a fine-bore choice, millimetres; `None` for every
    /// other cycle.
    #[must_use]
    pub fn fine_bore_shift(&self) -> Option<[f64; 2]> {
        match *self {
            CycleChoice::FineBore { shift_x_um, shift_y_um } => {
                Some([f64::from(shift_x_um) / 1000.0, f64::from(shift_y_um) / 1000.0])
            }
            _ => None,
        }
    }
}

/// The shift door: the shift the chain makes at the bottom, or a refusal
/// by name. The bar (the tool's shank diameter) moves with the tip, so its
/// far side comes `shift` nearer to the opposite wall of the bore that the
/// tool just cut. The shift must be smaller than that room,
/// `(tool dia. - bar dia.) / 2`, or the bar rubs the finished wall on the
/// rapid out. `at` is the hole's mouth.
pub(super) fn shift_door(tool: &Tool, choice: CycleChoice, at: Point3) -> Result<[f64; 2], GenError> {
    let Some([dx, dy]) = choice.fine_bore_shift() else {
        return Err(GenError::invalid_at("fine bore cycle with no shift stated".to_string(), at));
    };
    let s = dx.hypot(dy);
    let limit = f64::from(FINE_BORE_MAX_SHIFT_UM) / 1000.0;
    if !(s.is_finite() && s > 0.0 && dx.abs() <= limit && dy.abs() <= limit) {
        return Err(GenError::invalid_at(
            format!("fine bore shift ({dx}, {dy}) is not usable: it must be more than zero and at most {limit} on each axis"),
            at,
        ));
    }
    let (d, bar) = (tool.diameter, tool.shank_d);
    if !(bar.is_finite() && bar > 0.0) {
        return Err(GenError::invalid_at(
            "fine bore: the tool states no bar (shank) diameter, so the room for the shift is unknown".to_string(),
            at,
        ));
    }
    let room = 0.5 * (d - bar);
    if s >= room {
        return Err(GenError::invalid_at(
            format!(
                "fine bore shift {s} does not fit: the bar dia. {bar} in the dia. {d} bore has {} of room - \
                 the bar would rub the opposite wall on the way out; use a smaller shift or a thinner bar",
                room.max(0.0)
            ),
            at,
        ));
    }
    Ok([dx, dy])
}
