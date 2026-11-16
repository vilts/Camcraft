//! The room law of control-side compensation (#40): before a G41/G42
//! span is recorded, the PART contour it programs must be one the
//! control can offset by the register - no inside arc tighter than the
//! tool radius, no edge beside an inside corner too short for the
//! offset lines to meet on it, no gap narrower than the tool diameter.
//!
//! Provenance: the offset-curve rules of a radius-compensating control
//! (RS-274 / ISO 6983 G41 / G42: the control intersects the offset
//! lines at an inside corner, so each edge loses r * tan(turn / 2) at
//! every inside end, and an inside arc of radius R offsets to R - r)
//! and the classic offset-curve self-intersection test (offset the ring
//! by r; a ring that splits had a neck under 2 r), from machine-tool
//! handbooks and plane-curve offset literature. Our own model; no
//! vendor material consulted.

use crate::geo2d::{Loop, Point2, Region};

use super::arcs::Seg;
use super::contour::{ContourParams, Side};
use super::contour_comp::refused;
use super::face::GenError;
use super::ring::dist;

/// A vertex turn under this reads as a facet of a tessellated arc, so
/// the refusal names the arc's radius rather than a short edge.
const ARC_FACET_TURN: f64 = std::f64::consts::FRAC_PI_3;

/// The tool stands left of the programmed ring's travel (the region's
/// material side) on an Inside contour, right of it otherwise.
fn tool_left(p: &ContourParams) -> bool {
    p.side == Side::Inside
}

fn hint() -> &'static str {
    " - use Compensation: In computer"
}

/// The turn at `b` from `a -> b` to `b -> c`: (signed cross, angle).
fn turn(a: Point2, b: Point2, c: Point2) -> (f64, f64) {
    let (ux, uy) = (b.x - a.x, b.y - a.y);
    let (vx, vy) = (c.x - b.x, c.y - b.y);
    let cross = ux * vy - uy * vx;
    let dot = ux * vx + uy * vy;
    (cross, cross.abs().atan2(dot))
}

/// The inside-corner law over the programmed ring `l` (material on its
/// left, as every normalized region loop): each edge must hold the
/// offset trims of its inside ends, r * tan(turn / 2) apiece. A sharp
/// inside corner between long edges passes (the control rounds it with
/// the tool); an arc's facets tighter than the tool radius do not.
pub(super) fn corners(l: &Loop, p: &ContourParams) -> Result<(), GenError> {
    let r = p.tool_diameter * 0.5;
    let slack = p.tolerance.max(1e-6) + 1e-3;
    let pts = l.points();
    let n = pts.len();
    if n < 3 {
        return Ok(());
    }
    let left = tool_left(p);
    // Per vertex: the turn angle when it turns toward the tool, else 0.
    let inside: Vec<f64> = (0..n)
        .map(|i| {
            let (cross, ang) = turn(pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            if (cross > 0.0) == left && cross != 0.0 {
                ang.min(std::f64::consts::PI - 1e-9)
            } else {
                0.0
            }
        })
        .collect();
    for i in 0..n {
        let j = (i + 1) % n;
        let (ta, tb) = (inside[i], inside[j]);
        if ta == 0.0 && tb == 0.0 {
            continue;
        }
        let len = dist(pts[i], pts[j]);
        let need = r * ((ta * 0.5).tan() + (tb * 0.5).tan());
        if len + slack >= need {
            continue;
        }
        let at = Point2::new((pts[i].x + pts[j].x) * 0.5, (pts[i].y + pts[j].y) * 0.5);
        if ta > 0.0 && tb > 0.0 && ta < ARC_FACET_TURN && tb < ARC_FACET_TURN {
            // A facet of an inside arc: chord = 2 R sin(phi / 2).
            // The larger turn is a whole facet's (the arc's end vertex
            // turns half a facet onto its tangent line).
            let phi = ta.max(tb);
            let rad = len / (2.0 * (phi * 0.5).sin()).max(1e-12);
            return Err(refused(format!(
                "cannot cut the inside corner at ({:.3}, {:.3}): its radius {rad:.3} mm is under the tool's {r:.3} mm{}",
                at.x,
                at.y,
                hint()
            )));
        }
        return Err(refused(format!(
            "cannot cut the inside corner at ({:.3}, {:.3}): its {len:.3} mm edge is shorter than the {need:.3} mm the tool's {r:.3} mm offset needs{}",
            at.x,
            at.y,
            hint()
        )));
    }
    Ok(())
}

/// The arc law over the fitted walk the post writes: an inside G2/G3
/// (curving toward the tool) must not be tighter than the tool radius.
/// The walk runs in the direction of travel, where the tool stands left
/// under climb (G41) and right under conventional (G42) on either side
/// (#1555) - not the stored ring's side `tool_left` reads.
pub(super) fn arcs(walk: &[Seg], start: Point2, p: &ContourParams) -> Result<(), GenError> {
    let r = p.tool_diameter * 0.5;
    let left = p.climb;
    let mut at = start;
    for seg in walk {
        match seg {
            Seg::Line(qs) => {
                if let Some(q) = qs.last() {
                    at = *q;
                }
            }
            Seg::Arc { center, ccw, to } => {
                let rad = dist(*center, *to);
                if *ccw == left && rad + 1e-6 < r {
                    let mid = Point2::new((at.x + to.x) * 0.5, (at.y + to.y) * 0.5);
                    return Err(refused(format!(
                        "cannot cut the inside corner at ({:.3}, {:.3}): its radius {rad:.3} mm is under the tool's {r:.3} mm{}",
                        mid.x,
                        mid.y,
                        hint()
                    )));
                }
                at = *to;
            }
        }
    }
    Ok(())
}

/// The neck law: the ring offset by the register toward the tool must
/// stay one ring. A split names the ring's point farthest from the
/// offset - the narrow place the control's offset crosses itself at.
pub(super) fn neck(l: &Loop, p: &ContourParams) -> Result<(), GenError> {
    let r = p.tool_diameter * 0.5;
    let Ok(alone) = Region::new(vec![l.clone()]) else {
        return Err(refused("could not read its programmed ring as a simple loop"));
    };
    // Alone the ring reads as material CCW; a hole loop (CW) flips, so
    // the tool's side flips with it.
    let grow = tool_left(p) != l.is_ccw();
    let off = alone.offset(if grow { r } else { -r })?;
    if off.loops().len() == 1 {
        return Ok(());
    }
    // The neck sits between the pieces: the midpoint of the closest
    // pair of points on two different pieces of the offset.
    let mut best: Option<(Point2, f64)> = None;
    let pieces = off.loops();
    for (i, a) in pieces.iter().enumerate() {
        for b in pieces.iter().skip(i + 1) {
            for qa in a.points() {
                for qb in b.points() {
                    let d = dist(*qa, *qb);
                    if best.is_none_or(|(_, w)| d < w) {
                        best = Some((Point2::new((qa.x + qb.x) * 0.5, (qa.y + qb.y) * 0.5), d));
                    }
                }
            }
        }
    }
    let first = l.points().first().copied().unwrap_or(Point2::new(0.0, 0.0));
    let at = best.map_or(first, |(q, _)| q);
    Err(refused(format!(
        "cannot cut the gap at ({:.3}, {:.3}): it is narrower than the tool's {:.3} mm diameter, so the offset contour crosses itself{}",
        at.x,
        at.y,
        p.tool_diameter,
        hint()
    )))
}
