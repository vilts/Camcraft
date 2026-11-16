//! Law (a), Feed-Feed crossings by the orientation predicate - textbook computational geometry (O'Rourke, "Computational Geometry in C", public material) (split from `engine.rs` by M13.7; structure only), grid-bucketed by the uniform-grid broad phase (Franklin 1989, "Uniform grids: a technique for intersection detection on serial and parallel machines", Auto-Carto 9; M11.4 C round 4).
//!
//! Every Feed chord is filed into the uniform grid cells its bounding
//! box covers, and only chords sharing a cell are tested - a proper
//! crossing puts both chords' boxes over one point, so no crossing pair
//! is missed, and each pair is tested once, in the first cell their
//! boxes share (the lowest-corner rule). The predicate is the law's own
//! (`proper_cross_touch` at a shared depth), unchanged; the pair set it
//! reports is the all-pairs law's exactly ([`crossed_pairs_all`], kept
//! as the reference the equivalence tests hold it to).

use super::laws::proper_cross_touch;
use super::types::{Mode, VerifyContext, Warning};

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::Point2;

/// A Feed chord: its move, its XY ends, the move's Z ends.
type Chord = (usize, Point2, Point2, f64, f64);

/// A chord covering more cells than this is tested against every chord
/// instead (one long move must not flood the grid).
const WIDE_CELLS: usize = 256;

/// Cells per axis at most (the grid stays a small table).
const MAX_SIDE: f64 = 4096.0;

/// (a) Crossing: Feed-Feed pairs, proper XY crossings at shared depth.
/// Each feed contributes its chords (one for a straight move, so a
/// straight path walks exactly the pair set it always did); reports
/// are per move pair, ascending and deduped - indices hold stable
/// whatever the chord count.
pub(super) fn crossings(path: &Toolpath, cx: &VerifyContext, chord_tol: f64) -> Vec<Warning> {
    // Mode::Envelope stands this law down by design (M2.2c): a
    // crossed raster pattern makes two Feed chords cross in XY at
    // shared depth wherever its two families share a point, and the
    // stepover's own overlap already has neighboring lines covering
    // the same material - that intersection is the pattern's
    // substance, not a path defect. The waterline ring never made a
    // proper crossing, so only the multi-direction patterns reveal
    // the predicate's limits. What a harmful chord would be - one
    // biting into the surface or sailing above it - law (f) measures
    // directly, on the swept ball against the model, move by move.
    // Mode::Revolve stands it down too (M5.2 C): a turning path's
    // radial plunges all share the axis in XY, so every pair of
    // centers in a groove's ladder "crosses" at the very line the
    // part spins on - the XY reading of the predicate says nothing
    // true about a meridian path, which the revolve verdict judges.
    // Mode::Radial stands it down too (M5.2 F2): two live-tool holes
    // drilled at one height across one another ARE the cross-drilled
    // intersection the part asks for - the crossing point is kerf in
    // both bores, not a collision.
    if matches!(cx.mode, Mode::Envelope | Mode::Revolve | Mode::Radial | Mode::Rotary) {
        return Vec::new();
    }
    crossed_pairs(path, chord_tol)
        .into_iter()
        .map(|moves| Warning::Crossing { moves })
        .collect()
}

/// Every Feed chord of `path` at `chord_tol`, in move order.
fn chords(path: &Toolpath, chord_tol: f64) -> Vec<Chord> {
    let mut feeds = Vec::new();
    for (i, m) in path.moves.iter().enumerate() {
        if m.kind != MoveKind::Feed {
            continue;
        }
        for (a, b) in m.chords(chord_tol) {
            feeds.push((i, a, b, m.from.z, m.to.z));
        }
    }
    feeds
}

/// The law's pair predicate: two chords of different moves cross in XY
/// at a shared depth.
fn crosses(a: &Chord, b: &Chord) -> bool {
    let (ia, a0, a1, za0, za1) = *a;
    let (ib, b0, b1, zb0, zb1) = *b;
    if ia == ib {
        return false; // chords of one move share their joints
    }
    // Two planar chords cross only at a shared plane, exactly as
    // always. When a chord descends (a ramp diagonal, a helix arc
    // chord), a Z-interval that touches the other's plane at a single
    // endpoint is a GRAZE of the already-cut floor at the dive's own
    // start or end depth - which is what every 3-D entry does by
    // construction, over XY the level law already proved open. For a
    // non-planar pair only a shared depth RANGE (two descents whose
    // bands overlap) counts as crossing. (M2.1a's superset law is what
    // makes the graze safe; without it this would be a gouge.)
    let planar_pair = za0 == za1 && zb0 == zb1;
    let z_hit = if planar_pair {
        za0 == zb0
    } else {
        za0.max(za1).min(zb0.max(zb1)) - za0.min(za1).max(zb0.min(zb1)) > 0.0
    };
    // A shared vertex a rounding apart (within `CROSSING_TOUCH`) is a
    // touch, as an exact one always was (#1129).
    z_hit && proper_cross_touch(a0, a1, b0, b1)
}

/// The crossing move pairs, ascending and deduped, testing all chord
/// pairs: the quadratic reference the bucketed law is held to.
#[doc(hidden)]
#[must_use]
pub fn crossed_pairs_all(path: &Toolpath, chord_tol: f64) -> Vec<[usize; 2]> {
    let feeds = chords(path, chord_tol);
    let mut crossed = Vec::new();
    for (k, a) in feeds.iter().enumerate() {
        for b in feeds.iter().skip(k + 1) {
            if crosses(a, b) {
                crossed.push([a.0.min(b.0), a.0.max(b.0)]);
            }
        }
    }
    crossed.sort_unstable();
    crossed.dedup();
    crossed
}

/// Whether a chord's XY ends are all finite (only those are gridded).
fn finite(c: &Chord) -> bool {
    c.1.x.is_finite() && c.1.y.is_finite() && c.2.x.is_finite() && c.2.y.is_finite()
}

/// The grid: origin, cell size, cells per axis.
struct Grid {
    lo: Point2,
    cell: f64,
    nx: usize,
    ny: usize,
}

impl Grid {
    /// The grid over the finite chords' bounds; the cell about the mean
    /// chord, or the bounds over the root of the count, whichever is
    /// larger - capped at [`MAX_SIDE`] cells per axis.
    fn over(feeds: &[Chord]) -> Self {
        let mut lo = Point2::new(f64::INFINITY, f64::INFINITY);
        let mut hi = Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        let (mut total, mut n) = (0.0f64, 0usize);
        for c in feeds.iter().filter(|c| finite(c)) {
            lo = Point2::new(lo.x.min(c.1.x).min(c.2.x), lo.y.min(c.1.y).min(c.2.y));
            hi = Point2::new(hi.x.max(c.1.x).max(c.2.x), hi.y.max(c.1.y).max(c.2.y));
            total += (c.2.x - c.1.x).hypot(c.2.y - c.1.y);
            n += 1;
        }
        let (w, h) = (hi.x - lo.x, hi.y - lo.y);
        let mut cell = if n > 0 { (total / n as f64).max((w * h / n as f64).sqrt()) } else { 1.0 };
        let side = w.max(h);
        if !(cell.is_finite() && cell > 0.0) {
            cell = side.max(1.0);
        }
        cell = cell.max(side / MAX_SIDE).max(1e-9);
        // Saturating float-to-int casts: NaN and negatives read 0.
        let nx = ((w / cell).floor() as usize).min(MAX_SIDE as usize).saturating_add(1);
        let ny = ((h / cell).floor() as usize).min(MAX_SIDE as usize).saturating_add(1);
        Grid { lo, cell, nx, ny }
    }

    /// A chord's cell index box `[x0, y0, x1, y1]`; `None` = tested
    /// against every chord (non-finite, or wider than [`WIDE_CELLS`]).
    fn span(&self, c: &Chord) -> Option<[usize; 4]> {
        if !finite(c) {
            return None;
        }
        let ix = |x: f64| (((x - self.lo.x) / self.cell).floor().max(0.0) as usize).min(self.nx - 1);
        let iy = |y: f64| (((y - self.lo.y) / self.cell).floor().max(0.0) as usize).min(self.ny - 1);
        let s = [ix(c.1.x.min(c.2.x)), iy(c.1.y.min(c.2.y)), ix(c.1.x.max(c.2.x)), iy(c.1.y.max(c.2.y))];
        let cells = s[2]
            .saturating_sub(s[0])
            .saturating_add(1)
            .saturating_mul(s[3].saturating_sub(s[1]).saturating_add(1));
        (cells <= WIDE_CELLS).then_some(s)
    }
}

/// The crossing move pairs, ascending and deduped, by the grid broad
/// phase: the same set as [`crossed_pairs_all`].
#[doc(hidden)]
#[must_use]
pub fn crossed_pairs(path: &Toolpath, chord_tol: f64) -> Vec<[usize; 2]> {
    let feeds = chords(path, chord_tol);
    let grid = Grid::over(&feeds);
    let spans: Vec<Option<[usize; 4]>> = feeds.iter().map(|c| grid.span(c)).collect();
    let mut cells: Vec<Vec<usize>> = vec![Vec::new(); grid.nx.saturating_mul(grid.ny)];
    let mut wide: Vec<usize> = Vec::new();
    for (k, s) in spans.iter().enumerate() {
        match s {
            Some(s) => {
                for y in s[1]..=s[3] {
                    for x in s[0]..=s[2] {
                        if let Some(list) = cells.get_mut(x + y * grid.nx) {
                            list.push(k);
                        }
                    }
                }
            }
            None => wide.push(k),
        }
    }
    let mut crossed = Vec::new();
    let mut test = |p: usize, q: usize| {
        let (lo, hi) = (p.min(q), p.max(q));
        if let (Some(a), Some(b)) = (feeds.get(lo), feeds.get(hi))
            && crosses(a, b)
        {
            crossed.push([a.0.min(b.0), a.0.max(b.0)]);
        }
    };
    for (ci, list) in cells.iter().enumerate() {
        let (cx, cy) = (ci % grid.nx, ci / grid.nx);
        for (u, &p) in list.iter().enumerate() {
            let Some(Some(sp)) = spans.get(p) else { continue };
            for &q in list.iter().skip(u + 1) {
                let Some(Some(sq)) = spans.get(q) else { continue };
                // Tested once: in the first cell both boxes cover.
                if sp[0].max(sq[0]) == cx && sp[1].max(sq[1]) == cy {
                    test(p, q);
                }
            }
        }
    }
    for (u, &p) in wide.iter().enumerate() {
        // Wide against every narrow chord, and wide pairs once.
        for (q, s) in spans.iter().enumerate() {
            if s.is_some() {
                test(p, q);
            }
        }
        for &q in wide.iter().skip(u + 1) {
            test(p, q);
        }
    }
    crossed.sort_unstable();
    crossed.dedup();
    crossed
}
