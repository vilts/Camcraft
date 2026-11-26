//! Provenance: issue #3020, original circular-bore fixtures and elementary cone geometry.
//! Gate map: covers camcraft_sim::verify (section and section::radius).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::verify::{Cutter, Mode, ToolGeom, VerifyContext, Warning, verify};

fn tool() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: 3.0,
        shank_len: 40.0,
        holder_radius: 10.0,
        holder_len: 60.0,
    }
}

fn cone() -> Cutter {
    Cutter { tip_angle: 120.0, ..Cutter::flat(&tool()) }
}

fn bore() -> Region {
    let points = (0..64).map(|i| {
        let angle = std::f64::consts::TAU * i as f64 / 64.0;
        Point2::new(2.5 * angle.cos(), 2.5 * angle.sin())
    }).collect();
    Region::new(vec![Loop::new(points).expect("circle loop")]).expect("bore region")
}

fn plunge(depth: f64) -> Move {
    Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 12.0), Point3::new(0.0, 0.0, 12.0 - depth), Some(80.0))
}

fn path(moves: Vec<Move>) -> Toolpath {
    Toolpath { moves, ..Default::default() }
}

fn judge(path: &Toolpath, cutter: Option<Cutter>, rims: bool, model: Option<&[Triangle]>, mode: Mode) -> Vec<Warning> {
    judge_at_top(path, cutter, rims, model, mode, 12.0)
}

fn judge_at_top(path: &Toolpath, cutter: Option<Cutter>, rims: bool, model: Option<&[Triangle]>, mode: Mode, stock_top_z: f64) -> Vec<Warning> {
    let region = bore();
    let geometry = tool();
    let declared = [(Point2::new(0.0, 0.0), 2.5)];
    verify(path, &VerifyContext {
        region: &region,
        tool: &geometry,
        cutter,
        model,
        mode,
        stock_top_z,
        clearance_z: 27.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        rims: if rims { &declared } else { &[] },
        ..Default::default()
    })
}

fn has_gouge(warnings: &[Warning]) -> bool {
    warnings.iter().any(|warning| matches!(warning, Warning::Gouge { .. }))
}

#[test]
fn shallow_axial_cone_uses_its_radius_at_stock_top() {
    let down = plunge(1.0);
    let up = Move::new(MoveKind::Feed, down.to, down.from, Some(80.0));
    for movement in [down, up] {
        let warnings = judge(&path(vec![movement]), Some(cone()), true, None, Mode::Boundary);
        assert!(!has_gouge(&warnings), "a radius 1.732 mm cone clears the 2.5 mm bore: {warnings:?}");
    }
}

#[test]
fn a_cone_wider_than_the_bore_still_gouges_before_and_after_full_radius() {
    // At depth 1.5, radius is 2.598; at depth 2, the full 3 mm radius enters.
    for depth in [1.5, 2.0] {
        let warnings = judge(&path(vec![plunge(depth)]), Some(cone()), true, None, Mode::Boundary);
        assert!(has_gouge(&warnings), "depth {depth} enlarges the finished bore: {warnings:?}");
    }
}

#[test]
fn untrusted_or_missing_cone_geometry_keeps_the_nominal_radius() {
    let mut mismatch = cone();
    mismatch.flute_len += 1.0;
    let ball = Cutter { corner_r: 3.0, ..Cutter::flat(&tool()) };
    for cutter in [None, Some(Cutter::flat(&tool())), Some(mismatch), Some(ball)] {
        let warnings = judge(&path(vec![plunge(1.0)]), cutter, true, None, Mode::Boundary);
        assert!(has_gouge(&warnings), "only a matching real cone may clear the nominal radius: {warnings:?}");
    }
}

#[test]
fn a_declared_drill_rim_and_boundary_context_are_required() {
    for (rims, model, mode) in [
        (false, None, Mode::Boundary),
        (true, None, Mode::Area),
    ] {
        let warnings = judge(&path(vec![plunge(1.0)]), Some(cone()), rims, model, mode);
        assert!(has_gouge(&warnings), "other wall contexts keep their nominal radius: {warnings:?}");
    }
}

#[test]
fn lateral_and_arc_feeds_keep_the_nominal_radius() {
    let lateral = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 11.0), Point3::new(0.2, 0.0, 11.0), Some(80.0));
    let arc = plunge(1.0).with_arc(Point2::new(0.2, 0.0), false);
    for movement in [lateral, arc] {
        let warnings = judge(&path(vec![movement]), Some(cone()), true, None, Mode::Boundary);
        assert!(has_gouge(&warnings), "the axial cone rule does not clear a lateral or arc feed: {warnings:?}");
    }
}

#[test]
fn explicit_or_modal_tool_axes_keep_the_nominal_radius() {
    let mut upright = plunge(1.0);
    upright.axis = Some([0.0, 0.0, 1.0]);
    let warnings = judge(&path(vec![upright]), Some(cone()), true, None, Mode::Boundary);
    assert!(has_gouge(&warnings), "an explicit pose keeps the posed verdict: {warnings:?}");

    let mut tilted = plunge(1.0);
    tilted.axis = Some([0.6, 0.0, 0.8]);
    let warnings = judge(&path(vec![tilted]), Some(cone()), true, None, Mode::Boundary);
    assert!(has_gouge(&warnings), "a tilted cone needs the posed law: {warnings:?}");

    // An omitted axis inherits the last pose; it does not reset to vertical.
    tilted.kind = MoveKind::Rapid;
    tilted.from = Point3::new(0.0, 0.0, 27.0);
    tilted.to = Point3::new(0.0, 0.0, 12.0);
    let warnings = judge(&path(vec![tilted, plunge(1.0)]), Some(cone()), true, None, Mode::Boundary);
    assert!(warnings.iter().any(|warning| matches!(warning, Warning::Gouge { at: 1 })), "the feed inherits the earlier tilted pose: {warnings:?}");
}

#[test]
fn nonfinite_stock_top_never_clears_the_nominal_radius() {
    for top in [f64::NAN, f64::INFINITY] {
        let warnings = judge_at_top(&path(vec![plunge(1.0)]), Some(cone()), true, None, Mode::Boundary, top);
        assert!(has_gouge(&warnings), "stock top {top} cannot prove a reduced cone radius: {warnings:?}");
    }
}

fn judge_with_part_top(movement: Move, rim_top_z: f64) -> Vec<Warning> {
    let region = bore();
    let mut geometry = tool();
    geometry.radius = 4.0;
    geometry.shank_radius = 4.0;
    let cutter = Cutter { tip_angle: 90.0, ..Cutter::flat(&geometry) };
    let rims = [(Point2::new(0.0, 0.0), 2.5)];
    verify(&path(vec![movement]), &VerifyContext {
        region: &region,
        tool: &geometry,
        cutter: Some(cutter),
        mode: Mode::Boundary,
        stock_top_z: 13.0,
        rim_top_z: Some(rim_top_z),
        clearance_z: 27.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        rims: &rims,
        ..Default::default()
    })
}

#[test]
fn extra_stock_above_the_part_does_not_enlarge_the_spot_footprint() {
    let warnings = judge_with_part_top(plunge(2.0), 12.0);
    assert!(!has_gouge(&warnings), "the 2 mm cone radius clears the 2.5 mm bore at the part top: {warnings:?}");
}

#[test]
fn a_spot_that_cuts_the_part_wall_still_gouges_below_extra_stock() {
    let warnings = judge_with_part_top(plunge(3.0), 12.0);
    assert!(has_gouge(&warnings), "the 3 mm cone radius cuts the 2.5 mm bore: {warnings:?}");
}

#[test]
fn nonfinite_part_top_never_clears_the_nominal_radius() {
    for top in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let warnings = judge_with_part_top(plunge(2.0), top);
        assert!(has_gouge(&warnings), "part top {top} cannot prove a reduced cone radius: {warnings:?}");
    }
}

#[test]
fn a_part_top_does_not_clear_a_lateral_feed() {
    let lateral = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 10.0), Point3::new(0.2, 0.0, 10.0), Some(80.0));
    let warnings = judge_with_part_top(lateral, 12.0);
    assert!(has_gouge(&warnings), "a lateral feed keeps the nominal cutter radius: {warnings:?}");
}
