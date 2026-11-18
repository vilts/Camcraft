//! Labelled command batching (#1942): `BatchCommand`, `History::begin_group`,
//! `History::commit_group`, `History::cancel_group`, clean/dirty tracking,
//! empty groups, nested groups, and reverse-order undo.
//!
//! Gate map (M4.4): covers app::commands (batch, mod).

use camcraft_app::commands::{
    AddTool, BatchCommand, CreateOperation, History, SetStock,
};
use camcraft_app::model;
use camcraft_app::operation::{OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::setup;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params() -> OpParams {
    camcraft_app::operation::seed_params(0)
}

#[test]
fn begin_and_commit_group_folds_multi_command_turn_into_one_undo_step() {
    let world = new_world();
    let mut history = History::default();
    assert!(!history.is_dirty());

    history.begin_group("AI chat: setup and face");
    assert!(history.in_group());

    let stock_cmd = SetStock::new(
        &world,
        StockShape::Box {
            dx: 80.0,
            dy: 80.0,
            dz: 20.0,
        },
        Location::default(),
    )
    .expect("valid stock");
    history.execute(&world, stock_cmd);

    let (_, tool) = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .expect("catalog tool");
    let tool_cmd = AddTool::new(&world, "EM-0400-2FL", tool).expect("valid tool");
    history.execute(&world, tool_cmd);

    let op_cmd = CreateOperation::new(
        &world,
        "Face1",
        "EM-0400-2FL",
        SrcRef::Stock,
        face_params(),
    )
    .expect("valid operation");
    history.execute(&world, op_cmd);

    assert!(history.commit_group());
    assert!(!history.in_group());
    assert_eq!(history.done_len(), 1);
    assert_eq!(history.top_label(), Some("AI chat: setup and face"));
    assert!(history.is_dirty());
    assert!(model::find_by_name(&world, "Face1").is_some());
    assert!(model::find_by_name(&world, "EM-0400-2FL").is_some());
    assert!(setup::stock_entity(&world, None).is_some());

    // One undo reverts all three commands in reverse order (op -> tool -> stock).
    assert!(history.undo(&world));
    assert_eq!(history.done_len(), 0);
    assert!(!history.is_dirty());
    assert!(model::find_by_name(&world, "Face1").is_none());
    assert!(model::find_by_name(&world, "EM-0400-2FL").is_none());
    assert!(setup::stock_entity(&world, None).and_then(|e| model::stock_def_of(&world, &e)).is_none());

    // One redo brings all three back in forward order.
    assert!(history.redo(&world));
    assert_eq!(history.done_len(), 1);
    assert!(history.is_dirty());
    assert!(model::find_by_name(&world, "Face1").is_some());
    assert!(model::find_by_name(&world, "EM-0400-2FL").is_some());
    assert!(setup::stock_entity(&world, None).and_then(|e| model::stock_def_of(&world, &e)).is_some());
}

#[test]
fn empty_and_nested_groups_and_direct_batch_command_behave_cleanly() {
    let world = new_world();
    let mut history = History::default();

    // Empty group leaves no undo entry.
    history.begin_group("empty");
    assert!(!history.commit_group());
    assert!(!history.in_group());
    assert_eq!(history.done_len(), 0);
    assert!(!history.is_dirty());

    // Direct BatchCommand construction and nested group commit.
    let (_, tool) = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .expect("catalog tool");
    let batch = BatchCommand::new(
        "direct",
        vec![Box::new(AddTool::new(&world, "EM-0400-2FL", tool.clone()).expect("tool"))],
    );
    assert_eq!(batch.label(), "direct");
    assert_eq!(batch.len(), 1);
    assert!(!batch.is_empty());

    history.begin_group("outer");
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 50.0,
                dy: 50.0,
                dz: 10.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    history.begin_group("inner");
    history.execute(&world, batch);
    assert!(history.commit_group());
    assert!(history.in_group());
    assert!(history.commit_group());
    assert!(!history.in_group());
    assert_eq!(history.done_len(), 1);
    assert_eq!(history.top_label(), Some("outer"));

    history.mark_clean();
    assert!(!history.is_dirty());
    assert!(history.undo(&world));
    assert!(history.is_dirty());
    assert!(model::find_by_name(&world, "EM-0400-2FL").is_none());
    assert!(history.redo(&world));
    assert!(!history.is_dirty());
    assert!(model::find_by_name(&world, "EM-0400-2FL").is_some());
}

#[test]
fn cancel_group_keeps_existing_commands_separate() {
    let world = new_world();
    let mut history = History::default();
    history.begin_group("interrupted");
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 40.0,
                dy: 40.0,
                dz: 10.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    let (_, tool) = catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0400-2FL")
        .expect("catalog tool");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
    assert!(history.cancel_group());
    assert!(!history.in_group());
    assert_eq!(history.done_len(), 2);
}
