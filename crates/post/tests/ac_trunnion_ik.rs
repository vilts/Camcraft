//! #1965: pure-Rust AC table-table forward/inverse kinematics, pole
//! holding, weighted branch continuity across A = 0, continuous C
//! unwrapping through clearance retracts, and rotary chord
//! subdivision in `camcraft-post`.
//!
//! Gate map (#1965): covers camcraft_post::kinematics.

use camcraft_post::cl::MoveKind;
use camcraft_post::kinematics::{
    forward_machine, forward_part, solve_path, AcSolver, AcTrunnionConfig, PartPose, PathMove,
    PathOptions, RotaryAxisConfig, RotarySignConvention, SectionBoundary,
};

fn standard_ac_trunnion() -> AcTrunnionConfig {
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
        a_pivot: [120.0, -40.0, -150.0],
        c_offset: [0.0, 12.5, 28.0],
        wcs_offset: [-15.0, 8.0, 42.0],
        work_datum: [0.0, 0.0, 0.0],
        sign: RotarySignConvention::TableRightHand,
        pole_sin_tol: 1e-6,
        weight_a: 1.0,
        weight_c: 1.0,
        initial_c_deg: 0.0,
    }
}

fn norm3(v: [f64; 3]) -> f64 { (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt() }
fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn unit3(v: [f64; 3]) -> [f64; 3] {
    let n = norm3(v);
    [v[0] / n, v[1] / n, v[2] / n]
}
fn axis_from_deg(tilt_deg: f64, azim_deg: f64) -> [f64; 3] {
    let (st, ct) = tilt_deg.to_radians().sin_cos();
    let (sa, ca) = azim_deg.to_radians().sin_cos();
    unit3([st * ca, st * sa, ct])
}
fn pose(tip: [f64; 3], axis: [f64; 3]) -> PartPose { PartPose { tip, axis } }

#[test]
fn round_trip_fk_ik_with_nonzero_offsets_within_micron() {
    let mut cfg = standard_ac_trunnion();
    cfg.a.travel = Some((-120.0, 120.0));
    cfg.a.zero_offset_deg = 4.5;
    cfg.c.zero_offset_deg = -12.25;
    cfg.work_datum = [10.0, -20.0, 5.0];

    for sign in [
        RotarySignConvention::TableRightHand,
        RotarySignConvention::ToolRightHand,
    ] {
        cfg.sign = sign;
        for a_axis in [[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], unit3([1.0, 0.08, -0.04])] {
            cfg.a.axis = a_axis;
            for c_axis in [[0.0, 0.0, 1.0], [0.0, 0.0, -1.0]] {
                cfg.c.axis = c_axis;
                let mut solver = AcSolver::new(&cfg).expect("valid trunnion config");
                let samples = [
                    ([25.0, -18.0, 12.0], [0.0, 0.0, 1.0]),
                    ([12.3, 44.1, -5.0], axis_from_deg(25.0, 37.0)),
                    ([-30.0, 14.0, 8.5], axis_from_deg(65.0, -142.0)),
                    ([0.0, 0.0, 0.0], axis_from_deg(15.0, 180.0)),
                ];
                for (tip, axis) in samples {
                    let part = PartPose { tip, axis };
                    let mach = solver
                        .solve_pose(part, SectionBoundary::Continuous)
                        .expect("pose should be reachable");
                    let (fwd_mach, spindle_dir) = forward_machine(&cfg, part, mach.a_deg, mach.c_deg)
                        .expect("forward_machine");
                    assert!(
                        norm3(sub3(fwd_mach.xyz, mach.xyz)) <= 1e-6,
                        "forward_machine xyz mismatch: {fwd_mach:?} vs {mach:?}"
                    );
                    assert!(
                        dot3(spindle_dir, cfg.spindle_axis) > 1.0 - 1e-10,
                        "tool axis did not align with spindle: {spindle_dir:?}"
                    );
                    let rt = forward_part(&cfg, mach).expect("forward_part");
                    let tip_err = norm3(sub3(rt.tip, part.tip));
                    assert!(
                        tip_err <= 0.001,
                        "round-trip tip error {tip_err} mm exceeds 0.001 mm"
                    );
                    let axis_dot = dot3(rt.axis, part.axis).clamp(-1.0, 1.0);
                    let axis_err_deg = axis_dot.acos().to_degrees();
                    assert!(
                        axis_err_deg <= 1e-6,
                        "round-trip axis error {axis_err_deg} deg too high"
                    );
                }
            }
        }
    }
}

#[test]
fn asymmetric_a_limits_select_reachable_branch_and_refuse_unreachable() {
    let cfg = standard_ac_trunnion(); // A travel: -110..30
    let mut solver = AcSolver::new(&cfg).expect("valid config");

    let pose_ok = solver
        .solve_pose(pose([10.0, 5.0, 2.0], axis_from_deg(45.0, 0.0)), SectionBoundary::Continuous)
        .expect("(-45, C) branch should be chosen");
    assert!((pose_ok.a_deg - (-45.0)).abs() < 1e-6, "expected A=-45, got {}", pose_ok.a_deg);
    let rt = forward_part(&cfg, pose_ok).expect("round trip");
    assert!(norm3(sub3(rt.tip, [10.0, 5.0, 2.0])) <= 0.001);

    // 120 deg tilt: neither +120 nor -120 fits in (-110, 30) -> named refusal.
    let err = solver
        .solve_pose(pose([0.0, 0.0, 0.0], axis_from_deg(120.0, 45.0)), SectionBoundary::Continuous)
        .expect_err("120 deg tilt must refuse");
    let msg = err.to_string();
    assert!(msg.contains("A") && msg.contains("-110") && msg.contains("30"), "got: {msg}");
}

#[test]
fn pole_near_zero_holds_previous_modal_c_instead_of_snapping_to_zero() {
    let cfg = standard_ac_trunnion();
    let mut solver = AcSolver::new(&cfg).expect("valid config");

    let first = solver
        .solve_pose(pose([5.0, 0.0, 0.0], axis_from_deg(15.0, 42.0)), SectionBoundary::Continuous)
        .expect("first tilted pose");
    let held_c = first.c_deg;
    assert!(held_c.abs() > 10.0, "precondition: non-zero C ({held_c})");

    for pole_axis in [[0.0, 0.0, 1.0], unit3([1e-8, -2e-8, 1.0])] {
        let pole = solver
            .solve_pose(pose([5.0, 0.0, 0.0], pole_axis), SectionBoundary::Continuous)
            .expect("pole pose");
        assert!(pole.a_deg.abs() < 1e-4, "expected A near 0 at pole, got {}", pole.a_deg);
        assert!((pole.c_deg - held_c).abs() < 1e-9, "expected modal {held_c}, got {}", pole.c_deg);
    }
}

#[test]
fn smooth_crossing_of_a_zero_without_180_deg_c_flip() {
    let mut cfg = standard_ac_trunnion();
    cfg.a.travel = Some((-90.0, 90.0));
    let mut solver = AcSolver::new(&cfg).expect("valid config");

    // Sweep across the vertical pole along one meridian:
    // +10 deg along +Y -> 0 -> -10 deg (which is 10 deg along -Y).
    let tilts = [10.0_f64, 4.0, 0.0, -4.0, -10.0];
    let mut solved = Vec::new();
    for &t in &tilts {
        let s = t.to_radians().sin();
        let c = t.to_radians().cos();
        // Smooth meridian in YZ plane tilted toward +35 deg azimuth in C.
        let (sa, ca) = 35.0_f64.to_radians().sin_cos();
        let axis = unit3([s * ca, s * sa, c]);
        let pose = solver
            .solve_pose(
                PartPose {
                    tip: [10.0, 5.0, 0.0],
                    axis,
                },
                SectionBoundary::Continuous,
            )
            .expect("pole crossing");
        solved.push(pose);
    }

    for w in solved.windows(2) {
        let dc = (w[1].c_deg - w[0].c_deg).abs();
        assert!(
            dc < 1.0,
            "C flipped across A=0: {} -> {} (dC={dc})",
            w[0].c_deg,
            w[1].c_deg
        );
    }
    assert!(
        solved[0].a_deg * solved[4].a_deg < 0.0,
        "A should cross zero smoothly from positive to negative (or vice versa): {:?}",
        solved.iter().map(|p| p.a_deg).collect::<Vec<_>>()
    );
}

#[test]
fn continuous_c_unwrapping_and_safe_retract_travel_refusal() {
    let mut cfg = standard_ac_trunnion();
    cfg.c.travel = Some((-450.0, 450.0));

    let mut moves = Vec::new();
    let total_steps = 12usize;
    for i in 0..total_steps {
        // Use 40 deg tilt so only the A = -40 deg branch fits (-110, 30), and
        // start at azimuth -90 deg so C begins at 0 deg and winds smoothly to -440 deg.
        let a0 = -90.0 + (i as f64) * 40.0;
        let a1 = -90.0 + ((i + 1) as f64) * 40.0;
        moves.push(PathMove {
            kind: MoveKind::Feed,
            from_tip: [10.0, 0.0, 0.0],
            to_tip: [10.0, 0.0, 0.0],
            from_axis: axis_from_deg(40.0, a0),
            to_axis: axis_from_deg(40.0, a1),
            feed: Some(500.0),
            safe_retract: false,
        });
    }

    let opts = PathOptions {
        chord_tol_mm: 0.05,
        max_angular_step_deg: Some(15.0),
        subdivide_rapids: false,
    };

    // Up to 440 deg fits inside (-450, 450) without flipping.
    let segs = solve_path(&cfg, &moves[..11], &opts).expect("440 deg fits in (-450, 450)");
    for w in segs.windows(2) {
        let dc = (w[1].to.c_deg - w[0].to.c_deg).abs();
        assert!(
            dc <= 15.0 + 1e-6,
            "continuous C step jumped unexpectedly: {} -> {}",
            w[0].to.c_deg,
            w[1].to.c_deg
        );
    }
    assert!(
        segs.last().expect("nonempty").to.c_deg.abs() > 360.0,
        "expected continuous unwrapped C past 360 deg, got {}",
        segs.last().expect("nonempty").to.c_deg
    );

    // Continuing to 480 deg without a safe retract must refuse by name.
    let err = solve_path(&cfg, &moves, &opts).expect_err("480 deg exceeds (-450, 450)");
    assert!(
        err.to_string().contains("C") && err.to_string().contains("450"),
        "refusal should name C and travel: {err}"
    );

    // A retract does not authorize an unchecked unwind; exhausted travel still refuses.
    let mut with_retract = moves[..11].to_vec();
    with_retract.push(PathMove {
        kind: MoveKind::Rapid,
        from_tip: [10.0, 0.0, 0.0],
        to_tip: [10.0, 0.0, 50.0],
        from_axis: axis_from_deg(40.0, 350.0),
        to_axis: axis_from_deg(40.0, 350.0),
        feed: None,
        safe_retract: true,
    });
    with_retract.push(PathMove {
        kind: MoveKind::Feed,
        from_tip: [10.0, 0.0, 50.0],
        to_tip: [10.0, 0.0, 0.0],
        from_axis: axis_from_deg(40.0, 350.0),
        to_axis: axis_from_deg(40.0, 390.0),
        feed: Some(500.0),
        safe_retract: false,
    });
    let err = solve_path(&cfg, &with_retract, &opts)
        .expect_err("unchecked clearance unwind must not be synthesized");
    assert!(
        err.to_string().contains("C") && err.to_string().contains("450"),
        "refusal should name C and travel: {err}"
    );
}

#[test]
fn subdivides_linear_moves_beyond_chord_tolerance_for_tip_accuracy() {
    let mut cfg = standard_ac_trunnion();
    cfg.a.travel = Some((-120.0, 120.0));

    let p0 = [35.0, -12.0, 8.0];
    let p1 = [-28.0, 22.0, 4.0];
    let u0 = axis_from_deg(30.0, 10.0);
    let u1 = axis_from_deg(30.0, 100.0);

    let tol = 0.005;
    let opts = PathOptions {
        chord_tol_mm: tol,
        max_angular_step_deg: None,
        subdivide_rapids: false,
    };

    let segs = solve_path(
        &cfg,
        &[PathMove {
            kind: MoveKind::Feed,
            from_tip: p0,
            to_tip: p1,
            from_axis: u0,
            to_axis: u1,
            feed: Some(400.0),
            safe_retract: false,
        }],
        &opts,
    )
    .expect("subdivide curved rotary move");

    assert!(
        segs.len() > 8,
        "expected 90-deg rotary sweep at >35mm radius to subdivide into many chords, got {}",
        segs.len()
    );

    for seg in &segs {
        for s in [0.25, 0.5, 0.75] {
            let xyz = [
                seg.from.xyz[0] + (seg.to.xyz[0] - seg.from.xyz[0]) * s,
                seg.from.xyz[1] + (seg.to.xyz[1] - seg.from.xyz[1]) * s,
                seg.from.xyz[2] + (seg.to.xyz[2] - seg.from.xyz[2]) * s,
            ];
            let a_deg = seg.from.a_deg + (seg.to.a_deg - seg.from.a_deg) * s;
            let c_deg = seg.from.c_deg + (seg.to.c_deg - seg.from.c_deg) * s;
            let actual = forward_part(&cfg, camcraft_post::kinematics::MachinePose { xyz, a_deg, c_deg })
                .expect("forward_part on midpoint");
            let desired_tip = [
                seg.tip_from[0] + (seg.tip_to[0] - seg.tip_from[0]) * s,
                seg.tip_from[1] + (seg.tip_to[1] - seg.tip_from[1]) * s,
                seg.tip_from[2] + (seg.tip_to[2] - seg.tip_from[2]) * s,
            ];
            let tip_err = norm3(sub3(actual.tip, desired_tip));
            assert!(
                tip_err <= tol + 1e-6,
                "interpolated tip error {tip_err} mm exceeded chord tolerance {tol} mm"
            );
        }
    }
}

#[test]
fn refuses_malformed_machine_geometry_and_non_finite_inputs() {
    let mut parallel = standard_ac_trunnion();
    parallel.a.axis = [0.0, 0.0, 1.0];
    let err = AcSolver::new(&parallel).expect_err("parallel A and C must refuse");
    assert!(err.to_string().to_lowercase().contains("parallel"));

    let mut zero = standard_ac_trunnion();
    zero.c.axis = [0.0, 0.0, 0.0];
    let err = AcSolver::new(&zero).expect_err("zero axis must refuse");
    assert!(err.to_string().contains("C"));

    let mut bad_travel = standard_ac_trunnion();
    bad_travel.a.travel = Some((30.0, -110.0));
    let err = AcSolver::new(&bad_travel).expect_err("inverted travel must refuse");
    assert!(err.to_string().contains("A"));

    let mut nan_solver = AcSolver::new(&standard_ac_trunnion()).expect("valid config");
    let err = nan_solver
        .solve_pose(
            PartPose {
                tip: [f64::NAN, 0.0, 0.0],
                axis: [0.0, 0.0, 1.0],
            },
            SectionBoundary::Continuous,
        )
        .expect_err("non-finite tip must refuse");
    assert!(err.to_string().to_lowercase().contains("finite"));
}
