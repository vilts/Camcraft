//! #1431: the browser build's Save and Post have no path to write, so the
//! app hands them the bytes instead - `project::save_text` and
//! `posting::post_setup_unwritten`. Pinned here: each is byte for byte
//! what the desktop's `save` / `post_setup` write, and the unwritten post
//! refuses exactly where the written one does.
//! Provenance: our own tests over our own gate; no HSMWorks material.
//!
//! Gate map (M4.4): covers app::posting, app::project.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project};

fn face(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
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

/// A box stock and one generated face op: a program the gate posts.
fn one_face() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0600-4FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0600-4FL", tool).expect("catalog tool"));
    let cmd = CreateOperation::new(&world, "Face1", "EM-0600-4FL", SrcRef::Stock, face(19.5));
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    (world, history)
}

fn temp(tag: &str, ext: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-unwritten-{tag}-{}.{ext}", std::process::id()))
}

#[test]
fn the_unwritten_post_is_the_written_program_byte_for_byte() {
    let (world, _h) = one_face();
    let path = temp("post", "nc");
    let written = posting::post_setup(&world, &path).expect("the face posts");
    let on_disk = std::fs::read_to_string(&path).expect("the program file");
    let _ = std::fs::remove_file(&path);
    let held = posting::post_setup_unwritten(&world).expect("the same gate passes");
    assert_eq!(held.program, on_disk, "the download is what the desktop writes");
    assert_eq!(held.program, written.program);
    assert_eq!((held.ops, held.lines), (written.ops, written.lines));
}

#[test]
fn the_unwritten_post_refuses_where_the_written_one_does() {
    // A stale op (its params changed, not regenerated) never posts.
    let (world, mut history) = one_face();
    let edit = camcraft_app::commands::EditOperationParams::new(&world, "Face1", face(19.0));
    history.execute(&world, edit.expect("edit"));
    let path = temp("stale", "nc");
    let _ = std::fs::remove_file(&path);
    let written = posting::post_setup(&world, &path).expect_err("a stale op is refused");
    assert!(!path.exists(), "a refusal writes nothing");
    let held = posting::post_setup_unwritten(&world).expect_err("and refused unwritten too");
    assert_eq!(held, written, "the same reason, by name");
}

#[test]
fn save_text_is_the_saved_project_file() {
    let (world, _h) = one_face();
    let path = temp("save", "camproj");
    project::save(&world, &path).expect("saved");
    let on_disk = std::fs::read_to_string(&path).expect("the project file");
    let _ = std::fs::remove_file(&path);
    assert_eq!(project::save_text(&world).expect("the text"), on_disk);
}
