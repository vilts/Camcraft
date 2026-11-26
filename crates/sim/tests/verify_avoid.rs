//! Unit tests for `sim::verify::avoid` (#2055): 3D clearance across
//! flutes, shank, holder, axial faces, and 3D edges.
//! Provenance: our own geometry checks; no vendor source consulted.

use camcraft_sim::cl::{ArcMove, ArcPlane, Move, MoveKind, PathBuilder, Toolpath};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::tools::ToolGeom;
use camcraft_sim::verify::{
    AVOID_PAD, AVOID_VERIFY_TOL, AvoidItem, AvoidSpec, Cutter, FixturePart, Warning, avoid_hits,
};

fn stepped_cutter() -> Cutter {
    Cutter::flat(&ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 6.0,
        shank_len: 12.0,
        holder_radius: 15.0,
        holder_len: 20.0,
    })
}

fn seg_path(a: [f64; 3], b: [f64; 3]) -> Toolpath {
    let mut pb = PathBuilder::new(Point3::new(a[0], a[1], a[2]));
    pb.push(MoveKind::Feed, Point3::new(b[0], b[1], b[2]), Some(500.0));
    pb.finish()
}

#[test]
fn detects_axial_holder_and_edge_clearance_violations() {
    let cutter = stepped_cutter();
    // Vertical wall at x = 0, y in [-5, 5], z in [0, 30], plus horizontal edge at y = -15, z = 10 over [-20..-10].
    let item = AvoidItem {
        label: "tower wall".into(),
        tris: vec![
            Triangle::new(Point3::new(0.0, -5.0, 0.0), Point3::new(0.0, 5.0, 0.0), Point3::new(0.0, 5.0, 30.0)),
            Triangle::new(Point3::new(0.0, -5.0, 0.0), Point3::new(0.0, 5.0, 30.0), Point3::new(0.0, -5.0, 30.0)),
        ],
        edges: vec![([-20.0, -15.0, 10.0], [-10.0, -15.0, 10.0])],
    };
    let spec = AvoidSpec { items: vec![item], cutter, clearance: 1.5 };

    // Move at z = 5, x = -15: flute r=3 is 12 mm from wall (ok), shank r=6 is 9 mm (ok),
    // HOLDER (z = 27..47, r = 15) hits wall z = 30 within 1.5 mm clearance (short = 1.5)!
    let hit_holder = avoid_hits(&seg_path([-15.0, -2.0, 5.0], [-15.0, 2.0, 5.0]), &spec, AVOID_VERIFY_TOL)
        .expect("verify runs");
    assert_eq!(hit_holder.len(), 1);
    assert!(matches!(hit_holder[0], Warning::AvoidHit { part: FixturePart::Holder, .. }), "{hit_holder:?}");

    // Move at z = 11.0 passing directly over edge at y = -15, z = 10.0 (only 1.0 mm axial clearance < 1.5):
    let hit_axial = avoid_hits(&seg_path([-15.0, -15.0, 11.0], [-14.0, -15.0, 11.0]), &spec, AVOID_VERIFY_TOL)
        .expect("verify runs");
    assert_eq!(hit_axial.len(), 1, "axial edge violation must be caught: {hit_axial:?}");

    // Move kept out at the generator's own keep-out distance passes clean:
    let ko = spec.keepout_region(5.0, 5.0, AVOID_PAD).expect("keepout");
    assert!(ko.contains(Point2::new(-16.0, 0.0)), "holder + clearance covers x = -16");
    let clear = avoid_hits(&seg_path([-16.6, -2.0, 5.0], [-16.6, 2.0, 5.0]), &spec, AVOID_VERIFY_TOL)
        .expect("verify runs");
    assert!(clear.is_empty(), "clear of holder + clearance: {clear:?}");
}

#[test]
fn detects_zero_clearance_graze_and_link_obstruction() {
    let cutter = stepped_cutter();
    let item = AvoidItem {
        label: "pin".into(),
        tris: vec![
            Triangle::new(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 0.0, 0.0), Point3::new(4.0, 4.0, 0.0)),
            Triangle::new(Point3::new(0.0, 0.0, 0.0), Point3::new(4.0, 4.0, 0.0), Point3::new(0.0, 4.0, 0.0)),
        ],
        edges: vec![([0.0, 25.0, 25.0], [4.0, 25.0, 25.0])],
    };
    let spec = AvoidSpec { items: vec![item], cutter, clearance: 0.0 };

    // Flute r = 3 touching edge/face boundary at x = -3.0 (distance = 0.0):
    let touch = avoid_hits(&seg_path([-3.0, 1.0, 0.0], [-3.0, 3.0, 0.0]), &spec, AVOID_VERIFY_TOL)
        .expect("verify runs");
    assert_eq!(touch.len(), 1, "zero-distance touch must block: {touch:?}");

    // Outside by 0.02 mm passes clean:
    let ok = avoid_hits(&seg_path([-3.02, 1.0, 0.0], [-3.02, 3.0, 0.0]), &spec, AVOID_VERIFY_TOL)
        .expect("verify runs");
    assert!(ok.is_empty(), "{ok:?}");

    // Rapid link crossing over edge at y = 25, z = 25 requires z_req > 25:
    let (h, idx) = spec.link_obstruction(Point2::new(-5.0, 25.0), Point2::new(8.0, 25.0), AVOID_PAD)
        .expect("link crosses pin");
    assert_eq!(idx, 1);
    assert!(h > 25.0, "must clear top z = 25: {h}");
}

#[test]
fn detects_protected_edge_contact_between_vertical_arc_samples() {
    let cutter = Cutter::flat(&ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 3.0,
        shank_len: 10.0,
        holder_radius: 3.0,
        holder_len: 10.0,
    });
    let xz_arc = Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: Point3::new(-10.0, 0.0, 10.0),
        to: Point3::new(0.0, 0.0, 0.0),
        feed: Some(500.0),
        arc: Some(ArcMove::in_plane(Point2::new(0.0, 10.0), false, ArcPlane::Xz)),
        axis: None,
    };
    // Exact review repro: contact occurs at arc fraction 0.984375 (midway between 0.5 mm point samples).
    let xz_spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "xz edge".into(),
            tris: vec![],
            edges: vec![(
                [-0.24541228522912326, 2.999, 0.5030118130379577],
                [-0.24541228522912326, 3.009, 0.5030118130379577],
            )],
        }],
        cutter,
        clearance: 0.0,
    };
    let path = Toolpath {
        moves: vec![xz_arc],
        cycles: vec![],
        notes: vec![],
        comps: vec![],
        threads: vec![],
        probes: vec![],
        breaks: vec![],
        plasma: None,
    };
    let hits = avoid_hits(&path, &xz_spec, AVOID_VERIFY_TOL).expect("verify runs");
    assert_eq!(hits.len(), 1, "XZ vertical arc must catch contact between samples: {hits:?}");
    assert!(matches!(hits[0], Warning::AvoidHit { part: FixturePart::Flute, .. }), "{hits:?}");

    // Symmetric YZ arc contact between 0.5 mm samples must also be caught.
    let yz_arc = Move {
        rotary: None,
        kind: MoveKind::Feed,
        from: Point3::new(0.0, -10.0, 10.0),
        to: Point3::new(0.0, 0.0, 0.0),
        feed: Some(500.0),
        arc: Some(ArcMove::in_plane(Point2::new(0.0, 10.0), true, ArcPlane::Yz)),
        axis: None,
    };
    let yz_spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "yz edge".into(),
            tris: vec![],
            edges: vec![(
                [2.999, -0.24541228522912326, 0.5030118130379577],
                [3.009, -0.24541228522912326, 0.5030118130379577],
            )],
        }],
        cutter,
        clearance: 0.0,
    };
    let yz_hits = avoid_hits(&Toolpath { moves: vec![yz_arc], ..path.clone() }, &yz_spec, AVOID_VERIFY_TOL)
        .expect("verify runs");
    assert_eq!(yz_hits.len(), 1, "YZ vertical arc must catch contact between samples: {yz_hits:?}");

    // An edge clearly outside the cutter radius + AVOID_PAD stays clean.
    let clear_spec = AvoidSpec {
        items: vec![AvoidItem {
            label: "clear edge".into(),
            tris: vec![],
            edges: vec![(
                [-0.24541228522912326, 3.03, 0.5030118130379577],
                [-0.24541228522912326, 3.04, 0.5030118130379577],
            )],
        }],
        cutter,
        clearance: 0.0,
    };
    let clear = avoid_hits(&path, &clear_spec, AVOID_VERIFY_TOL).expect("verify runs");
    assert!(clear.is_empty(), "vertical arc clear of protected edge must pass: {clear:?}");
}

fn slim_cutter() -> Cutter {
    Cutter::flat(&ToolGeom {
        radius: 1.0,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 1.0,
        shank_len: 10.0,
        holder_radius: 1.0,
        holder_len: 10.0,
    })
}

fn edge_spec(a: [f64; 3], b: [f64; 3]) -> AvoidSpec {
    AvoidSpec {
        items: vec![AvoidItem { label: "protected edge".into(), tris: vec![], edges: vec![(a, b)] }],
        cutter: slim_cutter(),
        clearance: 0.0,
    }
}

#[test]
fn omitted_axis_inherits_tilted_pose_for_protection() {
    let spec = edge_spec([20.0, 0.0, 0.0], [20.0, 0.001, 0.0]);
    let mut implicit = seg_path([-100.0, -100.0, 0.0], [-100.0, 0.0, 0.0]);
    implicit.moves[0].axis = Some([1.0, 0.0, 0.0]);
    implicit.moves.push(Move {
        from: Point3::new(-100.0, 0.0, 0.0),
        to: Point3::new(0.0, 0.0, 0.0),
        axis: None,
        ..implicit.moves[0]
    });
    let mut explicit = implicit.clone();
    explicit.moves[1].axis = Some([1.0, 0.0, 0.0]);
    let stated = avoid_hits(&explicit, &spec, 0.0001).expect("explicit pose verifies");
    let inherited = avoid_hits(&implicit, &spec, 0.0001).expect("inherited pose verifies");
    assert!(matches!(stated.as_slice(), [Warning::AvoidHit { at: 1, part: FixturePart::Shank, .. }]), "{stated:?}");
    assert_eq!(inherited, stated, "omitted pose must retain the tilted full assembly");

    let clear = edge_spec([20.0, 2.0, 0.0], [20.0, 2.001, 0.0]);
    assert!(avoid_hits(&implicit, &clear, 0.0001).expect("clear pose verifies").is_empty());
}

#[test]
fn detects_protected_edge_contact_between_rotating_poses() {
    let spec = edge_spec([20.33586674390217, 0.999, 20.674925000450518], [20.33586674390217, 1.0, 20.674925000450518]);
    let mut rotating = seg_path([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    rotating.moves[0].to = rotating.moves[0].from;
    rotating.moves[0].axis = Some([1.0, 0.0, 0.0]);
    rotating.moves.push(Move { axis: Some([0.0, 0.0, 1.0]), ..rotating.moves[0] });

    // Exact review repro: the holder touches this edge between midpoint poses.
    let theta = std::f64::consts::FRAC_PI_2 * (1.0 - 48.0 / 95.0);
    let mut contact = seg_path([0.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    contact.moves[0].to = contact.moves[0].from;
    contact.moves[0].axis = Some([theta.sin(), 0.0, theta.cos()]);
    let stationary = avoid_hits(&contact, &spec, 0.0001).expect("contact pose verifies");
    assert!(matches!(stationary.as_slice(), [Warning::AvoidHit { part: FixturePart::Holder, .. }]), "{stationary:?}");
    let swept = avoid_hits(&rotating, &spec, 0.0001).expect("rotating pose verifies");
    assert!(
        matches!(swept.as_slice(), [Warning::AvoidHit { at: 1, part: FixturePart::Holder, .. }]),
        "rotation must enclose contact between midpoint poses: {swept:?}"
    );

    let clear = edge_spec([20.33586674390217, 2.0, 20.674925000450518], [20.33586674390217, 2.001, 20.674925000450518]);
    assert!(avoid_hits(&rotating, &clear, 0.0001).expect("clear rotation verifies").is_empty());
}
