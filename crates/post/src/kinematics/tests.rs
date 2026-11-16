// Provenance: clean-room unit tests for camcraft_post::kinematics internals and edge cases.

use super::*;
use camcraft_cl::cl::{Move, MoveKind, Toolpath};
use camcraft_geometry::geometry::Point3;

fn base_cfg() -> AcTrunnionConfig {
    AcTrunnionConfig {
        a: RotaryAxisConfig {
            name: "A".into(),
            axis: [1.0, 0.0, 0.0],
            travel: Some((-110.0, 30.0)),
            zero_offset_deg: 0.0,
            rollover: false,
        },
        c: RotaryAxisConfig {
            name: "C".into(),
            axis: [0.0, 0.0, 1.0],
            travel: Some((-720.0, 720.0)),
            zero_offset_deg: 0.0,
            rollover: false,
        },
        spindle_axis: [0.0, 0.0, 1.0],
        a_pivot: [10.0, 20.0, -30.0],
        c_offset: [0.0, 5.0, 15.0],
        wcs_offset: [-3.0, 7.0, 25.0],
        work_datum: [100.0, -50.0, 10.0],
        sign: RotarySignConvention::TableRightHand,
        pole_sin_tol: 1e-6,
        weight_a: 1.0,
        weight_c: 1.0,
        initial_c_deg: 0.0,
    }
}

#[test]
fn rollover_wraps_into_0_to_360_at_initial_and_safe_retract() {
    let mut cfg = base_cfg();
    cfg.c.rollover = true;
    cfg.c.travel = Some((-450.0, 450.0));

    let mut solver = AcSolver::new(&cfg).expect("valid rollover config");
    // Tilt toward +40 deg azimuth with X trunnion: (+20, C=+50) or (-20, C=-130 -> 230 rollover).
    // With rollover primary center 180 deg, (-20, 230) is closer to 180 or (+20, 50) -> verify both lie in [0, 360).
    let p = PartPose {
        tip: [0.0, 0.0, 0.0],
        axis: [
            30.0_f64.to_radians().sin() * 140.0_f64.to_radians().cos(),
            30.0_f64.to_radians().sin() * 140.0_f64.to_radians().sin(),
            30.0_f64.to_radians().cos(),
        ],
    };
    let m0 = solver
        .solve_pose(p, SectionBoundary::Continuous)
        .expect("rollover first pose");
    assert!(
        (0.0..360.0).contains(&m0.c_deg),
        "rollover first move must land in [0, 360), got {}",
        m0.c_deg
    );

    // Wind past 360 deg via continuous moves, then re-wrap at SafeRetract back into [0, 360).
    for step in 1..=3 {
        let az = (140.0 + f64::from(step) * 80.0).to_radians();
        let axis = [
            30.0_f64.to_radians().sin() * az.cos(),
            30.0_f64.to_radians().sin() * az.sin(),
            30.0_f64.to_radians().cos(),
        ];
        solver
            .solve_pose(PartPose { tip: [0.0, 0.0, 0.0], axis }, SectionBoundary::Continuous)
            .expect("wind");
    }
    let wrapped = solver
        .rewrap_c_at_retract()
        .expect("rewrap succeeds")
        .expect("modal exists");
    assert!(
        (0.0..360.0).contains(&wrapped),
        "rewrap under rollover=true must target [0, 360), got {wrapped}"
    );
}

#[test]
fn positive_only_a_limits_reorient_c_at_pole_within_chord_tolerance() {
    let mut cfg = base_cfg();
    cfg.a.travel = Some((0.0, 90.0)); // blocks negative A completely

    let t = 15.0_f64.to_radians();
    let mv = PathMove {
        kind: MoveKind::Feed,
        from_tip: [25.0, -10.0, 0.0],
        to_tip: [25.0, -10.0, 0.0],
        from_axis: [0.0, t.sin(), t.cos()],
        to_axis: [0.0, -t.sin(), t.cos()],
        feed: Some(300.0),
        safe_retract: false,
    };
    let opts = PathOptions {
        chord_tol_mm: 0.01,
        max_angular_step_deg: Some(10.0),
        subdivide_rapids: false,
    };
    let segs = solve_path(&cfg, &[mv], &opts).expect("pole re-index succeeds");
    assert!(segs.len() > 18, "expected subdivided C re-index across pole, got {}", segs.len());

    for seg in &segs {
        assert!(seg.from.a_deg >= -1e-9 && seg.to.a_deg >= -1e-9);
        let da = (seg.to.a_deg - seg.from.a_deg).abs();
        let dc = (seg.to.c_deg - seg.from.c_deg).abs();
        assert!(da <= 10.0 + 1e-6 && dc <= 10.0 + 1e-6, "step too large: dA={da} dC={dc}");
        let mid_mach = MachinePose {
            xyz: [
                0.5 * (seg.from.xyz[0] + seg.to.xyz[0]),
                0.5 * (seg.from.xyz[1] + seg.to.xyz[1]),
                0.5 * (seg.from.xyz[2] + seg.to.xyz[2]),
            ],
            a_deg: 0.5 * (seg.from.a_deg + seg.to.a_deg),
            c_deg: 0.5 * (seg.from.c_deg + seg.to.c_deg),
        };
        let rt = forward_part(&cfg, mid_mach).expect("forward_part");
        let mid_tip = [
            0.5 * (seg.tip_from[0] + seg.tip_to[0]),
            0.5 * (seg.tip_from[1] + seg.tip_to[1]),
            0.5 * (seg.tip_from[2] + seg.tip_to[2]),
        ];
        let err = ((rt.tip[0] - mid_tip[0]).powi(2)
            + (rt.tip[1] - mid_tip[1]).powi(2)
            + (rt.tip[2] - mid_tip[2]).powi(2))
        .sqrt();
        assert!(err <= 0.01 + 1e-6, "midpoint chord error {err} exceeded 0.01");
    }
}

#[test]
fn solve_toolpath_converts_cl_moves_and_detects_retract() {
    let mut tp = Toolpath::default();
    tp.moves.push(Move {
        kind: MoveKind::Feed,
        from: Point3 { x: 10.0, y: 0.0, z: 0.0 },
        to: Point3 { x: 20.0, y: 0.0, z: 0.0 },
        feed: Some(400.0),
        arc: None,
        axis: Some([0.0, 20.0_f64.to_radians().sin(), 20.0_f64.to_radians().cos()]),
        rotary: None,
    });
    tp.moves.push(Move {
        kind: MoveKind::Rapid,
        from: Point3 { x: 20.0, y: 0.0, z: 0.0 },
        to: Point3 { x: 20.0, y: 0.0, z: 50.0 },
        feed: None,
        arc: None,
        axis: Some([0.0, 20.0_f64.to_radians().sin(), 20.0_f64.to_radians().cos()]),
        rotary: None,
    });

    let cfg = base_cfg();
    let solved = solve_toolpath(&cfg, &tp, &PathOptions::default()).expect("solve_toolpath");
    assert_eq!(solved.len(), 2);
    assert_eq!(solved[0].source_index, 0);
    assert_eq!(solved[1].source_index, 1);
    let rt = forward_part(&cfg, solved[1].to).expect("round trip");
    assert!((rt.tip[2] - 50.0).abs() < 1e-6);
}

#[test]
fn refuses_non_positive_path_options_and_parallel_spindle() {
    let mut cfg = base_cfg();
    cfg.spindle_axis = [1.0, 0.0, 0.0]; // parallel to A=[1,0,0]
    let err = AcSolver::new(&cfg).expect_err("parallel A and spindle must fail");
    assert!(err.to_string().contains("parallel"));

    let valid = base_cfg();
    let bad_opts = PathOptions {
        chord_tol_mm: 0.0,
        ..PathOptions::default()
    };
    let err = solve_path(&valid, &[], &bad_opts).expect_err("zero chord_tol must fail");
    assert!(err.to_string().contains("chord_tol_mm"));
}
