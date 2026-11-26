//! M10.1 C: the honest after-state. A tool deleted out from under an
//! operation, its name reused by something that is not a tool, or (in an
//! old file) a tool whose kind the op's family cannot cut with, leaves the
//! op visible and BLOCKED by name: generation refuses before any
//! generator runs, the post refuses through `program_ops` (the one door
//! the sim and the machine check share), and undo / a fitting reassignment
//! clears it. An old file loads untouched and round-trips byte-identical.
//!
//! Provenance: our own command-layer design (AGENTS.md Prime Directive:
//! refuse visibly, never cut silently; format = compatibility contract);
//! no vendor material consulted.
//!
//! Gate map (M4.4): covers app::operation::tool_gate (tool_problem),
//! app::commands (GenerateOperation, RegenerateAll, DeleteTool,
//! DeleteEntity, EditOperationLinks), app::posting (program_ops,
//! post_setup), app::project (save/open).
//! Lane shape (M4.4): all tests here run in well under a second.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, DeleteEntity, DeleteTool, EditOperationLinks,
    GenerateOperation, History, ImportModel, RegenerateAll, SetStock,
};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef, tool_gate};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

const GONE: &str =
    "Face1's tool 'EM-0400-2FL' is gone; a Face operation cuts with a flat end mill or bull-nose";

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn face() -> OpParams {
    OpParams::Face { holes: Default::default(),
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
    }
}

/// Box stock, one flat end mill, Face1 on the stock - generated, fresh,
/// and passing the post's door.
fn faced() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    let em = catalog_tool("EM-0400-2FL");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", em).expect("tool"));
    let create = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face());
    h.execute(&world, create.expect("an end mill faces"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    assert!(posting::program_ops(&world).is_ok(), "the healthy program passes the door");
    (world, h)
}

fn face1(world: &World) -> flecs_ecs::prelude::Entity {
    model::find_by_name(world, "Face1").expect("the op stays visible")
}

fn generate_refusal(world: &World) -> String {
    match GenerateOperation::new(world, "Face1") {
        Ok(_) => panic!("a blocked op must not generate"),
        Err(CommandError::Machining(msg)) => msg,
        Err(other) => panic!("expected the machining refusal, got {other}"),
    }
}

fn tmp_path(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-after-state-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

#[test]
fn a_deleted_tool_blocks_generation_and_the_post_by_name() {
    let (world, mut h) = faced();
    h.execute(&world, DeleteTool::new(&world, "EM-0400-2FL").expect("delete"));
    let op = face1(&world);
    assert!(model::toolpath_of(&world, &op).is_some(), "visible: the old cache is not destroyed");
    assert_eq!(tool_gate::tool_problem_of(&world, &op).as_deref(), Some(GONE));
    assert_eq!(generate_refusal(&world), GONE, "refused by name, before any generator");
    let regen = RegenerateAll::new(&world);
    assert!(regen.is_empty(), "the batch skips it too");
    assert_eq!(regen.skipped(), [(String::from("Face1"), format!("could not generate the toolpath: {GONE}"))]);
    let door = posting::program_ops(&world).expect_err("a blocked op never passes the door");
    assert!(door.contains(GONE), "{door}");
    let nc = tmp_path("gone.nc");
    let posted = posting::post_setup(&world, &nc).map(|p| p.ops).expect_err("no post");
    assert!(posted.contains(GONE), "{posted}");
    assert!(!nc.exists(), "nothing partial is ever written");
}

#[test]
fn undoing_the_delete_clears_the_warning() {
    let (world, mut h) = faced();
    h.execute(&world, DeleteTool::new(&world, "EM-0400-2FL").expect("delete"));
    assert!(tool_gate::tool_problem_of(&world, &face1(&world)).is_some());
    assert!(h.undo(&world));
    let op = face1(&world);
    assert_eq!(tool_gate::tool_problem_of(&world, &op), None, "the world is mended");
    assert_eq!(operation::status_of(&world, &op), OpStatus::Ok, "the old cache is good again");
    assert!(posting::program_ops(&world).is_ok());
    GenerateOperation::new(&world, "Face1").expect("generation opens again");
    // The tree's own delete door (DeleteEntity) meets the same law.
    h.execute(&world, DeleteEntity::new(&world, "EM-0400-2FL").expect("tree delete"));
    assert_eq!(tool_gate::tool_problem_of(&world, &face1(&world)).as_deref(), Some(GONE));
    assert!(h.undo(&world));
    assert_eq!(tool_gate::tool_problem_of(&world, &face1(&world)), None);
}

#[test]
fn a_name_reused_by_a_solid_is_no_longer_a_tool() {
    let (world, mut h) = faced();
    h.execute(&world, DeleteTool::new(&world, "EM-0400-2FL").expect("delete"));
    let block = cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0));
    h.execute(&world, ImportModel::from_mesh("EM-0400-2FL", block));
    let why = tool_gate::tool_problem_of(&world, &face1(&world)).expect("blocked");
    assert_eq!(
        why,
        "Face1's tool 'EM-0400-2FL' is no longer a tool; a Face operation cuts with a flat end mill or bull-nose"
    );
    assert_eq!(generate_refusal(&world), why);
}

/// An old file, written before the tool law: Face1's cached path and its
/// stamp were taken against a tool that is a grooving blade. Built here
/// the only way such a file can arise now - the tool swapped out from
/// under the op, then the stored stamp set to what that world digests
/// (exactly what a pre-law build's generation would have written).
/// `tag` keeps each caller's temp file its own: the suite's tests run
/// in parallel threads of one process, and a shared path races.
fn pre_law_file(tag: &str) -> String {
    let (world, mut h) = faced();
    h.execute(&world, DeleteTool::new(&world, "EM-0400-2FL").expect("delete"));
    let blade = catalog_tool("GT-0200");
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", blade).expect("blade"));
    let op = model::operation_of(&world, &face1(&world)).expect("op");
    let old = op.stamp.expect("generated").0;
    let then = operation::current_stamp(&world, &op.tool, &op.src, &op.params, "Face1", None)
        .expect("the blade digests")
        .0;
    let path = tmp_path(&format!("pre-law-{tag}.camproj"));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    let (from, to) = (format!("\"stamp\": {old}"), format!("\"stamp\": {then}"));
    assert_eq!(text.matches(&from).count(), 1, "one stamp on the wire: {text}");
    text.replace(&from, &to)
}

#[test]
fn an_old_file_with_a_mismatched_tool_loads_blocked_and_round_trips() {
    let text = pre_law_file("roundtrip");
    let world = new_world();
    project::open(&world, &text).expect("an old file loads as it is - no migration, no refusal");
    let op = face1(&world);
    assert_eq!(
        operation::status_of(&world, &op),
        OpStatus::Ok,
        "the stamp matches: only the tool law can see the problem"
    );
    let why = "Face1's tool 'EM-0400-2FL' is a grooving blade; a Face operation cuts with a flat end mill or bull-nose";
    assert_eq!(tool_gate::tool_problem_of(&world, &op).as_deref(), Some(why), "surfaces on read");
    assert_eq!(generate_refusal(&world), why);
    let door = posting::program_ops(&world).expect_err("a fresh cache behind a wrong tool never posts");
    assert!(door.contains(why), "{door}");
    // Byte-identical: loading rewrote nothing.
    let path = tmp_path("resave.camproj");
    project::save(&world, &path).expect("resave");
    let again = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(again, text, "the project file round-trips untouched");
}

#[test]
fn a_fitting_reassignment_clears_the_block() {
    let world = new_world();
    project::open(&world, &pre_law_file("reassign")).expect("load");
    let mut h = History::default();
    h.execute(&world, AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("add"));
    let relink = EditOperationLinks::new(&world, "Face1", Some("EM-0600-4FL"), None);
    h.execute(&world, relink.expect("the fix is a fitting tool"));
    assert_eq!(tool_gate::tool_problem_of(&world, &face1(&world)), None);
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generates again"));
    assert!(posting::program_ops(&world).is_ok());
}
