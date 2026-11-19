//! Named layout checks for floating windows, fixture modals, and Machines.
//! #1991. Provenance: our own layout law over the egui_kittest AccessKit tree; no HSMWorks material involved.

use super::*;
use egui_kittest::kittest::NodeT;

type H = Harness<'static, ShellUi>;

/// Every anchor exists, sits inside `container`, and stacks top to bottom.
fn assert_stack_in(h: &H, container: egui::Rect, who: &str, anchors: &[&str]) {
    let mut last: Option<(&str, f32)> = None;
    for a in anchors {
        let Some(node) = h.query_by_label(a) else {
            panic!("{who}: no widget labelled {a:?}");
        };
        let r = node.rect();
        assert!(container.expand(1.0).contains_rect(r), "{who}: {a:?} {r:?} leaves {container:?}");
        if let Some((prev, y)) = last {
            assert!(r.top() > y, "{who}: {a:?} (top {}) must be under {prev:?} (top {y})", r.top());
        }
        last = Some((a, r.top()));
    }
}

/// Floating `egui::Window`: inside the screen, anchors inside it top to bottom.
fn assert_window_layout(h: &H, title: &str, anchors: &[&str]) {
    let screen = h.ctx.content_rect();
    let Some(win) = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some(title))
    else {
        panic!("{title} window: not open");
    };
    let wr = win.rect();
    assert!(screen.contains_rect(wr), "{title} window: {wr:?} leaves the screen {screen:?}");
    assert_stack_in(h, wr, title, anchors);
}

/// Modal area: its full frame stays on screen, including every anchor and the dismiss button.
fn assert_modal_layout(h: &H, id: &str, who: &str, anchors: &[&str], dismiss: &str) {
    let screen = h.ctx.content_rect();
    let modal = h.ctx.memory(|m| m.area_rect(egui::Id::new(id)));
    let Some(modal) = modal else { panic!("{who}: modal area is not laid out") };
    assert!(screen.contains_rect(modal), "{who}: {modal:?} leaves the screen {screen:?}");
    assert_stack_in(h, modal, who, anchors);
    let body = h.get_by_label(anchors[0]).parent().unwrap_or_else(|| panic!("{who}: missing modal body"));
    let button = body.get_by_role_and_label(egui::accesskit::Role::Button, dismiss).rect();
    assert!(modal.expand(1.0).contains_rect(button), "{who}: {dismiss} {button:?} leaves {modal:?}");
}

#[test]
fn layout_window_post_refusal() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.state_mut().state.post_refusal = Some(super::super::post_refusal::PostRefusal {
        path: std::path::PathBuf::from("program.nc"),
        reason: "Refused move at X0".to_owned(),
        doc_version: h.state().state.doc_version.saturating_sub(1),
    });
    h.run_steps(3);
    assert_window_layout(
        &h,
        "Post refused",
        &[
            "Refused move at X0",
            "Nothing was written to program.nc; a program already there is unchanged.",
            "The document has changed since this post was refused - post again to check it.",
        ],
    );
}

#[test]
fn layout_window_unsaved_prompt() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.state_mut().state.pending.push_back(CommandKind::Quit);
    h.run_steps(3);
    assert_modal_layout(
        &h,
        "unsaved-prompt",
        "Unsaved changes modal",
        &["Unsaved changes", "Save the changes to Untitled before closing?", "Don't Save"],
        "Cancel",
    );
}

fn assert_fixture_modal(cmd: CommandKind, who: &str, anchors: &[&str]) {
    let dismiss = if matches!(cmd, CommandKind::FixtureLibraryInsertDialog(_)) { "Close" } else { "Cancel" };
    let mut h = shell_harness();
    let dir = std::env::temp_dir().join(format!("camcraft-layout-fixlib-{}", std::process::id()));
    h.state_mut().state.fixture_lib_dir = Some(dir.clone());
    h.state_mut().state.pending.push_back(cmd);
    h.run_steps(3);
    assert_modal_layout(&h, "fixture-library", who, anchors, dismiss);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn layout_window_fixture_library() {
    let _g = snapshot_guard();
    assert_fixture_modal(
        CommandKind::FixtureLibrarySaveDialog(String::new()),
        "Fixture library save modal",
        &["Save fixture to library", "Placed by", "Bottom centre", "Note", "Cancel"],
    );
    assert_fixture_modal(
        CommandKind::FixtureLibraryInsertDialog(String::new()),
        "Fixture library insert modal",
        &["Insert from library", "Datum at (into the setup)", "Turn about Z (yaw)", "Delete from library…"],
    );
}

#[test]
fn layout_window_fixture_vise() {
    let _g = snapshot_guard();
    assert_fixture_modal(
        CommandKind::BuiltinViseDialog(String::new()),
        "Fixture vise modal",
        &["Add built-in vise", "Size (jaw width)", "Jaw width", "Base height", "Place"],
    );
}

#[test]
fn layout_window_fixture_plate() {
    let _g = snapshot_guard();
    assert_fixture_modal(
        CommandKind::FixturePlateDialog(String::new()),
        "Fixture plate modal",
        &["Add fixture plate", "Preset", "Length (X)", "Dowel holes", "Place"],
    );
}

#[test]
fn layout_window_fixture_accessory() {
    let _g = snapshot_guard();
    assert_fixture_modal(
        CommandKind::AccessoryDialog(String::new()),
        "Fixture accessory modal",
        &["Add accessory", "Parallels", "Height", "Origin X", "Place"],
    );
}

#[test]
fn layout_window_machines() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.state_mut().state.machines_pane.user_dir = Some(None);
    super::super::layout::apply(
        &mut h.state_mut().dock,
        crate::state::LayoutRequest::Toggle(crate::state::Panel::Machines),
    );
    h.run_steps(3);
    let screen = h.ctx.content_rect();
    let Some(leaf) = super::layout::leaf_rect(&h.state().dock, &Tab::Machines) else {
        panic!("Machines pane: the Machines leaf is not laid out");
    };
    assert!(screen.contains_rect(leaf), "Machines pane: {leaf:?} leaves the screen {screen:?}");
    assert_stack_in(&h, leaf, "Machines pane", &["Refresh", "New blank", "Select a machine to see its properties."]);
}

#[test]
fn layout_window_gcode() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.state_mut().state.post_view = Some(PostView {
        path: std::path::PathBuf::from("program.nc"),
        program: "%\nG00 G90 G54 X12.5 Y-40. S8000 M3\nM30\n%".to_owned(),
        line_pos: vec![None; 4],
        open: true,
        line: 0,
        blocks: Vec::new(),
        trace: false,
        not_judged: None,
    });
    h.run_steps(3);
    assert_window_layout(&h, "G-code - program", &["trace", "G00 G90 G54 X12.5 Y-40. S8000 M3", "M30"]);
}
