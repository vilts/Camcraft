//! The program orchestrator (M2.3 stage B): `simulate_document` on a live
//! world, not a hand-built grid. The kernel's own laws are tested in
//! `stock_sim.rs`; what only THIS file can see is the wiring - ops walked
//! in posting order through `posting::program_ops`, cached moves read as
//! world millimeters, the stock's placement shifting the lattice (never
//! the moves: the sim lives in the world frame the viewport draws), and
//! every refusal naming its reason instead of judging garbage.
//!
//! Fixtures reuse the posting test's command walk (settle: a fresh,
//! silent world). The three verdict fixtures share one law: a 60x60x20
//! box stock, faced at z 18 by a 4 mm tool - against a model top of 18
//! that is clean, 20 an undercut (gouge), 16 a standing layer (rest),
//! each 2 mm exactly, located.
//!
//! Gate map (M4.4): covers app::commands, app::geometry, motion::machining2d, app::model, app::operation, app::posting, app::schema, geometry::geometry::stock, app::stock_sim, sim::stock_sim, tools::tools, app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube, quat_from_yaw};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, STOCK_NAME};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{DEV_TOL, simulate_document};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn catalog_tool(name: &str) -> camcraft_app::tools::Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn face_params(cut_z: f64) -> OpParams {
    OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Stock centered on `at` (box stock's own law: XY centered, z from
/// the placement up), a model box of `model_top` on the same
/// footprint, one Face op faced to z 18 - generated and fresh, the way
/// a document must be for the sim (or for the post) to run it.
fn faced_doc(at: Point3, model_top: f64) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let loc = Location { pos: at, rot: IDENTITY_Q };
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, loc);
    history.execute(&world, cmd.expect("box stock"));
    let cmd = AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("catalog tool"));
    let lo = Point3::new(at.x - 30.0, at.y - 30.0, 0.0);
    let hi = Point3::new(at.x + 30.0, at.y + 30.0, model_top);
    history.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
    let cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Stock,
        face_params(18.0),
    );
    history.execute(&world, cmd.expect("create face"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    let e = model::find_by_name(&world, "Face1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    assert!(cache.warnings.is_empty(), "fixture must generate silent: {:?}", cache.warnings);
    (world, history)
}

fn origin() -> Point3 {
    Point3::new(0.0, 0.0, 0.0)
}

#[test]
fn a_faced_program_is_judged_clean() {
    // The program's whole job was to bring the stock to the model's
    // top, and it did: every column reads |dev| <= DEV_TOL. This is
    // the verdict the Release 0.2.0 story rests on - "clean" must
    // mean exactly that, at the display tolerance, on a real world.
    let (world, _h) = faced_doc(origin(), 18.0);
    let sim = simulate_document(&world).expect("a fresh program simulates");
    assert!(sim.dev.is_clean(), "faced to the model top must be clean: {:?}", &sim.dev.dev[..8]);
    assert_eq!(sim.dev.rest_cells, 0);
    assert_eq!(sim.dev.gouge_cells, 0);
    assert_eq!(sim.dev.tol, DEV_TOL);
    assert!(sim.surface.verts.len() >= 6, "the sheet carries the grid's top");
    assert!(sim.surface.vcls.iter().all(|c| *c == 0.0), "clean paints as stock, not as a warning");
}

#[test]
fn an_undercut_program_locates_its_gouge() {
    // Faced to 18 where the model wants 20: a 2 mm plane cut too deep,
    // everywhere, and the verdict says WHERE.
    let (world, _h) = faced_doc(origin(), 20.0);
    let sim = simulate_document(&world).expect("simulates");
    assert!(!sim.dev.is_clean());
    assert!(sim.dev.gouge_cells > 0 && sim.dev.rest_cells == 0);
    assert!((sim.dev.max_gouge - 2.0).abs() < 1e-6, "the gouge is the whole 2 mm layer");
    assert!(sim.dev.gouge_volume > 0.0);
    let at = sim.dev.max_gouge_at.expect("a located verdict");
    assert!((at.z - 18.0).abs() < 1e-6, "the sim's surface at the worst gouge is the cut floor");
    assert!(at.x.abs() <= 30.1 && at.y.abs() <= 30.1, "inside the centered footprint: {at:?}");
    // The class channel agrees with the numbers: the sheet carries red.
    assert!(sim.surface.vcls.contains(&2.0));
}

#[test]
fn an_overcut_program_locates_its_rest() {
    // Faced to 18 where the model stands only 16 tall: 2 mm of stock
    // never reached. Monotonic subtraction's other face - rest is
    // what the program did not touch.
    let (world, _h) = faced_doc(origin(), 16.0);
    let sim = simulate_document(&world).expect("simulates");
    assert!(!sim.dev.is_clean());
    assert!(sim.dev.rest_cells > 0 && sim.dev.gouge_cells == 0);
    assert!((sim.dev.max_rest - 2.0).abs() < 1e-6);
    let at = sim.dev.max_rest_at.expect("a located verdict");
    assert!((at.z - 18.0).abs() < 1e-6, "rest is reported at the sim surface, not the model");
    assert!(sim.surface.vcls.contains(&1.0));
}

#[test]
fn the_lattice_moves_with_the_stock() {
    // Stage B's placement law: the sim shifts the GRID by the stock's
    // world position (moves stay world millimeters, the same numbers
    // the viewport draws). A twin 100,50 mm away must judge the same
    // verdict at the same offset - counts equal, location shifted.
    let (wa, _ha) = faced_doc(origin(), 16.0);
    let (wb, _hb) = faced_doc(Point3::new(100.0, 50.0, 0.0), 16.0);
    let a = simulate_document(&wa).expect("origin twin");
    let b = simulate_document(&wb).expect("shifted twin");
    assert_eq!(a.dev.rest_cells, b.dev.rest_cells, "translation cannot change the count");
    assert!((a.dev.max_rest - b.dev.max_rest).abs() < 1e-12);
    let (pa, pb) = (a.dev.max_rest_at.expect("a"), b.dev.max_rest_at.expect("b"));
    assert!((pb.x - pa.x - 100.0).abs() < 1e-6, "the verdict follows the stock: {pa:?} -> {pb:?}");
    assert!((pb.y - pa.y - 50.0).abs() < 1e-6);
}

#[test]
fn a_rotated_stock_is_refused_not_resampled() {
    // The axis-aligned law: an off-axis dexel grid cannot hold a
    // rotated solid, and silently ignoring the rotation would be the
    // silent wrong answer. The refusal names the law.
    let (world, mut history) = faced_doc(origin(), 18.0);
    let stock = model::find_by_name(&world, STOCK_NAME).expect("stock");
    model::set_transform(&world, &stock, Location { pos: Point3::ZERO, rot: quat_from_yaw(0.7) });
    // Rotate, then regenerate: the digest makes the op stale exactly as
    // it must (a rotated stock is a real setup change), and ONLY once
    // the program validates fresh does the sim get to apply its own
    // axis-aligned law - the refusal the user sees, not the gate's.
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate rotated"));
    let err = simulate_document(&world).expect_err("a rotated stock cannot be judged");
    assert!(err.contains("axis-aligned"), "{err}");
}

#[test]
fn missing_pieces_refuse_by_name() {
    // Empty document: the same refusal posting gives, by construction.
    let world = new_world();
    let err = simulate_document(&world).expect_err("nothing to judge");
    let same = posting::program_ops(&world).expect_err("same law, one copy");
    assert_eq!(err, same, "the sim's gate is the post's gate, verbatim");
    assert!(err.contains("no operations"), "{err}");

    // Operations but no stock: the sim judges material, not air. The
    // op must not read the stock - a stock-sourcing face would go
    // STALE at removal and the gate would refuse before the sim's own
    // refusal is ever reached. A model-sourced contour with fixed
    // heights is that op: it never asks the setup for anything.
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 5.0))),
    );
    let cmd = AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL"));
    history.execute(&world, cmd.expect("tool"));
    let params = OpParams::Contour { tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(5.0),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    let cmd = CreateOperation::new(&world, "Contour1", "EM-0600-4FL", SrcRef::Entity("part".into()), params);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Contour1").expect("generate without stock"));
    let err = simulate_document(&world).expect_err("air is not stock");
    // The M3.2 A wording: the refusal names WHICH setup stock is
    // missing ("no stock 'Stock'") - the law phrase, not the old
    // literal, so per-setup naming and this gate stay the same text.
    assert!(err.contains("no stock") && err.contains("judges material"), "{err}");
}

#[test]
fn an_unborn_or_stale_op_refuses_the_sim_exactly_like_the_post() {
    // The sharing law, made load-bearing: simulate_document walks
    // posting::program_ops, so the sim's gate IS the post's gate.
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, cmd.expect("stock"));
    let cmd = AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("tool"));
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params(18.0));
    history.execute(&world, cmd.expect("create"));
    // Never generated.
    let err = simulate_document(&world).expect_err("no toolpath yet");
    assert!(err.contains("Face1") && err.contains("generate it first"), "{err}");
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    // Stale it through the stock's placement - a millimeter up is a
    // setup change the digest reads (the same staleness the rotated
    // stock test feels), and the sim refuses on precisely what the
    // post refuses on.
    let stock = model::find_by_name(&world, STOCK_NAME).expect("stock");
    model::set_transform(&world, &stock, Location { pos: Point3::new(0.0, 0.0, 1.0), rot: IDENTITY_Q });
    let err = simulate_document(&world).expect_err("stale blocks the sim");
    assert!(err.contains("Face1") && err.contains("regenerate"), "{err}");
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("regenerate"));
    simulate_document(&world).expect("fresh again simulates");
}

#[test]
fn from_model_stock_judges_like_the_analytic_twin() {
    // The FromModel leg's contract (no from_shape, no rotation math):
    // stock copied FROM the model box, faced 2 mm under its own top,
    // is an undercut the same size as the analytic twin's.
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(60.0, 60.0, 20.0))));
    let cmd = SetStock::new(
        &world,
        StockShape::FromModel { source: "part".into() },
        Default::default(),
    );
    history.execute(&world, cmd.expect("stock from model"));
    let cmd = AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL"));
    history.execute(&world, cmd.expect("tool"));
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params(18.0));
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let sim = simulate_document(&world).expect("facet stock simulates");
    assert!(sim.dev.gouge_cells > 0, "faced 2 under the model top: an undercut");
    assert!((sim.dev.max_gouge - 2.0).abs() < 1e-6);
}

#[test]
fn a_bull_nose_face_leaves_its_corner_scallops() {
    // M13.1: the document sim sweeps a bull nose as itself. A 10 mm
    // tool faces to the model top at a 9.5 mm stepover: the square
    // end mill's disc overlaps pass to pass (clean), while the bull's
    // flat disc is only 8 mm wide - the 1 mm corners leave a ridge
    // between passes that the disc atom used to erase silently.
    let judged = |tool: &str| {
        let world = new_world();
        let mut history = History::default();
        let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
        history.execute(&world, cmd.expect("box stock"));
        history.execute(&world, AddTool::new(&world, tool, catalog_tool(tool)).expect("catalog tool"));
        let (lo, hi) = (Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 18.0));
        history.execute(&world, ImportModel::from_mesh("part", cube(lo, hi)));
        let mut params = face_params(18.0);
        if let OpParams::Face { stepover, .. } = &mut params {
            *stepover = 9.5;
        }
        let cmd = CreateOperation::new(&world, "Face1", tool, SrcRef::Stock, params);
        history.execute(&world, cmd.expect("create face"));
        history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
        simulate_document(&world).expect("the program simulates")
    };
    let flat = judged("EM-1000-4FL");
    let bull = judged("BB-1000-R1");
    assert_eq!(flat.dev.rest_cells, 0, "the square disc overlaps: clean");
    assert!(bull.dev.rest_cells > 0, "the bull's corners leave ridges between passes");
    // The ridge is the corner's own arc: never higher than the corner.
    assert!(bull.dev.max_rest > DEV_TOL && bull.dev.max_rest <= 1.0 + 1e-9, "ridge {}", bull.dev.max_rest);
    assert_eq!(bull.dev.gouge_cells, 0, "a corner never cuts deeper than the floor");
}
