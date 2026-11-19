//! Migration review requirements survive copying and deleting the source.
//! Gate map: covers app::commands::operation::create, app::project, app::posting.
use camcraft_app::{
    commands::{
        DeleteOperation, GenerateOperation, History, MarkOperationReviewed, PasteOperation,
    },
    model, posting, project,
    world::new_world,
};

fn v63_project_with_pocket_adaptive(extra: &str) -> String {
    let extra = if extra.is_empty() {
        String::new()
    } else {
        format!(", {extra}")
    };
    format!(
        r#"{{
        "version": 63,
        "entities": [
            {{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {{"box": {{"dx": 12.0, "dy": 12.0, "dz": 20.0}}}}}},
            {{"name": "EM-0400-2FL", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {{"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 4.0, "shank_len": 35.0,
                      "holder_d": 32.0, "holder_len": 30.0,
                      "material_group": "general", "rpm": 12000.0, "feed": 800.0}}}},
            {{"name": "P1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {{"tool": "EM-0400-2FL", "src": "stock", "params": {{"pocket": {{
                 "stepover": 2.0, "stock_to_leave": 0.0, "clearance_z": 25.0,
                 "top_z": 20.0, "bottom_z": 18.0, "stepdown": 5.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "strategy": "adaptive", "entry": "helix", "helix_radius": 1.5,
                 "optimal_load": 0.45, "blend_turns": true, "blend_radius": 0.4{extra}
             }}}}}}}}
        ]
    }}"#
    )
}

#[test]
fn pasted_legacy_operation_requires_review_after_source_deletion() {
    let world = new_world();
    let legacy = v63_project_with_pocket_adaptive(
        r#""rest": {"previous_tool_diameter": 10.0, "previous_corner_radius": 0.0, "previous_stock_to_leave": 0.0, "allowance": 0.0}"#,
    );
    project::open_noted(&world, &legacy).expect("legacy project opens");
    let original = model::find_by_name(&world, "P1").expect("source");
    let operation = model::operation_of(&world, &original).expect("source operation");
    let notes = model::operation_meta_of(&world, &original).needs_review;
    assert!(!notes.is_empty());
    let mut history = History::default();
    history.execute(
        &world,
        DeleteOperation::new(&world, "P1").expect("delete source"),
    );
    history.execute(
        &world,
        PasteOperation::new(&world, "Copy", operation, notes.clone()).expect("paste"),
    );
    let copy = model::find_by_name(&world, "Copy").expect("copy");
    assert_eq!(model::operation_meta_of(&world, &copy).needs_review, notes);
    history.undo(&world);
    assert!(model::find_by_name(&world, "Copy").is_none());
    history.redo(&world);
    let copy = model::find_by_name(&world, "Copy").expect("redo copy");
    assert_eq!(model::operation_meta_of(&world, &copy).needs_review, notes);
    history.execute(
        &world,
        GenerateOperation::new(&world, "Copy").expect("generate copy"),
    );
    let nc = std::env::temp_dir().join(format!("camcraft_copy_review_{}.nc", std::process::id()));
    let err = posting::post_setup(&world, &nc).expect_err("copy still needs review");
    assert!(
        err.contains("Copy") && err.to_lowercase().contains("review"),
        "{err}"
    );
    history.execute(
        &world,
        MarkOperationReviewed::new(&world, "Copy").expect("review copy"),
    );
    posting::post_setup(&world, &nc).expect("reviewed copy posts");
    history.undo(&world);
    assert!(
        posting::post_setup(&world, &nc).is_err(),
        "undo restores review requirement"
    );
    let _ = std::fs::remove_file(nc);
}
