//! The edge a no-pick 2D chamfer rides (#2435): the wall at the corner
//! depth, matched to the edge at the top plane.
//!
//! Provenance: the chamfer mill's cut law (machine-tool handbook
//! geometry, [`super::chamfer`]): the selected cone band's inner
//! radius reaches the wall at `top - width * tan(angle)`, with its
//! generatrix on the face. Where the wall is vertical the section at the top plane and
//! the section at the corner depth are one outline. Where the model
//! already carries a bevel (a modelled chamfer or round), the top
//! plane's outline is the bevel's TOP edge, standing inside the wall
//! by the bevel's width - a zero-radius tip on it sits inside the part.
//! The wall the cone band must touch is the section at the corner depth. Our own law
//! over this repo's sections; no vendor material consulted.

use crate::cl::TiltedFrame;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;

use super::face::GenError;

/// Plan slack of the match between a top edge and its wall, mm: the
/// chord sag two tessellated sections of one rounded corner can
/// disagree by. A loose match only picks which outline to ride; the
/// ride itself is always the wall section, and the program sim holds
/// a 2D chamfer's selected cone band to that wall (#2435) whatever was matched.
const MATCH_SLACK: f64 = 0.05;

/// Below this plan gap the wall IS the top edge (a vertical wall): the
/// top plane's own loop is ridden, unchanged, so every sharp-edged
/// part posts exactly what it did before #2435.
const SAME_EDGE: f64 = 1e-6;

/// The loops a no-pick 2D chamfer of `width` rides: for each loop of
/// `top` (the section at the top plane - the edges to break), the loop
/// of `corner` (the section at the authored corner depth - the wall the
/// selected cone band touches) that every vertex of the top loop stands
/// within `width` of,
/// in plan (one way only: a wall loop's mitred corner stands farther
/// than `width` from a bevel's top corner).
/// A vertical wall rides the top loop itself. A `corner` loop matched
/// to no top loop is a lower edge (a ledge, the plate under an island)
/// and is left alone: it is not an edge at the top plane.
/// #2596: when any top loop stands over a bevel, only the bevelled loops
/// are ridden; the sharp ones are edges the model leaves sharp.
///
/// # Errors
/// A top loop with no wall within `width` of it under it - a blind
/// pocket shallower than the chamfer's depth, an island whose wall
/// stands lower than that depth - refuses by name at that loop: the
/// tip would ride below the edge's floor, into the material (a part or
/// the stock, whichever the chamfer's source is - the text fits both, #2521).
/// #2887: a top loop bevelled on only part of its length (a modelled
/// chamfer on one side of a plate) refuses by name too: no whole
/// outline of it can be ridden. `lean` is [`wall_lean`]'s answer: the
/// only plan move by which a top loop may stand off its whole wall.
/// #2929: `mid` is the section half way down, `depth` the drop along
/// the tool axis from the top plane to the corner plane. A side that stands off
/// its wall as one straight face at a draft's slope is a drafted wall,
/// not a modelled chamfer: its loop rides the wall with the sharp ones.
pub fn ride_loops(
    top: &Region,
    mid: &Region,
    corner: &Region,
    width: f64,
    depth: f64,
    corner_z: f64,
    lean: Option<Point2>,
) -> Result<Region, GenError> {
    if !(width.is_finite() && width > 0.0 && corner_z.is_finite()) {
        return Err(GenError::Invalid("chamfer width must be positive".into()));
    }
    let reach = width + MATCH_SLACK;
    // #2596: the sharp edges (a vertical wall) and the bevelled ones (a
    // modelled chamfer or round) apart.
    let mut sharp: Vec<Loop> = Vec::with_capacity(top.loops().len());
    let mut out: Vec<Loop> = Vec::with_capacity(top.loops().len());
    for t in top.loops() {
        let wall = corner
            .loops()
            .iter()
            .filter(|c| c.is_ccw() == t.is_ccw())
            .map(|c| (c, gap(t.points(), c.points())))
            .filter(|(_, g)| *g <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        // #2887: a loop bevelled on only part of its length has no whole
        // outline to ride - refused by name, whichever way its
        // tessellation would have read it.
        let reading = wall.map(|(c, _)| read(t, c, mid, depth, lean));
        if let Some((bevel, sharp)) = reading.as_ref().and_then(|r| r.partly_bevelled(width)) {
            return Err(GenError::At {
                msg: format!(
                    "the edge loop near X{:.3} Y{:.3} carries a modelled chamfer on only part of its length - it is sharp near X{:.3} Y{:.3} - a chamfer with no picks would cut the sharp part; pick the edges to break",
                    bevel.x, bevel.y, sharp.x, sharp.y
                ),
                at: Point3::new(bevel.x, bevel.y, corner_z),
            });
        }
        match wall {
            // #2929: a drafted wall carries no modelled chamfer - its
            // loop is broken with the sharp ones, along the wall (its
            // corners may stand on the wall: the vertex gap reads 0).
            Some((c, _)) if reading.as_ref().is_some_and(|r| r.drafted && r.bevel.is_none()) => sharp.push(c.clone()),
            Some((_, g)) if g <= SAME_EDGE => sharp.push(t.clone()),
            Some((c, _)) => out.push(c.clone()),
            None => {
                let at = t.points().first().copied().unwrap_or(Point2::new(0.0, 0.0));
                return Err(GenError::At {
                    msg: format!(
                        "the edge near X{:.3} Y{:.3} has no wall under it down to the chamfer's depth Z{corner_z:.3} - a {width} mm chamfer there would ride into the material under it; lower the width or pick the edges to break",
                        at.x, at.y
                    ),
                    at: Point3::new(at.x, at.y, corner_z),
                });
            }
        }
    }
    // #2596: where the model carries a bevel at the top plane, the model
    // says which edges are chamfered - a sharp edge beside it is sharp
    // by design and is left alone (pick it to break it). A model with
    // no bevel at all breaks every top edge, as before (#303).
    if out.is_empty() {
        out = sharp;
        return Region::new(out).map_err(GenError::Geo);
    }
    let sides = |loops: &[Loop]| loops.iter().fold((0, 0), |(o, h), l| if l.is_ccw() { (o + 1, h) } else { (o, h + 1) });
    let want = sides(&out);
    let at = out.first().and_then(|l| l.points().first().copied()).unwrap_or(Point2::new(0.0, 0.0));
    let got = Region::new(out).map_err(GenError::Geo)?;
    // A bevelled edge whose enclosing edge is sharp (a modelled chamfer
    // on a pocket in a sharp plate) loses its nesting once the sharp
    // edge is left out: the region would read it the other way round,
    // and the ride would travel against its material. Refuse by name.
    if sides(got.loops()) != want {
        return Err(GenError::At {
            msg: format!(
                "the edge near X{:.3} Y{:.3} carries a modelled chamfer, but the edge around it is sharp - a chamfer with no picks cannot ride it alone; pick the edges to break",
                at.x, at.y
            ),
            at: Point3::new(at.x, at.y, corner_z),
        });
    }
    Ok(got)
}

/// #2887: a top loop bevelled on only part of its length. Judged per
/// segment of `top` - both ends and the middle against the wall ring -
/// so an extra collinear vertex, or the lack of one, changes nothing
/// (the vertex-only `gap` reads a one-sided bevel either way round).
/// A segment standing on the wall (within `SAME_EDGE`) is sharp; one
/// standing farther than `MATCH_SLACK` from it is bevelled; between
/// the two is tessellation noise and counts as neither. The loop is
/// partly bevelled ([`Reading::partly_bevelled`]) when it has a
/// bevelled segment AND more than `width` of sharp length: a ride of the wall would
/// cut the sharp length, a ride of the top loop would stand inside
/// the part at the bevel. #2936: a loop that is its wall moved whole by
/// the frame's `lean` (`leans`) carries no bevel at all. #2944: a
/// segment is judged in pieces, cut where a wall vertex stands over it,
/// so a side that runs on along the wall past its corner (an undercut
/// lip) reads as the sharp length plus the overhang it is. #2929: a
/// piece off its wall that [`drafted`] reads as a draft is no bevel.
fn read(top: &Loop, wall: &Loop, mid: &Region, depth: f64, lean: Option<Point2>) -> Reading {
    // Match one whole boundary with the same material side. A buried hole
    // cannot supply a draft point for an outer wall. Tied matches refuse.
    let mut middle: Vec<_> =
        mid.loops().iter().filter(|l| l.is_ccw() == wall.is_ccw()).map(|l| (l.points(), gap(l.points(), wall.points()).max(gap(wall.points(), l.points())))).collect();
    middle.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mid = middle.first().filter(|(_, d)| middle.get(1).is_none_or(|(_, next)| next - d > LEAN_SLACK)).map(|(p, _)| *p);
    let (top, wall) = (top.points(), wall.points());
    let mut r = Reading::default();
    if wall.is_empty() || lean.is_some_and(|l| leans(top, wall, l)) {
        return r;
    }
    for (i, &p) in top.iter().enumerate() {
        for (a, b) in pieces(p, top[(i + 1) % top.len()], wall) {
            let m = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            let d = near(a, wall).max(near(m, wall)).max(near(b, wall));
            if d <= SAME_EDGE {
                r.sharp_len += (b.x - a.x).hypot(b.y - a.y);
                r.sharp = r.sharp.or(Some(m));
            } else if lean.is_some_and(|l| drafted(a, b, wall, mid, depth, l)) {
                r.drafted = true;
            } else if d > MATCH_SLACK {
                r.bevel = r.bevel.or(Some(m));
            }
        }
    }
    r
}

/// How the pieces of one top loop stand over their wall ([`read`]): a
/// point on the first bevelled and the first sharp piece, the sharp
/// length, and whether any piece is a drafted wall (#2929).
#[derive(Debug, Default, PartialEq)]
struct Reading {
    bevel: Option<Point2>,
    sharp: Option<Point2>,
    sharp_len: f64,
    drafted: bool,
}

impl Reading {
    /// #2887: `Some((bevel, sharp))` when the loop has a bevelled piece
    /// AND more than `width` of sharp length.
    fn partly_bevelled(&self, width: f64) -> Option<(Point2, Point2)> {
        match (self.bevel, self.sharp) {
            (Some(b), Some(s)) if self.sharp_len > width => Some((b, s)),
            _ => None,
        }
    }
}

/// #2929: the steepest wall still read as a draft, `tan(15 deg)`.
/// Cast and moulded drafts stand well under it, a chamfer's face well
/// over it: a 45 degree bevel deeper than the ride is straight too.
const DRAFT_TAN: f64 = 0.267_949_192_431_122_7;

/// #2929: a straight drafted face has parallel section edges. Match
/// the entire piece to one corner-wall segment, then require its whole
/// half-depth segment on the corresponding boundary, in the same direction.
fn drafted(a: Point2, b: Point2, wall: &[Point2], mid: Option<&[Point2]>, depth: f64, lean: Point2) -> bool {
    if !(depth.is_finite() && depth > 0.0) {
        return false;
    }
    let Some(mid) = mid else { return false };
    let low = |p: Point2| Point2::new(p.x - lean.x, p.y - lean.y);
    let m = low(Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5));
    let Some((u, v)) = wall.iter().enumerate().map(|(i, &u)| (u, wall[(i + 1) % wall.len()])).min_by(|&(u, v), &(s, t)| seg(m, u, v).total_cmp(&seg(m, s, t))) else {
        return false;
    };
    let (dx, dy) = (v.x - u.x, v.y - u.y);
    let len = dx.hypot(dy);
    if len <= SAME_EDGE || (b.x - a.x) * dx + (b.y - a.y) * dy <= 0.0 || ((b.x - a.x) * dy - (b.y - a.y) * dx).abs() > LEAN_SLACK * len {
        return false;
    }
    let (wa, wb) = (seg_foot(low(a), u, v), seg_foot(low(b), u, v));
    // Clamping either projection at a wall corner cannot prove this face.
    let drift = |p: Point2, w: Point2| ((p.x - w.x) * dx + (p.y - w.y) * dy).abs() / len;
    if drift(low(a), wa) > LEAN_SLACK || drift(low(b), wb) > LEAN_SLACK {
        return false;
    }
    let d = seg(m, u, v);
    let max_d = (wa.x - low(a).x).hypot(wa.y - low(a).y).max((wb.x - low(b).x).hypot(wb.y - low(b).y));
    d > SAME_EDGE && max_d <= depth * DRAFT_TAN && covered(Point2::new((a.x + wa.x) * 0.5, (a.y + wa.y) * 0.5), Point2::new((b.x + wb.x) * 0.5, (b.y + wb.y) * 0.5), mid)
}

/// A complete directed segment must have contiguous boundary support.
/// Collinear mesh vertices may split that support, but a point crossing,
/// a reversed edge, or a gap cannot prove a straight face.
fn covered(a: Point2, b: Point2, ring: &[Point2]) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dy);
    if len <= SAME_EDGE {
        return false;
    }
    let project = |p: Point2| ((p.x - a.x) * dx + (p.y - a.y) * dy) / len;
    let on_line = |p: Point2| ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() <= LEAN_SLACK * len;
    let mut spans: Vec<_> = ring
        .iter()
        .enumerate()
        .filter_map(|(i, &u)| {
            let v = ring[(i + 1) % ring.len()];
            let (from, to) = (project(u), project(v));
            (to > from && on_line(u) && on_line(v)).then_some((from, to))
        })
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = 0.0;
    for (from, to) in spans {
        if from > end + LEAN_SLACK {
            return false;
        }
        end = end.max(to);
        if end >= len - LEAN_SLACK {
            return true;
        }
    }
    false
}

/// #2944: the segment `a`-`b` cut at the foot of each vertex of `wall`
/// that falls strictly inside it, in order from `a` to `b`.
fn pieces(a: Point2, b: Point2, wall: &[Point2]) -> Vec<(Point2, Point2)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let mut ts: Vec<f64> = if l2 > 1e-24 {
        wall.iter().map(|w| ((w.x - a.x) * dx + (w.y - a.y) * dy) / l2).filter(|t| *t > 1e-9 && *t < 1.0 - 1e-9).collect()
    } else {
        Vec::new()
    };
    ts.sort_by(f64::total_cmp);
    let at = |t: f64| Point2::new(a.x + t * dx, a.y + t * dy);
    let mut out = Vec::with_capacity(ts.len() + 1);
    let mut from = a;
    for t in ts {
        out.push((from, at(t)));
        from = at(t);
    }
    out.push((from, b));
    out
}

/// Plan slack of the lean test, mm: the sections stand on a 1e-6 mm
/// grid, so two copies of one outline disagree by about that much.
/// Far under `MATCH_SLACK`: no bevelled segment hides in it.
const LEAN_SLACK: f64 = 1e-4;

/// #2936: `top` is the whole of `wall` moved by `lean` - the two
/// sections of a prism whose upright walls lean in the operation's
/// frame (a box stock under a tilted frame). No segment of such a loop
/// is a modelled chamfer: the edge is the same edge all round, and the
/// ride is the wall at the corner depth. Each segment of each ring -
/// both ends and the middle - must stand on the other ring. A loop
/// bevelled on part of its length is no moved copy of its wall: its
/// bevelled side moved and its sharp sides did not. #2944: the move
/// is the frame's own, never one read off the sections - a bevel on
/// one side and an equal undercut lip on the other also move the whole
/// outline, but no upright frame leans a wall.
fn leans(top: &[Point2], wall: &[Point2], lean: Point2) -> bool {
    let on = |from: &[Point2], to: &[Point2], dx: f64, dy: f64| {
        from.iter().enumerate().all(|(i, &a)| {
            let b = from[(i + 1) % from.len()];
            let mid = Point2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
            [a, mid, b].iter().all(|p| near(Point2::new(p.x + dx, p.y + dy), to) <= LEAN_SLACK)
        })
    };
    !top.is_empty() && on(top, wall, -lean.x, -lean.y) && on(wall, top, lean.x, lean.y)
}

/// #2944: the plan move, from the corner plane up to the top plane
/// `depth` above it, of a wall that stands upright in the world, as the
/// operation's frame reads it. Zero with no frame or an upright one.
/// `None` when the tool axis lies in the world's XY (an upright wall has
/// no section there to move): no lean then excuses a moved loop.
#[must_use]
pub fn wall_lean(frame: Option<&TiltedFrame>, depth: f64) -> Option<Point2> {
    let Some(f) = frame else {
        return Some(Point2::new(0.0, 0.0));
    };
    // The world's +Z in the frame is (u.z, v.z, axis.z): climbing
    // `depth` along the axis climbs `depth / axis.z` along the wall.
    let k = depth / f.axis[2];
    let lean = Point2::new(f.u[2] * k, f.v[2] * k);
    (f.axis[2].abs() > 1e-9 && lean.x.is_finite() && lean.y.is_finite()).then_some(lean)
}

/// The farthest any vertex of `from` stands from the closed polyline
/// `to`, in plan (the one-sided Hausdorff gap over the vertices).
/// `+inf` when either side has no points.
fn gap(from: &[Point2], to: &[Point2]) -> f64 {
    if from.is_empty() || to.is_empty() {
        return f64::INFINITY;
    }
    from.iter()
        .map(|&p| near(p, to))
        .fold(0.0, f64::max)
}

/// Plan distance from `p` to the closed polyline `ring`.
fn near(p: Point2, ring: &[Point2]) -> f64 {
    let n = ring.len();
    (0..n)
        .map(|i| seg(p, ring[i], ring[(i + 1) % n]))
        .fold(f64::INFINITY, f64::min)
}

fn seg(p: Point2, a: Point2, b: Point2) -> f64 {
    let q = seg_foot(p, a, b);
    (q.x - p.x).hypot(q.y - p.y)
}

/// The point of the segment `a`-`b` nearest `p`.
fn seg_foot(p: Point2, a: Point2, b: Point2) -> Point2 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 1e-24 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    Point2::new(a.x + t * dx, a.y + t * dy)
}

#[cfg(test)]
#[path = "chamfer_ride_tests.rs"]
mod tests;
