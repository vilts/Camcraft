//! #3330: the user's start and exit points on the 5-axis contour's
//! rings (finishing3d::ring_seam), a module of the `multiaxis` suite.

use super::*;
use camcraft_motion::finishing3d::{RingSeam, waterline_seamed};
use camcraft_motion::multiaxis::contour5d_seamed;

fn seam(start: Option<(f64, f64)>, exit: Option<(f64, f64)>) -> RingSeam {
    let pt = |(x, y)| camcraft_motion::geo2d::Point2::new(x, y);
    RingSeam { start: start.map(pt), exit: exit.map(pt) }
}

/// The step that samples each cut move along its length: much finer
/// than any chord a gouge can take across a ring.
const STEP: f64 = 0.25;

/// Each ball centre segment of the automatic ladder's level cuts.
fn ladder(plan: &camcraft_motion::cl::Toolpath) -> Vec<(Point3, Point3)> {
    plan.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-12)
        .flat_map(|m| m.segments3(1e-4))
        .map(|(a, b)| (p3(a.x, a.y, a.z + R), p3(b.x, b.y, b.z + R)))
        .collect()
}

fn off(segs: &[(Point3, Point3)], c: Point3) -> f64 {
    let mut best = f64::INFINITY;
    for &(a, b) in segs {
        let d = b - a;
        let l2 = d.dot(d);
        let t = if l2 > 0.0 { ((c - a).dot(d) / l2).clamp(0.0, 1.0) } else { 0.0 };
        let e = c - p3(a.x + d.x * t, a.y + d.y * t, a.z + d.z * t);
        best = best.min(e.dot(e).sqrt());
    }
    best
}

/// Every cut move at a level rides a ring of the automatic ladder
/// along its whole length, not only at its end: each move is sampled
/// every [`STEP`] mm, so a chord across a ring (a gouge) is seen. The
/// pick changes the order, never the geometry (the fitted arcs of the
/// two walks agree to the fit's own quarter tolerance).
fn cuts_ride_the_ladder(plan: &camcraft_motion::cl::Toolpath, got: &camcraft_motion::cl::Toolpath) {
    let segs = ladder(plan);
    for m in got.moves.iter().filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-12) {
        for (a, b) in m.segments3(1e-3) {
            let d = b - a;
            let n = ((d.dot(d).sqrt() / STEP).ceil() as usize).max(1);
            for k in 0..=n {
                let t = k as f64 / n as f64;
                let c = p3(a.x + d.x * t, a.y + d.y * t, a.z + d.z * t + R);
                let e = off(&segs, c);
                assert!(e < TOL, "a cut stands {e} mm off the automatic ladder at {c:?} (move to {:?})", m.to);
            }
        }
    }
}

/// Each ring the path cuts: (first point, last point) of the feed run
/// at one level that a plunge begins. An open run keeps the ends its
/// partition gave it; a closed ring returns to its first point.
fn runs(t: &camcraft_motion::cl::Toolpath) -> Vec<(Point3, Point3)> {
    let mut out = Vec::new();
    let mut k = 0;
    while k < t.moves.len() {
        if t.moves[k].kind != MoveKind::Plunge {
            k += 1;
            continue;
        }
        let first = t.moves[k].to;
        let mut last = first;
        k += 1;
        while k < t.moves.len() && t.moves[k].kind == MoveKind::Feed && (t.moves[k].to.z - first.z).abs() < 1e-12 {
            last = t.moves[k].to;
            k += 1;
        }
        out.push((first, last));
    }
    out
}

fn on_ray(q: Point3, x: bool) -> bool {
    if x { q.y.abs() < 1e-9 && q.x > 0.0 } else { q.x.abs() < 1e-9 && q.y > 0.0 }
}

#[test]
fn contour5d_start_point_seats_every_ring_at_the_pick() {
    let facets = dome(15.0, 0.9, 8, 16);
    let p = finish(12.0, 9.8);
    let s = seam(Some((13.0, 0.0)), None);
    let plan = waterline(&facets, &p).unwrap();
    let got = waterline_seamed(&facets, &p, &s).unwrap();
    let closed: Vec<_> = runs(&got).into_iter().filter(|(a, b)| a == b).collect();
    assert!(closed.len() > 1, "the ladder has several closed rings: {}", closed.len());
    for (a, _) in &closed {
        assert!(on_ray(*a, true), "a closed ring starts off the pick's ray: {a:?}");
    }
    cuts_ride_the_ladder(&plan, &got);
    // The posed kind poses that same seated ladder.
    let posed = contour5d_seamed(&facets, &p, &control(20.0, 0.0, Some(45.0)), None, &s).unwrap();
    balls_are_the_plan(&got, &posed);
    assert!(posed.is_chained());
}

#[test]
fn contour5d_exit_point_ends_every_ring_at_the_pick() {
    let facets = dome(15.0, 0.9, 8, 16);
    let p = finish(12.0, 9.8);
    let plan = waterline(&facets, &p).unwrap();
    let got = waterline_seamed(&facets, &p, &seam(Some((13.0, 0.0)), Some((0.0, 13.0)))).unwrap();
    let seated: Vec<_> = runs(&got).into_iter().filter(|(a, _)| on_ray(*a, true)).collect();
    assert!(seated.len() > 1, "several closed rings: {}", seated.len());
    for (_, b) in &seated {
        assert!(on_ray(*b, false), "a ring leaves off the exit pick's ray: {b:?}");
    }
    cuts_ride_the_ladder(&plan, &got);
    let cut = |t: &camcraft_motion::cl::Toolpath| t.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| m.segments3(1e-3)).map(|(a, b)| (b - a).dot(b - a).sqrt()).sum::<f64>();
    assert!(cut(&got) > cut(&plan), "each lap runs on past its seam to the exit");
}

/// A closed box from `lo` to `hi`, twelve facets.
fn block(lo: Point3, hi: Point3) -> Vec<Triangle> {
    let c = |i: usize| p3(if i & 1 == 0 { lo.x } else { hi.x }, if i & 2 == 0 { lo.y } else { hi.y }, if i & 4 == 0 { lo.z } else { hi.z });
    [[0, 2, 3], [0, 3, 1], [4, 5, 7], [4, 7, 6], [0, 1, 5], [0, 5, 4], [2, 6, 7], [2, 7, 3], [0, 4, 6], [0, 6, 2], [1, 3, 7], [1, 7, 5]]
        .iter()
        .map(|f| Triangle::new(c(f[0]), c(f[1]), c(f[2])))
        .collect()
}

/// A canopy stands above the exit point, below the clearance plane.
/// The waterline's column trim opens each ring under it, so no ring is
/// cut beneath it and the exit pick has no closed ring: refused by the
/// exit point's name, never cut with a retract through the part. (The
/// exit column law itself is pinned on the untrimmed ladder, in the
/// crate's `ring_seam_tests`.)
#[test]
fn contour5d_exit_point_under_a_canopy_is_refused_by_name() {
    let mut facets = dome(15.0, 0.9, 8, 16);
    facets.extend(block(p3(-1.5, 11.5, 19.0), p3(1.5, 14.5, 19.5)));
    let p = finish(12.0, 9.8);
    let e = waterline_seamed(&facets, &p, &seam(None, Some((0.0, 13.0)))).err();
    let Some(GenError::Invalid(m)) = e else { panic!("the exit under the canopy is refused as invalid, got {e:?}") };
    assert!(m.starts_with("exit point (0.000, 13.000): "), "{m}");
    let posed = contour5d_seamed(&facets, &p, &control(20.0, 0.0, Some(45.0)), None, &seam(None, Some((0.0, 13.0))));
    assert!(matches!(&posed, Err(GenError::Invalid(m)) if m.starts_with("exit point (0.000, 13.000): ")), "{posed:?}");
}

#[test]
fn contour5d_pick_far_from_every_ring_is_refused_by_name() {
    let facets = dome(15.0, 0.9, 8, 16);
    let p = finish(12.0, 9.8);
    for (s, what) in [(seam(Some((100.0, 100.0)), None), "start point (100.000, 100.000)"), (seam(None, Some((100.0, 100.0))), "exit point (100.000, 100.000)")] {
        let e = contour5d_seamed(&facets, &p, &control(20.0, 0.0, Some(45.0)), None, &s).err();
        let Some(GenError::Invalid(m)) = e else { panic!("{what}: refused as invalid, got {e:?}") };
        assert!(m.contains(what) && m.contains("farther than"), "{m}");
    }
}

#[test]
fn contour5d_without_a_pick_is_the_automatic_ladder_byte_for_byte() {
    let facets = dome(15.0, 0.9, 8, 16);
    let p = finish(12.0, 9.8);
    let c = control(20.0, 0.0, Some(45.0));
    assert_eq!(waterline_seamed(&facets, &p, &RingSeam::default()).unwrap(), waterline(&facets, &p).unwrap());
    assert_eq!(contour5d_seamed(&facets, &p, &c, None, &RingSeam::default()).unwrap(), contour5d(&facets, &p, &c, None).unwrap());
}
