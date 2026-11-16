// Provenance: clean-room regression tests for AC trunnion joint interpolation and fail-closed path limits.
//! Gate map: covers camcraft_post::kinematics.

use camcraft_post::cl::{ArcPlane, Move, MoveKind, Toolpath};
use camcraft_post::geo2d::Point2;
use camcraft_post::geometry::Point3;
use camcraft_post::kinematics::{
    AcTrunnionConfig, KinematicsError, MachinePose, PathMove, PathOptions, forward_part,
    solve_path, solve_toolpath,
};

fn axis(tilt: f64, azimuth: f64) -> [f64; 3] {
    let (st, ct) = tilt.to_radians().sin_cos();
    let (sa, ca) = azimuth.to_radians().sin_cos();
    [st * ca, st * sa, ct]
}

fn feed(from_axis: [f64; 3], to_axis: [f64; 3]) -> PathMove {
    PathMove {
        kind: MoveKind::Feed,
        from_tip: [100.0, 0.0, 0.0],
        to_tip: [100.0, 0.0, 0.0],
        from_axis,
        to_axis,
        feed: Some(400.0),
        safe_retract: false,
    }
}

fn refuse(result: Result<Vec<camcraft_post::kinematics::SolvedMove>, KinematicsError>) -> String {
    match result {
        Err(err) => format!("{err:?}"),
        Ok(moves) => panic!("unsafe path accepted as {} segments", moves.len()),
    }
}

#[test]
fn clearance_retract_keeps_c_continuous_and_checks_the_entire_lift() {
    let mut cfg = AcTrunnionConfig::default();
    cfg.a.travel = Some((-110.0, 30.0));
    cfg.c.travel = Some((-450.0, 450.0));
    let mut path: Vec<_> = (0..11)
        .map(|i| {
            feed(
                axis(40.0, -90.0 + f64::from(i) * 40.0),
                axis(40.0, -50.0 + f64::from(i) * 40.0),
            )
        })
        .collect();
    let mut retract = feed(axis(40.0, 350.0), axis(40.0, 350.0));
    retract.kind = MoveKind::Rapid;
    retract.feed = None;
    retract.to_tip[2] = 50.0;
    retract.safe_retract = true;
    path.push(retract);
    let opts = PathOptions::default();
    let solved = solve_path(&cfg, &path, &opts).expect("clearance lift remains reachable");
    let lift: Vec<_> = solved.iter().filter(|mv| mv.source_index == 11).collect();
    assert!(!lift.is_empty());
    assert!((lift[0].from.c_deg + 440.0).abs() < 1e-6);
    for mv in lift {
        for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let pose = MachinePose {
                xyz: std::array::from_fn(|i| mv.from.xyz[i] + t * (mv.to.xyz[i] - mv.from.xyz[i])),
                a_deg: mv.from.a_deg + t * (mv.to.a_deg - mv.from.a_deg),
                c_deg: mv.from.c_deg + t * (mv.to.c_deg - mv.from.c_deg),
            };
            let actual = forward_part(&cfg, pose).expect("interpolated lift");
            let desired: [f64; 3] =
                std::array::from_fn(|i| mv.tip_from[i] + t * (mv.tip_to[i] - mv.tip_from[i]));
            let err = actual
                .tip
                .iter()
                .zip(desired)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt();
            assert!(
                err <= opts.chord_tol_mm,
                "clearance lift sweeps through part by {err} mm"
            );
            assert!(
                (pose.c_deg + 440.0).abs() < 1e-6,
                "C unwound before clearance: {pose:?}"
            );
        }
    }
    let mut continuation = feed(axis(40.0, 350.0), axis(40.0, 390.0));
    continuation.from_tip[2] = 50.0;
    path.push(continuation);
    let err = refuse(solve_path(&cfg, &path, &opts));
    assert!(
        err.contains("TravelLimitExceeded") && err.contains("C"),
        "{err}"
    );
}

#[test]
fn adjacent_cl_axis_change_is_refused_instead_of_teleporting() {
    let mut cfg = AcTrunnionConfig::default();
    cfg.a.travel = Some((-110.0, 30.0));
    let mut toolpath = Toolpath::default();
    for (from_x, to_x, azimuth) in [(10.0, 20.0, -90.0), (20.0, 30.0, -50.0)] {
        toolpath.moves.push(Move {
            kind: MoveKind::Feed,
            from: Point3 {
                x: from_x,
                y: 0.0,
                z: 0.0,
            },
            to: Point3 {
                x: to_x,
                y: 0.0,
                z: 0.0,
            },
            feed: Some(400.0),
            arc: None,
            axis: Some(axis(40.0, azimuth)),
            rotary: None,
        });
    }
    let err = refuse(solve_toolpath(&cfg, &toolpath, &PathOptions::default()));
    assert!(
        err.contains("DiscontinuousPath") && err.contains("source_index: 1"),
        "{err}"
    );
}

#[test]
fn continuous_subdivision_depth_cap_refuses_unmet_angular_tolerance() {
    let cfg = AcTrunnionConfig::default();
    let path = [feed(axis(40.0, -90.0), axis(60.0, -90.0))];
    let opts = PathOptions {
        max_angular_step_deg: Some(0.000001),
        ..PathOptions::default()
    };
    let err = refuse(solve_path(&cfg, &path, &opts));
    assert!(
        err.contains("SubdivisionLimitExceeded") && err.contains("continuous segment"),
        "{err}"
    );
}

#[test]
fn pole_reorientation_step_cap_refuses_unmet_angular_tolerance() {
    let cfg = AcTrunnionConfig {
        a: camcraft_post::kinematics::RotaryAxisConfig {
            travel: Some((0.0, 90.0)),
            ..AcTrunnionConfig::default().a
        },
        ..AcTrunnionConfig::default()
    };
    let path = [feed([0.0, 0.0, 1.0], axis(15.0, -90.0))];
    let opts = PathOptions {
        max_angular_step_deg: Some(0.000001),
        ..PathOptions::default()
    };
    let err = refuse(solve_path(&cfg, &path, &opts));
    assert!(
        err.contains("SubdivisionLimitExceeded") && err.contains("pole C reorientation"),
        "{err}"
    );
}

#[test]
fn pole_reorientation_step_cap_refuses_unmet_chord_tolerance() {
    let cfg = AcTrunnionConfig {
        a: camcraft_post::kinematics::RotaryAxisConfig {
            travel: Some((0.0, 90.0)),
            ..AcTrunnionConfig::default().a
        },
        ..AcTrunnionConfig::default()
    };
    let path = [feed([0.0, 0.0, 1.0], axis(15.0, -90.0))];
    let opts = PathOptions {
        chord_tol_mm: 1e-10,
        max_angular_step_deg: None,
        ..PathOptions::default()
    };
    let err = refuse(solve_path(&cfg, &path, &opts));
    assert!(
        err.contains("SubdivisionLimitExceeded") && err.contains("pole C reorientation"),
        "{err}"
    );
}

#[test]
fn continuous_subdivision_depth_cap_refuses_unmet_chord_tolerance() {
    let cfg = AcTrunnionConfig::default();
    let mut mv = feed(axis(40.0, -90.0), axis(60.0, -90.0));
    mv.from_tip[1] = 100.0;
    mv.to_tip[1] = 100.0;
    let path = [mv];
    let opts = PathOptions {
        chord_tol_mm: 1e-12,
        max_angular_step_deg: None,
        ..PathOptions::default()
    };
    let err = refuse(solve_path(&cfg, &path, &opts));
    assert!(
        err.contains("SubdivisionLimitExceeded") && err.contains("continuous segment"),
        "{err}"
    );
}

fn quarter_circle(plane: ArcPlane) -> Move {
    let (from, to, ccw) = match plane {
        ArcPlane::Xy => (
            Point3::new(100.0, 0.0, 0.0),
            Point3::new(0.0, 100.0, 0.0),
            true,
        ),
        ArcPlane::Xz => (
            Point3::new(100.0, 0.0, 0.0),
            Point3::new(0.0, 0.0, 100.0),
            false,
        ),
        ArcPlane::Yz => (
            Point3::new(0.0, 100.0, 0.0),
            Point3::new(0.0, 0.0, 100.0),
            true,
        ),
    };
    Move::new(MoveKind::Feed, from, to, Some(400.0)).with_arc_in(Point2::new(0.0, 0.0), ccw, plane)
}

fn assert_arc_refused(plane: ArcPlane) {
    let arc = quarter_circle(plane);
    for source_index in [0, 1] {
        let mut toolpath = Toolpath::default();
        if source_index == 1 {
            let from = Point3::new(arc.from.x, arc.from.y, arc.from.z + 10.0);
            toolpath
                .moves
                .push(Move::new(MoveKind::Feed, from, arc.from, Some(400.0)));
        }
        toolpath.moves.push(arc);
        assert_eq!(
            solve_toolpath(
                &AcTrunnionConfig::default(),
                &toolpath,
                &PathOptions::default()
            ),
            Err(KinematicsError::UnsupportedArc { source_index }),
            "{plane:?} quarter-circle must not become a straight cut"
        );
    }
}

#[test]
fn cl_xy_quarter_circle_is_refused_instead_of_cutting_across_part() {
    assert_arc_refused(ArcPlane::Xy);
}

#[test]
fn cl_xz_quarter_circle_is_refused_instead_of_cutting_across_part() {
    assert_arc_refused(ArcPlane::Xz);
}

#[test]
fn cl_yz_quarter_circle_is_refused_instead_of_cutting_across_part() {
    assert_arc_refused(ArcPlane::Yz);
}

#[test]
fn cl_straight_feed_remains_supported() {
    let mut straight = quarter_circle(ArcPlane::Xy);
    straight.arc = None;
    let mut toolpath = Toolpath::default();
    toolpath.moves.push(straight);
    let solved = solve_toolpath(
        &AcTrunnionConfig::default(),
        &toolpath,
        &PathOptions::default(),
    )
    .expect("straight feed remains supported");
    assert_eq!(solved.len(), 1);
    assert_eq!(solved[0].source_index, 0);
    assert_eq!(solved[0].tip_from, [100.0, 0.0, 0.0]);
    assert_eq!(solved[0].tip_to, [0.0, 100.0, 0.0]);
    assert_eq!(solved[0].kind, MoveKind::Feed);
    assert_eq!(solved[0].feed, Some(400.0));
}
