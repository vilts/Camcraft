//! Slot wall lap (#2906): one lap of the cutter-centre stadium after the
//! rows, so the round ends are cut to size. Plain circle and line geometry
//! (a stadium boundary as two half circles and two tangent sides); our own
//! design, no vendor material.

use crate::geo2d::Point2;

use super::arcs::Seg;

/// The end of the row at offset `d` on the round end about `c`:
/// `c + s q u + d n` with `q = sqrt(h^2 - d^2)` and `s = +1` at `b`, `-1` at `a`. The
/// rows and the lap both call this, so a row end and the lap vertex there
/// are the same point to the bit: the crossing law reads them as a touch.
pub(super) fn cap_point(c: Point2, u: Point2, n: Point2, h: f64, d: f64, at_b: bool) -> Point2 {
    let q = (h * h - d * d).max(0.0).sqrt();
    let q = if at_b { q } else { -q };
    Point2::new(c.x + u.x * q + n.x * d, c.y + u.y * q + n.y * d)
}

/// The lap of the cutter-centre stadium of half-width `h` on `a` -> `b`
/// (unit axis `u`, unit normal `n = rot90(u)`), from the tangent point
/// the last row ends on and back to it. `ds` are the row offsets,
/// ascending, from `-h` to `h`. `at_b` says the start is `b + h n` (else
/// `a + h n`). `ccw` walks the lap with the slot's air on the left (the
/// climb sense of the rows); `!ccw` walks it the other way. Returns the
/// walk (for the ladder) and the lap's vertices (for the pass polyline),
/// without the start point.
///
/// A row touches a round end only at its end point, so the rows alone
/// leave up to `h (1 - cos)` on a wide slot's ends (0.86 mm on a 10 mm
/// slot with a 6 mm tool). The lap rides that arc exactly and never leaves
/// the stadium. Each cap is split at every row end and at its tip: an arc
/// never spans more than 90 degrees, and each row join (a chord between
/// two row ends) meets the lap only at its two end points, never across
/// a flattened arc chord.
#[allow(clippy::too_many_arguments)]
pub(super) fn wall_lap(a: Point2, b: Point2, u: Point2, n: Point2, h: f64, ds: &[f64], at_b: bool, ccw: bool) -> (Vec<Seg>, Vec<Point2>) {
    // The cap split offsets: every row's `d`, plus the tip where no row is.
    let mut cd: Vec<f64> = ds.to_vec();
    if !cd.iter().any(|d| d.abs() < 1e-6) {
        cd.push(0.0);
    }
    cd.sort_by(f64::total_cmp);
    let (lo, hi) = (cd.first().copied().unwrap_or(-h), cd.last().copied().unwrap_or(h));
    // The boundary in CCW order (air on the left): `b`'s cap from -n to +n
    // (d rising), the +n side back to `a`, `a`'s cap from +n to -n (d
    // falling), the -n side on to `b`. Corner k is (on b?, its d).
    let corner = [(true, lo), (true, hi), (false, hi), (false, lo)];
    let start = if at_b { 1usize } else { 2 };
    let mut segs = Vec::new();
    let mut pts = Vec::new();
    for k in 0..4usize {
        let (i, j) = if ccw { ((start + k) % 4, (start + k + 1) % 4) } else { ((start + 4 - k) % 4, (start + 3 - k) % 4) };
        let ((on_b, d0), (on_b_to, d1)) = (corner[i], corner[j]);
        if on_b != on_b_to {
            let c = if on_b_to { b } else { a };
            let to = cap_point(c, u, n, h, d1, on_b_to);
            segs.push(Seg::Line(vec![to]));
            pts.push(to);
            continue;
        }
        // A cap: every split offset past `d0`, up to and with `d1`.
        let c = if on_b { b } else { a };
        let walk: Vec<f64> = if d1 > d0 {
            cd.iter().copied().filter(|&d| d > d0 && d <= d1).collect()
        } else {
            cd.iter().rev().copied().filter(|&d| d < d0 && d >= d1).collect()
        };
        for d in walk {
            let to = cap_point(c, u, n, h, d, on_b);
            segs.push(Seg::Arc { center: c, ccw, to });
            pts.push(to);
        }
    }
    (segs, pts)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DS: [f64; 3] = [-2.0, 0.5, 2.0];

    fn lap(at_b: bool, ccw: bool) -> (Vec<Seg>, Vec<Point2>) {
        let (a, b) = (Point2::new(-25.0, 15.0), Point2::new(25.0, 15.0));
        wall_lap(a, b, Point2::new(1.0, 0.0), Point2::new(0.0, 1.0), 2.0, &DS, at_b, ccw)
    }

    fn arcs(segs: &[Seg]) -> usize {
        segs.iter().filter(|s| matches!(s, Seg::Arc { .. })).count()
    }

    #[test]
    fn the_lap_closes_on_its_start() {
        for (at_b, ccw) in [(true, true), (true, false), (false, true), (false, false)] {
            let (segs, pts) = lap(at_b, ccw);
            let start = if at_b { Point2::new(25.0, 17.0) } else { Point2::new(-25.0, 17.0) };
            // Each cap: split at the tip (d = 0) and at the row d = 0.5.
            assert_eq!((segs.len(), arcs(&segs)), (8, 6), "two straights and three arcs on each cap");
            assert_eq!(pts.last().copied(), Some(start), "at_b={at_b} ccw={ccw}");
        }
    }

    #[test]
    fn each_row_end_is_a_lap_vertex() {
        let (a, b) = (Point2::new(-25.0, 15.0), Point2::new(25.0, 15.0));
        let (u, n) = (Point2::new(1.0, 0.0), Point2::new(0.0, 1.0));
        let (_, pts) = lap(true, true);
        for d in DS {
            for (c, on_b) in [(a, false), (b, true)] {
                let p = cap_point(c, u, n, 2.0, d, on_b);
                assert!(pts.contains(&p), "row end {p:?} is not a lap vertex");
            }
        }
    }

    #[test]
    fn ccw_from_b_runs_the_plus_n_side_first() {
        let (segs, pts) = lap(true, true);
        assert!(matches!(segs.first(), Some(Seg::Line(_))));
        assert_eq!(pts.first().copied(), Some(Point2::new(-25.0, 17.0)));
        assert!(matches!(segs.get(1), Some(Seg::Arc { ccw: true, .. })));
    }

    #[test]
    fn cw_from_b_turns_round_b_first() {
        let (segs, pts) = lap(true, false);
        assert!(matches!(segs.first(), Some(Seg::Arc { ccw: false, .. })));
        // From d = 2 down: the row at d = 0.5 first, then the tip.
        let first = cap_point(Point2::new(25.0, 15.0), Point2::new(1.0, 0.0), Point2::new(0.0, 1.0), 2.0, 0.5, true);
        assert_eq!(pts.first().copied(), Some(first));
        assert_eq!(pts.get(1).copied(), Some(Point2::new(27.0, 15.0)), "b's tip");
    }
}
