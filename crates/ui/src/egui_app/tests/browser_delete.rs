//! Provenance: original browser-menu tests over the existing undoable delete commands.
//! Gate map (M4.4): covers ui::egui_app::browser_pane, ui::egui_app::fixture_menu, ui::egui_app::tree_menu.

use super::*;

fn delete_from_menu(harness: &mut Harness<'static, ShellUi>, name: &str) {
    let delete_buttons = harness.query_all_by_role_and_label(egui::accesskit::Role::Button, "Delete").count();
    harness.get_by_label(&format!("Browser: {name}")).click_secondary();
    harness.run();
    assert_eq!(
        harness.query_all_by_role_and_label(egui::accesskit::Role::Button, "Delete").count(),
        delete_buttons + 1,
        "{name} adds a Delete menu entry",
    );
    // The foreground menu follows the shell's existing Delete buttons in the tree.
    harness.get_all_by_role_and_label(egui::accesskit::Role::Button, "Delete")
        .last().expect("the menu Delete button").click();
    harness.run_steps(2);
}

fn exists(harness: &Harness<'static, ShellUi>, name: &str) -> bool {
    model::find_by_name(&harness.state().state.world, name).is_some()
}

#[test]
fn model_context_delete_uses_clicked_model_and_supports_undo_redo() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::Select(Some("Contour1".into())));
    harness.run_steps(2);

    delete_from_menu(&mut harness, "Block");
    assert!(!exists(&harness, "Block"));
    assert!(exists(&harness, "Contour1"), "the previous selection survives");
    assert!(harness.state().state.tree_menu.is_none());

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert!(exists(&harness, "Block"));
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run_steps(2);
    assert!(!exists(&harness, "Block"));
    assert!(exists(&harness, "Contour1"));
}

#[test]
fn tool_context_delete_uses_clicked_tool_and_supports_undo_redo() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::DeleteOperation("Contour1".into()));
    harness.state_mut().state.pending.push_back(CommandKind::Select(Some("Block".into())));
    harness.run_steps(2);

    delete_from_menu(&mut harness, "EM-0600-4FL");
    assert!(!exists(&harness, "EM-0600-4FL"));
    assert!(exists(&harness, "Block"), "the previous selection survives");
    assert!(harness.state().state.tree_menu.is_none());

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert!(exists(&harness, "EM-0600-4FL"));
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run_steps(2);
    assert!(!exists(&harness, "EM-0600-4FL"));
    assert!(exists(&harness, "Block"));
}

#[test]
fn tool_context_delete_preserves_existing_missing_tool_block_and_undo() {
    let mut harness = shell_harness();
    harness.run_steps(2);

    delete_from_menu(&mut harness, "EM-0600-4FL");
    assert!(!exists(&harness, "EM-0600-4FL"));
    assert!(exists(&harness, "Contour1"));
    let (rows, _) = panes::browser_rows(&harness.state().state);
    let row = rows.iter().find(|row| row.label == "Contour1").expect("operation row remains");
    assert!(row.suffix.contains("BLOCKED"), "{}", row.suffix);
    assert!(row.suffix.contains("EM-0600-4FL"), "{}", row.suffix);

    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(2);
    assert!(exists(&harness, "EM-0600-4FL"));
    let (rows, _) = panes::browser_rows(&harness.state().state);
    let row = rows.iter().find(|row| row.label == "Contour1").expect("operation row remains");
    assert!(!row.suffix.contains("BLOCKED"), "{}", row.suffix);
}

#[test]
fn tool_context_delete_hides_missing_or_wrong_kind_targets() {
    let harness = shell_harness();
    let state = &harness.state().state;
    for name in ["missing tool", "Block"] {
        let target = TreeMenuTarget::Tool { name: name.into(), row: 0 };
        assert!(super::super::fixture_menu::menu_rows(state, &target).is_empty(), "{name}");
    }
    let target = TreeMenuTarget::Tool { name: "EM-0600-4FL".into(), row: 0 };
    assert!(!super::super::fixture_menu::menu_rows(state, &target).is_empty());
}
