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
pub(super) fn assert_window_layout(h: &H, title: &str, anchors: &[&str]) {
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

// #2397: the tool shape editor window - the Body / Profile tabs, then
// Close. #3930: the holder library left it for the tool library window.
#[test]
fn layout_window_tool_shape() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run_steps(2);
    crate::actions::open_tool_shape_editor(&mut h.state_mut().state, "EM-0600-4FL");
    h.run_steps(3);
    // The Profile tab: the points and the side view stand in the right
    // column, the outline controls in the left.
    h.get_by_label("Profile").click();
    h.run_steps(2);

    assert_window_layout(
        &h,
        "Tool shape: EM-0600-4FL",
        &["Body", "Holder profile", "Parametric", "Apply profile"],
    );
    assert!(h.query_by_label("Holder library").is_none(), "the shape window has no holder library");
    // Close is the window's fixed footer, under the shape.
    let win = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool shape: EM-0600-4FL"))
        .expect("the shape window");
    let close = win.get_by_label("Close");
    assert!(
        win.rect().contains_rect(close.rect()),
        "Close {close:?} stands inside the shape window {win:?}"
    );
    assert!(
        close.rect().top() > h.get_by_label("Apply profile").rect().top(),
        "Close is the window's footer, under the profile controls"
    );
    // #2007 through #2397: the fixed frame keeps every door inside it.
    let win_rect = win.rect();
    for a in ["Draw holder", "Draw cutter", "Import DXF…", "Export DXF…", "Import STEP…"] {
        for r in h.query_all_by_label(a).map(|n| n.rect()) {
            assert!(win_rect.contains_rect(r), "Tool shape window: {a:?} {r:?} leaves the window {win_rect:?}");
        }
    }
    // #2007 through #2397: the points table keeps all its rows - one
    // remove button for each point, and the holder line stands under
    // the last row, not across the table body. The table sits in the
    // view column, below its "height mm" header.
    let below = win.get_by_label("height mm").rect().bottom();
    let col = win.get_by_label("height mm").rect().left();
    let rows: Vec<egui::Rect> = win
        .query_all_by_label("×")
        .map(|n| n.rect())
        .filter(|r| r.top() > below && r.left() > col - 1.0)
        .collect();
    assert_eq!(rows.len(), h.state().state.tool_profile_ui.points.len(), "one remove button for each point");
    let last = rows.iter().fold(f32::MIN, |m, r| m.max(r.bottom()));
    let under = win.query_all_by_label_contains("holder Ø").next().expect("the holder line under the table").rect();
    assert!(under.top() >= last - 1.0, "Tool shape window: the table hides rows - its last row ends at {last}, the line under it starts at {}", under.top());
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

#[test]
fn layout_window_overwrite_prompt() {
    // #3608: a Save held because the project file on disk changed.
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.state_mut().state.save_guard.held = Some(std::path::PathBuf::from("job.camproj"));
    h.run_steps(3);
    assert_modal_layout(
        &h,
        "overwrite-prompt",
        "Overwrite modal",
        &[
            "job.camproj: the file on disk changed",
            "job.camproj on disk changed after this window opened or saved it. \
             Overwrite puts your document in place of that file. \
             Save As writes your document to a new file and does not change the file on disk.",
            "Overwrite",
        ],
        "Cancel",
    );
}

#[test]
fn layout_window_dxf_units() {
    // #1863: a drawing with no $INSUNITS opens the units prompt.
    let _g = snapshot_guard();
    let mut h = shell_harness();
    let dxf = "0\nSECTION\n2\nENTITIES\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n5\n0\nENDSEC\n0\nEOF\n";
    let source = crate::state::DxfSource::Bytes { name: "free.dxf".into(), data: std::sync::Arc::from(dxf.as_bytes()) };
    h.state_mut().state.pending.push_back(CommandKind::ImportDxf { source, units: None });
    h.run_steps(3);
    assert_modal_layout(&h, "dxf-units-prompt", "DXF units modal", &["DXF units", "Millimeters"], "Cancel");
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

/// #2339: the Panel setups window - the save row, the saved list, the
/// auto-load choice and the reset, top to bottom, inside the window.
#[test]
fn layout_window_panel_setups() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.state_mut().state.panel_setups_open = true;
    h.run_steps(3);
    assert_window_layout(
        &h,
        "Panel setups",
        &[
            "Save the open panels as they stand, or put a saved setup back.",
            "Saved setups",
            "No saved setups yet.",
            "Load the last setup when CamCraft starts",
            "Reset to default",
        ],
    );
}

/// #2818: the Display menu of the viewport toolbar. It opens above its
/// button, inside the viewport, with its groups from top to bottom.
#[test]
fn layout_window_display_menu() {
    let mut h = shell_harness();
    h.run();
    let pane = super::layout::leaf_rect(&h.state().dock, &Tab::Viewport).expect("viewport leaf");
    let button = hud(h.state()).nav_rect(viewcube::Nav::Display);
    let bar = crate::egui_app::viewport_bar::bar_rect(pane);
    assert!(bar.contains_rect(button), "the Display button sits in the toolbar");
    click_viewport(&mut h, button.center());
    assert_stack_in(
        &h,
        pane,
        "Display menu",
        &[
            "Display mode",
            "Shaded, with edges",
            "Shaded",
            "Wireframe",
            "Wireframe, see-through",
            "Shaded, with hidden edges",
            "Look at",
            "Look at a face",
            "Look at a work plane",
            "Graphics",
            "Grid",
            "Origin axes",
            "Perspective view",
            "Orthographic view",
            "Line width",
            "Anti-aliasing",
            "Performance",
            "Quality",
            "Ground shadow",
            "Ambient occlusion",
        ],
    );
    let last = h.get_by_label("Ambient occlusion").rect();
    assert!(last.bottom() <= button.top(), "the menu stands above the Display button");
}
