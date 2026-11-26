//! The STAMP's exact angle identities (moved out of `stamp.rs` at the
//! AGENTS 400-line seam; the law itself is documented there). The
//! generator tests (tests/multiaxis.rs) prove the choreography; these
//! prove the identities a dome mesh's band angle can only bound. The
//! world is a plane pitched 30 degrees about Y, tiled twice, and a
//! straight ladder of ball centers lying on it at the tangent distance
//! `|c - q| = r`: there, contact is face-interior, n is the same exact
//! vector at every step, and the feed chord lies IN the plane
//! (n . t = 0), so the lead and lag rotations hold their angles
//! exactly.

use crate::cl::{Move, MoveKind, Toolpath};
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;

use super::stamp::stamp_axes;
use super::tilt::{angle_between, AxisControl};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn len(v: Point3) -> f64 {
    (v.x * v.x + v.y * v.y + v.z * v.z).sqrt()
}

fn unit(v: Point3) -> Point3 {
    let l = len(v);
    p(v.x / l, v.y / l, v.z / l)
}

const PITCH: f64 = 30f64.to_radians();

fn pitched_plane() -> Vec<Triangle> {
    let v = |x: f64, y: f64| p(x, y, 5.0 + PITCH.tan() * x);
    let (a, b, c, d) = (v(-60.0, -60.0), v(60.0, -60.0), v(60.0, 60.0), v(-60.0, 60.0));
    vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

fn plane_normal() -> Point3 {
    unit(p(-PITCH.tan(), 0.0, 1.0))
}

/// The tangent ladder of the line y = 0 on the offset surface, five
/// steps ten millimeters apart, stated as the stamp reads its input:
/// upright-ball TIPS, the tangent centres `r` down Z.
fn ladder_centers(r: f64) -> Vec<Point3> {
    let n = plane_normal();
    (0..5)
        .map(|i| {
            let q = p(10.0 * i as f64 - 20.0, 0.0, 5.0 + PITCH.tan() * (10.0 * i as f64 - 20.0));
            p(q.x + n.x * r, q.y + n.y * r, q.z + n.z * r - r)
        })
        .collect()
}

fn ladder(centers: &[Point3], kinds: impl Fn(usize) -> MoveKind) -> Toolpath {
    Toolpath::new(
        centers
            .windows(2)
            .enumerate()
            .map(|(i, w)| Move::new(kinds(i), w[0], w[1], Some(800.0)))
            .collect(),
    )
}

fn axis_of(a: [f64; 3]) -> Point3 {
    p(a[0], a[1], a[2])
}

#[test]
fn the_stamp_poses_every_cut_move_at_the_exact_lead() {
    let r = 3.0;
    let centers = ladder_centers(r);
    let n = plane_normal();
    let t_hat = unit(centers[1] - centers[0]);
    let out = stamp_axes(
        &ladder(&centers, |_| MoveKind::Feed),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 15.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        },
        None,
    )
    .unwrap();
    // #3414: the first lean swings at a standstill first - a
    // zero-travel rapid ahead of the four cut moves.
    assert_eq!(out.moves.len(), 5);
    // The pivot law: the posed tip backed up r along its pose is the
    // planned ball centre (the input tip r up Z), to float dust.
    for (mv, want) in out.moves.iter().filter(|mv| mv.from != mv.to).zip(&centers[1..]) {
        let a = axis_of(mv.axis.expect("posed"));
        let c = p(mv.to.x + r * a.x, mv.to.y + r * a.y, mv.to.z + r * a.z);
        assert!(len(c - p(want.x, want.y, want.z + r)) < 1e-9, "ball moved: {c:?} vs {want:?}");
    }
    for mv in &out.moves {
        let a = axis_of(mv.axis.expect("a tangent ladder must pose every cut move"));
        assert!(
            (angle_between(n, a) - 15f64.to_radians()).abs() < 1e-9,
            "the lead identity broke on the pitched plane: {a:?}"
        );
        assert!(
            (a.dot(t_hat) - 15f64.to_radians().sin()).abs() < 1e-9,
            "the lean must go WITH the feed: {a:?}"
        );
    }
}

#[test]
fn the_stamp_folds_an_overlean_onto_the_cone_exactly() {
    let r = 3.0;
    let n = plane_normal();
    let out = stamp_axes(
        &ladder(&ladder_centers(r), |_| MoveKind::Feed),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 75.0,
            lag_deg: 0.0,
            max_tilt_deg: Some(40.0),
        },
        None,
    )
    .unwrap();
    for mv in &out.moves {
        let a = axis_of(mv.axis.expect("a posed cut move"));
        assert!(
            (angle_between(n, a) - 40f64.to_radians()).abs() < 1e-9,
            "the clamp must land the pose ON the cone: {a:?}"
        );
    }
}

#[test]
fn the_stamp_leans_side_with_the_lag_exactly() {
    let r = 3.0;
    let n = plane_normal();
    let out = stamp_axes(
        &ladder(&ladder_centers(r), |_| MoveKind::Feed),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 0.0,
            lag_deg: 25.0,
            max_tilt_deg: None,
        },
        None,
    )
    .unwrap();
    for mv in &out.moves {
        let a = axis_of(mv.axis.expect("a posed cut move"));
        assert!(
            (angle_between(n, a) - 25f64.to_radians()).abs() < 1e-9,
            "the lag identity broke on the pitched plane: {a:?}"
        );
    }
}

#[test]
fn the_stamp_poses_only_moves_that_touch() {
    // Rapid over the plane, a feed that never reaches it, then
    // the tangent ladder: only the touching feeds carry a pose,
    // and a rapid never does even when it lands on contact.
    let r = 3.0;
    let centers = ladder_centers(r);
    let air = p(30.0, 0.0, 45.0);
    let ms = vec![
        Move::new(MoveKind::Rapid, p(-40.0, 0.0, 45.0), centers[0], Some(3000.0)),
        Move::new(MoveKind::Feed, centers[0], air, Some(800.0)),
        Move::new(MoveKind::Feed, air, centers[1], Some(800.0)),
        Move::new(MoveKind::Feed, centers[1], centers[2], Some(800.0)),
    ];
    let out = stamp_axes(
        &Toolpath::new(ms),
        &pitched_plane(),
        r,
        &AxisControl {
            lead_deg: 15.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        },
        None,
    )
    .unwrap();
    assert_eq!(out.moves.len(), 4);
    // #3300: the path leans, so its opening states the upright Z column
    // (the post commands it as zero angles) - still no lean.
    let upright = |m: &Move| m.axis.is_some_and(|a| crate::cl::is_vertical(&a));
    assert!(upright(&out.moves[0]), "the rapid must hold no angle: {:?}", out.moves[0].axis);
    assert!(upright(&out.moves[1]), "a move ending in air is vertical: {:?}", out.moves[1].axis);
    assert!(out.moves[2].axis.is_some(), "the plunge onto the surface poses");
    assert!(out.moves[3].axis.is_some(), "the cut along the surface poses");
}

#[test]
fn drill_cycles_refuse_the_stamp_by_name() {
    let r = 3.0;
    let mut path = ladder(&ladder_centers(r), |_| MoveKind::Feed);
    path.cycles.push(crate::cl::Cycle {
        kind: crate::cl::CycleKind::Drill,
        xy: crate::geo2d::Point2::new(0.0, 0.0),
        r_z: 10.0,
        bottom_z: -5.0,
        entry_z: 5.0,
        peck: 0.0,
        dwell: 0.0,
        first_move: 0,
    });
    let e = stamp_axes(&path, &pitched_plane(), r, &AxisControl::default(), None).err();
    assert!(matches!(e, Some(GenError::Invalid(_))), "got {e:?}");
}

#[test]
fn the_stamp_leans_a_dive_with_the_cut_it_lands_into() {
    // M8.3's entry restatement: a dive dead down onto a flank carries
    // travel, but none ALONG a cut that has not started - the lead
    // must lean with the feed the dive lands into (the answer the
    // drive-curve family already gives), not inside the plunge plane.
    // The old law leaned the dive in (normal, vertical): a z tilt of
    // sin(lead); this world says the tilt belongs in y.
    let r = 3.0;
    let wall = |y: f64, z: f64| p(0.0, y, z);
    let facets = vec![
        Triangle::new(wall(-60.0, -60.0), wall(60.0, -60.0), wall(60.0, 60.0)),
        Triangle::new(wall(-60.0, -60.0), wall(60.0, 60.0), wall(-60.0, 60.0)),
    ];
    let ms = vec![
        Move::new(MoveKind::Rapid, p(r, 0.0, 10.0), p(r, 0.0, 5.0), None),
        Move::new(MoveKind::Plunge, p(r, 0.0, 5.0), p(r, 0.0, 0.0), Some(200.0)),
        Move::new(MoveKind::Feed, p(r, 0.0, 0.0), p(r, 10.0, 0.0), Some(800.0)),
    ];
    let out = stamp_axes(
        &Toolpath::new(ms),
        &facets,
        r,
        &AxisControl {
            lead_deg: 15.0,
            lag_deg: 0.0,
            max_tilt_deg: None,
        },
        None,
    )
    .unwrap();
    let n = p(1.0, 0.0, 0.0);
    for mv in [&out.moves[1], &out.moves[2]] {
        let a = axis_of(mv.axis.expect("a dive onto a flank poses"));
        assert!(
            (angle_between(n, a) - 15f64.to_radians()).abs() < 1e-9,
            "the lead identity must hold on the dive: {a:?}"
        );
        assert!(a.z.abs() < 1e-9, "the dive must not lean in the plunge plane: {a:?}");
        assert!(
            (a.y.abs() - 15f64.to_radians().sin()).abs() < 1e-9,
            "the lean rides the cut the dive lands into (+y): {a:?}"
        );
    }
}

/// The turn guard and the link law (the "swarf stamp flips
/// hemispheres" finding): a posed path whose consecutive cuts turn
/// past the link threshold is refused by name; linked, the same path
/// passes - the swing rides the plunge from the clearance plane, the
/// re-entry lands on the ball centre the previous cut finished on,
/// and the plunge law reads that re-entry as no plunge into solid.
#[test]
fn the_turn_guard_refuses_a_swing_in_contact_and_the_link_law_takes_it_to_air() {
    use super::stamp_link::{guard_turns, link_swings, LINK_TURN_DEG};
    use crate::plunge_law::{first_solid_plunge, Solid};
    let r = 3.0;
    let cut = |from: Point3, to: Point3, a: [f64; 3]| {
        Move::new(MoveKind::Feed, from, to, Some(500.0)).with_axis(a)
    };
    let (o, x1, x2) = (p(0.0, 0.0, 0.0), p(4.0, 0.0, 0.0), p(8.0, 0.0, 0.0));
    let (px, mx) = ([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]);
    // The first cut enters from the clearance plane (an air entry);
    // the second reverses the axis on the cut.
    let flip = Toolpath::new(vec![cut(p(0.0, 0.0, 25.0), x1, px), cut(x1, x2, mx)]);
    match guard_turns(&flip, 25.0) {
        Err(GenError::Invalid(msg)) => assert!(
            msg.contains("swing 180 deg at move 1"),
            "the refusal names the swing: {msg}"
        ),
        other => panic!("a 180 degree swing on a cut must refuse, got {other:?}"),
    }
    // The blade the tool lies on: the wall y = -3, which the lying
    // ball (centre on y = 0) touches along its whole flank. Its
    // contact normal is +Y - the one direction the flank leaves it.
    let wall = vec![Triangle::new(p(-50.0, -3.0, -50.0), p(50.0, -3.0, -50.0), p(0.0, -3.0, 50.0))];
    let (linked, links) = link_swings(&flip, &wall, r, 25.0, 100.0).unwrap();
    assert_eq!(links, 1);
    guard_turns(&linked, 25.0).expect("linked, the swing is in air");
    assert_eq!(linked.moves.len(), 5, "retract, swing, plunge, and the two cuts");
    let (lift, swing, plunge, re) =
        (&linked.moves[1], &linked.moves[2], &linked.moves[3], &linked.moves[4]);
    assert_eq!(lift.kind, MoveKind::Rapid);
    assert_eq!(lift.axis, Some(px), "the retract holds the outgoing pose");
    // The retract leaves along the contact normal (+Y), by the plunge
    // height (clearance + r above the exit centre), never up Z.
    let c0 = p(x1.x + r, x1.y, x1.z);
    let c_out = p(lift.to.x + r, lift.to.y, lift.to.z);
    assert!(len(c_out - p(c0.x, c0.y + 25.0 + r, c0.z)) < 1e-9, "retracted to {c_out:?}");
    assert_eq!(swing.kind, MoveKind::Rapid);
    assert_eq!(swing.axis, Some(mx), "the swing turns to the incoming pose in air");
    let c_swung = p(swing.to.x - r, swing.to.y, swing.to.z);
    assert!(len(c_swung - c_out) < 1e-9, "the swing pivots about the retracted centre");
    assert_eq!(plunge.kind, MoveKind::Plunge);
    assert_eq!(plunge.axis, Some(mx), "the plunge holds the incoming pose");
    // The same ball centre under both poses: x1 + r*px == re.from + r*mx.
    let c1 = p(re.from.x - r, re.from.y, re.from.z);
    assert!(len(c0 - c1) < 1e-9, "the re-entry lands on the centre the cut finished on");
    assert_eq!(plunge.to, re.from, "chained");
    let solid = Solid { top: 10.0, footprint: None };
    assert!(
        first_solid_plunge(&linked, r, &solid).is_none(),
        "the re-entry is not a plunge into solid"
    );
    let t = (LINK_TURN_DEG - 1.0).to_radians();
    let inside = Toolpath::new(vec![
        cut(o, x1, [0.0, 0.0, 1.0]),
        cut(x1, x2, [t.sin(), 0.0, t.cos()]),
    ]);
    assert!(guard_turns(&inside, 25.0).is_ok(), "a turn inside the threshold passes");
    assert_eq!(link_swings(&inside, &wall, r, 25.0, 100.0).unwrap().1, 0, "and is not linked");
}

// The swarf fallback (stamp_pose): a station under an OVERHANG, where
// the contact normal points down and the lean law's pose with it. The
// world is a ceiling - one big facet at z = 6 over a ball of radius 3
// whose centre rides z = 3, so the centre is tangent from below - and
// the ruling runs along X. The facet is not claimed ruled, so the lean
// law speaks first and says "point the spindle at the floor"; the
// fallback carries the ruling instead, level, with its flank proved in
// air. With a wall standing across BOTH ends of the ruling inside the
// flank's proof, no pose clears, and the stamp refuses by name.

fn ceiling() -> Vec<Triangle> {
    vec![Triangle::new(p(-50.0, -50.0, 6.0), p(50.0, -50.0, 6.0), p(0.0, 50.0, 6.0))]
}

fn under_the_ceiling() -> Toolpath {
    Toolpath::new(vec![Move::new(MoveKind::Feed, p(-1.0, 0.0, 0.0), p(1.0, 0.0, 0.0), Some(500.0))])
}

#[test]
fn the_swarf_fallback_carries_the_ruling_where_the_lean_law_points_under_the_horizon() {
    use super::stamp::stamp_axes_ruled;
    let c = AxisControl { lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: Some(60.0) };
    let posed = stamp_axes_ruled(&under_the_ceiling(), &ceiling(), 3.0, &c, p(1.0, 0.0, 0.0), &[false], 0.05)
        .expect("a level ruling clears the ceiling");
    let a = posed.moves[0].axis.expect("the station under the ceiling states a pose");
    assert!(a[2].abs() < 1e-12 && a[0].abs() > 0.999, "the pose is the level ruling, got {a:?}");
    // The lean law alone (no ruling to carry) still answers with the
    // overhang's normal, under the horizon: the fallback is the swarf
    // kinds' law, not a change to stage C.
    let lean = stamp_axes(&under_the_ceiling(), &ceiling(), 3.0, &c, None).unwrap();
    let b = lean.moves[0].axis.expect("the lean law poses the contact");
    assert!(b[2] < -0.9, "the lean law's own answer points at the floor: {b:?}");
}

#[test]
fn a_row_no_pose_clears_refuses_the_swarf_path_by_name() {
    use super::stamp::stamp_axes_ruled;
    let c = AxisControl { lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: Some(60.0) };
    let mut world = ceiling();
    // Walls across both ends of the ruling, 4.5 off the centre: not a
    // contact (3.5 past the ball), but squarely in the flank's path.
    for x in [4.5, -4.5] {
        world.push(Triangle::new(p(x, -20.0, -10.0), p(x, 20.0, -10.0), p(x, 0.0, 20.0)));
    }
    let err = stamp_axes_ruled(&under_the_ceiling(), &world, 3.0, &c, p(1.0, 0.0, 0.0), &[false, false, false], 0.05)
        .expect_err("walls across both ends of the ruling leave no pose");
    let msg = match err {
        GenError::Invalid(m) => m,
        other => panic!("the refusal is a named Invalid, got {other:?}"),
    };
    assert!(msg.contains("swarf pose at move 0") && msg.contains("under the horizon"), "{msg}");
}

// #3414's upright arms (the arc arm): an upright FEED ARC reached out
// of air under a lean swings back to upright at a standstill first -
// the rotaries never turn on the arc that reaches the part. The first
// feed leans on the pitched plane, the rapid away travels under that
// lean, and the air arc that follows states the upright column only
// after a zero-travel rapid commanded it.
#[test]
fn an_upright_arc_out_of_air_swings_at_a_standstill_first() {
    let r = 3.0;
    let centers = ladder_centers(r);
    let ms = vec![
        Move::new(MoveKind::Rapid, p(-40.0, 0.0, 45.0), centers[0], Some(3000.0)),
        Move::new(MoveKind::Feed, centers[0], centers[1], Some(800.0)),
        Move::new(MoveKind::Rapid, centers[1], p(40.0, 30.0, 45.0), None),
        Move::new(MoveKind::Feed, p(40.0, 30.0, 45.0), p(40.0, -30.0, 45.0), Some(800.0))
            .with_arc(crate::geo2d::Point2::new(55.0, 0.0), true),
    ];
    let out = stamp_axes(
        &Toolpath::new(ms),
        &pitched_plane(),
        r,
        &AxisControl { lead_deg: 15.0, lag_deg: 0.0, max_tilt_deg: None },
        None,
    )
    .unwrap();
    let arc = out.moves.iter().position(|m| m.arc.is_some()).expect("the arc feed posts");
    assert_eq!(out.moves[arc].kind, MoveKind::Feed);
    let (link, swing) = (&out.moves[arc - 2], &out.moves[arc - 1]);
    assert!(link.axis.is_some_and(|a| !crate::cl::is_vertical(&a)), "the link rapid holds the lean");
    assert_eq!(swing.kind, MoveKind::Rapid, "the arc approach carries its rotary turn");
    assert_eq!(swing.from, swing.to, "the swing travels");
    assert!(
        swing.axis.is_some_and(|a| crate::cl::is_vertical(&a)),
        "the swing states the upright pose: {:?}",
        swing.axis
    );
    assert!(
        crate::cl::is_vertical(&out.moves[arc].axis.expect("the arc states a pose")),
        "the arc carries a turn: {:?}",
        out.moves[arc].axis
    );
}
