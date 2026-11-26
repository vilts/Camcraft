//! #3892 (U1 of #3663): the named layout checks of the tool library
//! window. `page_layout.rs` is full, so the window's tests live here
//! (#1953: one named layout test for each page and window).
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::actions, ui::state.
//! Provenance: our own layout law over the egui_kittest AccessKit tree; no
//! HSMWorks material involved.

use super::page_layout_windows::assert_window_layout;
use crate::egui_app::tool_library_dialog::recipes_tab::{MARK, SUGGEST, TAB};
use crate::egui_app::tool_library_dialog::{HOLDERS_PAGE, TOOLS_PAGE, categories, holder_view, holders, preview};
use super::*;
use egui_kittest::kittest::NodeT;

type H = Harness<'static, ShellUi>;

fn library_window(h: &H) -> egui::Rect {
    h.query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool library"))
        .map(|n| n.rect())
        .unwrap_or_else(|| panic!("the Tool library window is not open"))
}

/// The tool library is its own window: (#3897) the Cutting tools | Holders
/// switch first, then the file buttons, the source row, then the Document
/// list with the categories on its left. It floats beside the dock and
/// leaves the Properties pane free, so both can be used at once.
#[test]
fn layout_window_tool_library() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Tools").click();
    h.run_steps(3);
    assert_window_layout(
        &h,
        "Tool library",
        &[TOOLS_PAGE, "New tool…", "Import tool holders", "Document", "Recipes"],
    );
    let win = library_window(&h);
    // #3897: the switch heads the window; the categories stand left of the rows.
    let node = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool library"))
        .unwrap_or_else(|| panic!("the Tool library window is not open"));
    let rect = |l: &str| node.query_all_by_label(l).next().unwrap_or_else(|| panic!("no {l:?} in the window")).rect();
    let (tools, holders) = (rect(TOOLS_PAGE), rect(HOLDERS_PAGE));
    assert!((tools.top() - holders.top()).abs() < 2.0 && tools.right() <= holders.left() + 1.0, "Cutting tools {tools:?} | Holders {holders:?}");
    let all = rect(categories::ALL);
    assert!(all.right() < rect("Recipes").left(), "the categories {all:?} stand left of the rows");
    let Some(props) = super::layout::leaf_rect(&h.state().dock, &Tab::Properties) else {
        panic!("the Properties leaf is not laid out");
    };
    assert!(
        !win.intersects(props),
        "the window {win:?} stands beside the Properties pane {props:?}, not over it"
    );
}

/// A closed window (its ✕ clears `state.tool_library`) opens again from
/// the ribbon; the dock never gets a Tool library tab.
#[test]
fn the_tool_library_window_closes_and_opens_again() {
    let mut h = shell_harness();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Tools").click();
    h.run_steps(3);
    let _ = library_window(&h);
    assert!(!crate::egui_app::layout::is_open(&h.state().dock, &Tab::Tools), "no dock tab");
    h.state_mut().state.tool_library = None;
    h.run_steps(2);
    assert!(
        !h.query_all_by_role(egui::accesskit::Role::Window)
            .any(|n| n.accesskit_node().label().as_deref() == Some("Tool library")),
        "a closed window is gone"
    );
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Tools").click();
    h.run_steps(3);
    let _ = library_window(&h);
}

/// #3893: a tool open in the window. The list folds into a rail on the
/// left; the editor stands right of it: the heading, the Tool and Details
/// tabs, the rows, then Apply at the foot. #3897: the preview (its 3D /
/// Side toggle and the Dimensions and Holder toggles) stands right of the
/// rows, under the heading. #3896: the Recipes tab stands between Tool and Details, and
/// its Suggest row stands under the tabs. The window still leaves the
/// Properties pane free. Queries stay inside the window, because a tool
/// can be open in Properties too.
#[test]
fn layout_window_tool_editor() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Tools").click();
    h.run_steps(2);
    let Some(name) = h.state().state.tool_names().into_iter().next() else {
        panic!("the seeded document has a tool");
    };
    h.state_mut().state.open_tool_in_library(&name);
    h.run_steps(3);
    let win = library_window(&h);
    let screen = h.ctx.content_rect();
    assert!(screen.contains_rect(win), "the window {win:?} stays on the screen {screen:?}");
    let node = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool library"))
        .unwrap_or_else(|| panic!("the Tool library window is not open"));
    let rect = |label: &str| {
        node.query_all_by_label(label)
            .next()
            .unwrap_or_else(|| panic!("the editor has no widget labelled {label:?}"))
            .rect()
    };
    let heading = format!("Tool: {name}");
    let mut last: Option<(&str, f32)> = None;
    for a in [heading.as_str(), "Details", "Diameter", "Apply"] {
        let r = rect(a);
        assert!(win.expand(1.0).contains_rect(r), "{a:?} {r:?} leaves the window {win:?}");
        if let Some((prev, y)) = last {
            assert!(r.top() > y, "{a:?} (top {}) must be under {prev:?} (top {y})", r.top());
        }
        last = Some((a, r.top()));
    }
    // #3895: the assembly group follows the geometry rows in the Tool tab
    // (it may stand below the fold of the scroll area).
    let group = rect(crate::egui_app::tool_library_dialog::assembly_group::HEADING);
    assert!(group.top() > rect("Diameter").top(), "the assembly group {group:?} stands under the geometry rows");
    assert!(group.left() >= win.left() - 1.0 && group.right() <= win.right() + 1.0, "the group {group:?} keeps to the window's width");
    // #3930: the folded Body section follows the assembly group.
    let body = rect(crate::egui_app::tool_library_dialog::assembly_group::BODY);
    assert!(body.top() > group.top(), "the Body section {body:?} stands under the assembly group {group:?}");
    assert!(node.query_all_by_label("Import body…").next().is_none(), "the Body section starts folded");
    let (view, dims, holder) = (rect("3D"), rect(preview::DIMS), rect(preview::HOLDER));
    assert!(view.top() > rect(&heading).top(), "the preview {view:?} stands under the heading");
    assert!(view.left() > rect("Diameter").right(), "the preview {view:?} stands right of the rows");
    assert!(dims.left() > view.left() && holder.left() > dims.left(), "3D, Dimensions, Holder on one row");
    assert!(win.expand(1.0).contains_rect(holder), "the toggles {holder:?} stay in the window {win:?}");
    // #3908: Flutes follows Holder on its row, or wraps under it.
    let flutes = rect(preview::FLUTES);
    let beside = flutes.left() > holder.left() && (flutes.center().y - holder.center().y).abs() < 2.0;
    assert!(beside || flutes.top() >= holder.bottom() - 1.0, "Flutes {flutes:?} follows Holder {holder:?}");
    assert!(win.expand(1.0).contains_rect(flutes), "the Flutes toggle {flutes:?} stays in the window {win:?}");
    let rail = rect("All tools");
    let row = rect(&name);
    assert!(rail.right() < rect("Diameter").left() && row.right() < rect(&heading).left(), "the rail {rail:?} stands left of the editor");
    assert!(node.query_all_by_label("Import library…").next().is_none(), "the file row folds away with the list");
    let Some(props) = super::layout::leaf_rect(&h.state().dock, &Tab::Properties) else {
        panic!("the Properties leaf is not laid out");
    };
    assert!(!win.intersects(props), "the window {win:?} stands beside the Properties pane {props:?}");
    let (recipes, details) = (rect(TAB), rect("Details"));
    assert!(
        (recipes.top() - details.top()).abs() < 2.0 && recipes.right() <= details.left() + 1.0,
        "the Recipes tab {recipes:?} stands left of Details {details:?} on the tab row"
    );
    super::tool_library_editor::click_window_button(&mut h, TAB);
    let node = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool library"))
        .unwrap_or_else(|| panic!("the Tool library window is not open"));
    let rect = |label: &str| {
        node.query_all_by_label(label)
            .next()
            .unwrap_or_else(|| panic!("the Recipes tab has no widget labelled {label:?}"))
            .rect()
    };
    let (tab, suggest, apply) = (rect(TAB), rect(SUGGEST), rect("Apply"));
    assert!(win.expand(1.0).contains_rect(suggest), "Suggest {suggest:?} stays in the window {win:?}");
    assert!(tab.top() < suggest.top() && suggest.top() < apply.top(), "Suggest {suggest:?} stands between the tabs and Apply");
    assert!(node.query_all_by_label(MARK).next().is_none() || rect(MARK).top() > suggest.top(), "the table follows the Suggest row");
    assert!(node.query_all_by_label("Diameter").next().is_none(), "the geometry rows stay on the Tool tab");
}

/// #3897 (U6): the Holders page. The taper column stands left of the
/// holder rows; the filter and the read-only note head the rows. A holder
/// opens in its view: the way back and the name first, the facts on the
/// left, the drawing's Dimensions toggle right of them.
#[test]
fn layout_window_holders() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run_steps(2);
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Tools").click();
    h.run_steps(2);
    super::tool_library_editor::click_window_button(&mut h, HOLDERS_PAGE);
    assert_window_layout(&h, "Tool library", &[HOLDERS_PAGE, "Holder filter", holders::READ_ONLY, "Used by"]);
    let win = library_window(&h);
    let node = super::tool_library_editor::library_window(&h);
    let rect = |l: &str| node.query_all_by_label(l).next().unwrap_or_else(|| panic!("no {l:?} in the window")).rect();
    let (all, name) = (rect(holders::ALL), rect("Name"));
    assert!(all.right() < name.left(), "the taper column {all:?} stands left of the rows {name:?}");
    assert!(win.expand(1.0).contains_rect(rect("Used by")), "the last head stays in the window");
    let Some(props) = super::layout::leaf_rect(&h.state().dock, &Tab::Properties) else {
        panic!("the Properties leaf is not laid out");
    };
    assert!(!win.intersects(props), "the window {win:?} stands beside the Properties pane {props:?}");
    super::tool_library_holders_ui::filter_to_holder(&mut h);
    super::tool_library_editor::click_window_button(&mut h, "BT40 ER32");
    let node = super::tool_library_editor::library_window(&h);
    let rect = |l: &str| node.query_all_by_label(l).next().unwrap_or_else(|| panic!("no {l:?} in the holder view")).rect();
    let (back, taper, dims) = (rect(holder_view::BACK), rect("Spindle taper"), rect(holder_view::DIMS));
    assert!(back.top() < taper.top(), "the way back {back:?} heads the view");
    assert!(dims.left() > taper.right(), "the drawing's toggle {dims:?} stands right of the facts {taper:?}");
    assert!(win.expand(1.0).contains_rect(dims), "the toggle stays in the window {win:?}");
}
