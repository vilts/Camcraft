//! #1071: a path that runs back over its own trace - the thread mill's
//! retract, its helix reversed at feed - is no crossing. The forward and
//! reversed arcs put their chord joints a last bit apart, and the
//! orientation predicate read those touches (and the collinear overlap)
//! as proper crossings: a matched thread mill was blocked by a crossing
//! pair per turn. A real crossing, at any angle a path can hold, still
//! reads as one.
//!
//! Gate map (#1071): covers camcraft_sim::verify (crossing).
//!
//! Provenance: the orientation predicate (O'Rourke, "Computational
//! Geometry in C") the law already cites; our own fixtures.

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{crossed_pairs, crossed_pairs_all};

fn feed(a: Point3, b: Point3) -> Move {
    Move::new(MoveKind::Feed, a, b, Some(400.0))
}

/// Half-turn helix arcs of growing radius about the origin, descending
/// half a pitch each, then the same ride reversed (the thread mill's
/// retract), from radius `r0` in steps of `dr`.
fn ride_and_retrace(r0: f64, dr: f64, turns: usize) -> Toolpath {
    let o = Point2::new(0.0, 0.0);
    let mut fwd = Vec::new();
    let (mut r, mut z) = (r0, 20.0);
    for _ in 0..turns {
        let a = Point3::new(r, 0.0, z);
        let b = Point3::new(-r, 0.0, z - 0.5);
        let c = Point3::new(r, 0.0, z - 1.0);
        fwd.push(feed(a, b).with_arc(o, true));
        fwd.push(feed(b, c).with_arc(o, true));
        let d = Point3::new(r + dr, 0.0, z - 1.0);
        fwd.push(feed(c, d));
        r += dr;
        z -= 1.0;
    }
    let back: Vec<Move> = fwd
        .iter()
        .rev()
        .map(|m| {
            let mut n = feed(m.to, m.from);
            if let Some(arc) = m.arc {
                n = n.with_arc(arc.center, !arc.ccw);
            }
            n
        })
        .collect();
    Toolpath { moves: fwd.into_iter().chain(back).collect(), ..Toolpath::default() }
}

#[test]
fn a_helix_run_back_over_its_own_trace_is_no_crossing() {
    // The #1071 thread mill's own numbers: r 2 stepping 0.1, and the
    // 2.5413 major ride.
    for (r0, dr) in [(2.0, 0.1), (2.5413, 0.0), (7.3, 0.037)] {
        let t = ride_and_retrace(r0, dr, 6);
        assert_eq!(crossed_pairs_all(&t, 0.01), Vec::<[usize; 2]>::new(), "r0 {r0}, dr {dr}");
        assert_eq!(crossed_pairs(&t, 0.01), Vec::<[usize; 2]>::new(), "r0 {r0}, dr {dr}");
    }
}

#[test]
fn a_real_crossing_still_reads_as_one_at_any_angle() {
    // Two planar feeds through one point at a shared depth, from square
    // to a shallow 0.001 rad: the touch guard (1 nm) never hides them.
    for ang in [std::f64::consts::FRAC_PI_2, 0.3, 0.01, 0.001] {
        let (c, s) = (ang.cos(), ang.sin());
        let t = Toolpath {
            moves: vec![
                feed(Point3::new(-10.0, 0.0, -1.0), Point3::new(10.0, 0.0, -1.0)),
                feed(Point3::new(-10.0 * c, -10.0 * s, -1.0), Point3::new(10.0 * c, 10.0 * s, -1.0)),
            ],
            ..Toolpath::default()
        };
        assert_eq!(crossed_pairs_all(&t, 0.01), vec![[0, 1]], "angle {ang}");
        assert_eq!(crossed_pairs(&t, 0.01), vec![[0, 1]], "angle {ang}");
    }
}
