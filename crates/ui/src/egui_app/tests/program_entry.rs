//! #155 in the browser: the operation list's menu adds a Pause / Home
//! entry after the row it was opened on, the entry is a row of its own
//! with its own menu (change, suppress, delete, add after), and every
//! row queues one undoable command - Ctrl+Z puts the list back.
//! Provenance: our own tests over our own shell; no vendor material.

use super::*;
use super::super::browser_tree::Leaf;
use super::super::tree_entry;
use camcraft_app::operation::AfterOperation;
use camcraft_app::program_entry;
use crate::state::EntryVerb;

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn list(state: &State) -> Vec<String> {
    program_entry::sequence(&state.world).into_iter().map(|(n, _)| n).collect()
}

fn entry(state: &State, name: &str) -> Option<camcraft_app::schema::ProgramEntry> {
    model::find_by_name(&state.world, name).and_then(|e| program_entry::entry_of(&state.world, &e))
}

/// The menu's add rows and an entry's own rows, by label: the structure
/// the click test below leans on.
#[test]
fn the_menus_carry_the_entry_rows() {
    let adds: Vec<&str> = tree_entry::add_lines().iter().map(|(l, _, _)| *l).collect();
    assert_eq!(adds, ["Add pause (M0) after", "Add go home after", "Add optional stop (M1) after"]);
    let e = camcraft_app::schema::ProgramEntry { action: AfterOperation::ProgramStop, setup: None, suppressed: false };
    let rows: Vec<&str> = tree_entry::entry_lines(&e).iter().map(|(l, _, _)| *l).collect();
    assert_eq!(rows[..4], ["Entry", "Does: pause (M0)", "Suppress entry", "Delete entry (undoable)"]);
    assert!(rows.ends_with(&adds), "an entry adds after itself too: {rows:?}");
}

#[test]
fn an_entry_is_a_row_of_the_list_and_undoes() {
    let mut harness = shell_harness();
    harness.run_steps(2);
    let state = &mut harness.state_mut().state;
    let first = list(state).first().cloned().expect("the seeded op");
    run(state, CommandKind::ProgramEntry(EntryVerb::Add { action: AfterOperation::ProgramStop, after: Some(first.clone()) }));
    assert_eq!(list(state), [first.clone(), "Pause1".to_owned()]);
    let (rows, leaves) = panes::browser_rows(state);
    let ix = leaves.iter().position(|l| *l == Leaf::Entry("Pause1".into())).expect("the entry row");
    assert!(matches!(&leaves[ix - 1], Leaf::Op(n) if *n == first), "right under its op");
    assert!(rows[ix].suffix.contains("Pause (M0)"), "{}", rows[ix].suffix);
    run(state, CommandKind::ProgramEntry(EntryVerb::Suppress { name: "Pause1".into(), suppressed: true }));
    let (rows, _) = panes::browser_rows(state);
    assert!(rows[ix].suffix.contains("SUPPRESSED"), "{}", rows[ix].suffix);
    run(state, CommandKind::DeleteByName("Pause1".into()));
    assert!(entry(state, "Pause1").is_none());
    run(state, CommandKind::Undo);
    assert!(entry(state, "Pause1").is_some_and(|e| e.suppressed), "undo restores it as it was");
    run(state, CommandKind::Undo);
    run(state, CommandKind::Undo);
    assert_eq!(list(state), [first], "undo removes the add");
}

/// The op row's menu: its first add row queues the pause after the op;
/// then the entry row's own menu suppresses and deletes it.
#[test]
fn the_operation_menu_adds_an_entry_and_its_own_menu_edits_it() {
    let mut harness = shell_harness();
    harness.run_steps(2);
    let (_, leaves) = panes::browser_rows(&harness.state().state);
    let row = leaves.iter().position(|leaf| matches!(leaf, Leaf::Op(name) if name == "Contour1"))
        .expect("operation row");
    let row_height = theme::current().row_height;
    let at_row = |r: usize| egui::pos2(90.0, 221.0 + (r as f32 + 0.5) * row_height);
    let open = |harness: &mut Harness<'static, ShellUi>, r: usize| {
        set_pos(harness, at_row(r));
        button_at(harness, egui::PointerButton::Secondary, at_row(r), true);
        button_at(harness, egui::PointerButton::Secondary, at_row(r), false);
    };
    let pick = |harness: &mut Harness<'static, ShellUi>, r: usize, k: f32| {
        let at = egui::pos2(90.0, 221.0 + (r as f32 + 1.0) * row_height + panes::DROP_GAP + k * row_height);
        set_pos(harness, at);
        button_at(harness, egui::PointerButton::Primary, at, true);
        button_at(harness, egui::PointerButton::Primary, at, false);
        harness.run_steps(2);
    };
    // Operation, Rename, Copy, Paste, Clear, Delete, Suppress, After,
    // then the add rows (#155).
    open(&mut harness, row);
    pick(&mut harness, row, 8.5);
    let state = &harness.state().state;
    assert_eq!(entry(state, "Pause1").map(|e| e.action), Some(AfterOperation::ProgramStop), "{}", state.status);
    let (_, leaves) = panes::browser_rows(state);
    let entry_row = leaves.iter().position(|l| *l == Leaf::Entry("Pause1".into())).expect("the entry row");
    assert_eq!(entry_row, row + 1, "right under the op");

    open(&mut harness, entry_row);
    assert_eq!(harness.state().state.tree_menu, Some(TreeMenuTarget::Entry("Pause1".into())));
    // Entry, Does, Suppress, Delete.
    pick(&mut harness, entry_row, 2.5);
    assert!(entry(&harness.state().state, "Pause1").is_some_and(|e| e.suppressed));
    open(&mut harness, entry_row);
    pick(&mut harness, entry_row, 3.5);
    assert!(entry(&harness.state().state, "Pause1").is_none(), "deleted");
    run(&mut harness.state_mut().state, CommandKind::Undo);
    assert!(entry(&harness.state().state, "Pause1").is_some(), "Ctrl+Z restores it");
}
