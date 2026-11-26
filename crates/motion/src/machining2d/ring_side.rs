//! One ring set for every start side of an area round a part (#3744).
//! The rings are the level curves of the distance from the part (the
//! area's holes), clipped to the cutter-centre area: Inside walks them
//! from the part out, Outside walks the very same pieces from the far
//! edge in ([`mirror`]), and Auto only picks one of the two ([`resolve`]).
//! The start side changes the start point and the ring order, never the
//! rings. The ring ends on the area's edge are shared with its walk
//! ([`share_ends`]).
//!
//! Provenance: contour-parallel offset rings measured from the selected
//! contour (Held, "On the Computational Geometry of Pocket Machining",
//! Springer LNCS 500, 1991); the mirror and the Auto rule are ours. No
//! vendor material consulted.

use crate::geo2d::{Loop, Point2, Region};

use super::face::GenError;
use super::level_order::{Piece, WALL, on_wall};
use super::pass_start::{PassStart, StartSide};
use super::ring_fill::RING;
use super::ring::dist;
use super::pocket_strategy::PocketStrategy;

/// The pieces of one part's ring set, from the part out (the Inside
/// walk), turned into the Outside walk of the same set: each piece runs
/// the other way (the uncut side swaps with the cut side, so climb stays
/// climb), the area's outer edge becomes ring 0, a level `l` of `top`
/// (ring `j` is level `RING * j`, its fills between, #3788) becomes
/// `top + RING - l`, and the part wall is cut last. Core pieces (a part
/// with no hole) keep their own order.
pub(super) fn mirror(pieces: &mut [Piece]) {
    let max = pieces.iter().filter(|p| !p.core && p.level != WALL).map(|p| p.level).max().unwrap_or(0);
    let top = max.div_ceil(RING).saturating_mul(RING);
    for p in pieces.iter_mut().filter(|p| !p.core) {
        p.pts.reverse();
        p.level = match p.level {
            WALL => 0,
            0 => WALL,
            l => top.saturating_add(RING).saturating_sub(l),
        };
    }
}

/// The one place where Auto chooses a start side for the rings of an
/// area round a part (#3744): Outside when the stock footprint is known
/// and the outer edge of the cutter-centre area stands in open air all
/// round (an Outside start there owes no wall-first cut), else Inside.
/// Every other case (a pocket, a row strategy, a side already chosen)
/// is returned as it is.
pub(super) fn resolve(center: &Region, strategy: PocketStrategy, tool_r: f64, side: &StartSide) -> Result<StartSide, GenError> {
    if !resolves(strategy, side) {
        return Ok(side.clone());
    }
    if !(tool_r.is_finite() && tool_r > 0.0) {
        return Err(GenError::Invalid("the tool radius must be positive and finite".into()));
    }
    let open = match side.stock.as_ref() {
        Some(stock) => {
            let mut open = true;
            for l in outer_loops(center)? {
                if on_wall(l.points(), Some(stock), tool_r, 0.25 * tool_r) {
                    open = false;
                    break;
                }
            }
            open
        }
        None => false,
    };
    let start = if open { PassStart::Outside } else { PassStart::Inside };
    Ok(StartSide { start, ..side.clone() })
}

/// Does [`resolve`] choose a side for `side` (Auto round a part, a ring
/// strategy)?
pub(super) fn resolves(strategy: PocketStrategy, side: &StartSide) -> bool {
    side.start == PassStart::Auto && side.around_part && matches!(strategy, PocketStrategy::Waterline | PocketStrategy::OffsetRings)
}

/// The counter-clockwise loops of `region` that no other one encloses:
/// its outer edges (material nested in a hole has its own loop inside).
fn outer_loops(region: &Region) -> Result<Vec<&Loop>, GenError> {
    let ccw: Vec<&Loop> = region.loops().iter().filter(|l| l.is_ccw()).collect();
    let mut out = Vec::new();
    for (i, l) in ccw.iter().enumerate() {
        let Some(&p) = l.points().first() else { continue };
        let mut nested = false;
        for (k, o) in ccw.iter().enumerate() {
            if k != i && Region::new(vec![(*o).clone()])?.contains(p) {
                nested = true;
                break;
            }
        }
        if !nested {
            out.push(*l);
        }
    }
    Ok(out)
}

/// Each end of a ring piece that stands on the area's boundary becomes
/// a vertex of the boundary walk it stands on (ring 0 or a wall): the
/// two then share the point exactly, so the verifier's crossing law
/// reads a touch there, never a crossing. #3744: the clip puts an end on
/// Clipper's lattice, some nanometres off the boundary chord it stands on,
/// so the end is snapped within [`SNAP`] (a closed walk's closing chord
/// included): onto the chord as a new vertex, or onto the vertex it
/// stands next to.
pub(super) fn share_ends(out: &mut [Piece]) {
    let ends: Vec<Point2> = out
        .iter()
        .filter(|p| !p.closed && p.level != 0 && p.level != WALL)
        .flat_map(|p| p.pts.first().into_iter().chain(p.pts.last()).copied().collect::<Vec<_>>())
        .collect();
    for p in out.iter_mut().filter(|p| p.level == 0 || p.level == WALL) {
        for &e in &ends {
            share(p, e);
        }
    }
}

/// How far (mm) a ring end may stand off a boundary walk and still be
/// read as on it: well above the lattice error of the clip, well below
/// the side tolerance of `level_rings` (a micron).
const SNAP: f64 = 1e-4;

/// Make `e` a vertex of the boundary walk `p` when it stands within
/// [`SNAP`] of it.
fn share(p: &mut Piece, e: Point2) {
    let n = p.pts.len();
    if n < 2 || p.pts.contains(&e) {
        return;
    }
    let segs = if p.closed { n } else { n - 1 };
    let mut best: Option<(f64, usize, f64)> = None;
    for i in 0..segs {
        let (a, b) = (p.pts[i], p.pts[(i + 1) % n]);
        let (t, d) = foot(e, a, b);
        if d <= SNAP && best.is_none_or(|(bd, _, _)| d < bd) {
            best = Some((d, i, t));
        }
    }
    let Some((_, i, t)) = best else { return };
    let len = dist(p.pts[i], p.pts[(i + 1) % n]);
    // The two ends of an open walk are shared with the walk next to it
    // (ring 0 and a wall meet there): they never move.
    let fixed = |k: usize| !p.closed && (k == 0 || k == n - 1);
    if t * len <= SNAP {
        if !fixed(i) {
            p.pts[i] = e;
        }
    } else if (1.0 - t) * len <= SNAP {
        if !fixed((i + 1) % n) {
            p.pts[(i + 1) % n] = e;
        }
    } else {
        p.pts.insert(i + 1, e);
    }
}

/// The foot of `e` on the segment `a -> b`: its parameter (clamped to
/// the segment) and its distance.
fn foot(e: Point2, a: Point2, b: Point2) -> (f64, f64) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 && dd.is_finite() { (((e.x - a.x) * dx + (e.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (t, dist(e, Point2::new(a.x + t * dx, a.y + t * dy)))
}

/// A ring piece as a set of points: open pieces from the smaller end,
/// closed rings from their smallest vertex and counter-clockwise. Two
/// walks of one ring in any start point and sense read the same.
#[cfg(test)]
pub(super) fn normal(p: &Piece) -> (bool, Vec<Point2>) {
    let lex = |a: &Point2, b: &Point2| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y));
    let mut pts = p.pts.clone();
    if p.closed {
        if pts.len() > 1 && pts.first() == pts.last() {
            pts.pop();
        }
        let area: f64 = (0..pts.len()).map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            a.x * b.y - b.x * a.y
        }).sum();
        if area < 0.0 {
            pts.reverse();
        }
        if let Some(k) = (0..pts.len()).min_by(|&a, &b| lex(&pts[a], &pts[b])) {
            pts.rotate_left(k);
        }
    } else if let (Some(a), Some(b)) = (pts.first(), pts.last())
        && lex(b, a).is_lt()
    {
        pts.reverse();
    }
    (p.closed, pts)
}

#[cfg(test)]
#[path = "ring_side_tests.rs"]
mod tests;
