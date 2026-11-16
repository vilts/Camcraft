//! #1362: undoing `DeleteSetup` must give back the program exactly as it
//! was - the setup in its old place in the setup order, and therefore
//! the operations and the posted program in their old order. Redo takes
//! it out again. The setup order is the name order (`setup::setups`), so
//! a re-spawned setup lands where its name sorts, wherever it was
//! created; these tests pin that through the post, so a later change to
//! how the order is stored cannot lose a position on undo unnoticed.
//!
//! Gate map (#1362): covers app::commands (DeleteSetup, CreateSetup),
//! app::setup, app::posting.

use std::path::PathBuf;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CommandError, CreateOperation, CreateSetup, DeleteSetup,
    GenerateOperation, History, SetStock,
};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
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

fn box_stock(world: &World, history: &mut History) {
    let shape = StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 };
    run(world, history, SetStock::new(world, shape, Location::default()), "box stock");
}

fn face(world: &World, history: &mut History, name: &str) {
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    run(world, history, CreateOperation::new(world, name, TOOL, SrcRef::Stock, params), name);
    run(world, history, GenerateOperation::new(world, name), "generate");
}

/// Three setups, each with its own box stock. The first and the last machine
/// one Face each; the middle one is empty, so it may be deleted. The names
/// are created out of name order ("Middle" last) to show which order is
/// the law: the restored setup comes back where its name sorts, not at
/// the end of the creation sequence.
fn three_setups() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    box_stock(&world, &mut history);
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    run(&world, &mut history, CreateSetup::new(&world, Some("A_First"), None), "A_First");
    face(&world, &mut history, "Face_A");
    run(&world, &mut history, CreateSetup::new(&world, Some("C_Last"), None), "C_Last");
    box_stock(&world, &mut history);
    face(&world, &mut history, "Face_C");
    run(&world, &mut history, CreateSetup::new(&world, Some("B_Middle"), None), "B_Middle");
    box_stock(&world, &mut history);
    run(&world, &mut history, ActivateSetup::new(&world, Some("A_First")), "activate first");
    (world, history)
}

fn ops(world: &World) -> Vec<String> {
    setup::operation_order(world).into_iter().map(|(n, _)| n).collect()
}

/// The posted program's text, in a per-test temp file.
fn posted(world: &World, tag: &str) -> String {
    let path: PathBuf = std::env::temp_dir().join(format!("camcraft-1362-{}-{tag}.nc", std::process::id()));
    posting::post_setup(world, &path).unwrap_or_else(|e| panic!("post refused ({tag}): {e}"));
    let text = std::fs::read_to_string(&path).expect("posted file");
    let _ = std::fs::remove_file(&path);
    text
}

#[test]
fn undoing_delete_of_a_middle_setup_restores_its_place_and_the_program() {
    let (world, mut history) = three_setups();
    let names = vec!["A_First".to_owned(), "B_Middle".to_owned(), "C_Last".to_owned()];
    assert_eq!(setup::names(&world), names);
    assert_eq!(ops(&world), vec!["Face_A".to_owned(), "Face_C".to_owned()]);
    let before = posted(&world, "before");
    let a = before.find("Face_A").expect("Face_A is posted");
    let c = before.find("Face_C").expect("Face_C is posted");
    assert!(a < c, "the first setup's operation posts first:\n{before}");

    run(&world, &mut history, DeleteSetup::new(&world, "B_Middle"), "delete middle");
    assert_eq!(setup::names(&world), vec!["A_First".to_owned(), "C_Last".to_owned()]);

    assert!(history.undo(&world), "undo the delete");
    assert_eq!(setup::names(&world), names, "undo puts the setup back at its index");
    assert_eq!(setup::active(&world).as_deref(), Some("A_First"));
    assert_eq!(ops(&world), vec!["Face_A".to_owned(), "Face_C".to_owned()]);
    assert_eq!(posted(&world, "undone"), before, "the posted program is unchanged by delete + undo");

    assert!(history.redo(&world), "redo the delete");
    assert_eq!(setup::names(&world), vec!["A_First".to_owned(), "C_Last".to_owned()]);
    assert!(history.undo(&world), "undo again");
    assert_eq!(setup::names(&world), names);
    assert_eq!(posted(&world, "undone-twice"), before);
}

#[test]
fn undoing_delete_of_the_first_setup_puts_it_back_first() {
    // The first setup is the one `None` resolves to, so its place matters
    // to every undeclared reference: it must come back first, not last.
    let (world, mut history) = three_setups();
    run(&world, &mut history, DeleteSetup::new(&world, "B_Middle"), "delete middle");
    let world_names = setup::names(&world);
    assert_eq!(world_names.first().map(String::as_str), Some("A_First"));
    // Move Face_A out so A_First is empty, then delete it and undo.
    run(
        &world,
        &mut history,
        camcraft_app::commands::SetOperationSetup::new(&world, "Face_A", Some("C_Last")),
        "move Face_A",
    );
    run(&world, &mut history, DeleteSetup::new(&world, "A_First"), "delete first");
    assert_eq!(setup::first(&world).as_deref(), Some("C_Last"));
    assert!(history.undo(&world), "undo the delete");
    assert_eq!(setup::first(&world).as_deref(), Some("A_First"));
    assert_eq!(setup::names(&world), vec!["A_First".to_owned(), "C_Last".to_owned()]);
}
