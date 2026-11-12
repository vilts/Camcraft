//! #261 part 3: a refused generation hands back the moves it had built
//! as a display-only ghost - and an operation with a ghost is still an
//! operation with NO toolpath for every law: nothing is cached, the
//! post refuses it as "no toolpath yet", and the program simulation
//! has nothing of it to play. The ghost rides the command's error only.
//!
//! Provenance: our own tests over our own refusal model; no vendor
//! material consulted.
//!
//! Gate map (#261): covers app::commands (operation::generate),
//! app::operation (generate), app::posting, app::stock_sim, sim::stock_sim, app::script.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CommandError, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::geo2d::Point2;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use camcraft_motion::approach::CENTER_CUT_REFUSAL;

/// A 60 x 60 x 20 box and a 10 mm slot cut with a 10 mm end mill that
/// is not centre-cutting: the slot is as wide as the tool, so no helix
/// or ramp fits at a pass start and the ladder refuses mid-way.
fn slot_world(center_cutting: bool) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default()).expect("box"));
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog entry");
    h.execute(&world, AddTool::new(&world, "EM", Tool { center_cutting, ..em }).expect("tool"));
    let slot = OpParams::Slot {
        a: Point2::new(10.0, 10.0),
        b: Point2::new(50.0, 20.0),
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 3.0,
        stepover: 5.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        entry: Entry3D::Plunge,
        helix_radius: 1.5,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
    };
    h.execute(&world, CreateOperation::new(&world, "SLT", "EM", SrcRef::Stock, slot).expect("create"));
    world
}

fn refusal(world: &World) -> CommandError {
    match GenerateOperation::new(world, "SLT") {
        Ok(_) => panic!("the slot was meant to refuse"),
        Err(e) => e,
    }
}

#[test]
fn a_refused_slot_hands_back_its_built_moves_in_world() {
    let e = refusal(&slot_world(false));
    assert!(e.to_string().contains(CENTER_CUT_REFUSAL), "refused by the law: {e}");
    let g = e.ghost().expect("the moves built before the refusal ride it");
    assert!(!g.is_empty() && !g.truncated(), "{g:?}");
    // World coordinates: the ghost stands over the slot, inside the box.
    for m in g.moves() {
        for p in [m.from, m.to] {
            assert!((0.0..=60.0).contains(&p.x) && (0.0..=60.0).contains(&p.y), "{p:?}");
        }
    }
    assert!(format!("{e:?}").contains("Ghost("), "Debug names the ghost's size only: {e:?}");
}

#[test]
fn an_op_with_a_ghost_has_no_toolpath_and_the_post_and_the_sim_refuse_it() {
    let world = slot_world(false);
    let e = refusal(&world);
    assert!(e.ghost().is_some());
    let op = model::find_by_name(&world, "SLT").expect("op");
    // Never cached: the operation has no path, no stamp.
    assert!(model::toolpath_of(&world, &op).is_none(), "a ghost is never a toolpath");
    assert_eq!(operation::status_of(&world, &op), OpStatus::Never);
    // The post's program gate refuses it exactly as an ungenerated op.
    let post = camcraft_app::posting::program_ops(&world).expect_err("nothing to post");
    assert!(post.contains("SLT: no toolpath yet"), "{post}");
    // And the program simulation has nothing to play.
    assert!(camcraft_app::stock_sim::simulate_document(&world).is_err(), "no path, no simulation");
}

#[test]
fn a_centre_cutting_tool_generates_the_slot_and_carries_no_ghost() {
    // Not vacuous: the same slot is a real path for a tool that may plunge.
    let world = slot_world(true);
    let cmd = GenerateOperation::new(&world, "SLT").expect("a centre-cutting slot");
    let mut h = History::default();
    h.execute(&world, cmd);
    let op = model::find_by_name(&world, "SLT").expect("op");
    assert!(model::toolpath_of(&world, &op).is_some());
}

#[test]
fn the_script_and_mcp_refusal_names_the_ghosts_size_not_its_moves() {
    let world = slot_world(false);
    let e = refusal(&world);
    let n = e.ghost().map(|g| g.len()).expect("ghost");
    let text = camcraft_app::script::refusal_text("SLT", &e);
    assert!(text.starts_with(&format!("SLT - {e}")), "{text}");
    assert!(text.contains(&format!("a partial path of {n} move(s) was built before the refusal; it is not kept")), "{text}");
    // A refusal with no ghost reads exactly as before.
    let plain = CommandError::Machining("X: nope".into());
    assert_eq!(camcraft_app::script::refusal_text("X", &plain), format!("X - {plain}"));
}
