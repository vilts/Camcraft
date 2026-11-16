//! Issue #396 at the app level: a lathe holder mesh that models its
//! insert plate reads the plate's edge, cutting its own chip, as a rub;
//! an insert zone about the tip leaves the plate out of the judge. The
//! zone never hides the holder: a shank that rubs the shoulder is still
//! convicted with it set, and a zone larger than one of the tool's
//! inserts, or holding the whole body, is refused by name.
//!
//! Gate map (M4.4): covers camcraft_tools::lathe_insert_zone (through
//! the app), camcraft_app::tool_body_lathe (build, fits, the zone's
//! caps), commands::tool_body (LatheBodySpec::zone,
//! SetToolBody::insert_zone), project::tool_body_wire (`insert_zone`).
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: our own fixtures, specced from the turning kernel's
//! design notes and ISO insert sizes; no vendor material consulted.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{ChangeTool, CommandError, CreateOperation, GenerateOperation, History, ImportModel, LatheBodySpec, LoadMachine, SetStock, SetToolBody};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting, project, tool_body};
use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

use common::bore::*;

const INSERT: &str = "TN-1600-0R8";
const AS_MODELLED: LathePlacement = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };
/// A zone about the nose holding the plate below, with room to spare.
const ZONE: InsertZone = InsertZone { lo: [-1.0, -5.0, -1.5], hi: [6.5, 0.5, 6.5] };

/// A closed axis-aligned box over `[lo, hi]`, 12 facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
    let c = |i: usize| Point3::new(if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] });
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))]).collect()
}

/// The insert plate a vendor assembly models with its holder: a 4.76
/// thick slab under the rake face, its bottom on the nose circle, its
/// leading edge 1.3 ahead of the nose centre toward the chuck.
fn plate() -> Vec<Triangle> {
    cuboid([-0.8, -4.76, -1.3], [6.0, 0.0, 6.0])
}

/// The plate on a shank whose foot stands `foot` along the spindle from
/// the tip (0.5 clears the shoulder; -8 reaches it).
fn assembly(foot: f64, zone: Option<InsertZone>) -> LatheBodySpec {
    let mut facets = plate();
    facets.extend(cuboid([2.0, -6.0, foot], [40.0, 6.0, 10.0]));
    LatheBodySpec { source: "assembly.step".into(), facets, place: AS_MODELLED, zone }
}

/// The stepped part (r 12 below z 0, r 6 above to z 20), the insert,
/// and a Ø24 bar from z -16 to 20 when `bar`.
fn od_world(bar: bool) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    if bar {
        let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
        let shape = camcraft_app::stock::StockShape::Cylinder { diameter: 24.0, height: 36.0 };
        history.execute(&world, SetStock::new(&world, shape, at).expect("bar stock"));
    }
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, INSERT);
    let stepped = revolve(&[(0.0, -16.0), (12.0, -16.0), (12.0, 0.0), (6.0, 0.0), (6.0, 20.0), (0.0, 20.0)]);
    history.execute(&world, ImportModel::from_mesh("step", stepped));
    (world, history)
}

fn rough() -> OpParams {
    OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

fn finish() -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

fn op(world: &World, history: &mut History, name: &str, params: OpParams) {
    history.execute(world, CreateOperation::new(world, name, INSERT, SrcRef::Entity("step".into()), params).expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn regen(world: &World, history: &mut History, names: &[&str]) {
    for n in names {
        history.execute(world, GenerateOperation::new(world, n).expect("regenerate"));
    }
}

fn holder_hits(world: &World, op: &str) -> Vec<(usize, bool, f64)> {
    cache(world, op).expect("cached").warnings.iter().filter_map(|w| match *w {
        Warning::HolderHit { at, part, short_mm } => Some((at, part, short_mm)),
        _ => None,
    }).collect()
}

fn post(world: &World, tag: &str) -> Result<(), String> {
    let out = std::env::temp_dir().join(format!("camcraft-zone-396-{tag}-{}.nc", std::process::id()));
    let r = posting::post_setup(world, &out);
    let _ = std::fs::remove_file(&out);
    r.map(|_| ())
}

fn refuse(r: Result<SetToolBody, CommandError>, says: &str) {
    match r {
        Err(CommandError::Invalid(m)) => assert!(m.contains(says), "'{says}' not in: {m}"),
        Ok(_) => panic!("accepted a zone that should say '{says}'"),
        Err(e) => panic!("{e:?}"),
    }
}

fn zone_of(world: &World) -> Option<InsertZone> {
    match tool_body::body_of(world, &model::find_by_name(world, INSERT).expect("tool"))?.frame {
        tool_body::BodyFrame::Lathe { zone, .. } => zone,
        tool_body::BodyFrame::Spindle { .. } => None,
    }
}

#[test]
fn a_mesh_with_its_insert_plate_is_refused_on_a_plain_od_pass_and_posts_clean_with_the_zone() {
    // The plain OD finish down the neck, no bar: the shank alone clears
    // (turning_holder's own fixture), so only the plate can rub - its
    // box reaches the nose circle's bottom, on the finished skin.
    let (world, mut history) = od_world(false);
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(assembly(0.5, None))).expect("the assembly"));
    op(&world, &mut history, "F1", finish());
    assert!(!holder_hits(&world, "F1").is_empty(), "the plate reads as a rub without a zone");
    let e = post(&world, "plate").expect_err("a plate read as a rub blocks the post");
    assert!(e.contains("holder body at move"), "{e}");

    // The zone over the plate, as one undoable command.
    history.execute(&world, SetToolBody::insert_zone(&world, INSERT, Some(ZONE)).expect("a zone one insert fills"));
    // Undo takes the zone back off (mesh and placement kept); redo
    // lands it again.
    history.undo(&world);
    assert_eq!(zone_of(&world), None);
    assert!(tool_body::body_of(&world, &model::find_by_name(&world, INSERT).expect("tool")).is_some(), "the body stays");
    history.redo(&world);
    assert_eq!(zone_of(&world), Some(ZONE));
    regen(&world, &mut history, &["F1"]);
    assert!(holder_hits(&world, "F1").is_empty(), "{:?}", cache(&world, "F1").map(|c| c.warnings));
    post(&world, "zoned").expect("the zoned assembly posts clean");

    // And the spec route lands the same body as the sibling command.
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(assembly(0.5, Some(ZONE)))).expect("zoned spec"));
    assert_eq!(zone_of(&world), Some(ZONE));
}

#[test]
fn a_real_shoulder_rub_is_still_convicted_with_the_zone_set() {
    // The shank's foot reaches 8 below the tip: at the ride's foot it
    // stands at r 8.8 over the r 12 shoulder - the zone over the plate
    // takes nothing of that away.
    let (world, mut history) = od_world(false);
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(assembly(-8.0, Some(ZONE)))).expect("zoned assembly"));
    op(&world, &mut history, "F1", finish());
    let hits = holder_hits(&world, "F1");
    assert!(!hits.is_empty() && hits.iter().all(|h| h.1), "the shoulder rub went unnamed: {hits:?}");
    let worst = hits.iter().map(|h| h.2).fold(0.0, f64::max);
    assert!((worst - (12.0 - 8.8)).abs() < 0.02, "shortfall {worst}");
    let e = post(&world, "shoulder").expect_err("the rub blocks the post");
    assert!(e.contains("rubs the part"), "{e}");
}

#[test]
fn a_zone_larger_than_an_insert_or_holding_the_body_is_refused_by_name() {
    let (world, mut history) = od_world(false);
    add_tool(&world, &mut history, "GT-0200");
    // No lathe body yet: nothing to zone.
    refuse(SetToolBody::insert_zone(&world, INSERT, Some(ZONE)), "carries no lathe body");
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(assembly(-8.0, None))).expect("the assembly"));
    // Reaching up the shank to cover its shoulder: past the 16 mm body
    // the insert sits on, by either route.
    refuse(SetToolBody::insert_zone(&world, INSERT, Some(InsertZone { hi: [6.5, 0.5, 20.0], ..ZONE })), "reaches 20.000 mm from the tip along z, past the 16.000 mm");
    refuse(SetToolBody::lathe(&world, INSERT, Some(assembly(-8.0, Some(InsertZone { lo: [-1.0, -5.0, -30.0], ..ZONE })))), "past the 16.000 mm");
    // Nor is there a zone to clear on a tool with no lathe body.
    refuse(SetToolBody::insert_zone(&world, "GT-0200", None), "carries no lathe body");
    refuse(SetToolBody::lathe(&world, INSERT, Some(assembly(-8.0, Some(InsertZone { lo: [-1.0, -5.0, -1.0], hi: [6.5, 0.5, f64::NAN] })))), "finite");
    refuse(SetToolBody::insert_zone(&world, INSERT, Some(InsertZone { lo: [0.5, -5.0, -1.0], ..ZONE })), "must hold the tip");
    // The plate alone, wholly inside its zone: nothing left to judge.
    let alone = LatheBodySpec { source: "plate.step".into(), facets: plate(), place: AS_MODELLED, zone: Some(ZONE) };
    refuse(SetToolBody::lathe(&world, INSERT, Some(alone)), "holds the whole lathe body");
    // A 2 mm grooving blade's zone along the spindle is its band.
    let blade = LatheBodySpec { source: "blade.step".into(), facets: cuboid([0.0, -0.9, -1.0], [40.0, 0.9, 1.0]), place: AS_MODELLED, zone: Some(InsertZone { lo: [-0.5, -1.0, -1.5], hi: [5.0, 1.0, 1.5] }) };
    refuse(SetToolBody::lathe(&world, "GT-0200", Some(blade)), "past the 1.000 mm");
    // A ChangeTool to a smaller insert body than the zone needs refuses too.
    history.execute(&world, SetToolBody::insert_zone(&world, INSERT, Some(ZONE)).expect("zone"));
    let mut small = model::tool_of(&world, &model::find_by_name(&world, INSERT).expect("tool")).expect("a tool");
    small.diameter = 6.0;
    small.nose_radius = 0.4;
    match ChangeTool::new(&world, INSERT, small) {
        Err(CommandError::Invalid(m)) => assert!(m.contains("insert zone"), "{m}"),
        Ok(_) => panic!("a tool too small for its zone was accepted"),
        Err(e) => panic!("{e:?}"),
    }
}

#[test]
fn the_insert_zone_round_trips_through_the_project_file() {
    let (world, mut history) = od_world(false);
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(assembly(0.5, Some(ZONE)))).expect("zoned assembly"));
    let path = std::env::temp_dir().join(format!("camcraft_insert_zone_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = camcraft_app::world::new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    assert_eq!(first, second, "save-load-save is byte-identical");
    assert!(first.contains("\"insert_zone\""), "the zone is on file");
    let body = |w: &World| tool_body::body_of(w, &model::find_by_name(w, INSERT).expect("tool"));
    assert_eq!(body(&world2), body(&world), "facets, placement, zone and bands all agree");
    // A zone too large for the tool on file refuses the load by name.
    let mut doc: serde_json::Value = serde_json::from_str(&first).expect("json");
    let zone = doc
        .pointer_mut("/entities/0/tool/body/insert_zone/hi/2")
        .expect("the zone sits on the insert's body");
    *zone = serde_json::json!(20.0);
    let wild = serde_json::to_string_pretty(&doc).expect("json");
    std::fs::write(&path, wild).expect("write");
    let world3 = camcraft_app::world::new_world();
    let e = project::load(&world3, &path).expect_err("a wild zone on file");
    let _ = std::fs::remove_file(&path);
    assert!(format!("{e:?}").contains("insert zone reaches 20.000 mm"), "{e:?}");
}


#[test]
fn in_a_bar_the_plate_cutting_its_own_chip_is_a_stock_rub_and_the_zone_clears_it() {
    // Rough then finish in a Ø24 bar: the plate's leading edge, 1.3
    // ahead of the nose centre, stands in the shell the insert is
    // cutting. Without a zone that is a rub; with it the holder law has
    // nothing to say (the shank trails in the rough's cut).
    let (world, mut history) = od_world(true);
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(assembly(0.5, None))).expect("the assembly"));
    op(&world, &mut history, "R1", rough());
    op(&world, &mut history, "F1", finish());
    let today: Vec<_> = holder_hits(&world, "R1").into_iter().chain(holder_hits(&world, "F1")).collect();
    assert!(today.iter().any(|h| !h.1), "the plate's chip went unread as a stock rub: {today:?}");
    history.execute(&world, SetToolBody::insert_zone(&world, INSERT, Some(ZONE)).expect("zone"));
    regen(&world, &mut history, &["R1", "F1"]);
    assert!(holder_hits(&world, "R1").is_empty(), "{:?}", cache(&world, "R1").map(|c| c.warnings));
    assert!(holder_hits(&world, "F1").is_empty(), "{:?}", cache(&world, "F1").map(|c| c.warnings));
}
