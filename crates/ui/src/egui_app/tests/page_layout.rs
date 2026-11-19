//! #1953: one small, named layout test for each page and window, so a UI
//! refit PR that moves one page updates that page's test in the same PR,
//! and a failure names the page. Behaviour (labels, roles, enabled state,
//! commands) is asserted elsewhere and holds without these positions.
//!
//! Each page test asserts the coarse law only: the page's anchor widgets
//! are in the Properties leaf and stack top to bottom in the given order.
//! The pages that also have a detailed layout test keep it:
//! - Stock: `stock_size::the_stock_page_is_laid_out_in_groups_like_the_wcs_page`
//! - setup Machine / Tool list / Settings / Fixtures: `machine_page`,
//!   `tool_list_page`, `setup_settings_page`, `fixture_rows`
//! - tool editor window: `structural::tool_editor_structure`,
//!   `structural::tool_cutter_params_structure`
//! - machine editor window: `structural::wire_machine_editor_structure`
//! - G-code window: `gcode_viewer`
//! - dock panes: `layout`
//!
//! Provenance: our own page law over the egui_kittest AccessKit tree; no
//! HSMWorks material involved.


use super::structural::{at, at_where, painted};
use super::*;
use crate::state::SetupPage;

type H = Harness<'static, ShellUi>;

/// The seeded shell with one setup (`Setup1`) and `t` targeted.
fn page(t: UiTarget) -> H {
    page_in(shell_harness(), t)
}

/// `page` in a harness of the caller's size.
fn page_in(mut h: H, t: UiTarget) -> H {
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, t);
    h.run_steps(3);
    h
}

/// The page law: each anchor is drawn, lies across the Properties leaf's
/// width, starts inside its height, and the anchors stack top to bottom.
fn assert_page_layout(h: &H, page: &str, anchors: &[&str]) {
    let Some(leaf) = super::layout::leaf_rect(&h.state().dock, &Tab::Properties) else {
        panic!("{page} page: the Properties leaf is not laid out");
    };
    let mut last: Option<(&str, f32)> = None;
    for a in anchors {
        let node = if *a == "Delete" {
            h.get_by_role_and_label(egui::accesskit::Role::Pane, "Properties").query_by_label(a)
        } else {
            h.query_by_label(a)
        };
        let Some(node) = node else {
            panic!("{page} page: no widget labelled {a:?}");
        };
        let r = node.rect();
        assert!(
            r.left() >= leaf.left() - 1.0 && r.right() <= leaf.right() + 1.0,
            "{page} page: {a:?} {r:?} leaves the Properties leaf {leaf:?} across"
        );
        assert!(
            r.top() >= leaf.top() - 1.0 && r.top() < leaf.bottom(),
            "{page} page: {a:?} {r:?} starts outside the Properties leaf {leaf:?}"
        );
        if let Some((prev, y)) = last {
            assert!(r.top() > y, "{page} page: {a:?} (top {}) must be under {prev:?} (top {y})", r.top());
        }
        last = Some((a, r.top()));
    }
}

/// Where the seeded shell's parts sit: the ribbon above the dock, the
/// three leaves in Browser | Viewport | Properties order on one row, the
/// seed's rows in the browser, the cube in the viewport, the page in the
/// right leaf and the status strip at the bottom. Moved here from the
/// structural skeleton (#1953).
pub(super) fn assert_shell_layout(h: &H) {
    let texts = painted(h);
    let tab_y = at(&texts, "Browser").y;
    for t in ["New", "Open", "Save", "Post", "Stock", "Origin", "Undo", "Redo", "Simulate"] {
        assert!(at(&texts, t).y < tab_y, "shell: ribbon text {t:?} must sit above the dock tabs");
    }
    let ver = crate::build_info::toolbar_version();
    let build = texts.iter().find(|(t, _)| t.starts_with(&ver)).map(|(_, p)| *p);
    assert!(build.is_some_and(|b| b.y < tab_y), "shell: the build identity sits on the ribbon");
    let (b, v, p) = (at(&texts, "Browser"), at(&texts, "Viewport"), at(&texts, "Properties"));
    assert!(b.x < v.x && v.x < p.x, "shell: dock order must be Browser | Viewport | Properties");
    assert!((b.y - v.y).abs() < 1.0 && (v.y - p.y).abs() < 1.0, "shell: the tab titles share a row");
    let in_browser = |q: egui::Pos2| q.y > b.y && q.x < v.x;
    for row in ["Setups", "SETUP", "Stock", "Fixtures", "MODELS", "Block", "TOOLS"] {
        at_where(&texts, row, in_browser);
    }
    assert!(
        texts.iter().any(|(t, q)| t.starts_with("Contour1") && in_browser(*q)),
        "shell: the seed's operation row is in the browser"
    );
    for t in ["TOP", "RIGHT", "BACK"] {
        let q = at(&texts, t);
        assert!(q.x > v.x && q.x < p.x, "shell: view cube face {t:?} belongs to the viewport");
    }
    assert!(h.get_by_label("Operation: Contour1").rect().min.x > v.x, "shell: the page is the right leaf");
    // The status strip is the lowest painted row: under the dock, and the
    // Debug info door on it.
    let status = h.state().state.status.clone();
    let s = at(&texts, &status);
    let props = super::layout::leaf_rect(&h.state().dock, &Tab::Properties);
    let Some(props) = props else { panic!("shell: the Properties leaf is not laid out") };
    assert!(s.y >= props.bottom() - 1.0, "shell: the status line {s:?} sits under the dock {props:?}");
    let door = h.get_by_label("Debug info").rect();
    assert!(door.top() >= props.bottom() - 1.0, "shell: Debug info {door:?} is on the status strip");
}

#[test]
fn layout_shell() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run_steps(3);
    assert_shell_layout(&h);
}

#[test]
fn layout_shell_light() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    theme_set(&h.state_mut().state.world, &ThemeRef::Named("light".to_owned()));
    h.run_steps(3);
    let laid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| assert_shell_layout(&h)));
    theme_set(&h.state_mut().state.world, &ThemeRef::Dark);
    h.run_steps(1);
    if let Err(e) = laid {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn layout_page_stock() {
    let _g = snapshot_guard();
    let h = page(UiTarget::Stock);
    assert_page_layout(&h, "Stock", &["Shape", "Size", "Position and rotation", "Fit to models", "Apply"]);
}

#[test]
fn layout_page_wcs() {
    let _g = snapshot_guard();
    // #870: the origin line under the heading makes the page one row
    // taller; at 720 px the compass's bottom row scrolls under the footer,
    // so this page is laid out in a 900 px window.
    let tall = Harness::builder()
        .with_size(Vec2::new(1280.0, 900.0))
        .build_ui_state(frame, ShellUi::seeded());
    let h = page_in(tall, UiTarget::Wcs);
    // #870: the one "Set origin" sits in the footer, under the page body.
    assert_page_layout(&h, "WCS", &["Turn X+", "Turn X-", "Level", "Bottom", "Pick markers in viewport", "NW", "SW", "Set origin"]);
}

#[test]
fn layout_page_model() {
    let _g = snapshot_guard();
    let h = page(UiTarget::Model("Block".into()));
    assert_page_layout(&h, "Model", &["Model: Block", "STEP facet budget (next import)", "Turn X+", "Turn Y-", "Apply"]);
}

#[test]
fn layout_page_tool_library() {
    let _g = snapshot_guard();
    let h = page(UiTarget::Tool(String::new()));
    assert_page_layout(&h, "Tool library", &["New tool…", "Import library…", "Import tool holders"]);
}

#[test]
fn layout_page_tool() {
    let _g = snapshot_guard();
    let h = page(UiTarget::Tool("EM-0600-4FL".into()));
    // The selected tool edits its fields directly in Properties; catalog
    // actions belong to the separate Tool library tab.
    assert_page_layout(
        &h,
        "Tool",
        &["Tool: EM-0600-4FL", "Delete", "Type", "Diameter", "Corner radius", "Flute length"],
    );
}

#[test]
fn layout_page_operation() {
    let _g = snapshot_guard();
    let h = page(UiTarget::Operation("Contour1".into()));
    assert_page_layout(
        &h,
        "Operation",
        &["Operation: Contour1", "Geometry", "Lead in / Lead out", "Models", "Edges", "Generate"],
    );
}

#[test]
fn layout_page_setups() {
    let _g = snapshot_guard();
    let h = page(UiTarget::Setups);
    assert_page_layout(&h, "Setups", &["Setup1", "Carry stock and WCS over", "Export setup sheets…"]);
}

#[test]
fn layout_page_setup_settings() {
    let _g = snapshot_guard();
    let h = page(UiTarget::SetupNode("Setup1".into(), SetupPage::Settings));
    assert_page_layout(&h, "Setup settings", &["Its own stock (the Stock node)", "+Y by Edge", "+X by Edge", "Place origin"]);
}

#[test]
fn layout_page_setup_machine() {
    let _g = snapshot_guard();
    let h = page(UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    assert_page_layout(&h, "Setup machine", &["This setup has its own machine", "Machine library", "Refresh machine list", "Load machine (.mch)"]);
}

#[test]
fn layout_page_setup_post() {
    let _g = snapshot_guard();
    let h = page(UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    assert_page_layout(
        &h,
        "Setup post",
        &["This setup has its own post", "Load post (.cps)", "Skip pauses and homes", "This setup has its own post options"],
    );
}

#[test]
fn layout_page_setup_fixtures() {
    let _g = snapshot_guard();
    let h = page(UiTarget::SetupNode("Setup1".into(), SetupPage::Fixtures));
    assert_page_layout(&h, "Setup fixtures", &["Import fixture…", "Add built-in vise…", "Add accessory…", "Add fixture plate…"]);
}

#[test]
fn layout_page_setup_tools() {
    let _g = snapshot_guard();
    let h = page(UiTarget::SetupNode("Setup1".into(), SetupPage::Tools));
    assert_page_layout(&h, "Setup tool list", &["Tools in this setup", "Export tool list…", "Export setup sheets…"]);
}

/// The Debug info panel (a dock tab since #2262): inside the screen, its
/// Copy button inside its tab's leaf.
#[test]
fn layout_window_debug_info() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.state_mut().state.layout_request = Some(crate::state::LayoutRequest::ToggleDebugInfo);
    h.run_steps(3);
    assert!(h.state().state.debug_info_open, "Debug info panel: not open");
    let screen = h.ctx.content_rect();
    let leaf = super::layout::leaf_rect(&h.state().dock, &Tab::DebugInfo).expect("Debug info panel: not docked");
    assert!(screen.contains_rect(leaf), "Debug info panel: leaf {leaf:?} leaves the screen {screen:?}");
    let copy = h.get_by_label("Copy").rect();
    assert!(leaf.contains_rect(copy), "Debug info panel: Copy {copy:?} leaves its leaf {leaf:?}");
}

/// The Setup fixtures page with one selected model: its Mark button sits
/// below the add buttons, shares their left edge and matches their width.
#[test]
fn layout_page_setup_fixtures_mark() {
    let _g = snapshot_guard();
    let mut h = page(UiTarget::SetupNode("Setup1".into(), SetupPage::Fixtures));
    {
        let st = &mut h.state_mut().state;
        st.pending.push_back(CommandKind::Select(Some("Block".into())));
        st.drain_actions();
    }
    h.run_steps(2);
    let mark = h.get_by_label("Mark Block as fixture").rect();
    let plate = h.get_by_label("Add fixture plate…").rect();
    let import = h.get_by_label("Import fixture…").rect();
    assert!(mark.top() > plate.bottom(), "Mark under the adds: {mark:?} vs {plate:?}");
    assert_eq!(mark.left(), import.left(), "Mark starts at the left edge: {mark:?} vs {import:?}");
    assert!((mark.width() - import.width()).abs() < 3.0, "Mark is as wide as the adds: {mark:?} vs {import:?}");
}

/// The G-code viewer window: inside the screen, a line wider than the
/// viewer stays on one row at the listing's left edge, and toggling trace
/// keeps the window the same size.
#[test]
fn layout_window_gcode_wrap_and_trace() {
    let _g = snapshot_guard();
    let lines = [
        "%",
        "G00 G90 G54 X12.5 Y-40. S8000 M3",
        "M30",
        "(A COMMENT FAR WIDER THAN THE VIEWER WINDOW - TOOL 1 IS A 6 MM FLAT END MILL, 3 FLUTES, 20 MM FLUTE LENGTH, 50 MM STICKOUT)",
        "%",
    ];
    let mut h = shell_harness();
    h.state_mut().state.post_view = Some(PostView {
        path: std::path::PathBuf::from("p.nc"),
        program: lines.join("\n"),
        line_pos: vec![None; lines.len()],
        open: true,
        line: 0,
        blocks: Vec::new(),
        trace: false,
        not_judged: None,
    });
    h.run();
    let screen = h.ctx.content_rect();
    let gcode_rect = |h: &H| {
        h.ctx
            .memory(|m| m.area_rect(egui::Id::new("gcode-viewer")))
            .expect("G-code window: not open")
    };
    let before = gcode_rect(&h);
    assert!(screen.contains_rect(before), "G-code window: {before:?} leaves the screen {screen:?}");
    let wide = h.get_by_label(lines[3]).rect();
    let short = h.get_by_label(lines[2]).rect();
    assert!(
        (wide.height() - short.height()).abs() < 0.5,
        "G-code window: wide line {wide:?} wrapped vs {short:?}"
    );
    assert!(
        (wide.left() - short.left()).abs() < 0.5,
        "G-code window: wide line {wide:?} left differs from {short:?}"
    );
    h.get_by_label("trace").click();
    h.run_steps(5);
    let after = gcode_rect(&h);
    assert!(
        (after.width() - before.width()).abs() < 1.0 && (after.height() - before.height()).abs() < 1.0,
        "G-code window went {before:?} -> {after:?} when trace was ticked"
    );
    h.get_by_label("trace").click();
    h.run_steps(5);
    let back = gcode_rect(&h);
    assert!(
        (back.width() - before.width()).abs() < 1.0 && (back.height() - before.height()).abs() < 1.0,
        "G-code window went {before:?} -> {back:?} when trace was cleared"
    );
}
