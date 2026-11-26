//! #1241: a toolpath generated on machine A, then remounted onto a
//! machine B whose limits it breaks - by the setup's own machine
//! (`SetSetupMachine`) and, separately, by the document's
//! (`LoadMachine`). The trace answers the issue's three questions for
//! each law (stroke, spindle top, a tilting rotary, the machine's kind):
//! (1) the op stays Ok, by the M2.3 roomy-one law - machine limits are
//! live gate inputs, never stamp inputs (`fixtures.rs`
//! `workholding_never_stales_a_fresh_toolpath` pins the same law for
//! steel); (2) the verification gate judges the cached path against B,
//! here and now; (3) the post refuses by name and writes nothing. Undo
//! puts A back and the program posts again; redo and a save/reopen keep
//! B's refusal.
//! Provenance: our own fixtures over our own commands; every machine
//! number is invented here or read from our own shipped `.mch` files.
//!
//! Gate map (#1241): covers app::commands (LoadMachine, SetSetupMachine),
//! app::machine (check_ops), app::posting (verification_gate, post_setup,
//! family_at_post), app::operation (status_of), app::project (save/open).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine, SetSetupMachine,
    SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef, Tilt};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// Which mount the switch goes through.
#[derive(Clone, Copy, Debug)]
enum Via {
    /// The setup's own machine (`SetSetupMachine`).
    Setup,
    /// The document's machine, which the setup follows (`LoadMachine`).
    Document,
}

const SETUP: &str = "Setup1";

fn mill(model: &str, stroke: f64, spindle_max: f64) -> Machine {
    let axis = |n: &str| AxisLimits {
        name: n.into(),
        feed: Some(5000.0),
        rapid: Some(15000.0),
        travel: Some((-stroke, stroke)),
        ..Default::default()
    };
    Machine {
        model: model.into(),
        vendor: "CamCraft Testbench".into(),
        spindle_min: Some(50.0),
        spindle_max: Some(spindle_max),
        axes: vec![axis("X"), axis("Y"), axis("Z")],
        spindles: Some(1),
        ..Default::default()
    }
}

/// Machine A: roomy, a 24000 rpm top. Machine B: a +/-10 mm stroke and
/// a 1000 rpm top - the 60 mm face breaks both.
fn roomy() -> Machine {
    mill("RoomyMill", 200.0, 24000.0)
}
fn tight() -> Machine {
    mill("TightMill", 10.0, 1000.0)
}

fn mount(world: &World, h: &mut History, via: Via, path: &str, m: Machine) {
    match via {
        Via::Setup => h.execute(world, SetSetupMachine::new(world, SETUP, Some((path.into(), Some(m)))).expect("own")),
        Via::Document => h.execute(world, LoadMachine::new(world, path.into(), Some(m)).expect("document")),
    }
}

fn mount_builtin(world: &World, h: &mut History, via: Via, name: &str) {
    match via {
        Via::Setup => h.execute(world, SetSetupMachine::builtin(world, SETUP, name).expect("own builtin")),
        Via::Document => h.execute(world, LoadMachine::builtin(world, name).expect("document builtin")),
    }
}

fn add_tool(world: &World, h: &mut History, name: &str) {
    let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool");
    h.execute(world, AddTool::new(world, name, tool).expect("add tool"));
}

/// One setup, a 60x60x20 stock, one Face op generated on `a` (mounted
/// through `via` BEFORE generation, so the path is A's).
fn faced_on(via: Via, a: Machine) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    add_tool(&world, &mut h, "EM-0400-2FL");
    mount(&world, &mut h, via, "a.mch", a);
    let face = OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate on A"));
    (world, h)
}

/// The post_not_judged suite's parked 90-degree pocket (every cutting
/// move leans), generated on the shipped tilt-head through `via`.
fn tilted_on(via: Via) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
    add_tool(&world, &mut h, "EM-0600-4FL");
    h.execute(
        &world,
        // #1603: the part is what the stock-source pocket leaves - the
        // stock's last 10 mm along the tool, so the cut stops on it (a
        // part filling the stock would be cut, and convicted for it).
        ImportModel::from_mesh("part", cube(Point3::new(0.0, 0.0, -20.0), Point3::new(10.0, 30.0, -10.0))),
    );
    let loc = Location { pos: Point3::new(20.0, 15.0, -20.0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 30.0, dz: 10.0 }, loc).expect("stock"));
    mount_builtin(&world, &mut h, via, "Tilt-head 5-axis");
    let params = OpParams::Pocket { pass_start: Default::default(), rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        tilt: Some(Tilt { basis_version: 1, a_deg: 0.0, b_deg: 90.0, origin: [-25.0, 0.0, -15.0] }),
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(80.0),
        retract_z: Some(Height::fixed(70.0)),
        top_z: Height::fixed(50.0),
        bottom_z: Height::fixed(35.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 100.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        corner_relief: Default::default(),
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "P", "EM-0600-4FL", SrcRef::Stock, params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "P").expect("generate on the tilt-head"));
    (world, h)
}

fn status(world: &World, op: &str) -> OpStatus {
    operation::status_of(world, &model::find_by_name(world, op).expect("op"))
}

fn tmp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-1241-{tag}-{}.nc", std::process::id()))
}

/// Post into a scratch file: `Ok` when it posts, the refusal otherwise
/// - and a refusal must write nothing.
fn post(world: &World, tag: &str) -> Result<(), String> {
    let path = tmp(tag);
    let _ = std::fs::remove_file(&path);
    let out = posting::post_setup(world, &path).map(|_| ());
    if out.is_err() {
        assert!(!path.exists(), "{tag}: a refused post writes nothing");
    }
    let _ = std::fs::remove_file(&path);
    out
}

fn reopened(world: &World, tag: &str) -> World {
    let path = std::env::temp_dir().join(format!("camcraft-1241-{tag}-{}.ccam", std::process::id()));
    project::save(world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    let w = new_world();
    project::open(&w, &text).expect("reopen");
    w
}

/// The refusal names machine B and every law the path breaks there.
fn refused_on_tight(err: &str, tag: &str) {
    assert!(err.contains("machine TightMill:"), "{tag}: the refusal names B: {err}");
    assert!(err.contains("Face1: tool EM-0400-2FL runs") && err.contains("above the machine's 1000"), "{tag}: rpm: {err}");
    assert!(err.contains("Face1: move ") && err.contains("the stroke is -10.000..10.000"), "{tag}: stroke: {err}");
}

fn stroke_and_spindle(via: Via) {
    let tag = format!("{via:?}");
    let (world, mut h) = faced_on(via, roomy());
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "{tag}: fresh on A");
    post(&world, &format!("{tag}-a")).unwrap_or_else(|e| panic!("{tag}: A takes the program: {e}"));

    mount(&world, &mut h, via, "b.mch", tight());
    assert_eq!(camcraft_app::machine::of_setup(&world, Some(SETUP)).1.map(|m| m.model), Some("TightMill".into()));
    // (1) Not stale: the roomy-one law - the machine is a live gate input.
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "{tag}: a machine switch never stales the path");
    // (2) The gate judges the cached path against B now.
    let gate = posting::verification_gate(&world).join("; ");
    refused_on_tight(&gate, &format!("{tag} gate"));
    // (3) The post refuses by name.
    let err = post(&world, &format!("{tag}-b")).expect_err("A's path must not post on B unjudged");
    refused_on_tight(&err, &format!("{tag} post"));

    // Undo puts A back, staleness unmoved, and the program posts again.
    h.undo(&world);
    assert_eq!(status(&world, "Face1"), OpStatus::Ok, "{tag}: undo");
    assert!(posting::verification_gate(&world).is_empty(), "{tag}: A's gate is clean again");
    post(&world, &format!("{tag}-undo")).unwrap_or_else(|e| panic!("{tag}: posts after undo: {e}"));
    // Redo: B again, refused again.
    h.redo(&world);
    let err = post(&world, &format!("{tag}-redo")).expect_err("redo remounts B");
    refused_on_tight(&err, &format!("{tag} redo"));
    // Save and reopen on B: the reopened document refuses the same way.
    let w = reopened(&world, &tag);
    assert_eq!(status(&w, "Face1"), OpStatus::Ok, "{tag}: reopened fresh");
    let err = post(&w, &format!("{tag}-reopen")).expect_err("a reopened B document must refuse");
    refused_on_tight(&err, &format!("{tag} reopen"));
}

#[test]
fn a_setups_new_machine_judges_stroke_and_spindle_at_the_post() {
    stroke_and_spindle(Via::Setup);
}

#[test]
fn the_documents_new_machine_judges_stroke_and_spindle_at_the_post() {
    stroke_and_spindle(Via::Document);
}

fn rotary(via: Via) {
    let tag = format!("{via:?}-rotary");
    let (world, mut h) = tilted_on(via);
    assert_eq!(status(&world, "P"), OpStatus::Ok, "{tag}: fresh on the tilt-head");
    assert!(posting::verification_gate(&world).is_empty(), "{tag}: the tilt-head holds the pose");
    post(&world, &format!("{tag}-a")).unwrap_or_else(|e| panic!("{tag}: the tilt-head posts it: {e}"));

    mount_builtin(&world, &mut h, via, "VMC 3-axis");
    assert_eq!(status(&world, "P"), OpStatus::Ok, "{tag}: not stale (roomy-one law)");
    let err = post(&world, &format!("{tag}-b")).expect_err("a leaning path must not post on a 3-axis machine");
    assert!(err.contains("P: move ") && err.contains("tilts the tool") && err.contains("states no tilting rotary"), "{tag}: {err}");

    h.undo(&world);
    post(&world, &format!("{tag}-undo")).unwrap_or_else(|e| panic!("{tag}: posts after undo: {e}"));
    h.redo(&world);
    let err = post(&reopened(&world, &tag), &format!("{tag}-reopen")).expect_err("reopened on the VMC");
    assert!(err.contains("states no tilting rotary"), "{tag}: {err}");
}

#[test]
fn a_setups_new_machine_without_the_rotary_refuses_the_lean() {
    rotary(Via::Setup);
}

#[test]
fn the_documents_new_machine_without_the_rotary_refuses_the_lean() {
    rotary(Via::Document);
}

fn family(via: Via) {
    let tag = format!("{via:?}-family");
    let (world, mut h) = faced_on(via, roomy());
    mount_builtin(&world, &mut h, via, "Lathe");
    let err = post(&world, &format!("{tag}-b")).expect_err("a milling op must not post on a lathe");
    assert!(err.contains("Face1: a ") && err.contains("cannot run on its setup's"), "{tag}: {err}");
    h.undo(&world);
    post(&world, &format!("{tag}-undo")).unwrap_or_else(|e| panic!("{tag}: posts after undo: {e}"));
}

#[test]
fn a_setups_new_machine_of_another_kind_refuses_the_family() {
    family(Via::Setup);
}

#[test]
fn the_documents_new_machine_of_another_kind_refuses_the_family() {
    family(Via::Document);
}
