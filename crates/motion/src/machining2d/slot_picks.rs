//! Slot start and exit picks (#3325, design #1448): a start pick chooses
//! the slot end where the pass starts, an exit pick the end where it
//! stops. The pass of `slot(b, a)` is the pass of `slot(a, b)` turned
//! half a turn about the slot's middle (the stadium and the row law are
//! symmetric under that turn, and a half turn keeps the climb sense), so
//! a pick only ever swaps the two ends: no new move shape. A pick that
//! the pass cannot hold is refused by name, never moved.
//!
//! Provenance: elementary plane geometry (point to segment distance, a
//! half turn about a midpoint) over the general machining practice of a
//! chosen entry and exit position; our own design, no vendor material.

use crate::geo2d::Point2;
use crate::path_picks::{PathPicks, PickRole};

use super::face::{GenError, band};
use super::slots::SlotParams;

/// Two distances this near are the same distance (mm): a pick there
/// names no end.
const TIE: f64 = 1e-9;

/// The ends that [`slot`](super::slot) must run between so that the pass
/// starts and stops where the picks ask: `(a, b)` back exactly without a
/// pick (the shape of every earlier slot), else `(a, b)` or `(b, a)`.
///
/// `a` and `b` are the ends the pass runs between (after an end
/// extension). Without a swap, row 0 starts on `a` under climb and on
/// `b` otherwise, the rows alternate, and the wall lap closes on the end
/// of the last row: with an odd row count the pass stops at the end
/// opposite its start, with an even count at the same end.
///
/// Refused by name: a pick that is not finite, a pick farther from the
/// cutter path than a pick may sit (the contour's reach law, #1448), a
/// pick the same distance from both ends, and an exit pick at an end
/// where the pass that the start pick fixes does not stop.
pub fn picked_ends(a: Point2, b: Point2, p: &SlotParams, picks: &PathPicks) -> Result<(Point2, Point2), GenError> {
    if picks.is_empty() {
        return Ok((a, b));
    }
    if let Some(why) = picks.refusal() {
        return Err(GenError::Invalid(why));
    }
    p.validate()?;
    let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    if len == 0.0 || !len.is_finite() {
        // `slot` refuses coincident ends by name.
        return Ok((a, b));
    }
    let Some(h) = super::slot_band::slot_centre_band(p.width, p.tool_diameter, p.stock_to_leave)? else {
        // `slot` refuses a slot narrower than its tool by name.
        return Ok((a, b));
    };
    let rows = band(-h, h, 0.0, p.stepover)?.len();
    if rows == 0 {
        return Ok((a, b));
    }
    let starts_at_a = p.climb;
    let stops_at_a = p.climb != ((rows - 1) % 2 == 0);
    let start = nearest_is_a(a, b, h, p, picks, PickRole::Start)?;
    let exit = nearest_is_a(a, b, h, p, picks, PickRole::Exit)?;
    let swap = match (start, exit) {
        (Some(at_a), _) => at_a != starts_at_a,
        (None, Some(at_a)) => at_a != stops_at_a,
        (None, None) => false,
    };
    let (from, to) = if swap { (b, a) } else { (a, b) };
    if let (Some(_), Some(exit_at_a), Some(q)) = (start, exit, picks.first(PickRole::Exit))
        && exit_at_a != (stops_at_a != swap)
    {
        let end = |at_a: bool| if at_a { a } else { b };
        let (s, e) = (end(starts_at_a != swap), end(stops_at_a != swap));
        return Err(GenError::Invalid(format!(
            "exit point ({:.3}, {:.3}): the pass that starts at the slot end ({:.3}, {:.3}) has {rows} \
             row(s) and stops at the slot end ({:.3}, {:.3}), not at the end nearest the exit point - \
             pick the exit at that end, or clear the exit point or the start point",
            q.xy.x, q.xy.y, s.x, s.y, e.x, e.y
        )));
    }
    Ok((from, to))
}

/// Is `a` the slot end nearest the pick of `role`? `Ok(None)` without
/// that pick. Refused by name where the pick is out of reach of the
/// cutter path or names no end.
fn nearest_is_a(a: Point2, b: Point2, h: f64, p: &SlotParams, picks: &PathPicks, role: PickRole) -> Result<Option<bool>, GenError> {
    let Some(q) = picks.first(role).map(|k| k.xy) else {
        return Ok(None);
    };
    let what = role.word();
    let at = format!("{what} point ({:.3}, {:.3})", q.x, q.y);
    let dist = |c: Point2| ((q.x - c.x).powi(2) + (q.y - c.y).powi(2)).sqrt();
    // The cutter path fills the stadium of half-width `h` on `a` -> `b`.
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let t = (((q.x - a.x) * dx + (q.y - a.y) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    let off = (dist(Point2::new(a.x + t * dx, a.y + t * dy)) - h).max(0.0);
    let lim = p.tool_diameter * 1.5 + p.stock_to_leave;
    if off.is_nan() || off > lim {
        return Err(GenError::Invalid(format!(
            "{at} is {off:.3} mm from the slot's cutter path, farther than the {lim:.3} mm a {what} point \
             may sit from it - pick on or beside the slot end to {what} at"
        )));
    }
    let (da, db) = (dist(a), dist(b));
    if (da - db).abs() <= TIE {
        return Err(GenError::Invalid(format!(
            "{at} is the same distance from both slot ends - pick nearer the end to {what} at"
        )));
    }
    Ok(Some(da < db))
}
