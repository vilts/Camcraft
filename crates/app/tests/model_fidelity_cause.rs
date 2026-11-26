//! #2996: only a recorded file-tolerance cause can widen a model verdict.
//! Provenance: our own command, persistence and mixed-material laws.
//! Gate map (#2996): covers app::model_fidelity, app::commands (import_model,
//! update_model, delete_entity, set_stock, setup, setup_delete), app::project.

use camcraft_app::commands::{
    ActivateSetup, CreateSetup, DeleteEntity, DeleteSetup, History, ImportModel, SetStock,
    UpdateModel,
};
use camcraft_app::geometry::{Mesh, Point3, cube};
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;
use camcraft_app::{fidelity_note, model, model_fidelity, project};
use flecs_ecs::prelude::World;

fn brick() -> Mesh {
    cube(
        Point3::new(-20.0, -10.0, 0.0),
        Point3::new(20.0, 10.0, 10.0),
    )
}

fn cause(world: &World, name: &str, want: bool) {
    let e = model::find_by_name(world, name).expect("entity");
    assert_eq!(
        model_fidelity::file_tolerance_of(world, &e),
        want,
        "{name} cause"
    );
}

fn import(world: &World, h: &mut History) {
    h.execute(
        world,
        ImportModel::from_mesh("file", brick())
            .with_fidelity(Some(0.019))
            .with_file_tolerance(true),
    );
    cause(world, "file", true);
}

#[test]
fn cause_follows_import_copy_update_delete_and_setup_undo() {
    let world = new_world();
    let mut h = History::default();
    import(&world, &mut h);
    h.undo(&world);
    assert!(model::find_by_name(&world, "file").is_none());
    h.redo(&world);
    cause(&world, "file", true);
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "file".into(),
            },
            Default::default(),
        )
        .expect("stock"),
    );
    cause(&world, "Stock", true);
    h.execute(&world, DeleteEntity::new(&world, "file").expect("delete"));
    cause(&world, "Stock", true);
    h.undo(&world);
    cause(&world, "file", true);
    h.execute(
        &world,
        UpdateModel::new(&world, "file", brick())
            .expect("update")
            .with_fidelity(Some(0.012)),
    );
    cause(&world, "file", false);
    cause(&world, "Stock", false);
    h.undo(&world);
    cause(&world, "file", true);
    cause(&world, "Stock", true);
    h.redo(&world);
    cause(&world, "file", false);
    h.undo(&world);
    h.execute(
        &world,
        UpdateModel::new(&world, "file", brick())
            .expect("repair")
            .keeping_fidelity(),
    );
    cause(&world, "file", true);
    cause(&world, "Stock", true);
    h.execute(&world, ImportModel::from_mesh("plain", brick()));
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "plain".into(),
            },
            Default::default(),
        )
        .expect("plain stock"),
    );
    cause(&world, "Stock", false);
    h.undo(&world);
    cause(&world, "Stock", true);
    h.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("first setup"),
    );
    let first = camcraft_app::setup::active(&world).expect("active");
    h.execute(
        &world,
        CreateSetup::new(&world, Some("S2"), Some(&first)).expect("clone setup"),
    );
    cause(&world, "S2/Stock", true);
    h.execute(
        &world,
        ActivateSetup::new(&world, Some(&first)).expect("activate"),
    );
    h.execute(
        &world,
        DeleteSetup::new(&world, "S2").expect("delete setup"),
    );
    h.undo(&world);
    cause(&world, "S2/Stock", true);
}

#[test]
fn cause_round_trips_and_a_missing_record_keeps_the_old_law() {
    let world = new_world();
    let mut h = History::default();
    import(&world, &mut h);
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "file".into(),
            },
            Default::default(),
        )
        .expect("stock"),
    );
    let path = std::env::temp_dir().join(format!(
        "camcraft-2996-cause-{}.camproj",
        std::process::id()
    ));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    for row in doc["entities"].as_array().expect("rows") {
        if row["name"] == "file" || row["name"] == "Stock" {
            assert_eq!(
                row["file_tolerance"], true,
                "the saved row records the cause"
            );
        }
    }
    let loaded = new_world();
    project::load(&loaded, &path).expect("load");
    cause(&loaded, "file", true);
    cause(&loaded, "Stock", true);
    assert!((model_fidelity::verdict_tol(&loaded).expect("wide") - 0.005).abs() < 1e-12);
    let mut old = doc;
    for row in old["entities"].as_array_mut().expect("rows") {
        row.as_object_mut().expect("row").remove("file_tolerance");
    }
    std::fs::write(&path, serde_json::to_string(&old).expect("json")).expect("write");
    let old_world = new_world();
    project::load(&old_world, &path).expect("old file loads");
    cause(&old_world, "file", false);
    cause(&old_world, "Stock", false);
    assert!(
        model_fidelity::verdict_tol(&old_world).is_err(),
        "missing cause cannot widen .019"
    );
    assert!(fidelity_note::program_notes(&old_world).is_empty());
    let _ = std::fs::remove_file(path);
}

#[test]
fn mixed_file_and_budget_models_keep_the_strict_law() {
    let world = new_world();
    let mut h = History::default();
    import(&world, &mut h);
    h.execute(
        &world,
        ImportModel::from_mesh("budget", brick()).with_fidelity(Some(0.012)),
    );
    cause(&world, "budget", false);
    assert!(
        model_fidelity::verdict_tol(&world).is_err(),
        "a mixed source cannot borrow file eligibility"
    );
    assert!(
        !fidelity_note::program_notes(&world)
            .iter()
            .any(|s| s.contains("budget"))
    );
}

#[test]
fn update_undo_restores_each_copy_cause_without_widening_a_legacy_copy() {
    let world = new_world();
    let mut h = History::default();
    import(&world, &mut h);
    h.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel {
                source: "file".into(),
            },
            Default::default(),
        )
        .expect("stock"),
    );
    h.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("first setup"),
    );
    let first = camcraft_app::setup::active(&world).expect("active");
    let first_stock = camcraft_app::setup::stock_entity_name(&world, Some(&first));
    h.execute(
        &world,
        CreateSetup::new(&world, Some("S2"), Some(&first)).expect("copy setup"),
    );
    let legacy = model::find_by_name(&world, "S2/Stock").expect("copy");
    model_fidelity::set(&world, &legacy, Some(0.019));
    cause(&world, &first_stock, true);
    cause(&world, "S2/Stock", false);
    h.execute(
        &world,
        UpdateModel::new(&world, "file", brick())
            .expect("update")
            .with_fidelity(Some(0.012))
            .with_file_tolerance(true),
    );
    cause(&world, &first_stock, true);
    cause(&world, "S2/Stock", true);
    h.undo(&world);
    cause(&world, &first_stock, true);
    cause(&world, "S2/Stock", false);
}
