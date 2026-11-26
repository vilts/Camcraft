//! Pass starts at on the 3-D rough's levels (#3361): which start side a
//! level's rings take, the notes for a value a level cannot use, and the
//! rings themselves through the 2D ring law (`level_ring_plan`, #3316).
//! Provenance: the 2D law's own (contour-parallel rings as level curves
//! of the distance from one side, Held 1991); no vendor material
//! consulted.

use super::params::Pocket3DParams;

use crate::cl::PathBuilder;
use crate::geo2d::{Point2, Region};
use crate::machining2d::{
    GenError, PASS_START_REST_NOTE, PASS_START_ZIGZAG_NOTE, PassStart, PocketStrategy, RINGS_FALLBACK, StartSide,
    level_ring_plan,
};

/// Said once per path where Adaptive plans its HSM levels with Inside or
/// Outside: the HSM spiral grows out of its entry bore and has no ring
/// order, so the choice is not used there.
pub const PASS_START_ADAPTIVE_NOTE: &str =
    "Pass starts at: the Adaptive (HSM) plan spirals out of its entry bore and has no inside or outside order; the choice is used only by the ring strategies (Waterline, Offset rings)";

/// The level's start side. Rows have no inside or outside, the rest
/// pass walks its own crumbs, and the wrap's developed area ends at the
/// seam, not at a wall: each keeps the Auto order, and a forced value
/// there is said by name, once.
pub(super) fn start_side(
    b: &mut PathBuilder,
    p: &Pocket3DParams,
    (rest, shell, on_wrap): (bool, bool, bool),
    stock: Option<&Region>,
) -> StartSide {
    let forced = p.pass_start != PassStart::Auto;
    if forced && matches!(p.strategy, PocketStrategy::Zigzag | PocketStrategy::Auto) {
        b.note_once(PASS_START_ZIGZAG_NOTE);
    } else if forced && rest {
        b.note_once(PASS_START_REST_NOTE);
    } else if forced && p.strategy == PocketStrategy::Adaptive && !shell {
        b.note_once(PASS_START_ADAPTIVE_NOTE);
    }
    StartSide {
        start: if rest || on_wrap { PassStart::Auto } else { p.pass_start },
        stock: if forced { stock.cloned() } else { None },
        around_part: false,
    }
}

/// The level's rings of `strategy` from `side`. Auto is `plan_passes`
/// byte for byte. The planner's warnings go on the builder once each; a
/// fallback of the completeness guard is said for its level, by Z.
pub(super) fn side_rings(
    b: &mut PathBuilder,
    center: &Region,
    p: &Pocket3DParams,
    strategy: PocketStrategy,
    side: &StartSide,
    z: f64,
) -> Result<Vec<Vec<Point2>>, GenError> {
    let r = p.tool_diameter * 0.5;
    let (passes, notes) = level_ring_plan(center, r, p.tiling_step(), p.climb, strategy, side)?;
    for n in notes {
        if n.contains(RINGS_FALLBACK) {
            b.note(format!("Z {z:.3}: {n}"));
        } else {
            b.note_once(&n);
        }
    }
    Ok(passes)
}
