//! Operation labels and explicit program order through commands and project files.
//! Provenance: our own document and undo contract.
//! Gate map: covers app::commands, app::model, app::project, app::schema, app::setup.

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, History, RenameOperation, ReorderOperation, SetStock,
};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::setup;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn params() -> OpParams {
    OpParams::Face {
        holes: Default::default(), tilt: None, blend_turns: false, blend_radius: None,
        stepover: 3.0, clearance_z: Height::fixed(25.0), cut_z: Height::fixed(19.5),
        top_z: None, stepdown: None, feed: 1200.0, plunge_feed: 300.0, climb: true,
        picks: Default::default(),
    }
}

fn seed() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default())
            .expect("stock"),
    );
    let tool = catalog().into_iter().find(|(name, _)| *name == "EM-0400-2FL")
        .map(|(_, tool)| tool).expect("catalog tool");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    (world, history)
}

fn add(world: &World, history: &mut History, name: &str) {
    history.execute(world, CreateOperation::new(world, name, "EM-0400-2FL", SrcRef::Stock, params())
        .expect("operation"));
}

fn keys(world: &World) -> Vec<String> {
    setup::operation_order(world).into_iter().map(|(name, _)| name).collect()
}

fn label(world: &World, name: &str) -> String {
    let entity = model::find_by_name(world, name).expect("operation key");
    model::operation_label(world, name, &entity)
}

#[test]
fn rename_keeps_identity_order_and_undo_history() {
    let (world, mut history) = seed();
    add(&world, &mut history, "A");
    add(&world, &mut history, "B");
    let original = keys(&world);
    history.execute(&world, RenameOperation::new(&world, "B", "Finish pass").expect("rename"));
    assert_eq!(label(&world, "B"), "Finish pass");
    assert_eq!(keys(&world), original, "display name must not change machining order");
    assert!(RenameOperation::new(&world, "A", "Finish pass").is_err(), "duplicate label");
    assert!(RenameOperation::new(&world, "A", "B").is_err(), "stable key cannot be shadowed");
    history.undo(&world);
    assert_eq!(label(&world, "B"), "B");
    history.redo(&world);
    assert_eq!(label(&world, "B"), "Finish pass");
    assert!(model::find_by_name(&world, "B").is_some(), "existing commands still resolve the key");
}

#[test]
fn reorder_stays_within_setup_and_survives_undo_and_save() {
    let (world, mut history) = seed();
    add(&world, &mut history, "A");
    add(&world, &mut history, "B");
    history.execute(&world, CreateSetup::new(&world, None, None).expect("first setup"));
    history.execute(&world, CreateSetup::new(&world, None, None).expect("second setup"));
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default())
            .expect("second setup stock"),
    );
    add(&world, &mut history, "C");
    add(&world, &mut history, "D");
    assert_eq!(keys(&world), ["A", "B", "C", "D"]);
    assert!(ReorderOperation::new(&world, "B", "C", false).is_err(), "cross-setup drag refused");
    history.execute(&world, ReorderOperation::new(&world, "B", "A", false).expect("reorder"));
    assert_eq!(keys(&world), ["B", "A", "C", "D"]);
    history.undo(&world);
    assert_eq!(keys(&world), ["A", "B", "C", "D"]);
    history.redo(&world);
    assert_eq!(keys(&world), ["B", "A", "C", "D"]);
    history.execute(&world, RenameOperation::new(&world, "B", "First cut").expect("rename"));
    assert_eq!(keys(&world), ["B", "A", "C", "D"]);

    let path = std::env::temp_dir().join(format!("camcraft-operation-metadata-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let loaded = new_world();
    project::load(&loaded, &path).expect("load");
    assert_eq!(keys(&loaded), ["B", "A", "C", "D"]);
    assert_eq!(label(&loaded, "B"), "First cut");
    let copy = std::env::temp_dir().join(format!("camcraft-operation-metadata-copy-{}.camproj", std::process::id()));
    project::save(&loaded, &copy).expect("save loaded world");
    assert_eq!(std::fs::read(&path).expect("original file"), std::fs::read(&copy).expect("round-trip file"));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&copy);
}

#[test]
fn version_49_operations_keep_legacy_name_order() {
    let (world, mut history) = seed();
    add(&world, &mut history, "Z_Face");
    add(&world, &mut history, "A_Face");
    let path = std::env::temp_dir().join(format!("camcraft-operation-v49-{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let mut file: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).expect("file"))
        .expect("JSON");
    file["version"] = serde_json::json!(49);
    // A real v49 file never carried positions: strip the ones #167 stores.
    if let Some(entities) = file["entities"].as_array_mut() {
        for entity in entities {
            if let Some(op) = entity.get_mut("operation").and_then(|o| o.as_object_mut()) {
                op.remove("order");
            }
        }
    }
    std::fs::write(&path, serde_json::to_vec_pretty(&file).expect("JSON")).expect("downgrade fixture");
    let loaded = new_world();
    project::load(&loaded, &path).expect("v49 migration");
    let _ = std::fs::remove_file(&path);
    assert_eq!(keys(&loaded), ["A_Face", "Z_Face"]);
    assert_eq!(label(&loaded, "A_Face"), "A_Face");
}

#[test]
fn created_operations_post_in_creation_order_not_name_order() {
    let (world, mut history) = seed();
    add(&world, &mut history, "Rough");
    add(&world, &mut history, "Finish");
    assert_eq!(keys(&world), ["Rough", "Finish"], "#167: creation order, not name order");
    add(&world, &mut history, "B");
    add(&world, &mut history, "A");
    assert_eq!(keys(&world), ["Rough", "Finish", "B", "A"]);
    history.undo(&world);
    assert_eq!(keys(&world), ["Rough", "Finish", "B"]);
    history.redo(&world);
    assert_eq!(keys(&world), ["Rough", "Finish", "B", "A"]);
}

#[test]
fn creating_after_legacy_unordered_ops_keeps_their_name_order() {
    let (world, mut history) = seed();
    add(&world, &mut history, "Z_Face");
    add(&world, &mut history, "A_Face");
    // An older project's ops carry no stored position: they read in name order.
    for name in ["Z_Face", "A_Face"] {
        let entity = model::find_by_name(&world, name).expect("op");
        entity.entity_view(&world).set(camcraft_app::schema::OperationMeta::default());
    }
    assert_eq!(keys(&world), ["A_Face", "Z_Face"]);
    add(&world, &mut history, "M_Face");
    assert_eq!(keys(&world), ["A_Face", "Z_Face", "M_Face"], "legacy order kept, new op last");
    history.undo(&world);
    assert_eq!(keys(&world), ["A_Face", "Z_Face"]);
    let legacy = model::find_by_name(&world, "Z_Face").expect("op");
    assert_eq!(model::operation_meta_of(&world, &legacy).order, None, "undo restores the unpinned position");
}
