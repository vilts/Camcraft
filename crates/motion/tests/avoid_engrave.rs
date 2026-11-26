//! Integration tests for `trace_avoid` (#3349).
//! Provenance: independent `camcraft-motion` tests checking `camcraft-sim::verify::avoid`.
//! Gate map: covers motion::machining2d::trace_avoid, sim::verify::avoid.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::{on_boundary, trace, trace_avoid, TraceParams};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{avoid_hits, AVOID_PAD, AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter};

fn rect_loop(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .unwrap()
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let p = |x, y, z| Point3::new(x, y, z);
    let v = |i: usize| p([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| -> [Triangle; 2] {
        [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))]
    };
    let mut t = Vec::with_capacity(12);
    t.extend(quad(4, 5, 7, 6));
    t.extend(quad(0, 2, 3, 1));
    t.extend(quad(0, 1, 5, 4));
    t.extend(quad(2, 6, 7, 3));
    t.extend(quad(1, 3, 7, 5));
    t.extend(quad(2, 0, 4, 6));
    t
}

fn stepped_cutter() -> Cutter {
    Cutter::flat(&ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 8.0,
        shank_radius: 5.5,
        shank_len: 10.0,
        holder_radius: 13.0,
        holder_len: 20.0,
    })
}

fn base_trace() -> TraceParams {
    TraceParams {
        clearance_z: 20.0,
        retract_z: Some(5.0),
        top_z: 0.0,
        bottom_z: -2.0,
        stepdown: 2.0,
        feed: 600.0,
        plunge_feed: 150.0,
        climb: true,
        stock_top: None,
    }
}

#[test]
fn empty_avoid_matches_plain_trace_exactly() {
    let reg = Region::new(vec![rect_loop(-20.0, -15.0, 20.0, 15.0)]).unwrap();
    let empty = AvoidSpec { items: vec![], cutter: stepped_cutter(), clearance: 0.0 };
    for climb in [true, false] {
        let mut p = base_trace();
        p.climb = climb;
        assert_eq!(trace_avoid(&reg, &p, &empty).unwrap(), trace(&reg, &p).unwrap());
    }
}

#[test]
fn trace_avoid_keeps_flutes_shank_holder_and_links_clear() {
    // Two separate engraved squares with a tall protected tower between them,
    // plus a holder-height overhang on the east side and a bare edge on the south.
    let reg = Region::new(vec![
        rect_loop(-28.0, -12.0, -12.0, 12.0),
        rect_loop(12.0, -12.0, 28.0, 12.0),
    ])
    .unwrap();
    let mut tris = box_facets([-4.0, -15.0, -6.0], [4.0, 15.0, 12.0]);
    // Overhang at z in [18..25]: clears the z=-2..6 flute and z=6..16 shank,
    // but hits the z=16..36 holder (r=13) when passing near x=28.
    tris.extend(box_facets([33.0, -5.0, 18.0], [40.0, 5.0, 25.0]));
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "tower & holder ledge".into(),
            tris,
            edges: vec![([-25.0, -14.5, -1.0], [-15.0, -14.5, -1.0])],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };

    for climb in [true, false] {
        let mut p = base_trace();
        p.climb = climb;
        let path = trace_avoid(&reg, &p, &spec).expect("trace_avoid succeeds");
        assert!(path.is_chained(), "path must remain chained");
        assert!(path.all_finite(), "path must remain finite");
        let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
        assert!(hits.is_empty(), "climb={climb}: avoid_hits must be empty: {hits:?}");

        // Must have cut on both islands (around x < -12 and x > 12) and elevated the rapid link over z=12.
        assert!(path.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.x < -12.0));
        assert!(path.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.x > 12.0));
        assert!(
            path.moves.iter().any(|m| m.kind == MoveKind::Rapid && m.to.z > 13.0 && (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1.0),
            "link across tall tower must elevate above z=12 + clearance"
        );
        // East wall near holder overhang (x=28, y in [-5..5]) must have been trimmed to protect holder (33 - 13 - 1 = 19).
        for m in path.moves.iter().filter(|m| m.kind == MoveKind::Feed) {
            if m.from.x > 19.0 && m.to.x > 19.0 {
                assert!(
                    m.from.y.abs() >= 5.0 || m.to.y.abs() >= 5.0 || (m.from.x <= 19.0 && m.to.x <= 19.0),
                    "unexpected cut near holder ledge: {m:?}"
                );
            }
        }
    }
}

#[test]
fn trace_avoid_rotates_closed_loop_seam_around_overhead_plunge_blocker() {
    // Closed square [-20..20] x [-20..20]; Clipper2 normalizes the start vertex `seam`.
    // At cut depth z=-2, tool top is -2 + 8 + 10 + 20 = 36 < 38 (passes under cleanly),
    // while at clearance_z=20 tool spans 20..58 (hits z=38..42 at `seam`).
    let reg = Region::new(vec![rect_loop(-20.0, -20.0, 20.0, 20.0)]).unwrap();
    let seam = reg.loops()[0].points()[0];
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "overhead bridge".into(),
            tris: box_facets([seam.x - 5.0, seam.y - 5.0, 38.0], [seam.x + 5.0, seam.y + 5.0, 42.0]),
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };

    let plain = trace(&reg, &base_trace()).expect("plain trace");
    let plain_hits = avoid_hits(&plain, &spec, AVOID_VERIFY_TOL).expect("verify plain");
    assert!(!plain_hits.is_empty(), "overhead bridge at normalized seam must hit plain trace");

    let path = trace_avoid(&reg, &base_trace(), &spec).expect("rotates seam outside bridge");
    assert!(path.is_chained());
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
    assert!(hits.is_empty(), "seam rotation must keep plunges/retracts clear: {hits:?}");
    let plunge = path.moves.iter().find(|m| m.kind == MoveKind::Plunge).expect("plunge");
    assert!(
        (plunge.from.x - seam.x).abs() > 1.0 || (plunge.from.y - seam.y).abs() > 1.0,
        "seam must rotate away from {seam:?}, got plunge at {:?}",
        plunge.from
    );
    assert!(path.moves.iter().any(|m| {
        m.kind == MoveKind::Feed && (m.to.x - seam.x).abs() < 1e-6 && (m.to.y - seam.y).abs() < 1e-6
    }));
}

#[test]
fn trace_avoid_trims_open_run_endpoints_under_overhead_clearance_blocker() {
    // The protected edge breaks the square's north side at cut depth, so the
    // level becomes one open run. The overhead bridge is above the cut but in
    // the hold of the clearance plunge and retract. The planner must trim the
    // open run's blocked ends (`endpoints_blocked` -> `trim_open_run`).
    let reg = Region::new(vec![rect_loop(-20.0, -20.0, 20.0, 20.0)]).unwrap();
    let spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "broken edge & overhead bridge".into(),
            tris: box_facets([3.0, 12.0, 38.0], [14.0, 28.0, 42.0]),
            edges: vec![([-2.0, 20.0, -1.0], [2.0, 20.0, -1.0])],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };

    let path = trace_avoid(&reg, &base_trace(), &spec).expect("trace_avoid succeeds");
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
    assert!(hits.is_empty(), "open-run ends must stay clear of the bridge: {hits:?}");

    // The planner trims to the clearance keep-out: the z sweep of the level.
    let ko = spec.keepout_region(-2.0, 20.0, AVOID_PAD).expect("clearance keep-out");
    let clear = |p: Point3| {
        let xy = Point2::new(p.x, p.y);
        !(ko.contains(xy) && !on_boundary(&ko, xy))
    };
    for m in path.moves.iter().filter(|m| m.kind == MoveKind::Plunge) {
        assert!(clear(m.from) && clear(m.to), "plunge inside the overhead bridge keep-out: {m:?}");
    }
    let retract = path
        .moves
        .iter()
        .rev()
        .find(|m| m.kind == MoveKind::Rapid && m.from.z < 0.0)
        .expect("retract from the cut");
    assert!(clear(retract.from), "retract inside the overhead bridge keep-out: {retract:?}");
}

#[test]
fn trace_avoid_notes_partial_level_block_and_refuses_total_block() {
    let reg = Region::new(vec![rect_loop(-10.0, -10.0, 10.0, 10.0)]).unwrap();
    // Covers all of [-10..10] up to z=-3.5. At z=-2.0, tip clears by 1.5 > 1.0; at z=-5.0, tip hits!
    let partial = AvoidSpec {
        items: vec![AvoidItem {
            label: "lower shelf".into(),
            tris: box_facets([-15.0, -15.0, -8.0], [15.0, 15.0, -3.5]),
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    let mut p = base_trace();
    p.top_z = 0.0;
    p.bottom_z = -5.0;
    p.stepdown = 2.0; // levels: -2.0, -4.0, -5.0
    let path = trace_avoid(&reg, &p, &partial).expect("upper level remains safe");
    assert!(avoid_hits(&path, &partial, AVOID_VERIFY_TOL).expect("verify").is_empty());
    assert!(
        path.notes.iter().any(|n| n.contains("lower shelf") && n.contains("blocked")),
        "expected partial level note: {:?}",
        path.notes
    );

    // Full block at all levels refuses naming the protected item.
    let full = AvoidSpec {
        items: vec![AvoidItem {
            label: "total shield".into(),
            tris: box_facets([-25.0, -25.0, -10.0], [25.0, 25.0, 5.0]),
            edges: vec![],
        }],
        cutter: stepped_cutter(),
        clearance: 0.5,
    };
    let err = trace_avoid(&reg, &p, &full).expect_err("totally blocked engrave must refuse");
    assert!(format!("{err:?}").contains("total shield"), "expected item name in refusal: {err:?}");
}
