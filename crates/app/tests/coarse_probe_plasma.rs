//! #1053: probe and plasma on a model imported coarser than the importer
//! promises. #864 and #904 refused both outright; their own laws now give
//! the model's excess back out of their slack while a positive slack is
//! left, so they are judged - and past that the program gate refuses them
//! by name (the registry test in `coarse_laws` reads the gate for every
//! kind). The cone and engrave rides and the cross drill stay refused;
//! `fidelity_law`'s header says why.
//!
//! What "right" means here, per law: a reading between the tightened and
//! the plain slack is convicted on the coarse model and silent at full
//! fidelity; a real collision is convicted either way; past the bound the
//! law reads its plain slack (the gate's refusal stands in for it).
//!
//! Gate map (#1053): covers app::operation (probe_op's protected-move
//! law, plasma_op, fidelity_law), motion::plasma (kerf and cover laws),
//! app::model_fidelity (op_problem).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own box and plate fixtures (the probe_reach and
//! plasma suites' shapes) over our own laws; no vendor material.

use flecs_ecs::prelude::World;

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::{Loop, Point2, Region};
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::probe_op::{stylus_hits_excess, stylus_reach_hits_excess, stylus_slack, STYLUS_SLACK};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::plasma::{plasma_profile, PlasmaParams, KERF_TOL};
use camcraft_app::probing::{probe_cycle, ProbeDir, ProbeMode, ProbeParams};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{probe_tool, Tool};
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

/// A model flagged 0.012 mm: 0.007 mm past the 0.005 mm promise, which
/// leaves 0.003 mm of the 0.01 mm stylus slack and 0.018 mm of the
/// 0.025 mm kerf tolerance.
const EXCESS: f64 = 0.007;
const TIGHT: f64 = 0.012;

// ---- probe -----------------------------------------------------------------

fn edge(target: Point3, depth: f64) -> ProbeParams {
    ProbeParams {
        mode: ProbeMode::Edge,
        dir: ProbeDir::XPos,
        dir_y: ProbeDir::YPos,
        target,
        radius: 0.0,
        ball_diameter: 4.0,
        approach: 5.0,
        overtravel: 1.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth,
        inset: 5.0,
        clearance_z: 80.0,
    }
}

fn boxes(b: &[([f64; 3], [f64; 3])]) -> Vec<Triangle> {
    b.iter()
        .flat_map(|(lo, hi)| cube(Point3::new(lo[0], lo[1], lo[2]), Point3::new(hi[0], hi[1], hi[2])).facets.to_vec())
        .collect()
}

/// The whole protected-move law on a model `excess` coarser than promised.
fn judge(t: &Toolpath, tool: &Tool, facets: &[Triangle], excess: f64) -> (Vec<Warning>, Vec<Warning>) {
    let ball = stylus_hits_excess(t, tool.diameter * 0.5, facets, excess);
    let reach = stylus_reach_hits_excess(t, tool, None, facets, &ball, excess);
    (ball, reach)
}

#[test]
fn the_stylus_slack_gives_the_excess_back_and_reads_plain_past_the_bound() {
    assert_eq!(stylus_slack(0.0), STYLUS_SLACK, "full fidelity is untouched");
    assert!((stylus_slack(EXCESS) - (STYLUS_SLACK - EXCESS)).abs() < 1e-12);
    // Past the bound the law reads plain; the program gate refuses the
    // operation instead (`coarse_laws`' registry test, and below).
    assert_eq!(stylus_slack(0.012), STYLUS_SLACK);
    assert_eq!(stylus_slack(f64::NAN), STYLUS_SLACK, "a non-finite excess is no excess");
}

#[test]
fn a_ball_between_the_slacks_is_a_gouge_only_on_the_coarse_model() {
    // A 20 mm block touched on its -X face at X 0, the expected contact
    // set `x` inside the face: the ball ends `x` nearer the facets than
    // its radius.
    let block = boxes(&[([0.0, 0.0, 0.0], [40.0, 40.0, 20.0])]);
    let tool = probe_tool(4.0, 30.0);
    let at = |x: f64| probe_cycle(&edge(Point3::new(x, 20.0, 20.0), 3.0)).expect("edge");
    let gouged = |x: f64, excess: f64| judge(&at(x), &tool, &block, excess).0.iter().any(|w| matches!(w, Warning::Gouge { .. }));
    // The tangent touch is clean on either model.
    assert!(!gouged(0.0, 0.0) && !gouged(0.0, EXCESS), "a tangent touch stays clean");
    // 0.006 mm in: inside the plain 0.01 slack, past the tightened 0.003.
    assert!(!gouged(0.006, 0.0), "silent at full fidelity");
    assert!(gouged(0.006, EXCESS), "convicted on the coarse model");
    // A real gouge (0.05 mm in) is convicted either way.
    assert!(gouged(0.05, 0.0) && gouged(0.05, EXCESS));
}

#[test]
fn a_stem_between_the_slacks_is_a_shank_hit_only_on_the_coarse_model() {
    // The probe_reach overhang, its lip standing `lip` mm out past the
    // touched face from Z 12 up: the ball (top at Z 9) clears it, the
    // Ø4 stem riding tangent to the face below takes `lip` of it.
    let tool = probe_tool(4.0, 30.0);
    let t = probe_cycle(&edge(Point3::new(0.0, 20.0, 10.0), 3.0)).expect("edge");
    let hit = |lip: f64, excess: f64| {
        let facets = boxes(&[([0.0, 0.0, 0.0], [40.0, 40.0, 10.0]), ([-lip, 0.0, 12.0], [40.0, 40.0, 30.0])]);
        let (ball, reach) = judge(&t, &tool, &facets, excess);
        assert!(ball.is_empty(), "the ball clears the lip: {ball:?}");
        reach.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. }))
    };
    assert!(!hit(0.006, 0.0), "silent at full fidelity");
    assert!(hit(0.006, EXCESS), "convicted on the coarse model");
    assert!(hit(0.05, 0.0) && hit(0.05, EXCESS), "a real hit is convicted either way");
}

/// A probe document on a stepped block imported at `coarse_mm`: a lip
/// standing `lip` mm out over the touched face, as above.
fn probed(coarse_mm: Option<f64>, lip: f64) -> Vec<Warning> {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 40.0, dz: 30.0 }, Location::default()).expect("stock"));
    let mut facets = boxes(&[([0.0, 0.0, 0.0], [40.0, 40.0, 10.0])]);
    facets.extend(boxes(&[([-lip, 0.0, 12.0], [40.0, 40.0, 30.0])]));
    h.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(facets)).with_fidelity(coarse_mm));
    h.execute(&world, AddTool::new(&world, "PROBE-4", probe_tool(4.0, 30.0)).expect("tool"));
    let params = OpParams::Probe {
        clearance_z: Height::fixed(80.0),
        mode: ProbeMode::Edge,
        dir: ProbeDir::XPos,
        dir_y: ProbeDir::YPos,
        target: [0.0, 20.0, 10.0],
        radius: 0.0,
        approach: 5.0,
        overtravel: 1.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    };
    h.execute(&world, CreateOperation::new(&world, "Probe1", "PROBE-4", SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Probe1").expect("generate"));
    let e = model::find_by_name(&world, "Probe1").expect("op");
    model::toolpath_of(&world, &e).expect("cached").warnings
}

fn shank_hit(w: &[Warning]) -> bool {
    w.iter().any(|w| matches!(w, Warning::ShankHit { part: true, .. }))
}

#[test]
fn the_probe_op_judges_with_its_models_excess() {
    // The generator hands the law the model's excess: the same marginal
    // lip is silent on a full-fidelity import and refused on one flagged
    // 0.012 mm; a real lip is refused on both.
    assert!(!shank_hit(&probed(None, 0.006)), "{:?}", probed(None, 0.006));
    assert!(shank_hit(&probed(Some(TIGHT), 0.006)), "{:?}", probed(Some(TIGHT), 0.006));
    assert!(shank_hit(&probed(None, 0.05)) && shank_hit(&probed(Some(TIGHT), 0.05)));
}

// ---- plasma ----------------------------------------------------------------

fn torch(model_excess: f64) -> PlasmaParams {
    PlasmaParams {
        kerf: 1.5,
        control_comp: false,
        reversed: false,
        // No leads: the kerf law, not the lead's room, is what speaks.
        lead_in: 0.0,
        lead_out: 0.0,
        small_hole_dia: 32.0,
        small_hole_feed: 0.65,
        overburn: 0.0,
        thickness: 6.0,
        feed: Some(2500.0),
        clearance_z: 10.0,
        cut_z: 0.0,
        pierce_height: None,
        cut_height: None,
        pierce_delay: None,
        model_excess,
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).expect("rect")
}

/// A 100 x 60 plate cut on its drawn outline, the part it must keep
/// standing `grow` mm past that outline - the true edge beyond the
/// mesh's chord.
fn cut(grow: f64, excess: f64) -> Result<Toolpath, String> {
    let loops = vec![rect(0.0, 0.0, 100.0, 60.0)];
    let drawn = Region::new(loops.clone()).expect("plate");
    let part = if grow > 0.0 { drawn.offset(grow).expect("grown part") } else { drawn };
    plasma_profile(&loops, &[], &part, &torch(excess)).map_err(|e| e.to_string())
}

#[test]
fn the_kerf_tolerance_gives_the_excess_back_and_reads_plain_past_the_bound() {
    assert_eq!(torch(0.0).kerf_tol(), KERF_TOL);
    assert!((torch(EXCESS).kerf_tol() - (KERF_TOL - EXCESS)).abs() < 1e-12);
    assert_eq!(torch(0.03).kerf_tol(), KERF_TOL, "past the bound the law reads plain");
}

#[test]
fn a_kerf_between_the_tolerances_is_refused_only_on_the_coarse_model() {
    assert!(cut(0.0, 0.0).is_ok() && cut(0.0, EXCESS).is_ok(), "the drawn plate cuts on either model");
    // The part 0.02 mm past the outline: inside the plain 0.025 mm,
    // past the tightened 0.018 mm.
    assert!(cut(0.02, 0.0).is_ok(), "silent at full fidelity: {:?}", cut(0.02, 0.0).err());
    let why = cut(0.02, EXCESS).expect_err("refused on the coarse model");
    assert!(why.contains("would carry the kerf into the part"), "refused by the kerf law, by name: {why}");
    // A real reach into the part (0.1 mm) is refused either way.
    assert!(cut(0.1, 0.0).is_err() && cut(0.1, EXCESS).is_err());
}

// ---- the gate --------------------------------------------------------------

/// The fidelity clause's word on a probe or plasma op on a model
/// imported at `coarse_mm` (the registry test's fixture, shortened).
fn gate(params: OpParams, tool: (&str, Tool), coarse_mm: Option<f64>) -> Option<String> {
    let world: World = new_world();
    let mut h = History::default();
    let brick = cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0));
    h.execute(&world, ImportModel::from_mesh("bar", brick).with_fidelity(coarse_mm));
    h.execute(&world, AddTool::new(&world, tool.0, tool.1).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", tool.0, SrcRef::Entity("bar".into()), params).expect("create"));
    let op = model::operation_of(&world, &model::find_by_name(&world, "OP").expect("op")).expect("an op");
    camcraft_app::model_fidelity::op_problem(&world, "OP", &op)
}

#[test]
fn probe_and_plasma_are_judged_while_a_tolerance_is_left_and_refused_by_name_past_it() {
    let probe = || OpParams::Probe {
        clearance_z: Height::fixed(80.0),
        mode: ProbeMode::Face,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: [0.0, 0.0, 10.0],
        radius: 0.0,
        approach: 5.0,
        overtravel: 1.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: None,
        tolerance: 0.5,
    };
    let p = || ("PROBE-4", probe_tool(4.0, 30.0));
    assert_eq!(gate(probe(), p(), None), None);
    assert_eq!(gate(probe(), p(), Some(TIGHT)), None, "judged while the slack is left");
    let why = gate(probe(), p(), Some(0.03)).expect("refused past the bound");
    assert!(why.starts_with("OP: model 'bar' was imported too coarsely to verify"), "{why}");
}
