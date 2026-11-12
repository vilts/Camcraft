//! Verification warnings: clean generated paths stay silent, and each of
//! the five planted offenders yields exactly its warning class.
//!
//! Gate map (M4.4): covers cl::cl, motion::finishing3d, geometry::geo2d, app::geometry, motion::machining2d, sim::verify - and the ENVELOPE verdict lane of sim::stock_sim (scan/gouge/engine), whose law changes must re-run this binary and its ignored release pair (the E2 lesson: a kernel-law commit that names no suite here goes unchecked through the narrow gate).
//! Lane shape (M4.4): 15 structural verifies run in milliseconds; the two
//! cone_facets(96) envelope verdicts are #[ignore]d - heavy lane, run them with
//! `cargo test --release -p camcraft-app -- --ignored` at milestone close.

use camcraft_app::cl::{Cycle, CycleKind, Move, MoveKind, Toolpath};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::Point3;
use camcraft_app::machining2d::{
    contour, pocket, trace, ContourParams, PocketParams, PocketStrategy, Side, TraceParams,
};
use camcraft_app::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    Region::new(vec![Loop::new(vec![
        p2(x0, y0),
        p2(x1, y0),
        p2(x1, y1),
        p2(x0, y1),
    ])
    .unwrap()])
    .unwrap()
}

/// Pocketable 30x30 with a 10x10 part island at the center.
fn plate_with_island() -> Region {
    let mut loops = vec![rect(0.0, 0.0, 30.0, 30.0).loops()[0].clone()];
    loops.push(
        Loop::new(vec![p2(10.0, 10.0), p2(20.0, 10.0), p2(20.0, 20.0), p2(10.0, 20.0)]).unwrap(),
    );
    Region::new(loops).unwrap()
}

fn deep_tool() -> ToolGeom {
    ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 9.0,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn short_tool() -> ToolGeom {
    ToolGeom {
        flute_len: 2.0,
        ..deep_tool()
    }
}

fn ctx<'a>(region: &'a Region, stock: Option<&'a Region>, tool: &'a ToolGeom, mode: Mode) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock,
        // Every fixture here is a 2.5-D op: no mesh, the static laws.
        model: None,
        tool,
        cutter: None,
        mode,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        // 10 um: the pocket's corner-avoidance samples can land a chord
        // error inside the reach circle (geo2d round-join flattening is
        // 5 um); anything past that is a real gouge, the plant below is
        // 500 um deep.
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: None,
        // No workholding in these laws' worlds (M3.2 B): the empty
        // slice and the absent spindle band keep every verdict exactly
        // what it was before the stage existed.
        fixtures: &[],
        spindle: None,
        ..Default::default()
    }
}

#[test]
fn clean_pocket_verifies_silent() {
    // Zigzag pocket around an island, deep flute: no law may fire.
    let r = plate_with_island();
    let stock = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(
        &r,
        &PocketParams {
            links: camcraft_app::links::Links::PLANAR,
            tool_diameter: 8.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
        },
    )
    .unwrap();
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Area)), Vec::new());
}

#[test]
fn a_ramp_pocket_verifies_silent() {
    // The M5.1 H ramp on the plane: the diagonal between levels cuts
    // inside the one center area every level shares, so no law may
    // fire on it - the same claim the ladder's chord test makes, now
    // checked by the verifier's own eyes.
    let r = rect(0.0, 0.0, 30.0, 30.0);
    let stock = rect(-4.0, -4.0, 34.0, 34.0);
    let t = pocket(
        &r,
        &PocketParams {
            links: camcraft_app::links::Links::PLANAR,
            tool_diameter: 8.0,
            stepover: 4.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -6.0,
            stepdown: 3.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
            entry: camcraft_app::machining3d::Entry3D::Ramp,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
        },
    )
    .unwrap();
    assert!(
        t.moves.iter().any(|m| {
            m.kind == MoveKind::Feed && m.from.z != m.to.z && (m.from.x != m.to.x || m.from.y != m.to.y)
        }),
        "the fixture must actually ramp, or the test proves nothing"
    );
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Area)), Vec::new());
}

#[test]
fn a_helix_pocket_verifies_silent() {
    // The sinking orbit: arcs are feeds like any other, and where the
    // disc has room (30x30 with a 2 mm lead on an 8 mm tool) the dive
    // stays inside the floor at every z it crosses.
    let r = rect(0.0, 0.0, 30.0, 30.0);
    let stock = rect(-4.0, -4.0, 34.0, 34.0);
    let t = pocket(
        &r,
        &PocketParams {
            links: camcraft_app::links::Links::PLANAR,
            tool_diameter: 8.0,
            stepover: 4.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -6.0,
            stepdown: 3.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
            entry: camcraft_app::machining3d::Entry3D::Helix,
            helix_radius: 2.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
        },
    )
    .unwrap();
    assert!(
        t.moves.iter().any(|m| m.arc.is_some() && m.to.z < 0.0),
        "the fixture must actually sink on arcs, or the test proves nothing"
    );
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Area)), Vec::new());
}

#[test]
fn clean_contour_verifies_silent() {
    // 30x30 plate, 10x10 through hole: the finish ring with a deep tool
    // touches but never bites, and its shank clears the wall top.
    let r = plate_with_island();
    let t = contour(
        &r,
        &ContourParams {
            tool_diameter: 8.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: 8.0,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            side: Side::Auto,
            lead: camcraft_app::machining2d::Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false,
            links: camcraft_app::links::Links::PLANAR,
        },
    )
    .unwrap();
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Boundary)), Vec::new());
}

#[test]
fn crossing_feeds_are_flagged() {
    // An X at shared depth, well clear of the walls so only the crossing
    // law can fire: one Crossing, nothing else.
    let t = Toolpath {
        moves: vec![
            Move::new(MoveKind::Feed, p(4.0, 4.0, -1.0), p(16.0, 16.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Feed, p(4.0, 16.0, -1.0), p(16.0, 4.0, -1.0), Some(500.0)),
        ],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Area)), vec![Warning::Crossing { moves: [0, 1] }]);
}

#[test]
fn feeds_on_different_levels_may_cross() {
    // Same X, but 2 mm apart in Z: air over the lower level, no warning.
    let t = Toolpath {
        moves: vec![
            Move::new(MoveKind::Feed, p(4.0, 4.0, -1.0), p(16.0, 16.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Feed, p(4.0, 16.0, -3.0), p(16.0, 4.0, -3.0), Some(500.0)),
        ],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Area)), Vec::new());
}

/// A square ring 0.5 mm closer to the walls of a 20x20 plate than the
/// r = 4 tool's tangency circle: every point of it bites past the
/// finish surface.
fn gouge_ring() -> Toolpath {
    let mut moves = vec![Move::new(MoveKind::Plunge, p(3.5, 3.5, 5.0), p(3.5, 3.5, -1.0), Some(100.0))];
    for (from, to) in [((3.5, 3.5), (16.5, 3.5)), ((16.5, 3.5), (16.5, 16.5)), ((16.5, 16.5), (3.5, 16.5)), ((3.5, 16.5), (3.5, 3.5))] {
        moves.push(Move::new(MoveKind::Feed, p(from.0, from.1, -1.0), p(to.0, to.1, -1.0), Some(500.0)));
    }
    Toolpath {
        moves,
        ..Toolpath::default()
    }
}

#[test]
fn a_positive_leave_never_licenses_cutting_the_wall() {
    // The 0.5 mm-deep ring with a 0.5 mm leave: the old tangency law
    // read `r - leave` (3.49 mm) and let the 3.5 mm ring pass - a
    // silent 0.5 mm gouge into the finished wall (Prime Directive).
    let t = gouge_ring();
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    let cx = VerifyContext { stock_to_leave: 0.5, ..ctx(&r, None, &tool, Mode::Boundary) };
    let warns = verify(&t, &cx);
    assert_eq!(warns.len(), 5, "{warns:?}");
    assert!(warns.iter().all(|w| matches!(w, Warning::Gouge { .. })), "{warns:?}");
}

#[test]
fn a_negative_leave_is_the_overcut_asked_for() {
    // A -0.6 mm leave asks the tool 0.6 mm past the wall: the 0.5 mm
    // ring is inside that request and stays clean; -0.4 mm is not.
    let t = gouge_ring();
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    let asked = VerifyContext { stock_to_leave: -0.6, ..ctx(&r, None, &tool, Mode::Boundary) };
    assert_eq!(verify(&t, &asked), Vec::new());
    let short = VerifyContext { stock_to_leave: -0.4, ..ctx(&r, None, &tool, Mode::Boundary) };
    assert!(verify(&t, &short).iter().any(|w| matches!(w, Warning::Gouge { .. })));
}

#[test]
fn gouge_ring_is_flagged() {
    let t = gouge_ring();
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    let warns = verify(&t, &ctx(&r, None, &tool, Mode::Boundary));
    assert_eq!(warns.len(), 5);
    assert!(warns.iter().all(|w| matches!(w, Warning::Gouge { .. })), "{warns:?}");
    assert_eq!(warns[0], Warning::Gouge { at: 0 });
}

#[test]
fn arcs_keep_warning_indices() {
    // A shallow arc from (4,10) to (16,10) whose chords sweep down to 2
    // mm of the y = 20 wall, while both endpoints sit exactly on the
    // r = 4 tool's tangency line: the law must sample inside the arc,
    // yet the warning still speaks a move index - one warning, at the
    // arc's own slot, whatever its chord count.
    let arc = Move::new(MoveKind::Feed, p(4.0, 10.0, -1.0), p(16.0, 10.0, -1.0), Some(500.0))
        .with_arc(p2(10.0, 11.75), false);
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let tool = deep_tool();
    let t = Toolpath {
        moves: vec![arc],
        ..Toolpath::default()
    };
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Boundary)), vec![Warning::Gouge { at: 0 }]);
    // The straight chord between the same endpoints never dips: silent.
    let straight = Toolpath {
        moves: vec![Move::new(MoveKind::Feed, p(4.0, 10.0, -1.0), p(16.0, 10.0, -1.0), Some(500.0))],
        ..Toolpath::default()
    };
    assert_eq!(verify(&straight, &ctx(&r, None, &tool, Mode::Boundary)), Vec::new());
    // Flattening a path to chords never shifts an index: crossing X,
    // one arm drawn as a full turn of chords-equivalent arc.
    let crossed = Toolpath {
        moves: vec![
            Move::new(MoveKind::Feed, p(4.0, 4.0, -1.0), p(16.0, 16.0, -1.0), Some(500.0)),
            Move::new(MoveKind::Feed, p(4.0, 16.0, -1.0), p(16.0, 4.0, -1.0), Some(500.0))
                .with_arc(p2(10.0, 10.0), true),
        ],
        ..Toolpath::default()
    };
    let warns = verify(&crossed, &ctx(&r, None, &tool, Mode::Area));
    assert!(warns.contains(&Warning::Crossing { moves: [0, 1] }), "{warns:?}");
}

#[test]
fn holder_hits_a_part_island_when_the_flute_is_short() {
    // Same pocket as the clean case with a 2 mm flute: the shank rides
    // below the island top, so the island is struck at the deepest
    // levels. part = true distinguishes it from uncut swarf.
    let r = plate_with_island();
    let stock = rect(0.0, 0.0, 30.0, 30.0);
    let t = pocket(
        &r,
        &PocketParams {
            links: camcraft_app::links::Links::PLANAR,
            tool_diameter: 8.0,
            stepover: 3.0,
            stock_to_leave: 0.0,
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -4.0,
            stepdown: 2.0,
            feed: 400.0,
            plunge_feed: 80.0,
            climb: true,
            strategy: PocketStrategy::Zigzag,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
            blend_turns: false,
        },
    )
    .unwrap();
    let tool = short_tool();
    let warns = verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Area));
    let hits: Vec<usize> = warns
        .iter()
        .filter_map(|w| match w {
            Warning::ShankHit { part: true, .. } => Some(1),
            _ => None,
        })
        .collect();
    assert!(!hits.is_empty(), "expected part-island ShankHit, got {warns:?}");
    assert!(warns.iter().all(|w| matches!(w, Warning::ShankHit { .. })), "{warns:?}");
}

#[test]
fn a_swinging_head_sweeps_the_island_wall_mid_turn() {
    // Law (c3), M5.1 stage G: the plant the endpoint laws cannot see.
    // The island wall stands at x=10, the tool column at x=4: the
    // shank (r 7 over an 8 mm flute) reaches 11 - a hair from the
    // wall's sampled column (10.05) when vertical, clear of it when
    // leaning away. The turns between the poses are the problem:
    //  - the plunge runs home to B-30 while descending to z=-20: mid
    //    path at turn fraction 0.0 the head is still vertical and the
    //    shank grazes the wall (law (c) only ever sees the -30 pose,
    //    and only at the to-points, where it misses);
    //  - the lateral creeps toward the wall while swinging B-30 to
    //    B+18: through fractions 0.25-0.75 the leaning column walks
    //    across the wall's annulus. Its stated pose at the to-point
    //    (tip x 6, leaning 18 degrees into the wall) puts the shank
    //    itself in the island: law (c) used to excuse that as the
    //    flutes' kerf, a disc it had moved under the leaning shank;
    //    since #903 the kerf stays at the tip and (c) convicts the
    //    move - (c)'s part verdict stands, so (c3) is not asked twice.
    let r = {
        let mut loops = vec![rect(0.0, 0.0, 30.0, 30.0).loops()[0].clone()];
        loops.push(
            Loop::new(vec![p2(10.0, 5.0), p2(20.0, 5.0), p2(20.0, 25.0), p2(10.0, 25.0)]).unwrap(),
        );
        Region::new(loops).unwrap()
    };
    let stock = rect(0.0, 0.0, 30.0, 30.0);
    let tool = ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 8.0,
        shank_radius: 7.0,
        shank_len: 6.0,
        holder_radius: 0.0,
        holder_len: 0.0,
    };
    let tilt = |deg: f64| [deg.to_radians().sin(), 0.0, deg.to_radians().cos()];
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-20.0, -20.0, 5.0), p(4.0, 15.0, 5.0), None),
        Move::new(MoveKind::Plunge, p(4.0, 15.0, 5.0), p(4.0, 15.0, -20.0), Some(80.0))
            .with_axis(tilt(-30.0)),
        Move::new(MoveKind::Feed, p(4.0, 15.0, -20.0), p(6.0, 15.0, -26.0), Some(200.0))
            .with_axis(tilt(18.0)),
    ]);
    let warns = verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Boundary));
    assert_eq!(
        warns,
        vec![
            Warning::ShankHit { at: 2, part: true },
            Warning::SweepHit { at: 1, part: true },
        ],
        "{warns:?}"
    );
}

#[test]
fn the_same_path_without_a_turn_convicts_under_c_not_c3() {
    // The twin: every pose stays home, so the identical geometry is
    // convicted by the endpoint law itself (the vertical shank does
    // reach the wall at the to-points) - and gains no SweepHit,
    // because a path that never changes pose has no mid-turn to
    // sweep. The kinematic law is blind when there is no kinematics.
    let r = {
        let mut loops = vec![rect(0.0, 0.0, 30.0, 30.0).loops()[0].clone()];
        loops.push(
            Loop::new(vec![p2(10.0, 5.0), p2(20.0, 5.0), p2(20.0, 25.0), p2(10.0, 25.0)]).unwrap(),
        );
        Region::new(loops).unwrap()
    };
    let stock = rect(0.0, 0.0, 30.0, 30.0);
    let tool = ToolGeom {
        radius: 4.0,
        nose_radius: 0.0,
        flute_len: 8.0,
        shank_radius: 7.0,
        shank_len: 6.0,
        holder_radius: 0.0,
        holder_len: 0.0,
    };
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-20.0, -20.0, 5.0), p(4.0, 15.0, 5.0), None),
        Move::new(MoveKind::Plunge, p(4.0, 15.0, 5.0), p(4.0, 15.0, -20.0), Some(80.0)),
        Move::new(MoveKind::Feed, p(4.0, 15.0, -20.0), p(6.0, 15.0, -26.0), Some(200.0)),
    ]);
    let warns = verify(&t, &ctx(&r, Some(&stock), &tool, Mode::Boundary));
    assert_eq!(
        warns,
        vec![
            Warning::ShankHit { at: 1, part: true },
            Warning::ShankHit { at: 2, part: true },
        ],
        "{warns:?}"
    );
}

#[test]
fn low_rapid_through_stock_is_flagged() {
    // A lateral rapid 1 mm under the stock top crossing the plate.
    let t = Toolpath {
        moves: vec![
            Move::new(MoveKind::Rapid, p(-5.0, 15.0, 5.0), p(-5.0, 15.0, -1.0), None),
            Move::new(MoveKind::Rapid, p(-5.0, 15.0, -1.0), p(25.0, 15.0, -1.0), None),
        ],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 30.0, 30.0);
    let tool = deep_tool();
    let warns = verify(&t, &ctx(&r, None, &tool, Mode::Area));
    assert_eq!(warns, vec![Warning::RapidInStock { at: 1 }]);
}

#[test]
fn engrave_paths_skip_the_gouge_law() {
    // Trace sits exactly on the boundary: d = 0 would "gouge" any tool.
    let r = rect(0.0, 0.0, 20.0, 20.0);
    let t = trace(
        &r,
        &TraceParams {
            clearance_z: 5.0,
            retract_z: None,
            top_z: 0.0,
            bottom_z: -1.0,
            stepdown: 1.0,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
        },
    )
    .unwrap();
    let tool = deep_tool();
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Engrave)), Vec::new());
}

#[test]
fn non_finite_moves_are_caught_defensively() {
    let t = Toolpath {
        moves: vec![Move::new(MoveKind::Feed, p(0.0, 0.0, 0.0), p(f64::NAN, 1.0, 0.0), Some(500.0))],
        ..Toolpath::default()
    };
    let r = rect(0.0, 0.0, 10.0, 10.0);
    let tool = deep_tool();
    let warns = verify(&t, &ctx(&r, None, &tool, Mode::Area));
    assert!(warns.contains(&Warning::NonFinite { at: 0 }), "{warns:?}");
}

#[test]
fn warnings_carry_readable_messages() {
    let s = Warning::Crossing { moves: [3, 9] }.to_string();
    assert!(s.contains("3") && s.contains("9"), "{s}");
    let s = Warning::ShankHit { at: 1, part: true }.to_string();
    assert!(s.contains("part"), "{s}");
}

fn cycle(xy_hole: Point2, entry_z: f64, bottom_z: f64, first_move: usize) -> Cycle {
    Cycle {
        kind: CycleKind::Drill,
        xy: xy_hole,
        r_z: entry_z,
        bottom_z,
        entry_z,
        peck: 0.0,
        dwell: 0.0,
        first_move,
    }
}

/// One hole at the middle of a big plate: rapid over, plunge, retract.
/// The plate edges sit 50 mm from the hole (outside every band annulus),
/// so the move laws stay silent at any depth and only the cycle reach
/// law can speak.
fn hole_in_the_middle(bottom_z: f64) -> Toolpath {
    let at = p(50.0, 50.0, 5.0);
    let mut path = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(-50.0, -50.0, 5.0), at, None),
        Move::new(MoveKind::Plunge, at, p(50.0, 50.0, bottom_z), Some(120.0)),
        Move::new(MoveKind::Rapid, p(50.0, 50.0, bottom_z), at, None),
    ]);
    path.cycles = vec![cycle(p2(50.0, 50.0), 5.0, bottom_z, 1)];
    path
}

#[test]
fn a_deeper_than_the_tool_hole_warns_from_its_cycle() {
    // Law (e): entry to bottom spans 136 mm; flute+shank+holder is 130.
    // The plunge itself provokes nothing - the reach lives in the cycle.
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let tool = deep_tool();
    let t = hole_in_the_middle(-131.0);
    assert_eq!(
        verify(&t, &ctx(&r, None, &tool, Mode::Boundary)),
        vec![Warning::ShankHit { at: 1, part: true }]
    );
}

#[test]
fn a_hole_within_reach_verifies_silent() {
    // Same chain, 10 mm short of the reach law: nothing fires - and
    // nothing may, because this is the shape every clean drill op makes.
    let r = rect(0.0, 0.0, 100.0, 100.0);
    let tool = deep_tool();
    let t = hole_in_the_middle(-115.0);
    assert_eq!(verify(&t, &ctx(&r, None, &tool, Mode::Boundary)), Vec::new());
}

// ---------------------------------------------------------------------------
// Mode::Envelope - the M2.2 law: MEASURE the swept ball against the mesh.
// The verdict must follow the stated tolerance, name the nearest move,
// and refuse to call silence a proof when the measurement cannot run.
// ---------------------------------------------------------------------------

use camcraft_app::finishing3d::{self, Finish3DParams};
use camcraft_app::geometry::Triangle;

fn cone_facets(n_lon: usize) -> Vec<Triangle> {
    let mut facets = Vec::new();
    let apex = p(0.0, 0.0, 20.0);
    let base: Vec<_> = (0..n_lon)
        .map(|j| {
            let th = std::f64::consts::TAU * j as f64 / n_lon as f64;
            p(20.0 * th.cos(), 20.0 * th.sin(), 0.0)
        })
        .collect();
    for j in 0..n_lon {
        facets.push(Triangle::new(apex, base[j], base[(j + 1) % n_lon]));
    }
    let center = p(0.0, 0.0, 0.0);
    for j in 0..n_lon {
        facets.push(Triangle::new(center, base[(j + 1) % n_lon], base[j]));
    }
    facets
}

fn ball() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        ..deep_tool()
    }
}

fn env_ctx<'a>(
    region: &'a Region,
    model: Option<&'a [Triangle]>,
    tool: &'a ToolGeom,
    finish_tol: Option<f64>,
) -> VerifyContext<'a> {
    VerifyContext {
        region,
        stock: None,
        model,
        tool,
        cutter: None,
        mode: Mode::Envelope,
        // At the planted feeds' rapid plane: law (d)'s rest-material
        // heuristic must not invent stock above where the rapids run
        // (the cone's own rapids sit at clearance, unaffected).
        stock_top_z: 15.0,
        clearance_z: 25.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    }
}

fn cone_waterline() -> Toolpath {
    finishing3d::waterline(
        &cone_facets(96),
        &Finish3DParams {
            pattern: finishing3d::FinishPattern::Waterline,
            stepover: None,
            tool_diameter: 6.0,
            tolerance: 0.1,
            clearance_z: 25.0,
            retract_z: None,
            top_z: 16.0,
            bottom_z: 0.5,
            feed: 500.0,
            plunge_feed: 100.0,
            climb: true,
            ramp: false,
            min_step: None,
            ramp_angle: 45.0,
            entry_ramp: false,
            steep_stepover: None,
            pencil_reference: None,
        },
    )
    .expect("cone waterline")
}

#[test]
#[ignore = "verdict lane: full envelope scan of cone_facets(96); release ~seconds - run at milestone close with -- --ignored"]
fn envelope_clean_path_verifies_silent() {
    // The 45-degree cone at its scallop budget: measured rest sits AT
    // the tolerance the stepdown was derived from, inside the
    // measurement's own quantization gate - the law's promise, kept.
    let f = cone_facets(96);
    let t = cone_waterline();
    let r = rect(-20.0, -20.0, 20.0, 20.0);
    let tool = ball();
    assert_eq!(verify(&t, &env_ctx(&r, Some(&f), &tool, Some(0.1))), Vec::new());
}

/// The same path, every point dropped by dz - a whole op set to the
/// wrong z, which is what a planted real deviation looks like.
fn sink(t: Toolpath, dz: f64) -> Toolpath {
    // Only the endpoints move: an arc's center is stated in XY (the Z
    // ride lives in the endpoints), so a pure z shift leaves the arc
    // - and the pose - exactly as authored. Flattening arcs to their
    // chords here would sink a path that never existed.
    Toolpath::new(
        t.moves
            .iter()
            .map(|m| Move {
                from: p(m.from.x, m.from.y, m.from.z - dz),
                to: p(m.to.x, m.to.y, m.to.z - dz),
                ..*m
            })
            .collect(),
    )
}

#[test]
#[ignore = "verdict lane: full envelope scan of cone_facets(96); release ~seconds - run at milestone close with -- --ignored"]
fn envelope_verdict_follows_the_stated_tol() {
    // Two halves, one law: the verifier judges against the user's
    // budget, and never against the measurement's own yardstick.
    // First half - the clean rings at tol 0.01 raise no envelope
    // verdict. An earlier
    // draft of this test read 0.1155 here and called it the
    // generator's chord error; it was the scan's - the cell/4 chord
    // sagitta E1's tol/4 budget retired (with ~0.010 of 96-gon
    // planarization). Tightening the yardstick must not invent a
    // defect, so the envelope's silence is the phantom-trench guard,
    // stated. What the tighter budget DOES read since #212 is the
    // rings' own spacing: a 45-degree cone laddered for tol 0.1 leaves
    // ~0.1 cusps between rings (0.085 at the lattice, at rho 16.8 on
    // the flank), past the 0.01 budget - the flank's reach used to
    // stop at 4·tol past the ball (0.04 here) and read nothing. That
    // is the non-blocking scallop note, real, and under the 0.1
    // promise the rings were laddered to.
    // Second half - what DOES follow the budget is a real deviation:
    // the same rings sunk 0.09, which on the 45-degree cone stands
    // ~0.064 of surface inside the envelope. Under the 0.1 promise
    // they were generated at, that is a kept promise (gate ~0.123);
    // at 0.01 the quantization floor (~0.033) still resolves it, and
    // the user's budget says otherwise - the verdict speaks.
    let f = cone_facets(96);
    let r = rect(-20.0, -20.0, 20.0, 20.0);
    let tool = ball();
    let t = cone_waterline();
    let tight = verify(&t, &env_ctx(&r, Some(&f), &tool, Some(0.01)));
    let [Warning::Scallop { mm, gate_mm, .. }] = tight.as_slice() else {
        panic!("a tighter yardstick must prove no defect it cannot resolve - and invent none: {tight:?}");
    };
    assert!(
        *mm > *gate_mm && *mm <= 0.1 + (3.0f64 / 8.0).powi(2) / 6.0,
        "the rings' own cusp, over the 0.01 budget and under the 0.1 they were laddered to: {tight:?}"
    );
    let sunk = sink(t, 0.09);
    let ws = verify(&sunk, &env_ctx(&r, Some(&f), &tool, Some(0.1)));
    assert!(ws.is_empty(), "0.064 of intrusion under a 0.1 promise is a promise kept: {ws:?}");
    let warns = verify(&sunk, &env_ctx(&r, Some(&f), &tool, Some(0.01)));
    let [Warning::Envelope { gouge_mm, at, .. }] = warns.as_slice() else {
        panic!("expected one envelope verdict, got {warns:?}");
    };
    assert!(*gouge_mm > 0.04, "the sunk ride breaches the 0.01 budget: {warns:?}");
    assert!(*gouge_mm < 0.12, "0.09 of center drop is ~0.064 of intrusion, not a trench: {warns:?}");
    assert!(*at >= 2, "the nearest cut, not a rapid: {warns:?}");
}

#[test]
fn envelope_gouge_names_the_nearest_move() {
    // A planted offender: a feed run straight THROUGH the solid at tip
    // z 5. Its centre line pierces the cube's side walls, so the facet
    // distance reads 0 there and the intrusion saturates at the radius
    // - 3 mm on exactly the move that made it. (The column law this
    // test was written against called the whole 5 mm trench the ball's
    // doing; facet distance measures what the envelope itself covers,
    // and its deepest possible reading is r, never more.)
    let f: Vec<Triangle> = camcraft_app::geometry::cube(p(-10.0, -10.0, 0.0), p(10.0, 10.0, 10.0)).facets.to_vec();
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(0.0, 0.0, 15.0), p(-15.0, 0.0, 15.0), None),
        Move::new(MoveKind::Plunge, p(-15.0, 0.0, 15.0), p(-15.0, 0.0, 5.0), Some(100.0)),
        Move::new(MoveKind::Feed, p(-15.0, 0.0, 5.0), p(15.0, 0.0, 5.0), Some(500.0)),
        Move::new(MoveKind::Rapid, p(15.0, 0.0, 5.0), p(15.0, 0.0, 15.0), None),
    ]);
    let r = rect(-10.0, -10.0, 10.0, 10.0);
    let tool = ball();
    let warns = verify(&t, &env_ctx(&r, Some(&f), &tool, Some(0.1)));
    let [Warning::Envelope { at, gouge_mm, .. }] = warns.as_slice() else {
        panic!("expected one envelope verdict, got {warns:?}");
    };
    assert_eq!(*at, 2, "the through-feed owns the gouge");
    assert!((gouge_mm - 3.0).abs() <= 0.05, "a buried centre reads exactly r: {gouge_mm}");
}

#[test]
fn an_envelope_that_cannot_measure_proves_nothing() {
    // No mesh or no budget: the scan cannot run, so nothing is proven -
    // and proves-not reports as a gouge at the first cut, never as
    // silence (the law (b) slice-proof precedent).
    let t = Toolpath::new(vec![
        Move::new(MoveKind::Rapid, p(20.0, 0.0, 15.0), p(25.0, 0.0, 15.0), None),
        Move::new(MoveKind::Feed, p(25.0, 0.0, 15.0), p(30.0, 0.0, 15.0), Some(500.0)),
    ]);
    let r = rect(-10.0, -10.0, 10.0, 10.0);
    let tool = ball();
    let f: Vec<Triangle> = camcraft_app::geometry::cube(p(-10.0, -10.0, 0.0), p(10.0, 10.0, 10.0)).facets.to_vec();
    assert_eq!(
        verify(&t, &env_ctx(&r, None, &tool, Some(0.1))),
        vec![Warning::Gouge { at: 1 }],
        "no mesh to measure"
    );
    assert_eq!(
        verify(&t, &env_ctx(&r, Some(&f), &tool, None)),
        vec![Warning::Gouge { at: 1 }],
        "no budget to measure against"
    );
}
