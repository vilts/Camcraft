//! M11.1 A turning-point blends on the facing serpentine: each legal
//! row-end reversal is ONE G2/G3 semicircle on the step's own endpoints,
//! measured never to enter the footprint, never to reach more than its
//! radius past the sharp sweep, and to leave the footprint's coverage
//! unchanged; every refused turn is named in the notes.
//!
//! Gate map (M11.1): covers camcraft_motion::machining2d (face, blend).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{face, FaceParams, MIN_BLEND_RADIUS};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

fn p(stepover: f64, blend: bool) -> FaceParams {
    FaceParams { holes: Default::default(),
        tool_diameter: 10.0,
        stepover,
        clearance_z: 5.0,
        cut_z: -2.0,
        top_z: None,
        stepdown: None,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        blend_turns: blend,
        stock_top: None,
    }
}

fn arcs(t: &Toolpath) -> Vec<&Move> {
    t.moves.iter().filter(|m| m.arc.is_some()).collect()
}

/// Exact XY distance from `q` to a move: a segment, or the arc's swept
/// circle portion (angular test against the endpoints' sense).
fn dist(q: (f64, f64), m: &Move) -> f64 {
    let (ax, ay, bx, by) = (m.from.x, m.from.y, m.to.x, m.to.y);
    let ends = ((q.0 - ax).hypot(q.1 - ay)).min((q.0 - bx).hypot(q.1 - by));
    match m.arc {
        None => {
            let (dx, dy) = (bx - ax, by - ay);
            let l2 = dx * dx + dy * dy;
            if l2 == 0.0 {
                return ends;
            }
            let t = (((q.0 - ax) * dx + (q.1 - ay) * dy) / l2).clamp(0.0, 1.0);
            (q.0 - ax - t * dx).hypot(q.1 - ay - t * dy)
        }
        Some(a) => {
            let (cx, cy) = (a.center.x, a.center.y);
            let ang = |x: f64, y: f64| (y - cy).atan2(x - cx);
            let tau = std::f64::consts::TAU;
            let norm = |v: f64| v.rem_euclid(tau);
            let (s, e, qa) = (ang(ax, ay), ang(bx, by), ang(q.0, q.1));
            // Sweep measured in the travel sense.
            let (sweep, off) = if a.ccw {
                (norm(e - s), norm(qa - s))
            } else {
                (norm(s - e), norm(s - qa))
            };
            let rho = (ax - cx).hypot(ay - cy);
            if off <= sweep {
                ((q.0 - cx).hypot(q.1 - cy) - rho).abs()
            } else {
                ends
            }
        }
    }
}

fn path_dist(q: (f64, f64), t: &Toolpath, z: f64) -> f64 {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .map(|m| dist(q, m))
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn off_is_the_sharp_serpentine_and_says_nothing() {
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p(8.0, false)).unwrap();
    assert!(!t.has_arcs());
    assert!(t.notes.is_empty());
}

#[test]
fn every_legal_reversal_is_one_semicircle() {
    // Rows 5/13/21/29/35 under a 10 mm tool: strides 8,8,8,6 - all within
    // the diameter, so four reversals, four arcs, and no step moves.
    let sharp = face(&rect(0.0, 0.0, 40.0, 40.0), &p(8.0, false)).unwrap();
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p(8.0, true)).unwrap();
    assert!(t.is_chained() && t.all_finite());
    assert!(t.notes.is_empty(), "{:?}", t.notes);
    let a = arcs(&t);
    assert_eq!(a.len(), 4);
    // The arc IS the step, curved: same count, same endpoints, and every
    // other move equal to the sharp path's.
    assert_eq!(t.len(), sharp.len());
    for (b, s) in t.moves.iter().zip(&sharp.moves) {
        assert_eq!((b.kind, b.from, b.to, b.feed), (s.kind, s.from, s.to, s.feed));
    }
    let ys = [5.0, 13.0, 21.0, 29.0, 35.0];
    for (i, m) in a.iter().enumerate() {
        let arc = m.arc.unwrap();
        let rho = (ys[i + 1] - ys[i]) * 0.5;
        assert_eq!(m.kind, MoveKind::Feed);
        assert_eq!(m.feed, Some(500.0));
        assert_eq!((m.from.y, m.to.y), (ys[i], ys[i + 1]));
        assert_eq!(m.from.x, m.to.x, "a semicircle returns to its x");
        // Climb: even rows run +X and turn left (ccw) at x_out = 45, odd
        // rows right (cw) at x_in = -5; the apex bulges outward.
        let forward = i % 2 == 0;
        assert_eq!(arc.ccw, forward);
        assert_eq!(m.from.x, if forward { 45.0 } else { -5.0 });
        let apex = if forward { m.from.x + rho } else { m.from.x - rho };
        assert!(apex > 45.0 || apex < -5.0, "the bulge points away from the footprint");
        assert!((arc.center.y - (ys[i] + ys[i + 1]) * 0.5).abs() < 1e-9);
        assert!((arc.center.x - m.from.x).abs() < 1e-9);
    }
}

#[test]
fn conventional_and_staged_blend_every_level() {
    let mut q = p(8.0, true);
    q.climb = false;
    q.top_z = Some(1.0);
    q.stepdown = Some(1.5);
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &q).unwrap();
    // Levels -0.5, -2.0 (#2004: no air level at Top Z 1.0): four
    // reversals each.
    assert_eq!(arcs(&t).len(), 8);
    // Conventional: row 0 runs -X, so the first turn is to the right.
    assert!(!arcs(&t)[0].arc.unwrap().ccw);
    assert!(t.is_chained() && t.all_finite());
}

#[test]
fn the_blend_never_enters_the_footprint_and_reaches_at_most_its_radius_out() {
    // Measured, not assumed, on a grid over the footprint and its whole
    // overrun: (1) inside the footprint the blended tool reaches exactly
    // what the sharp tool reached - no new cut, no lost cut; (2) outside
    // it, the blended reach never exceeds the sharp reach by more than
    // the largest blend radius (the tool-radius bound); (3) every arc
    // point stands at least a tool radius off the footprint - the
    // verifier's boundary law, so the blended path verifies clean.
    for stepover in [8.0, 10.0, 4.0, 9.5] {
        let region = rect(0.0, 0.0, 40.0, 30.0);
        let sharp = face(&region, &p(stepover, false)).unwrap();
        let blend = face(&region, &p(stepover, true)).unwrap();
        assert!(blend.has_arcs(), "stepover {stepover}");
        let r = 5.0;
        let rho_max = (stepover * 0.5_f64).min(r);
        let eps = 1e-6;
        let n = 300;
        for i in 0..=n {
            for j in 0..=n {
                let x = -20.0 + 80.0 * f64::from(i) / f64::from(n);
                let y = -6.0 + 42.0 * f64::from(j) / f64::from(n);
                let (db, ds) = (path_dist((x, y), &blend, -2.0), path_dist((x, y), &sharp, -2.0));
                let inside = (0.0..=40.0).contains(&x) && (0.0..=30.0).contains(&y);
                if inside {
                    assert!(db > r - eps || ds <= r + eps, "stepover {stepover}: new cut at ({x},{y})");
                    assert!(ds > r - eps || db <= r + eps, "stepover {stepover}: lost cut at ({x},{y})");
                }
                assert!(
                    db > r + eps || ds <= r + rho_max + eps,
                    "stepover {stepover}: blend reaches ({x},{y}) past its bound"
                );
            }
        }
        for m in arcs(&blend) {
            for (a, b) in m.chords(0.001) {
                for q in [a, b] {
                    let off = (-q.x).max(q.x - 40.0).max(-q.y).max(q.y - 30.0);
                    assert!(off >= r - eps, "arc point ({},{}) within r of the footprint", q.x, q.y);
                }
            }
        }
    }
}

#[test]
fn a_stride_wider_than_the_tool_keeps_its_corner_and_says_so() {
    // Rows 5/17/29/35: strides 12,12 exceed the 10 mm diameter (the arc
    // would reach more than a radius past the overrun); the final 6 mm
    // snap blends.
    let t = face(&rect(0.0, 0.0, 40.0, 40.0), &p(12.0, true)).unwrap();
    assert_eq!(arcs(&t).len(), 1);
    assert_eq!(t.notes.len(), 1, "{:?}", t.notes);
    assert!(t.notes[0].contains("2 of 3") && t.notes[0].contains("wider"));
    let status = t.status_summary("Face1", 0, camcraft_motion::units::Unit::Millimeters);
    assert!(status.contains("1 arc(s)") && status.contains("kept sharp"), "{status}");
}

#[test]
fn a_sub_resolution_snap_stride_stays_sharp_by_name() {
    // A 10.005-tall footprint under a 10 mm tool: two rows 0.005 apart -
    // a semicircle of 2.5 um radius is below the arc resolution.
    let t = face(&rect(0.0, 0.0, 40.0, 10.005), &p(8.0, true)).unwrap();
    assert!(!t.has_arcs());
    assert_eq!(t.notes.len(), 1, "{:?}", t.notes);
    assert!(t.notes[0].contains("resolution"));
    // Just above the floor, the same snap row blends.
    let tall = 10.0 + 2.0 * MIN_BLEND_RADIUS + 0.001;
    let t = face(&rect(0.0, 0.0, 40.0, tall), &p(8.0, true)).unwrap();
    assert_eq!(t.arc_count(), 1);
    assert!(t.notes.is_empty(), "{:?}", t.notes);
}

#[test]
fn one_row_has_no_turn_and_no_note() {
    let t = face(&rect(0.0, 0.0, 40.0, 6.0), &p(8.0, true)).unwrap();
    assert!(!t.has_arcs() && t.notes.is_empty());
}

#[test]
fn blended_facing_is_deterministic() {
    let a = face(&rect(0.0, 0.0, 33.3, 27.1), &p(7.0, true)).unwrap();
    let b = face(&rect(0.0, 0.0, 33.3, 27.1), &p(7.0, true)).unwrap();
    assert_eq!(a, b);
}
