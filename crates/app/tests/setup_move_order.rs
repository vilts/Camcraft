//! #3022: moving operations into another setup keeps the program order
//! the user set. A moved operation lands at the end of its target setup,
//! so operations moved one by one keep their relative order; a move that
//! does not change the effective setup (implicit `None` to the first
//! setup) keeps its position. Undo gives back the old order exactly.
//!
//! Gate map (#3022): covers app::commands (SetOperationSetup), app::setup.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CommandError, CreateOperation, CreateSetup, History,
    ReorderOperation, SetOperationSetup, SetStock,
};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn run(
    world: &World,
    history: &mut History,
    cmd: Result<impl camcraft_app::commands::Command + 'static, CommandError>,
    what: &str,
) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e:?}")));
}

fn face(world: &World, history: &mut History, name: &str) {
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_radius: None,
        blend_turns: false,
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
    run(world, history, CreateOperation::new(world, name, TOOL, SrcRef::Stock, params), name);
}

fn base() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 };
    run(&world, &mut history, SetStock::new(&world, shape, Location::default()), "box stock");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    (world, history)
}

fn ops(world: &World) -> Vec<String> {
    setup::operation_order(world).into_iter().map(|(n, _)| n).collect()
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// The issue's case: operations made with no setup, in an order that is
/// not the name order, then moved one by one into a new named setup.
#[test]
fn moving_implicit_operations_into_a_named_setup_keeps_their_order() {
    let (world, mut history) = base();
    let program = ["Zeta", "Alpha", "Mid"];
    for name in program {
        face(&world, &mut history, name);
    }
    assert_eq!(ops(&world), names(&program));
    run(&world, &mut history, CreateSetup::new(&world, Some("PlateSetup"), None), "add setup");
    for name in program {
        run(&world, &mut history, SetOperationSetup::new(&world, name, Some("PlateSetup")), name);
        assert_eq!(ops(&world), names(&program), "after the move of {name}");
    }
    for _ in program {
        assert!(history.undo(&world), "undo a move");
        assert_eq!(ops(&world), names(&program));
    }
}

/// Two declared setups: operations moved from Front land after Back's own
/// operation, in the order of the moves. Undo restores both setups.
#[test]
fn moved_operations_append_to_the_target_setup_in_move_order() {
    let (world, mut history) = base();
    run(&world, &mut history, CreateSetup::new(&world, Some("Back"), None), "Back");
    face(&world, &mut history, "Resident");
    run(&world, &mut history, CreateSetup::new(&world, Some("Front"), None), "Front");
    run(&world, &mut history, ActivateSetup::new(&world, Some("Front")), "activate Front");
    let shape = StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 };
    run(&world, &mut history, SetStock::new(&world, shape, Location::default()), "Front stock");
    for name in ["Zeta", "Alpha", "Mid"] {
        face(&world, &mut history, name);
    }
    // Put Front in a user order that is neither creation nor name order.
    run(&world, &mut history, ReorderOperation::new(&world, "Mid", "Zeta", false), "Mid first");
    assert_eq!(ops(&world), names(&["Resident", "Mid", "Zeta", "Alpha"]));
    let before = ops(&world);

    for name in ["Mid", "Zeta", "Alpha"] {
        run(&world, &mut history, SetOperationSetup::new(&world, name, Some("Back")), name);
    }
    assert_eq!(ops(&world), names(&["Resident", "Mid", "Zeta", "Alpha"]));

    // Move Zeta back to Front and then into Back again: it lands last.
    run(&world, &mut history, SetOperationSetup::new(&world, "Zeta", Some("Front")), "out");
    run(&world, &mut history, SetOperationSetup::new(&world, "Zeta", Some("Back")), "in");
    assert_eq!(ops(&world), names(&["Resident", "Mid", "Alpha", "Zeta"]));

    for _ in 0..5 {
        assert!(history.undo(&world), "undo a move");
    }
    assert_eq!(ops(&world), before, "undo restores the old program order");
}
