//! Detect holes uses the bounded Bore ring plan (#1566).
//! Provenance: concentric floor-band coverage from the motion kernel;
//! detection and generation share the same cutter reach and ring budget.

use flecs_ecs::prelude::World;

use crate::drilling::{Hole, THROUGH_CLEAR};
use crate::machining2d::{bore_leaves_core, max_bore_diameter};
use crate::model;
use crate::operation::OpParams;

/// Detect holes' answer: the params to store, and one sentence per hole
/// the op's kind could not take (#1556).
#[derive(Clone, Debug, PartialEq)]
pub struct DetectedHoles {
    pub params: OpParams,
    pub skipped: Vec<String>,
    /// One sentence per hole kept but drilled undersize: wider than the
    /// Drill op.s drill (#2496). A pilot drill is valid, so they stay.
    pub undersize: Vec<String>,
}

/// A Bore op's freshly detected list, cut to the holes its ring plan
/// clears; every other kind passes untouched. All holes too wide is a
/// refusal naming each one - an empty Bore op is never stored.
pub(super) fn skip_unborable(world: &World, name: &str, params: &mut OpParams) -> Result<Vec<String>, String> {
    let OpParams::Bore { holes, stock_to_leave, break_through, .. } = params else {
        return Ok(Vec::new());
    };
    let Some((d, corner)) = op_tool_size(world, name) else {
        return Ok(Vec::new());
    };
    let skipped = keep_borable(holes, d, corner, *stock_to_leave, break_through.unwrap_or(THROUGH_CLEAR));
    if holes.is_empty() {
        return Err(format!("no hole this Bore op can make: {}", skipped.join("; ")));
    }
    Ok(skipped)
}

/// The diameter and corner radius of the tool the named operation
/// holds, when both exist.
fn op_tool_size(world: &World, name: &str) -> Option<(f64, f64)> {
    let op = model::find_by_name(world, name).and_then(|e| model::operation_of(world, &e))?;
    model::find_by_name(world, &op.tool)
        .and_then(|t| model::tool_of(world, &t))
        .map(|t| (t.diameter, t.corner_r))
}

/// Keep holes whose floor band and radial ring count the kernel supports.
/// Invalid tool values stay for the named generator validation doors.
pub(super) fn keep_borable(holes: &mut Vec<Hole>, tool_diameter: f64, corner: f64, leave: f64, run: f64) -> Vec<String> {
    let sane = tool_diameter.is_finite()
        && tool_diameter > 0.0
        && corner.is_finite()
        // The kernel's own corner door (its float allowance included).
        && corner >= 0.0
        && corner <= tool_diameter * 0.5 + 1e-6
        && leave.is_finite()
        && leave >= 0.0;
    if !sane {
        return Vec::new();
    }
    let mut skipped = Vec::new();
    holes.retain(|h| {
        let through = h.through.then_some(run);
        let keep = !h.radius.is_finite() || !bore_leaves_core(h.radius, tool_diameter, corner, leave, through);
        if !keep {
            let max = max_bore_diameter(tool_diameter, corner, leave, through);
            skipped.push(format!(
                "the {:.3} mm hole at ({:.3}, {:.3}) was skipped: wider than the {max:.3} mm a \
                 {tool_diameter:.3} mm tool clears within 4096 rings, or its floor reach is too small - \
                 use a flat end mill, a larger tool, or a pocket operation",
                h.radius * 2.0,
                tidy(h.xy.x),
                tidy(h.xy.y)
            ));
        }
        keep
    });
    skipped
}

/// A coordinate as the note prints it: float noise under the printed
/// precision reads 0.000, never -0.000.
fn tidy(v: f64) -> f64 {
    if v.abs() < 5e-4 { 0.0 } else { v }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Point2;

    fn hole(dia: f64, through: bool) -> Hole {
        Hole { xy: Point2::new(5.0, -3.0), radius: dia * 0.5, entry_z: 0.0, floor_z: -4.0, through, stepped: false }
    }

    #[test]
    fn detect_keeps_wide_bores_with_square_and_bull_nose_tools() {
        for (dia, corner) in [(4.0, 0.0), (12.0, 2.0)] {
            let mut holes = vec![hole(74.3, false), hole(74.3, true)];
            assert!(keep_borable(&mut holes, dia, corner, 0.5, THROUGH_CLEAR).is_empty());
            assert_eq!(holes.len(), 2);
        }
    }

    #[test]
    fn detect_names_holes_past_the_ring_budget() {
        let limit = max_bore_diameter(4.0, 0.0, 0.5, None);
        let mut holes = vec![hole(74.3, false), hole(limit + 0.02, false)];
        let skipped = keep_borable(&mut holes, 4.0, 0.0, 0.5, THROUGH_CLEAR);
        assert_eq!(holes.len(), 1);
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].contains("4096"), "{skipped:?}");
    }

    #[test]
    fn detect_refuses_a_blind_ball_floor_but_keeps_the_through_hole() {
        let mut holes = vec![hole(24.0, false), hole(24.0, true)];
        let skipped = keep_borable(&mut holes, 6.0, 3.0, 0.0, THROUGH_CLEAR);
        assert_eq!(holes.len(), 1);
        assert!(holes[0].through);
        assert_eq!(skipped.len(), 1);
        assert!(skipped[0].contains("floor"), "{skipped:?}");
    }
}
