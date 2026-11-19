//! #2496: a drill makes its own diameter. A Drill op whose Drill-kind
//! tool rides a hole wider than the tool leaves that hole undersize, and
//! no other law sees it: the containment law judges a cut into the part,
//! never material left standing in a hole. Each such hole is named at
//! Detect holes and as an advisory warning on the toolpath; a pilot
//! drill is a valid plan, so the post is not held. A hole narrower than
//! the drill is a cut into the part wall and stays the gouge law's.
//! Provenance: general drilling practice (a twist drill cuts its own
//! diameter); our own law, no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::cl::{MoveKind, Toolpath};
use crate::drilling::{CycleChoice, Hole};
use crate::model;
use crate::operation::OpParams;
use crate::tools::{Tool, ToolKind};
use crate::verify::Warning;

/// How much wider than the drill a hole may be modelled and still read
/// as the drill's size, mm on the diameter. A twist drill cuts up to about
/// a tenth over its size, and the project's own matched fixtures model a
/// hole that much over its drill (a 4 mm drill in a 4.1 mm hole); a hole
/// modelled wider by more than this is a size the drill does not make.
pub const DRILL_SIZE_TOL: f64 = 0.1;

/// Float slack on the band, so a hole modelled exactly `DRILL_SIZE_TOL`
/// over reads as the drill's size.
const SLACK: f64 = 1e-6;

/// The Z holes that `tool` drills undersize in `params`. Empty unless the
/// op is a Drill on the mill's Z holes, the tool is a Drill kind and the
/// cycle drills to size (Auto, Drill, Peck, Ream). A spot dimple, a tap
/// and a bore cycle size the hole another way; a radial list is the
/// turret's and is not read here.
#[must_use]
pub fn undersize_holes<'a>(tool: &Tool, params: &'a OpParams) -> Vec<&'a Hole> {
    let OpParams::Drill { holes, cross, cycle, .. } = params else {
        return Vec::new();
    };
    let sizes = matches!(cycle, CycleChoice::Auto | CycleChoice::Drill | CycleChoice::Peck | CycleChoice::Ream);
    let d = tool.diameter;
    if tool.kind != ToolKind::Drill || !sizes || !cross.is_empty() || !(d.is_finite() && d > 0.0) {
        return Vec::new();
    }
    holes.iter().filter(|h| h.radius.is_finite() && h.radius * 2.0 - d > DRILL_SIZE_TOL + SLACK).collect()
}

/// Push one [`Warning::DrillUndersize`] for each undersize hole, at the
/// first cutting move over its centre (move 0 when none is found).
pub(super) fn undersize_into(warns: &mut Vec<Warning>, path: &Toolpath, tool: &Tool, params: &OpParams) {
    for h in undersize_holes(tool, params) {
        let over = |x: f64, y: f64| (x - h.xy.x).abs() < 1e-6 && (y - h.xy.y).abs() < 1e-6;
        let at = path
            .moves
            .iter()
            .position(|m| m.kind != MoveKind::Rapid && over(m.from.x, m.from.y) && over(m.to.x, m.to.y))
            .unwrap_or(0);
        let w = Warning::DrillUndersize { at, hole_mm: h.radius * 2.0, drill_mm: tool.diameter };
        if !warns.contains(&w) {
            warns.push(w);
        }
    }
}

/// Detect holes' sentence for each undersize hole of the named op's
/// freshly detected `params`: the holes stay in the list (a pilot drill
/// is valid), and each is named by size and place.
pub(super) fn detect_notes(world: &World, name: &str, params: &OpParams) -> Vec<String> {
    let Some(tool) = model::find_by_name(world, name)
        .and_then(|e| model::operation_of(world, &e))
        .and_then(|op| model::find_by_name(world, &op.tool))
        .and_then(|t| model::tool_of(world, &t))
    else {
        return Vec::new();
    };
    undersize_holes(&tool, params)
        .into_iter()
        .map(|h| {
            format!(
                "the {:.3} mm hole at ({:.3}, {:.3}) is wider than the {:.3} mm drill: it is drilled {:.3} mm (a pilot) - \
                 use a {:.3} mm drill, or follow with a Bore or a reamer",
                h.radius * 2.0,
                tidy(h.xy.x),
                tidy(h.xy.y),
                tool.diameter,
                tool.diameter,
                h.radius * 2.0,
            )
        })
        .collect()
}

/// A coordinate as the note prints it: float noise under the printed
/// precision reads 0.000, never -0.000.
fn tidy(v: f64) -> f64 {
    if v.abs() < 5e-4 { 0.0 } else { v }
}

#[cfg(test)]
#[path = "drill_size_tests.rs"]
mod tests;
