//! The cross drill against the FINISHED PART (issue #117): the radial
//! twin of the axial hole verdict (`drilling::axial_warnings`). The
//! reach (#84), spindle (#62) and rapid (#77/#79) laws judge where the
//! drill stands and travels, none what it leaves: a cross hole on a
//! solid bar with no modelled hole, or one plunged past the modelled
//! hole's floor, cuts steel the part keeps. Here every move of the
//! finished chain sweeps the drill - its radius along the hole axis
//! from the station to the tip, the point cone included, shank and
//! holder above the flutes - through the part's solid (the verify
//! crate's solid law, posed along each move's stated tool axis), and
//! any part material inside that volume past the gouge tolerance the
//! axial check uses refuses the op by name - the hole, how deep the
//! tip got before it met the part, and what that means - so nothing
//! reaches the post.
//!
//! Provenance: "never cut the part you keep" is shop practice in any
//! machining handbook; the swept-tool-in-solid test is the verify
//! crate's (ray-parity point-in-solid and swept-atom distance, cited
//! there); the bisection is textbook. #842: the drill carries its
//! modelled body's band stack, the shape the simulation draws. Our own
//! design notes; no vendor material consulted.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::drilling::CrossHole;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::Tool;
use crate::verify::{self, Warning};

use super::material::GOUGE_TOL;

/// Sample spacing of the solid law along a vertical-plane arc, mm (the
/// cross chain is straight lines, swept exactly; the containment
/// kinds' own step).
const STEP: f64 = 0.25;

/// Bisection rounds when reading how deep the tip got: 2^-24 of the
/// plunge, far under any tolerance.
const ROUNDS: usize = 24;

fn law() -> verify::SolidLaw {
    verify::SolidLaw { step: STEP, allow: GOUGE_TOL, hole_walls: true }
}

/// The cross holes' axes, whose round walls the solid law excuses
/// (#1205: measured about each hole's own line, never a flat).
fn walls(holes: &[CrossHole]) -> Vec<verify::HoleAxis> {
    holes.iter().map(|h| verify::HoleAxis { at: h.origin, dir: [h.dir.x, h.dir.y, h.dir.z] }).collect()
}

/// Refuse by name a cross-drill chain (world frame, the part's own)
/// whose drill enters the finished part's solid. `holes` is the world
/// hole list the user modelled - a moved-out raw mouth stays on its
/// line, so the hole is named by the line the offending move rides.
/// `cutter` is the drill as every stack law reads it (#842,
/// `Cutter::of(tool).with_body(..)`): with a modelled body its bands,
/// not the cylinder enclosing it. It is re-judged against `tool`'s own
/// geometry ([`verify::Cutter::judged`]): a cutter that no longer is this
/// tool's falls back to the square stack, which can only over-report.
pub(super) fn part_clear(
    t: &Toolpath,
    facets: &[Triangle],
    tool: &Tool,
    cutter: &verify::Cutter,
    holes: &[CrossHole],
) -> Result<(), GenError> {
    if facets.is_empty() {
        return Ok(()); // no part, nothing kept: the other laws own the chain
    }
    let cutter = verify::Cutter::judged(&tool.geom(), Some(*cutter));
    let Some(w) = verify::solid_hits_walled(t, facets, &cutter, &law(), &walls(holes)).into_iter().next() else {
        return Ok(());
    };
    let (at, what) = match w {
        Warning::Gouge { at } => (at, "cuts into"),
        Warning::RapidInStock { at } => (at, "rapids into"),
        Warning::ShankHit { at, .. } => (at, "rubs its shank or holder on"),
        _ => (0, "enters"),
    };
    let Some(m) = t.moves.get(at) else {
        return Err(GenError::Invalid("a cross hole drills into the finished part".into()));
    };
    let Some(h) = hole_of(m.to, holes) else {
        return Err(GenError::Invalid(format!(
            "the cross drill {what} the finished part at move {at}, off every modelled hole's line"
        )));
    };
    let o = h.origin;
    let depth = -h.floor_d;
    let why = match w {
        Warning::Gouge { .. } if m.kind != MoveKind::Rapid => {
            let met = met_at(m, facets, &cutter, h);
            // A real hole carries the drill at least its own radius in
            // before a floor stops it; steel met sooner - the face's
            // own curvature - is a hole the model does not have.
            if met < 0.5 * tool.diameter {
                format!(
                    "the drill meets steel at the mouth - the model has no hole there (asked {depth} mm deep); model the cross hole in the part, or drop it from the list"
                )
            } else {
                format!(
                    "the tip meets steel {met:.3} mm past the mouth, short of the {depth} mm asked - the drill runs past the modelled hole's floor (or the model's hole is narrower than the drill); match the depth and size to the model"
                )
            }
        }
        _ => "the finished part stands in the drill's path - model the hole the tool travels".to_string(),
    };
    Err(GenError::invalid_at(
        format!(
            "cross hole at ({}, {}, {}) along ({}, {}, {}) {what} the finished part: {why}",
            o.x, o.y, o.z, h.dir.x, h.dir.y, h.dir.z
        ),
        o,
    ))
}

/// The hole whose axis line `p` lies on (nearest line, within the
/// hole's own radius - a plunge rides its line exactly).
fn hole_of(p: Point3, holes: &[CrossHole]) -> Option<&CrossHole> {
    holes
        .iter()
        .filter_map(|h| {
            let v = Point3::new(p.x - h.origin.x, p.y - h.origin.y, p.z - h.origin.z);
            let s = v.dot(h.dir);
            let off = (v.dot(v) - s * s).max(0.0).sqrt();
            (off.is_finite() && off <= h.radius.max(GOUGE_TOL)).then_some((off, h))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, h)| h)
}

/// How far past the hole's mouth (along its `dir`) the tip of the
/// convicted plunge `m` stands when the drill first meets the part:
/// the clean prefix of the plunge, bisected. 0 when the plunge's own
/// start is already in the part.
fn met_at(m: &Move, facets: &[Triangle], cutter: &verify::Cutter, h: &CrossHole) -> f64 {
    let at = |f: f64| {
        Point3::new(
            m.from.x + (m.to.x - m.from.x) * f,
            m.from.y + (m.to.y - m.from.y) * f,
            m.from.z + (m.to.z - m.from.z) * f,
        )
    };
    let hits = |f: f64| {
        let probe = Toolpath { moves: vec![Move { to: at(f), ..*m }], ..Toolpath::default() };
        !verify::solid_hits_walled(&probe, facets, cutter, &law(), &walls(std::slice::from_ref(h))).is_empty()
    };
    let (mut lo, mut hi) = (0.0f64, 1.0f64);
    if hits(0.0) {
        hi = 0.0;
    } else {
        for _ in 0..ROUNDS {
            let mid = 0.5 * (lo + hi);
            if hits(mid) {
                hi = mid;
            } else {
                lo = mid;
            }
        }
    }
    let p = at(hi);
    let v = Point3::new(p.x - h.origin.x, p.y - h.origin.y, p.z - h.origin.z);
    v.dot(h.dir).max(0.0)
}

#[cfg(test)]
#[path = "cross_part_tests.rs"]
mod tests;
