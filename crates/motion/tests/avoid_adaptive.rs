//! "Do not touch" on 2D Adaptive, Spiral and Trochoidal methods (#3346).
//! Provenance: independent `camcraft-motion` tests checking `camcraft-sim::verify::avoid`.
//! Gate map (M4.4): covers motion::machining2d::adaptive_avoid, motion::machining2d::adaptive_troch,
//! motion::machining2d::pocket.

use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_motion::machining2d::{
    AdaptiveTrochParams, CornerRelief, PassStart, PocketParams, PocketStrategy, StartSide, adaptive_spiral_avoid,
    adaptive_spiral_avoid_on, adaptive_trochoidal_avoid, adaptive_trochoidal_with, pocket, ARC_FIT_TOL,
};
use camcraft_motion::tools::ToolGeom;
use camcraft_motion::verify::{AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter, avoid_hits, crossed_pairs};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![
        Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)])
            .expect("loop"),
    ])
    .expect("region")
}

fn box_facets(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| {
        Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1])
    };
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)]
        .into_iter()
        .flatten()
        .collect()
}

fn stepped_cutter() -> Cutter {
    Cutter::flat(&ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 8.0,
        shank_radius: 4.5,
        shank_len: 10.0,
        holder_radius: 12.0,
        holder_len: 20.0,
    })
}

fn params() -> PocketParams {
    params_at(DEFAULT_OPTIMAL_LOAD)
}

/// The Spiral tests run at a high load: fewer passes keep the debug run
/// short, and the law is the same.
fn params_at(load: f64) -> PocketParams {
    PocketParams {
        tool_diameter: 6.0,
        stepover: 2.5,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        top_z: 0.0,
        bottom_z: -5.0,
        stepdown: 2.5,
        feed: 600.0,
        plunge_feed: 150.0,
        climb: true,
        strategy: PocketStrategy::Adaptive,
        retract_z: Some(2.0),
        entry: Entry3D::Helix,
        helix_radius: 1.5,
        optimal_load: load,
        blend_turns: false,
        blend_radius: None,
        links: Links::ROUGH3D,
    }
}

/// A 36 x 16 mm channel with a pin that stands 2 mm over the top at its east end.
fn channel_and_pin() -> (Region, AvoidSpec) {
    let spec = AvoidSpec {
        items: vec![AvoidItem { label: "east pin".into(), tris: box_facets([28.0, 5.0, -10.0], [33.0, 11.0, 2.0]), edges: vec![] }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    (rect(0.0, 0.0, 36.0, 16.0), spec)
}

#[test]
fn a_trochoidal_adaptive_keeps_the_full_tool_clear_of_a_protected_pin() {
    let (region, spec) = channel_and_pin();
    let t = AdaptiveTrochParams::default();
    // The plain lanes run to the channel's east end, through the pin.
    let plain = adaptive_trochoidal_with(&region, &params(), &t).expect("plain trochoidal");
    assert!(!avoid_hits(&plain, &spec, AVOID_VERIFY_TOL).expect("verify").is_empty(), "the plain path must reach the pin");

    let path = adaptive_trochoidal_avoid(&region, &params(), &t, &spec).expect("protected trochoidal");
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
    assert!(hits.is_empty(), "the protected path touches the pin: {hits:?}");
    assert!(path.moves.iter().any(|m| (m.to.z + 5.0).abs() < 1e-6), "the path reaches the floor");
    assert!(path.notes.iter().any(|n| n.contains("Do not touch (east pin)")), "{:?}", path.notes);
}

#[test]
fn an_empty_avoid_gives_the_plain_trochoidal_path() {
    let (region, _) = channel_and_pin();
    let t = AdaptiveTrochParams::default();
    let empty = AvoidSpec { items: vec![], cutter: stepped_cutter(), clearance: 1.0 };
    let far = AvoidSpec {
        items: vec![AvoidItem { label: "far block".into(), tris: box_facets([200.0, 200.0, -10.0], [210.0, 210.0, 2.0]), edges: vec![] }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    let plain = adaptive_trochoidal_with(&region, &params(), &t).expect("plain trochoidal");
    assert_eq!(adaptive_trochoidal_avoid(&region, &params(), &t, &empty).expect("empty"), plain);
    assert_eq!(adaptive_trochoidal_avoid(&region, &params(), &t, &far).expect("far"), plain);
}

#[test]
fn a_trochoidal_adaptive_with_no_room_left_is_refused_with_the_item_named() {
    let (region, _) = channel_and_pin();
    let shield = AvoidSpec {
        items: vec![AvoidItem { label: "full shield".into(), tris: box_facets([-5.0, -5.0, -10.0], [41.0, 21.0, 5.0]), edges: vec![] }],
        cutter: stepped_cutter(),
        clearance: 0.5,
    };
    let err = adaptive_trochoidal_avoid(&region, &params(), &AdaptiveTrochParams::default(), &shield)
        .expect_err("a covered channel must be refused");
    let msg = err.to_string();
    assert!(msg.contains("full shield") && msg.contains("Do not touch"), "{msg}");
}

#[test]
fn a_spiral_adaptive_keeps_the_full_tool_clear_of_a_protected_pin_with_no_crossing() {
    let (region, spec) = channel_and_pin();
    let plain = pocket(&region, &spiral()).expect("plain spiral");
    assert!(!avoid_hits(&plain, &spec, AVOID_VERIFY_TOL).expect("verify").is_empty(), "the plain path must reach the pin");

    let path = adaptive_spiral_avoid(&region, &spiral(), &spec, &CornerRelief::default()).expect("protected spiral");
    let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
    assert!(hits.is_empty(), "the protected path touches the pin: {hits:?}");
    assert!(crossed_pairs(&path, ARC_FIT_TOL).is_empty(), "the protected passes cross");
    assert!(path.moves.iter().any(|m| (m.to.z + 5.0).abs() < 1e-6), "the path reaches the floor");
    assert!(path.notes.iter().any(|n| n.contains("Do not touch (east pin)")), "{:?}", path.notes);

    // A pin in the middle leaves a ring channel round it.
    let mid = AvoidSpec {
        items: vec![AvoidItem { label: "mid pin".into(), tris: box_facets([15.0, 6.0, -10.0], [21.0, 10.0, 2.0]), edges: vec![] }],
        cutter: stepped_cutter(),
        clearance: 0.5,
    };
    let wide = rect(-4.0, -12.0, 40.0, 28.0);
    let ring = adaptive_spiral_avoid(&wide, &spiral(), &mid, &CornerRelief::default()).expect("ring spiral");
    assert!(avoid_hits(&ring, &mid, AVOID_VERIFY_TOL).expect("verify").is_empty(), "the ring path touches the pin");
    assert!(crossed_pairs(&ring, ARC_FIT_TOL).is_empty(), "the ring passes cross");
}

#[test]
fn a_spiral_adaptive_with_no_keepout_on_the_region_is_the_plain_path() {
    let (region, _) = channel_and_pin();
    let none = CornerRelief::default();
    let empty = AvoidSpec { items: vec![], cutter: stepped_cutter(), clearance: 1.0 };
    let far = AvoidSpec {
        items: vec![AvoidItem { label: "far block".into(), tris: box_facets([200.0, 200.0, -10.0], [210.0, 210.0, 2.0]), edges: vec![] }],
        cutter: stepped_cutter(),
        clearance: 1.0,
    };
    let plain = pocket(&region, &spiral()).expect("plain spiral");
    assert_eq!(adaptive_spiral_avoid(&region, &spiral(), &empty, &none).expect("empty"), plain);
    assert_eq!(adaptive_spiral_avoid(&region, &spiral(), &far, &none).expect("far"), plain);

    let shield = AvoidSpec {
        items: vec![AvoidItem { label: "full shield".into(), tris: box_facets([-5.0, -5.0, -10.0], [41.0, 21.0, 5.0]), edges: vec![] }],
        cutter: stepped_cutter(),
        clearance: 0.5,
    };
    let msg = adaptive_spiral_avoid(&region, &spiral(), &shield, &none).expect_err("covered").to_string();
    assert!(msg.contains("full shield") && msg.contains("Do not touch"), "{msg}");
}

#[test]
fn a_spiral_adaptive_with_a_pass_start_keeps_the_full_tool_clear_of_a_protected_pin() {
    // #3355 x #3346: Inside and Outside read the area that the keep-out
    // leaves; neither may bring the tool back to the pin.
    let (region, spec) = channel_and_pin();
    let none = CornerRelief::default();
    let auto = adaptive_spiral_avoid(&region, &spiral(), &spec, &none).expect("auto");
    let side = |start| StartSide { start, stock: Some(rect(-20.0, -20.0, 60.0, 40.0)), around_part: false };
    assert_eq!(
        adaptive_spiral_avoid_on(&region, &spiral(), &spec, &none, &StartSide::default()).expect("auto side"),
        auto,
        "Auto is the four-argument path"
    );
    for start in [PassStart::Inside, PassStart::Outside] {
        let path = adaptive_spiral_avoid_on(&region, &spiral(), &spec, &none, &side(start)).expect("pass start");
        let hits = avoid_hits(&path, &spec, AVOID_VERIFY_TOL).expect("verify");
        assert!(hits.is_empty(), "{start:?}: the protected path touches the pin: {hits:?}");
        assert!(path.moves.iter().any(|m| (m.to.z + 5.0).abs() < 1e-6), "{start:?}: the path reaches the floor");
        assert!(path.notes.iter().any(|n| n.contains("Do not touch (east pin)")), "{start:?}: {:?}", path.notes);
    }
}

fn spiral() -> PocketParams {
    params_at(0.4)
}
