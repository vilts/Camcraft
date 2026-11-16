//! The straight-chord law of the planar generators: does a chord stay
//! inside (or on) a closed region? Split from `slots.rs` by #1620, which
//! also closed its collinear hole (below).
//!
//! Provenance: segment/polygon intersection and point-in-polygon tests
//! (O'Rourke, "Computational Geometry in C", 2nd ed., ch. 7); our own
//! arrangement of them. No vendor material consulted.

use crate::geo2d::{Point2, Region};

use super::ring::{BOUNDARY_TOL, on_boundary};

/// Parameter slack along the chord: a touch closer than this to either
/// end is the end itself.
const EPS: f64 = 1e-9;

/// True when the straight chord `a` -> `b` never passes outside the
/// closed region: both ends in it, no loop edge properly crossing the
/// open chord, and every piece of the chord between two points where it
/// touches the boundary inside or on it. A chord running along a
/// boundary edge is exactly the tangency a wall pass is, so grazing
/// reads as clear.
///
/// Why pieces and not one midpoint (#1620): without a proper crossing
/// the chord can still leave the region where it runs along an edge and
/// slips off its end at a vertex whose other edge turns away - two
/// pieces of one area that meet the chord line only along their own
/// walls (the cutter-centre pieces either side of a part that pinches
/// the stock's band shut). Every vertex the chord touches without a
/// crossing splits the chord (the ends of a grazed edge are such
/// vertices), and the midpoint of each piece decides for that piece:
/// between two touches it crosses no boundary, so it lies wholly in the
/// closed region or wholly outside it.
///
/// A vertex is a touch when it lies within [`BOUNDARY_TOL`] mm of the
/// chord's line, measured as a distance - the same slack `on_boundary`
/// gives the chord's ends. An edge-parameter test alone misses it on a
/// short edge (a chamfer facet a few microns long), where a nanometer
/// off the line is many parameter units (#1620 review).
pub fn chord_clears(region: &Region, a: Point2, b: Point2) -> bool {
    if (!region.contains(a) && !on_boundary(region, a))
        || (!region.contains(b) && !on_boundary(region, b))
    {
        return false;
    }
    if a == b {
        return true;
    }
    let d = Point2::new(b.x - a.x, b.y - a.y);
    let mut cuts: Vec<f64> = vec![0.0, 1.0];
    for l in region.loops() {
        cuts.extend(l.points().iter().filter_map(|v| vertex_touch(a, d, *v)));
    }
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let c = pts[i];
            let e = pts[(i + 1) % n];
            let g = Point2::new(e.x - c.x, e.y - c.y);
            let ca = Point2::new(c.x - a.x, c.y - a.y);
            let den = d.x * g.y - d.y * g.x;
            if den == 0.0 {
                // Parallel to this edge. An edge on the chord's line is
                // a graze; its ends are vertices that the edges next to
                // it meet (below), and those touches split the chord.
                continue;
            }
            let t = (ca.x * g.y - ca.y * g.x) / den;
            let u = (ca.x * d.y - ca.y * d.x) / den;
            if !(EPS < t && t < 1.0 - EPS) {
                continue;
            }
            if u > EPS && u < 1.0 - EPS {
                return false; // crosses the open edge: a real crossing
            }
            if (-EPS..=1.0 + EPS).contains(&u) {
                // The chord goes through a loop vertex. That cuts the
                // boundary only if the two edges meeting there are on
                // opposite sides of the chord line; both on one side is
                // tangency (a fillet touching the straight it joins).
                let j = if u < 0.5 { i } else { (i + 1) % n };
                let p0 = pts[(j + n - 1) % n];
                let p2 = pts[(j + 1) % n];
                let h0 = (p0.x - a.x) * d.y - (p0.y - a.y) * d.x;
                let h2 = (p2.x - a.x) * d.y - (p2.y - a.y) * d.x;
                if h0 * h2 < 0.0 {
                    return false;
                }
                cuts.push(t);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|x, y| (*x - *y).abs() <= EPS);
    cuts.windows(2).all(|w| {
        let s = (w[0] + w[1]) * 0.5;
        let m = Point2::new(a.x + d.x * s, a.y + d.y * s);
        region.contains(m) || on_boundary(region, m)
    })
}

/// Where (as a chord parameter strictly between the ends) vertex `v`
/// touches the chord `a` -> `a + d`, when it lies within
/// [`BOUNDARY_TOL`] mm of the chord's line.
fn vertex_touch(a: Point2, d: Point2, v: Point2) -> Option<f64> {
    let dd = d.x * d.x + d.y * d.y;
    if !dd.is_finite() || dd <= 0.0 {
        return None;
    }
    let (vx, vy) = (v.x - a.x, v.y - a.y);
    let h = vx * d.y - vy * d.x; // the distance off the line, times |d|
    let t = (vx * d.x + vy * d.y) / dd;
    (h * h <= BOUNDARY_TOL * BOUNDARY_TOL * dd && EPS < t && t < 1.0 - EPS).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo2d::Loop;

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
        let p = Point2::new;
        Loop::new(vec![p(x0, y0), p(x1, y0), p(x1, y1), p(x0, y1)]).expect("loop")
    }

    /// #1620's shape: two pieces whose bottoms lie on one line with a
    /// gap between them. The chord from one piece's inner corner along
    /// the shared line to the far corner of the other grazes the second
    /// piece's wall and leaves the area across the gap.
    #[test]
    fn a_chord_along_two_pieces_walls_across_the_gap_between_them_does_not_clear() {
        let region = Region::new(vec![sq(-32.0, -32.0, -8.0, -8.0), sq(8.0, -32.0, 32.0, -8.0)]).expect("region");
        let (a, b) = (Point2::new(-8.0, -32.0), Point2::new(32.0, -32.0));
        assert!(!chord_clears(&region, a, b), "the gap x in (-8, 8) is outside the area");
        assert!(!chord_clears(&region, b, a), "either way along it");
        // Its two grazing halves on each piece's own wall still clear.
        assert!(chord_clears(&region, Point2::new(-32.0, -32.0), a));
        assert!(chord_clears(&region, Point2::new(8.0, -32.0), b));
    }

    /// The graze law the wall passes ride stays: a chord along one edge,
    /// past a convex corner tangent to the chord, inside one piece.
    #[test]
    fn a_chord_grazing_one_pieces_wall_still_clears() {
        let region = Region::new(vec![sq(0.0, 0.0, 30.0, 30.0)]).expect("region");
        assert!(chord_clears(&region, Point2::new(0.0, 0.0), Point2::new(30.0, 0.0)));
        assert!(chord_clears(&region, Point2::new(5.0, 0.0), Point2::new(25.0, 0.0)));
        assert!(chord_clears(&region, Point2::new(0.0, 0.0), Point2::new(30.0, 30.0)));
        assert!(!chord_clears(&region, Point2::new(0.0, 0.0), Point2::new(31.0, 0.0)));
    }

    /// The review's probe: the second piece's near corner is a 0.014 mm
    /// chamfer. A chord end under a nanometer below the wall line is on
    /// the boundary, and the chord line then passes the chamfer's lower
    /// vertex by a fraction of a nanometer - many parameter units of that
    /// short edge. The vertex still splits the chord, so the 16 mm gap
    /// is still read.
    #[test]
    fn a_chord_end_a_hair_off_the_wall_line_still_splits_at_a_short_chamfer() {
        let p = Point2::new;
        let chamfered = Loop::new(vec![p(8.01, -32.0), p(32.0, -32.0), p(32.0, -8.0), p(8.0, -8.0), p(8.0, -31.99)]).expect("loop");
        let region = Region::new(vec![sq(-32.0, -32.0, -8.0, -8.0), chamfered]).expect("region");
        for d in [0.0, 1e-12, 1e-10, 5e-10, 9e-10, -5e-10] {
            assert!(!chord_clears(&region, p(-8.0, -32.0), p(32.0, -32.0 - d)), "end {d:e} mm off the line: the gap x in (-8, 8) is outside");
        }
    }

    /// The vertex touch is a distance in mm, from both sides of
    /// `BOUNDARY_TOL`, on a long chord and a short one.
    #[test]
    fn a_vertex_touches_the_chord_within_the_boundary_tolerance_and_not_beyond() {
        let p = Point2::new;
        for len in [40.0, 0.01] {
            let (a, d) = (p(0.0, 0.0), p(len, 0.0));
            for side in [1.0, -1.0] {
                let near = p(len * 0.25, side * 0.9 * BOUNDARY_TOL);
                let far = p(len * 0.25, side * 1.1 * BOUNDARY_TOL);
                let t = vertex_touch(a, d, near).expect("0.9 tol off the line touches");
                assert!((t - 0.25).abs() < 1e-12, "at its foot on the chord: {t}");
                assert_eq!(vertex_touch(a, d, far), None, "1.1 tol off the line does not touch (len {len})");
            }
            assert_eq!(vertex_touch(a, d, a), None, "a chord end is not a split");
            assert_eq!(vertex_touch(a, d, p(len * 1.5, 0.0)), None, "a vertex past the end is not a split");
        }
    }
}
