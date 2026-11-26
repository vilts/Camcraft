//! #3362: Pass starts at (Auto / Inside / Outside) on the 3D finishing
//! passes with nested offset loops (Contour and Flat). The fixture is a
//! 20 x 20 x 10 mm block, a ball of radius 3, a stepover of 3 mm. The
//! top face gives four square passes, half-widths 10, 7, 4 and 1 from
//! the block centre. Auto and Outside cut them outside-in, Inside cuts
//! them inside-out. The set of cutting moves does not change: only the
//! sequence of the passes moves. Other patterns do not read the value.
//!
//! Gate map: covers camcraft_motion::finishing3d (shadow, flat).
//! Provenance: our own regression for #3362; no vendor material
//! consulted.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{
    contour, contour_from, flat, flat_from, reads_pass_start, waterline, Finish3DParams,
    FinishPattern, GapLink,
};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::PassStart;

const TOP: f64 = 10.0;

/// The 20 x 20 x 10 block, centred on the origin, outward normals.
fn block() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| Point3::new(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, TOP), q(1.0, -1.0, TOP), q(1.0, 1.0, TOP), q(-1.0, 1.0, TOP));
    let quad = |p: [Point3; 4]| [Triangle::new(p[0], p[1], p[2]), Triangle::new(p[0], p[2], p[3])];
    [quad([e, f, g, h]), quad([a, b, f, e]), quad([b, c, g, f]), quad([c, d, h, g]), quad([d, a, e, h])]
        .into_iter()
        .flatten()
        .collect()
}

fn params(pattern: FinishPattern) -> Finish3DParams {
    Finish3DParams {
        gap_link: GapLink::Jump,
        pattern,
        stepover: Some(3.0),
        tool_diameter: 6.0,
        tolerance: 0.1,
        clearance_z: 25.0,
        retract_z: None,
        top_z: TOP,
        bottom_z: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

fn run(pattern: FinishPattern, start: PassStart) -> Toolpath {
    let f = block();
    let p = params(pattern);
    match pattern {
        FinishPattern::Contour => contour_from(&f, &p, start).expect("contour"),
        FinishPattern::Flat => flat_from(&f, &p, start).expect("flat"),
        other => panic!("not a nested-loop pattern: {other:?}"),
    }
}

/// Is this move a cut on the top face (both ends at the top, inside the
/// block's own outline)?
fn top_cut(m: &camcraft_motion::cl::Move) -> bool {
    let hw = |q: Point3| q.x.abs().max(q.y.abs());
    m.kind == MoveKind::Feed
        && (m.from.z - TOP).abs() < 1e-6
        && (m.to.z - TOP).abs() < 1e-6
        && hw(m.from) <= TOP + 1e-6
        && hw(m.to) <= TOP + 1e-6
}

/// The half-widths of the top passes, in cut order (one entry for each
/// change of pass).
fn pass_order(t: &Toolpath) -> Vec<i64> {
    let mut v: Vec<i64> = Vec::new();
    for m in t.moves.iter().filter(|m| top_cut(m)) {
        let h = (m.to.x.abs().max(m.to.y.abs()) * 100.0).round() as i64;
        if v.last() != Some(&h) {
            v.push(h);
        }
    }
    v
}

/// Every cutting move, as a sorted multiset of rounded end points - the
/// order of the moves removed. A cutting move is a non-rapid move that
/// is not a pure vertical rise: the rise out of the last loop before a
/// link starts where that loop ends, so it follows the order.
fn cut_set(t: &Toolpath) -> Vec<[i64; 7]> {
    let k = |x: f64| (x * 1e5).round() as i64;
    let mut v: Vec<[i64; 7]> = t
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .filter(|m| !((m.to.x - m.from.x).hypot(m.to.y - m.from.y) < 1e-9 && m.to.z > m.from.z))
        .map(|m| [k(m.from.x), k(m.from.y), k(m.from.z), k(m.to.x), k(m.to.y), k(m.to.z), m.kind as i64])
        .collect();
    v.sort_unstable();
    v
}

/// No rapid moves sideways at or below the top face: each link between
/// passes rises before it travels.
fn no_low_rapid(what: &str, t: &Toolpath) {
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        let sideways = (m.to.x - m.from.x).hypot(m.to.y - m.from.y) > 1e-9;
        if sideways {
            assert!(
                m.from.z > TOP + 1e-6 && m.to.z > TOP + 1e-6,
                "{what}: a sideways rapid at or below the top face: {m:?}"
            );
        }
    }
}

#[test]
fn inside_cuts_the_innermost_offset_pass_first_on_contour_and_flat() {
    for pattern in [FinishPattern::Contour, FinishPattern::Flat] {
        let auto = run(pattern, PassStart::Auto);
        let inside = run(pattern, PassStart::Inside);
        let outside = run(pattern, PassStart::Outside);
        // The first four entries are the offset passes; Contour's ring
        // tail (the flank rings) follows them. `cut_set` below proves
        // that the tail and every pass keep their moves.
        let (a, i, o) = (pass_order(&auto), pass_order(&inside), pass_order(&outside));
        assert!(a.len() >= 4 && i.len() >= 4 && o.len() >= 4, "{pattern:?}: {a:?} {i:?} {o:?}");
        assert_eq!(a[..4], [1000, 700, 400, 100], "{pattern:?}: Auto cuts outside-in: {a:?}");
        assert_eq!(o[..4], [1000, 700, 400, 100], "{pattern:?}: Outside cuts outside-in: {o:?}");
        assert_eq!(i[..4], [100, 400, 700, 1000], "{pattern:?}: Inside cuts inside-out: {i:?}");
        assert_eq!(auto.moves, outside.moves, "{pattern:?}: Outside is Auto, move for move");
        assert_ne!(auto.moves, inside.moves, "{pattern:?}: Inside changes the order");
        let (sa, si) = (cut_set(&auto), cut_set(&inside));
        let only_a: Vec<_> = sa.iter().filter(|x| !si.contains(x)).collect();
        let only_i: Vec<_> = si.iter().filter(|x| !sa.contains(x)).collect();
        assert!(only_a.is_empty() && only_i.is_empty() && sa.len() == si.len(), "{pattern:?}: the same cuts ({} vs {}): auto only {only_a:?}; inside only {only_i:?}", sa.len(), si.len());
        for (what, t) in [("auto", &auto), ("inside", &inside), ("outside", &outside)] {
            no_low_rapid(&format!("{pattern:?} {what}"), t);
            assert!(t.all_finite(), "{pattern:?} {what}: finite");
        }
    }
}

/// `Auto` through the new door is the old door, byte for byte.
#[test]
fn auto_is_the_old_contour_and_flat_move_for_move() {
    let f = block();
    let c = params(FinishPattern::Contour);
    assert_eq!(contour(&f, &c).expect("contour").moves, contour_from(&f, &c, PassStart::Auto).expect("auto").moves);
    let fl = params(FinishPattern::Flat);
    assert_eq!(flat(&f, &fl).expect("flat").moves, flat_from(&f, &fl, PassStart::Auto).expect("auto").moves);
}

/// Only Contour and Flat read the value; Waterline (disjoint rings at
/// one Z) and every other pattern do not.
#[test]
fn only_the_nested_loop_patterns_read_pass_start() {
    assert!(reads_pass_start(FinishPattern::Contour));
    assert!(reads_pass_start(FinishPattern::Flat));
    for p in [
        FinishPattern::Waterline,
        FinishPattern::Parallel,
        FinishPattern::ParallelMulti,
        FinishPattern::Scallop,
        FinishPattern::Radial,
        FinishPattern::Spiral,
        FinishPattern::Project,
        FinishPattern::SteepShallow,
        FinishPattern::Pencil,
        FinishPattern::Corner,
        FinishPattern::Ramp,
        FinishPattern::MorphSpiral,
        FinishPattern::Geodesic,
        FinishPattern::Blend,
        FinishPattern::Morph,
        FinishPattern::Flow,
    ] {
        assert!(!reads_pass_start(p), "{p:?} has no inside or outside order");
    }
    // The waterline door takes no start side at all: one path.
    let f = block();
    let w = waterline(&f, &params(FinishPattern::Waterline)).expect("waterline");
    assert!(!w.moves.is_empty());
}
