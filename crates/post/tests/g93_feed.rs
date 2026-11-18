//! Pinning tests for the shared G93 inverse-time feed solver (#1966).
//!
//! Provenance: clean-room tests against RS-274D G93 (`F = 1 / dt_min`, min^-1).
//! Gate map (M4.4): covers post::post, post::g93, post::rotary_cap, post::kinematics, post::machine.

use std::path::Path;

use camcraft_post::cl::MoveKind;
use camcraft_post::kinematics::{
    AcTrunnionConfig, PathMove, PathOptions, SolvedMove, solve_path,
};
use camcraft_post::machine::parse_mch;
use camcraft_post::post::g93::{
    AxisSpeedLimits, G93Decision, G93Error, G93Options, G93SegmentInput, LinearUnit,
    format_g93_feed_word, solve_g93_segment, solve_g93_segment_in_units, solve_g93_solved_moves,
};

fn trunnion_cfg() -> AcTrunnionConfig {
    AcTrunnionConfig {
        c_offset: [0.0, 0.0, -50.0],
        wcs_offset: [0.0, 0.0, 20.0],
        ..AcTrunnionConfig::default()
    }
}

#[test]
fn part_space_tip_distance_sets_dt_cut_even_when_pivot_inflates_machine_xyz() {
    // Part tip moves 10 mm along +X at 1000 mm/min => dt_cut = 0.01 min, F = 100 min^-1.
    // Meanwhile, a simultaneous tilt swings the machine X slide by 25 mm.
    // Because no axis ceiling is exceeded (limits = unstated), F must reflect
    // part-space ||tip_1 - tip_0|| / v_prog = 10 / 1000 = 0.01 min -> F = 100.
    let seg = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [0.0, 0.0, 0.0],
        tip_to: [10.0, 0.0, 0.0],
        machine_xyz_from: [0.0, 0.0, 120.0],
        machine_xyz_to: [25.0, 0.0, 120.0],
        rotary_deg_from: [0.0, 0.0, 0.0],
        rotary_deg_to: [5.0, 0.0, 0.0],
        feed_mm_per_min: Some(1000.0),
    };
    let res = solve_g93_segment(&seg, &AxisSpeedLimits::default(), &G93Options::default())
        .expect("unconstrained G93 segment");
    let G93Decision::Feed(timing) = res else {
        panic!("expected Feed, got {res:?}");
    };
    assert!((timing.ds_tip_mm - 10.0).abs() < 1e-12);
    assert!((timing.dt_cut_min - 0.01).abs() < 1e-12);
    assert!((timing.dt_lim_min - 0.0).abs() < 1e-12);
    assert!((timing.dt_min - 0.01).abs() < 1e-12);
    assert!((timing.f_inv_min - 100.0).abs() < 1e-9);
}

#[test]
fn rotary_and_linear_max_normal_speed_ceilings_clamp_dt_min_upward() {
    // Tip moves 2 mm at 1000 mm/min => dt_cut = 0.002 min (would be F500).
    // Rotary A moves 30 deg with max_normal_speed = 1500 deg/min => dt_lim_A = 0.02 min (F50).
    // dt_min must be max(0.002, 0.02) = 0.02 min => F = 50 min^-1.
    let limits = AxisSpeedLimits {
        linear_mm_per_min: [Some(5000.0), Some(5000.0), Some(3000.0)],
        rotary_deg_per_min: [Some(1500.0), None, Some(3000.0)],
    };
    let seg_rot = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [0.0, 0.0, 0.0],
        tip_to: [2.0, 0.0, 0.0],
        machine_xyz_from: [0.0, 0.0, 0.0],
        machine_xyz_to: [2.0, 0.0, 0.0],
        rotary_deg_from: [0.0, 0.0, 0.0],
        rotary_deg_to: [30.0, 0.0, 0.0],
        feed_mm_per_min: Some(1000.0),
    };
    let G93Decision::Feed(t_rot) = solve_g93_segment(&seg_rot, &limits, &G93Options::default())
        .expect("rotary-capped segment")
    else {
        panic!("expected Feed");
    };
    assert!((t_rot.dt_cut_min - 0.002).abs() < 1e-12);
    assert!((t_rot.dt_lim_min - 0.02).abs() < 1e-12);
    assert!((t_rot.dt_min - 0.02).abs() < 1e-12);
    assert!((t_rot.f_inv_min - 50.0).abs() < 1e-9);

    // Pivot swing moves machine Z by 30 mm while tip moves only 1 mm at 1000 mm/min.
    // Linear Z limit is 1500 mm/min => dt_lim_Z = 30 / 1500 = 0.02 min => F = 50 min^-1.
    let lin_limits = AxisSpeedLimits {
        linear_mm_per_min: [Some(5000.0), Some(5000.0), Some(1500.0)],
        rotary_deg_per_min: [None, None, None],
    };
    let seg_lin = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [0.0, 0.0, 0.0],
        tip_to: [1.0, 0.0, 0.0],
        machine_xyz_from: [0.0, 0.0, 0.0],
        machine_xyz_to: [1.0, 0.0, 30.0],
        rotary_deg_from: [0.0, 0.0, 0.0],
        rotary_deg_to: [10.0, 0.0, 0.0],
        feed_mm_per_min: Some(1000.0),
    };
    let G93Decision::Feed(t_lin) = solve_g93_segment(&seg_lin, &lin_limits, &G93Options::default())
        .expect("linear-capped segment")
    else {
        panic!("expected Feed");
    };
    assert!((t_lin.dt_min - 0.02).abs() < 1e-12);
    assert!((t_lin.f_inv_min - 50.0).abs() < 1e-9);
}

#[test]
fn rotary_only_reorientation_uses_limit_or_fallback_and_never_divides_by_zero() {
    // Pure rotary index on spot: tip does not move in part space (ds_tip == 0),
    // C rotates 90 deg, Z compensates by 0 here.
    let limits = AxisSpeedLimits {
        linear_mm_per_min: [None; 3],
        rotary_deg_per_min: [None, None, Some(1800.0)],
    };
    let seg = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [10.0, 0.0, 5.0],
        tip_to: [10.0, 0.0, 5.0],
        machine_xyz_from: [10.0, 0.0, 5.0],
        machine_xyz_to: [10.0, 0.0, 5.0],
        rotary_deg_from: [0.0, 0.0, 0.0],
        rotary_deg_to: [0.0, 0.0, 90.0],
        feed_mm_per_min: Some(500.0),
    };
    let G93Decision::Feed(t1) = solve_g93_segment(&seg, &limits, &G93Options::default())
        .expect("rotary-only with axis limit")
    else {
        panic!("expected Feed");
    };
    // 90 deg / 1800 deg/min = 0.05 min => F = 20.
    assert!((t1.dt_min - 0.05).abs() < 1e-12);
    assert!((t1.f_inv_min - 20.0).abs() < 1e-9);

    // Unstated machine limits + explicit configured rotary_only_deg_per_min: 90 deg / 900 = 0.1 min -> F10.
    let opts = G93Options {
        rotary_only_deg_per_min: Some(900.0),
        ..G93Options::default()
    };
    let G93Decision::Feed(t2) = solve_g93_segment(&seg, &AxisSpeedLimits::default(), &opts)
        .expect("rotary-only with configured angular speed")
    else {
        panic!("expected Feed");
    };
    assert!((t2.dt_min - 0.1).abs() < 1e-12);
    assert!((t2.f_inv_min - 10.0).abs() < 1e-9);

    // Unstated limits and no configured angular speed: uses finite v_prog fallback (90 / 500 = 0.18 min -> F = 5.555...).
    let G93Decision::Feed(t3) = solve_g93_segment(&seg, &AxisSpeedLimits::default(), &G93Options::default())
        .expect("rotary-only with v_prog fallback")
    else {
        panic!("expected Feed");
    };
    assert!(t3.f_inv_min.is_finite() && (t3.f_inv_min - (500.0 / 90.0)).abs() < 1e-9);

    // Rotary-only with NEITHER feed NOR limits MUST refuse cleanly, never return NaN/Inf.
    let no_speed = G93SegmentInput { feed_mm_per_min: None, ..seg };
    let err = solve_g93_segment(&no_speed, &AxisSpeedLimits::default(), &G93Options::default())
        .expect_err("rotary-only without feed or rotary limits must refuse");
    assert!(matches!(err, G93Error::MissingFeedAndLimits { .. }));
}

#[test]
fn duplicate_zero_motion_skips_and_bounds_clamp_and_bad_inputs_refuse() {
    let dup = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [1.0, 2.0, 3.0],
        tip_to: [1.0, 2.0, 3.0],
        machine_xyz_from: [1.0, 2.0, 3.0],
        machine_xyz_to: [1.0, 2.0, 3.0],
        rotary_deg_from: [15.0, 0.0, 45.0],
        rotary_deg_to: [15.0, 0.0, 45.0],
        feed_mm_per_min: Some(1000.0),
    };
    assert_eq!(
        solve_g93_segment(&dup, &AxisSpeedLimits::default(), &G93Options::default())
            .expect("zero-motion duplicate skips"),
        G93Decision::Skip
    );

    // Tiny move clamping to f_max.
    let tiny = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [0.0, 0.0, 0.0],
        tip_to: [1e-5, 0.0, 0.0],
        machine_xyz_from: [0.0, 0.0, 0.0],
        machine_xyz_to: [1e-5, 0.0, 0.0],
        rotary_deg_from: [0.0; 3],
        rotary_deg_to: [0.0; 3],
        feed_mm_per_min: Some(10_000.0),
    };
    let opts = G93Options::default();
    let G93Decision::Feed(clamped_hi) = solve_g93_segment(&tiny, &AxisSpeedLimits::default(), &opts)
        .expect("tiny move clamps")
    else {
        panic!("expected Feed");
    };
    assert!((clamped_hi.f_inv_min - opts.f_max).abs() < 1e-12);

    // Very slow / huge move clamping to f_min (and word never truncates F0.001 to 0).
    let huge = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [0.0, 0.0, 0.0],
        tip_to: [5000.0, 0.0, 0.0],
        machine_xyz_from: [0.0, 0.0, 0.0],
        machine_xyz_to: [5000.0, 0.0, 0.0],
        rotary_deg_from: [0.0; 3],
        rotary_deg_to: [0.0; 3],
        feed_mm_per_min: Some(1.0),
    };
    let G93Decision::Feed(clamped_lo) = solve_g93_segment(&huge, &AxisSpeedLimits::default(), &opts)
        .expect("very slow move clamps to f_min")
    else {
        panic!("expected Feed");
    };
    assert!((clamped_lo.f_inv_min - opts.f_min).abs() < 1e-12);
    assert_eq!(format_g93_feed_word(clamped_lo.f_inv_min), "0.001");

    // Floor formatting never rounds upward into a speed violation.
    assert_eq!(format_g93_feed_word(12.34569), "12.3456");

    // Non-positive or non-finite feed rate refuses by name.
    let bad_feed = G93SegmentInput { feed_mm_per_min: Some(0.0), ..huge };
    let err = solve_g93_segment(&bad_feed, &AxisSpeedLimits::default(), &opts)
        .expect_err("zero feed refuses");
    assert!(matches!(err, G93Error::InvalidFeedRate(_)));

    let neg_limit = AxisSpeedLimits {
        linear_mm_per_min: [Some(-10.0), None, None],
        rotary_deg_per_min: [None; 3],
    };
    let err_lim = solve_g93_segment(&huge, &neg_limit, &opts).expect_err("negative axis limit refuses");
    assert!(matches!(err_lim, G93Error::InvalidAxisLimit { axis: "X", .. }));

    let bad_rot = AxisSpeedLimits {
        linear_mm_per_min: [None; 3],
        rotary_deg_per_min: [None, Some(0.0), None],
    };
    assert!(matches!(
        solve_g93_segment(&huge, &bad_rot, &opts),
        Err(G93Error::InvalidAxisLimit { axis: "B", .. })
    ));
    let bad_opt = G93Options { f_min: 10.0, f_max: 5.0, ..G93Options::default() };
    assert!(matches!(
        solve_g93_segment(&huge, &AxisSpeedLimits::default(), &bad_opt),
        Err(G93Error::InvalidOptions(_))
    ));
    assert_eq!(format_g93_feed_word(50.0), "50");
    assert_eq!(format_g93_feed_word(1e-7), "0.0001");
}

#[test]
fn g20_inch_and_g21_mm_produce_identical_g93_f_and_integrates_with_ik_and_mch() {
    let tip0_mm = [25.4, 0.0, 0.0];
    let tip1_mm = [50.8, 25.4, 0.0]; // 1" in X, 1" in Y => sqrt(2)" = 35.9210... mm
    let xyz0_mm = [25.4, 0.0, 127.0];
    let xyz1_mm = [50.8, 38.1, 127.0];
    let rot0 = [0.0, 0.0, 0.0];
    let rot1 = [15.0, 0.0, 30.0];

    let seg_mm = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: tip0_mm,
        tip_to: tip1_mm,
        machine_xyz_from: xyz0_mm,
        machine_xyz_to: xyz1_mm,
        rotary_deg_from: rot0,
        rotary_deg_to: rot1,
        feed_mm_per_min: Some(508.0), // 20 ipm
    };
    let limits_mm = AxisSpeedLimits {
        linear_mm_per_min: [Some(2540.0), Some(1270.0), Some(2540.0)],
        rotary_deg_per_min: [Some(600.0), None, Some(1200.0)],
    };
    let opts = G93Options::default();
    let res_mm = solve_g93_segment_in_units(LinearUnit::Mm, &seg_mm, &limits_mm, &opts).expect("mm");

    let seg_in = G93SegmentInput {
        kind: MoveKind::Feed,
        tip_from: [1.0, 0.0, 0.0],
        tip_to: [2.0, 1.0, 0.0],
        machine_xyz_from: [1.0, 0.0, 5.0],
        machine_xyz_to: [2.0, 1.5, 5.0],
        rotary_deg_from: rot0,
        rotary_deg_to: rot1,
        feed_mm_per_min: Some(20.0), // ipm when unit = Inch
    };
    let limits_in = AxisSpeedLimits {
        linear_mm_per_min: [Some(100.0), Some(50.0), Some(100.0)], // ipm when unit = Inch
        rotary_deg_per_min: [Some(600.0), None, Some(1200.0)],     // deg/min stays deg/min
    };
    let res_in = solve_g93_segment_in_units(LinearUnit::Inch, &seg_in, &limits_in, &opts).expect("inch");

    let (G93Decision::Feed(t_mm), G93Decision::Feed(t_in)) = (res_mm, res_in) else {
        panic!("expected Feed in both units");
    };
    assert!(
        (t_mm.f_inv_min - t_in.f_inv_min).abs() < 1e-12,
        "G20 F={} must equal G21 F={}",
        t_in.f_inv_min,
        t_mm.f_inv_min
    );

    // Load real machine definitions and integrate with kinematics::solve_path.
    let mch_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tilthead.mch");
    let mch = parse_mch(&std::fs::read_to_string(&mch_path).expect("tilthead.mch")).expect("parse mch");
    let mch_limits = AxisSpeedLimits::from_machine(&mch);
    assert_eq!(mch_limits.linear_mm_per_min, [Some(5000.0), Some(5000.0), Some(3000.0)]);
    assert_eq!(mch_limits.rotary_deg_per_min, [Some(2000.0), Some(2000.0), None]);

    let path_moves = vec![
        PathMove {
            kind: MoveKind::Rapid,
            feed: None,
            from_tip: [10.0, 0.0, 20.0],
            to_tip: [10.0, 0.0, 10.0],
            from_axis: [0.0, 0.0, 1.0],
            to_axis: [0.0, 0.0, 1.0],
            safe_retract: false,
        },
        PathMove {
            kind: MoveKind::Feed,
            feed: Some(600.0),
            from_tip: [10.0, 0.0, 10.0],
            to_tip: [20.0, 5.0, 10.0],
            from_axis: [0.0, 0.0, 1.0],
            to_axis: [0.0, (25.0_f64).to_radians().sin(), (25.0_f64).to_radians().cos()],
            safe_retract: false,
        },
    ];
    let cfg = trunnion_cfg();
    let solved: Vec<SolvedMove> = solve_path(&cfg, &path_moves, &PathOptions::default()).expect("ik path");
    let g93_out = solve_g93_solved_moves(&solved, &mch_limits, &opts).expect("g93 over solved path");
    assert_eq!(g93_out.len(), solved.len());
    assert_eq!(g93_out[0].1, G93Decision::Rapid);
    assert!(matches!(g93_out[1].1, G93Decision::Feed(t) if t.f_inv_min > 0.0 && t.f_inv_min.is_finite()));
}
