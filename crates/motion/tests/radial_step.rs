//! Issue 1512: which number spaces a pocket's passes. Zigzag, Waterline
//! and Auto step by the stated stepover; Adaptive and Offset rings step by
//! the optimal load and never read the stepover - the Passes page shows
//! only the field that spaces the passes, so the doors must not refuse a
//! job over a stepover the strategy does not read.
//! Provenance: our own design (the tiling-step law of `machining2d::pocket`
//! and `machining3d::params`), per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d, camcraft_motion::machining3d.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{GenError, PocketParams, PocketStrategy, pocket};
use camcraft_motion::machining3d::{Pocket3DParams, pocket3d};

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

fn pp(strategy: PocketStrategy, stepover: f64, optimal_load: f64) -> PocketParams {
    PocketParams {
        blend_radius: None,
        links: camcraft_motion::links::Links::PLANAR,
        tool_diameter: 10.0,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: None,
        top_z: 0.0,
        bottom_z: -3.0,
        stepdown: 3.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load,
        blend_turns: false,
    }
}

const LOAD: f64 = camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;

/// The distinct Y values the zigzag rows cut at (feed moves that run
/// along X), rounded to a micron.
fn row_ys(t: &Toolpath) -> Vec<i64> {
    let mut ys: Vec<i64> = Vec::new();
    for m in &t.moves {
        let (p, q) = (m.to, m.from);
        if m.kind == MoveKind::Feed && (p.y - q.y).abs() < 1e-9 && (p.x - q.x).abs() > 1e-6 {
            let y = (p.y * 1000.0).round() as i64;
            if !ys.contains(&y) {
                ys.push(y);
            }
        }
    }
    ys.sort_unstable();
    ys
}

#[test]
fn zigzag_rows_follow_the_stepover() {
    let r = rect(0.0, 0.0, 60.0, 40.0);
    let fine = pocket(&r, &pp(PocketStrategy::Zigzag, 3.0, LOAD)).unwrap();
    let coarse = pocket(&r, &pp(PocketStrategy::Zigzag, 6.0, LOAD)).unwrap();
    let (yf, yc) = (row_ys(&fine), row_ys(&coarse));
    assert!(yf.len() > yc.len(), "3 mm rows {yf:?} must outnumber 6 mm rows {yc:?}");
    // Rows never stand further apart than the stepover asked for.
    for (ys, step) in [(&yf, 3.0), (&yc, 6.0)] {
        for w in ys.windows(2) {
            let gap = (w[1] - w[0]) as f64 / 1000.0;
            assert!(gap <= step + 1e-3, "row gap {gap} over the {step} mm stepover");
        }
    }
}

#[test]
fn a_stepover_past_reach_is_refused_by_name() {
    let r = rect(0.0, 0.0, 60.0, 40.0);
    match pocket(&r, &pp(PocketStrategy::Zigzag, 25.0, LOAD)) {
        Err(GenError::Invalid(msg)) => {
            assert!(msg.contains("stepover 25.000 mm"), "{msg}");
            assert!(msg.contains("10.000 mm"), "the reach must be named: {msg}");
        }
        other => panic!("a 25 mm stepover on a 10 mm tool must refuse, got {other:?}"),
    }
}

#[test]
fn load_strategies_step_by_the_load_not_the_stepover() {
    let r = rect(0.0, 0.0, 60.0, 40.0);
    let s = PocketStrategy::OffsetRings;
    let a = pocket(&r, &pp(s, 3.0, LOAD)).unwrap();
    let b = pocket(&r, &pp(s, 25.0, LOAD)).unwrap();
    assert_eq!(a, b, "the stepover is not read, the path must not move");
    let light = pocket(&r, &pp(s, 3.0, 0.10)).unwrap();
    let heavy = pocket(&r, &pp(s, 3.0, 0.30)).unwrap();
    assert_ne!(light, heavy, "the optimal load spaces the passes");
    assert!(light.moves.len() > heavy.moves.len(), "a lighter load cuts more, narrower passes");
}

#[test]
fn adaptive_clearing_never_reads_the_stepover() {
    // The HSM planner is the slow one in debug: a small pocket, two runs.
    let r = rect(0.0, 0.0, 16.0, 12.0);
    let a = pocket(&r, &pp(PocketStrategy::Adaptive, 3.0, LOAD)).unwrap();
    let b = pocket(&r, &pp(PocketStrategy::Adaptive, 25.0, LOAD)).unwrap();
    assert_eq!(a, b, "the stepover is not read, the path must not move");
}

#[test]
fn steps_by_load_names_the_load_strategies() {
    assert!(PocketStrategy::Adaptive.steps_by_load());
    assert!(PocketStrategy::OffsetRings.steps_by_load());
    for s in [PocketStrategy::Zigzag, PocketStrategy::Waterline, PocketStrategy::Auto] {
        assert!(!s.steps_by_load(), "{s:?} reads the stepover");
    }
}

/// A solid axis-aligned box as six quads, outward CCW.
fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1])
    };
    let mut t = Vec::with_capacity(12);
    for (a, b, c, d) in [(4, 5, 7, 6), (0, 2, 3, 1), (0, 1, 5, 4), (2, 6, 7, 3), (1, 3, 7, 5), (2, 0, 4, 6)] {
        t.push(Triangle::new(v(a), v(b), v(c)));
        t.push(Triangle::new(v(a), v(c), v(d)));
    }
    t
}

fn p3(strategy: PocketStrategy, stepover: f64) -> Pocket3DParams {
    Pocket3DParams {
        links: camcraft_motion::links::Links::ROUGH3D,
        tool_diameter: 8.0,
        stepover,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        top_z: 0.0,
        bottom_z: -10.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        strategy,
        retract_z: None,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: LOAD,
    }
}

#[test]
fn pocket3d_offset_rings_ignore_a_stale_wide_stepover() {
    let slab = box_facets([-20.0, -20.0, -20.0], [20.0, 20.0, -10.0]);
    let region = camcraft_motion::geometry::slice::project(&slab, 64).unwrap();
    let narrow = pocket3d(&slab, &region, &p3(PocketStrategy::OffsetRings, 3.0)).unwrap();
    let wide = pocket3d(&slab, &region, &p3(PocketStrategy::OffsetRings, 25.0)).unwrap();
    assert_eq!(narrow.moves, wide.moves, "the rings step by the load, not the stepover");
    // Under Zigzag the same number is read, and past the tool it refuses
    // by name.
    match pocket3d(&slab, &region, &p3(PocketStrategy::Zigzag, 25.0)) {
        Err(GenError::Invalid(msg)) => assert!(msg.contains("stepover 25 mm"), "{msg}"),
        other => panic!("Zigzag at 25 mm on an 8 mm tool must refuse, got {other:?}"),
    }
}
