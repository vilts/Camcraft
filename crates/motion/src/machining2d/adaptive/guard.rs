//! The plan's own crossing check (M11.4 C round 2): the verifier's
//! crossing law (`verify`, law (a): two feeds proper-crossing in XY at
//! one depth) asked of the planned walks before they are posted, so a
//! plan that would cross itself is refused here, by name, not posted
//! for the verifier to flag. Proper crossing by the orientation
//! predicate (O'Rourke, *Computational Geometry in C*, the textbook
//! segment test), segments bucketed on a uniform grid; ours. Issue
//! #1048: a chord end within [`TOUCH`] of one of the other chord's END
//! points reads as on that chord's line, so two chords that meet at one
//! vertex reached twice (a rounding apart) touch, as an exact shared
//! vertex always did, and do not cross. The same reading as the
//! verifier's law (a) (`camcraft_sim::verify::CROSSING_TOUCH` and
//! `proper_cross_touch`, #1129), so a plan this accepts is not held there.

use crate::geo2d::Point2;

/// How near a chord end stands to one of the other chord's END points
/// to count as that vertex (mm): 1 pm, a point-to-point distance. Kept
/// equal to the verifier's `camcraft_sim::verify::CROSSING_TOUCH` (no
/// motion -> sim dependency; change both together). Float rounding of a
/// vertex reached twice on a 10 m workspace is ~1e-12 mm (f64 at 1e4
/// mm), a thousand times below this; cutting tolerances are microns.
/// Only END points are excused: within `TOUCH` of the other chord's
/// LINE is not a touch, because a walk passing through a chord at a
/// vertex a picometre off its interior would make both of its chords
/// there read as touches and hide a crossing of any depth (the #1134
/// review's pass-through probe). Away from end points the exact sign
/// stays the law.
pub(super) const TOUCH: f64 = 1e-9;

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Do segments `ab` and `cd` cross properly (each strictly straddles
/// the other's line)?
pub(super) fn proper(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let (d1, d2) = (orient(a, b, c), orient(a, b, d));
    let (d3, d4) = (orient(c, d, a), orient(c, d, b));
    ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0)) && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
}

/// Which side of the line through `ab` the point `p` stands, -1, 0 or
/// 1, by the exact sign of the orientation - except that `p` within
/// [`TOUCH`] of `a` or `b` (a shared vertex a rounding apart) reads 0.
fn side(a: Point2, b: Point2, p: Point2) -> i8 {
    let near = |q: Point2| (p.x - q.x).hypot(p.y - q.y) <= TOUCH;
    if near(a) || near(b) {
        return 0;
    }
    let o = orient(a, b, p);
    if o > 0.0 {
        1
    } else if o < 0.0 {
        -1
    } else {
        0
    }
}

/// Do chords `ab` and `cd` cross (issue #1048)? The exact-sign
/// proper-crossing test, with a chord end within [`TOUCH`] of one of
/// the other chord's end points read as on its line: each chord's ends
/// stand strictly on opposite sides of the other's line. A shared end
/// point (exact or a rounding apart), an exact vertex on the other
/// chord, a collinear overlap or retrace is no crossing; a vertex a
/// rounding PAST the other chord's interior keeps the exact sign, so a
/// walk passing through there is still reported. A non-finite end
/// always crosses - the safe reading.
pub(super) fn crosses(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    if ![a, b, c, d].iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
        return true;
    }
    let (s1, s2) = (side(a, b, c), side(a, b, d));
    let (s3, s4) = (side(c, d, a), side(c, d, b));
    s1 * s2 < 0 && s3 * s4 < 0
}

/// The first place where two segments of the walks cross ([`crosses`];
/// consecutive segments of one walk share a vertex and never count),
/// or `None`. A non-finite point is reported as a crossing at itself.
pub(crate) fn first_crossing(walks: &[Vec<Point2>]) -> Option<Point2> {
    let mut first = None;
    scan(walks, |_, _, p| {
        first = Some(p);
        true
    });
    first
}

/// The pairs of walks (by index, ascending, once each) that cross
/// ([`crosses`]); a walk crossing itself pairs with itself, as does a
/// walk with a non-finite point.
pub(super) fn crossing_pairs(walks: &[Vec<Point2>]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    scan(walks, |a, b, _| {
        pairs.push((a.min(b), a.max(b)));
        false
    });
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

/// Every crossing of two segments of the walks, as (walk, walk, point),
/// until `f` says stop; a non-finite point first, as its own walk's.
fn scan(walks: &[Vec<Point2>], mut f: impl FnMut(usize, usize, Point2) -> bool) {
    for (w, pts) in walks.iter().enumerate() {
        if let Some(&p) = pts.iter().find(|p| !(p.x.is_finite() && p.y.is_finite()))
            && f(w, w, p)
        {
            return;
        }
    }
    let segs: Vec<(usize, usize, Point2, Point2)> = walks
        .iter()
        .enumerate()
        .flat_map(|(w, pts)| pts.windows(2).enumerate().map(move |(i, s)| (w, i, s[0], s[1])))
        .filter(|s| (s.3.x - s.2.x).hypot(s.3.y - s.2.y) > 1e-12)
        .collect();
    let (mut lo, mut hi) = (Point2::new(f64::INFINITY, f64::INFINITY), Point2::new(f64::NEG_INFINITY, f64::NEG_INFINITY));
    for s in &segs {
        for p in [s.2, s.3] {
            lo = Point2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Point2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
    }
    if segs.len() < 2 || !(lo.x.is_finite() && hi.x.is_finite() && lo.y.is_finite() && hi.y.is_finite()) {
        return;
    }
    let n = ((segs.len() as f64).sqrt().ceil() as usize).clamp(1, 512);
    let (cw, ch) = (((hi.x - lo.x) / n as f64).max(1e-9), ((hi.y - lo.y) / n as f64).max(1e-9));
    let cell = |x: f64, y: f64| {
        let ix = (((x - lo.x) / cw) as usize).min(n - 1);
        let iy = (((y - lo.y) / ch) as usize).min(n - 1);
        (ix, iy)
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); n * n];
    for (k, s) in segs.iter().enumerate() {
        let (a, b) = (cell(s.2.x.min(s.3.x), s.2.y.min(s.3.y)), cell(s.2.x.max(s.3.x), s.2.y.max(s.3.y)));
        for iy in a.1..=b.1 {
            for ix in a.0..=b.0 {
                grid[ix + iy * n].push(k);
            }
        }
    }
    for bucket in &grid {
        for (u, &i) in bucket.iter().enumerate() {
            for &j in &bucket[u + 1..] {
                let (s, t) = (segs[i], segs[j]);
                if s.0 == t.0 && s.1.abs_diff(t.1) <= 1 {
                    continue;
                }
                if crosses(s.2, s.3, t.2, t.3) && f(s.0, t.0, s.3) {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "guard_tests.rs"]
mod tests;
