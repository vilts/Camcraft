//! #1440: the setup's Machine page follows the WCS page's layout - titled
//! groups (which machine, the machine file, the actions), the overview as
//! label | value rows, equal action buttons - and keeps what it said with
//! no machine. Structural twin: labels, order and coarse placement, no
//! pixels. Provenance: our own page law.

use super::*;
use crate::egui_app::machine_page::{FILE_HELP, GROUPS, NO_MACHINE};
use crate::state::SetupPage;

type H = Harness<'static, ShellUi>;

/// The shell at the app's default height (800 px). Kept for the click
/// tests: before #2105 was fixed the 720 px harness left the machine
/// page's button row below the fold (the ribbon grew 22 px in #2027).
fn tall_harness() -> H {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(frame, ShellUi::seeded())
}

fn open_machine(h: &mut H, machine: Option<&str>) {
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    // A fresh document has no machine: `None` loads nothing.
    if let Some(m) = machine {
        h.state_mut().state.pending.push_back(CommandKind::LoadBuiltinMachine(m.into()));
    }
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    h.run_steps(3);
}

fn rect(h: &H, label: &str) -> egui::Rect {
    let pane_top = h
        .state()
        .dock
        .iter_leaves()
        .find(|(_, l)| l.tabs.contains(&Tab::Properties))
        .map(|(_, l)| l.viewport.top())
        .expect("the properties leaf");
    let all: Vec<egui::Rect> = h
        .query_all_by_label(label)
        .map(|n| n.rect())
        .filter(|r| r.center().y >= pane_top)
        .collect();
    assert_eq!(all.len(), 1, "{label:?} drawn exactly once: {all:?}");
    all[0]
}

fn top(h: &H, label: &str) -> f32 {
    rect(h, label).top()
}

/// The value drawn on `label`'s row (Kind and Workspace can read alike).
fn value_on_row(h: &H, value: &str, label: egui::Rect) -> egui::Rect {
    let all: Vec<egui::Rect> = h.query_all_by_label(value).map(|n| n.rect()).collect();
    let near: Vec<egui::Rect> = all.iter().copied().filter(|r| r.top() - 6.0 <= label.top() && label.top() <= r.bottom()).collect();
    assert_eq!(near.len(), 1, "{value:?} once on the row of {label:?}: {all:?}");
    near[0]
}

/// Every label once, top to bottom in the order given.
fn assert_stacked(h: &H, labels: &[&str]) {
    let tops: Vec<f32> = labels.iter().map(|l| top(h, l)).collect();
    assert!(tops.windows(2).all(|w| w[0] < w[1]), "{labels:?} stack top to bottom: {tops:?}");
}

fn rows_of(h: &H) -> Vec<(&'static str, String)> {
    let st = &h.state().state;
    camcraft_app::machine::of(&st.world).1.map(|m| m.rows(st.units())).unwrap_or_default()
}

/// Each overview row is its label then its value, on one row, label left.
fn assert_overview_rows(h: &H, lo: f32, hi: f32) {
    let rows = rows_of(h);
    assert_eq!(rows.len(), 7, "the overview states seven facts: {rows:?}");
    let mut last = lo;
    for (label, value) in rows.iter().map(|(k, v)| (*k, v.clone())).chain([("Source", "Built-in: Tormach 770MX".to_owned())]) {
        let l = rect(h, label);
        let v = value_on_row(h, &value, l);
        assert!(l.top() > last && l.top() < hi, "{label} between the group titles: {l:?}");
        assert!(l.left() < v.left(), "{label} left of its value");
        last = l.top();
    }
}

#[test]
fn the_machine_page_is_laid_out_in_groups_like_the_wcs_page() {
    let mut h = shell_harness();
    open_machine(&mut h, Some("Tormach 770MX"));
    assert_stacked(&h, &GROUPS);
    // Which machine: the own / document choice sits under its title.
    let own = rect(&h, "This setup has its own machine");
    assert!(own.top() > top(&h, GROUPS[0]) && own.top() < top(&h, GROUPS[1]), "ownership under 'Which machine'");
    assert!(h.query_by_label_contains("Use the document's machine (").is_some());
    assert!(h.query_by_label_contains("Setups with different machines post one at a time").is_some());
    // Machine file: helper line, then the Library row, then the overview.
    assert_stacked(&h, &[GROUPS[1], FILE_HELP, "Library"]);
    assert!(top(&h, "Library") < top(&h, "Kind") && top(&h, "Source") < top(&h, GROUPS[2]));
    assert_overview_rows(&h, top(&h, "Library"), top(&h, GROUPS[2]));
    assert!(h.query_by_label("Built-in: Tormach 770MX").is_some());
    assert!(h.query_by_label(NO_MACHINE).is_none(), "the empty message is only for no machine");
    // Change machine: three equal buttons on one row, left to right.
    let b: Vec<egui::Rect> = ["Load other", "Edit", "Clear"].iter().map(|l| rect(&h, l)).collect();
    assert!(b.iter().all(|r| r.top() > top(&h, GROUPS[2])), "buttons under their title: {b:?}");
    assert!(b.windows(2).all(|w| (w[0].top() - w[1].top()).abs() < 1.0 && w[0].right() < w[1].left()), "{b:?}");
    assert!(b.windows(2).all(|w| (w[0].width() - w[1].width()).abs() < 1.0), "equal widths: {b:?}");
    assert!(h.query_by_label("Load machine (.mch)").is_none(), "the no-machine button is not drawn with a machine");
}

/// #2105: the ribbon is 22 px taller since #2027, and the machine page's
/// action row fell under the fold of a 720 px window: the user had to
/// scroll to reach the buttons. The page's visible bottom is NOT the
/// bottom of the Properties leaf: `pages.rs` gives the page's ScrollArea
/// the leaf height minus 40 px and draws the separator and the `Close`
/// footer in that band. So the edge to measure against is the top of
/// `Close`, read from the same pane as the buttons.
#[test]
fn the_machine_page_buttons_fit_without_scrolling_at_720_px() {
    let mut h = shell_harness();
    open_machine(&mut h, Some("Tormach 770MX"));
    let close = h
        .get_by_role_and_label(egui::accesskit::Role::Pane, "Properties")
        .query_by_label("Close")
        .expect("the Close footer of the Properties pane")
        .rect();
    let title = rect(&h, GROUPS[2]);
    assert!(title.top() < close.top(), "the action group title is in the fold: {title:?} vs Close {close:?}");
    for label in ["Load other", "Edit", "Clear"] {
        let r = rect(&h, label);
        assert!(
            r.bottom() <= close.top() + 1.0,
            "{label} {r:?} stands below the page's visible bottom - the Close footer {close:?} starts there"
        );
    }
}

/// A lathe states its own facts in the same rows - the page draws no
/// mill-only row and drops none.
#[test]
fn the_machine_page_lays_out_a_lathe_in_the_same_rows() {
    let mut h = shell_harness();
    open_machine(&mut h, Some("Lathe"));
    let rows = rows_of(&h);
    assert_eq!(rows.iter().map(|(k, _)| *k).collect::<Vec<_>>(), ["Kind", "Machine", "Spindle", "Axes", "Tool changer", "Coolant", "Workspace"]);
    assert_stacked(&h, &[GROUPS[1], "Library", "Kind", "Machine", "Spindle", "Axes", "Tool changer", "Coolant", "Workspace", "Source", GROUPS[2]]);
    for (label, value) in &rows {
        let l = rect(&h, label);
        let v = value_on_row(&h, value, l);
        assert!(l.left() < v.left(), "{label}: {l:?} {v:?}");
    }
    assert!(h.query_by_label("Built-in: Lathe").is_some());
    assert!(rows.iter().any(|(k, v)| *k == "Kind" && v == "Turning"), "a lathe: {rows:?}");
}

/// With no machine the page says what it said before the tidy, in the
/// same places, keeps the library and offers the load.
#[test]
fn the_machine_page_keeps_its_message_and_load_button_with_no_machine() {
    let mut h = shell_harness();
    open_machine(&mut h, None);
    assert!(rows_of(&h).is_empty(), "no machine is loaded");
    assert_stacked(&h, &[GROUPS[0], GROUPS[1], FILE_HELP, "Library", NO_MACHINE, GROUPS[2], "Load machine (.mch)"]);
    assert!(h.query_by_label_contains("Use the document's machine (").is_some());
    assert!(h.query_by_label("This setup has its own machine").is_some());
    for gone in ["Kind", "Spindle", "Axes", "Source", "Load other", "Edit", "Clear"] {
        assert!(h.query_by_label(gone).is_none(), "{gone} is drawn only with a machine");
    }
}

#[test]
fn the_machine_page_buttons_still_queue_their_commands() {
    let mut h = tall_harness();
    open_machine(&mut h, Some("Tormach 770MX"));
    assert!(camcraft_app::machine::of(&h.state().state.world).1.is_some());
    h.get_by_label("Clear").click();
    h.run_steps(3);
    assert!(camcraft_app::machine::of(&h.state().state.world).1.is_none(), "Clear dropped the machine");
    assert!(h.query_by_label(NO_MACHINE).is_some(), "and the page follows");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(3);
    assert!(camcraft_app::machine::of(&h.state().state.world).1.is_some(), "undo restores it");
    assert!(h.query_by_label("Load other").is_some());
}

/// "Load other" and "Edit" each queue their OWN dialog (undrained, so
/// the rfd modal never opens).
#[test]
fn load_other_and_edit_queue_their_own_dialogs() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(frame_undrained, ShellUi::seeded());
    let st = &mut h.state_mut().state;
    st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    st.pending.push_back(CommandKind::LoadBuiltinMachine("Tormach 770MX".into()));
    st.drain_actions();
    crate::actions::target(st, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    st.drain_actions();
    h.run_steps(3);
    for (label, want_load) in [("Load other", true), ("Edit", false)] {
        h.state_mut().state.pending.clear();
        h.get_by_label(label).click();
        h.run_steps(2);
        let q: Vec<_> = h.state().state.pending.iter().collect();
        let load = q.iter().any(|c| matches!(c, CommandKind::LoadMachineDialog));
        let edit = q.iter().any(|c| matches!(c, CommandKind::EditMachineDialog));
        assert_eq!((load, edit), (want_load, !want_load), "{label} queued {q:?}");
    }
}

/// With no setups the Post page's hint runs straight into the post
/// rows: no separator between them (upstream's layout).
#[test]
fn the_post_page_without_setups_has_no_extra_separator() {
    let mut h = shell_harness();
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode(String::new(), SetupPage::Post));
    h.run_steps(3);
    let hint = rect(&h, "A document without setups uses the document's machine and post.");
    let post = rect(&h, "Post");
    let gap = post.top() - hint.bottom();
    assert!(gap < 6.0, "hint and post rows sit one item spacing apart, got {gap}");
}

/// A rotary machine's raised-rotary rows still come last, under the
/// actions.
#[test]
fn the_machine_page_keeps_the_rotary_rows_last() {
    let mut h = shell_harness();
    open_machine(&mut h, Some("Tormach 770MX + microARC 4"));
    assert_stacked(&h, &[GROUPS[2], "Clear", crate::egui_app::setup_rotary_page::HEADING]);
}

/// A setup that is not the active one draws the activate prompt and none
/// of the groups.
#[test]
fn the_machine_page_of_an_inactive_setup_offers_only_activation() {
    let mut h = shell_harness();
    for _ in 0..2 {
        h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    }
    h.state_mut().state.pending.push_back(CommandKind::ActivateSetup(Some("Setup1".into())));
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup2".into(), SetupPage::Machine));
    h.run_steps(3);
    assert!(h.query_by_label("Activate Setup2").is_some());
    for gone in GROUPS {
        assert!(h.query_by_label(gone).is_none(), "{gone} waits for activation");
    }
}

/// #1119: the Machine page's library combo carries an accessible name.
#[test]
fn the_machine_page_names_every_input() {
    let mut h = shell_harness();
    open_machine(&mut h, Some("Tormach 770MX"));
    let (bad, seen) = super::super::fixture_plate_tests::unnamed_inputs(&h);
    assert!(seen >= 1, "only {seen} inputs found on the machine page");
    assert!(bad.is_empty(), "unnamed inputs on machine page: {bad:?}");
}
