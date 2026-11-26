//! #2002: selected tool settings and the catalog occupy separate dock surfaces.
//! #3930: a saved tool's Properties page is a read-only summary with the
//! door "Edit in tool library"; the tool library window edits the tool.
//! Provenance: our own structural tests over the shipping browser and ribbon handlers.
//! Gate map: covers ui::egui_app::pages, ui::egui_app::layout, ui::egui_app::browser_pane,
//! ui::egui_app::tool_settings, ui::egui_app::tool_summary, ui::egui_app::tool_library_dialog.

use super::tool_library_assembly::{open_body, tall_window};
use super::tool_library_editor::{H, click_window_button, draft, library_window, tool};
use super::*;
use crate::egui_app::tool_summary::EDIT_IN_LIBRARY;

const TOOL: &str = "EM-0600-4FL";

fn select_tool(h: &mut Harness<'static, ShellUi>) {
    super::super::super::browser_pane::click(
        &mut h.state_mut().state,
        panes::Leaf::Tool(TOOL.into()),
    );
    h.run_steps(3);
}

/// Whether a node carries `label`. A property row names its control by
/// the row's label (#1119), so a row can match twice; `is_some` on the
/// single-match query would panic.
fn here(h: &Harness<'static, ShellUi>, label: &str) -> bool {
    h.query_all_by_label(label).next().is_some()
}

/// The number fields of the Properties pane.
fn props_spins(h: &Harness<'static, ShellUi>) -> usize {
    h.get_by_role_and_label(egui::accesskit::Role::Pane, "Properties")
        .query_all_by_role(egui::accesskit::Role::SpinButton)
        .count()
}

#[test]
fn setup_tool_list_edit_opens_tool_settings_in_properties() {
    let mut h = shell_harness();
    super::super::super::browser_pane::click(
        &mut h.state_mut().state,
        panes::Leaf::Page(String::new(), crate::state::SetupPage::Tools),
    );
    h.run_steps(3);
    h.get_by_label("Edit").click_accesskit();
    h.run_steps(3);
    assert_eq!(h.state().state.ui_target, UiTarget::Tool(TOOL.into()), "the setup's tool row opens the named Properties subject");
    assert!(h.state().state.tool_shape_editor.is_none(), "the setup tool list edits Properties directly");
    assert!(here(&h, "Diameter"));
    assert!(h.query_by_label("Import library…").is_none());
}

#[test]
fn browser_library_door_opens_the_window_and_tool_selection_focuses_properties() {
    let mut h = shell_harness();
    h.get_by_label("View").click();
    h.run_steps(2);
    h.get_by_label("Tool library…").click();
    h.run_steps(3);
    assert!(h.query_by_label("Import library…").is_some(), "the browser door opens the catalog");
    select_tool(&mut h);
    assert!(here(&h, "Diameter"), "selecting a tool brings Properties in front of the catalog");
    // #3892: the library is a window beside the dock, so it stays open.
    assert!(h.query_by_label("Import library…").is_some(), "the library window stays open beside Properties");
    crate::actions::open_tool_shape_editor(&mut h.state_mut().state, TOOL);
    h.run_steps(2);
    assert!(h.state().state.tool_shape_editor.is_some());
    select_tool(&mut h);
    assert!(h.state().state.tool_shape_editor.is_none(), "browser selection closes the explicit tool editor");
    assert!(here(&h, "Diameter"), "the tool's settings return to Properties");
}

#[test]
fn selected_tool_settings_live_in_properties_without_the_catalog() {
    let mut h = shell_harness();
    select_tool(&mut h);
    assert!(h.state().state.tool_shape_editor.is_none(), "selection must show Properties directly");
    let properties_x = super::at(&painted(&h), "Properties").x;
    // #3930: the summary states the diameter; it is not a field.
    let diameter = h.get_by_label("6.000 mm").rect();
    assert!(diameter.min.x > properties_x && diameter.max.x <= 1280.0, "the summary belongs to Properties");
    assert_eq!(props_spins(&h), 0, "the summary has no number field");
    for label in ["New tool…", "Import library…", "Import tool holders", "Support features"] {
        assert!(h.query_by_label(label).is_none(), "catalog control {label:?} must not occupy Properties");
    }
    assert_eq!(h.state().state.ui_target, UiTarget::Tool(TOOL.into()));
}

/// #3930: Properties' summary opens the tool in the library window; a
/// drag there stages the window's draft only, Apply lands it through
/// `ChangeTool`, and Undo restores the tool. Before:
/// `properties_tool_apply_and_undo_use_the_command_history`.
#[test]
fn edit_in_tool_library_apply_and_undo_use_the_command_history() {
    let mut h = shell_harness();
    select_tool(&mut h);
    h.get_by_label(EDIT_IN_LIBRARY).click();
    h.run_steps(3);
    let before = tool(&h, TOOL).diameter;
    let at = library_window(&h)
        .query_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|n| n.value().as_deref() == Some("6.0 mm"))
        .expect("the window's diameter number")
        .rect()
        .center();
    h.drag_at(at);
    h.run();
    h.hover_at(at + Vec2::new(30.0, 0.0));
    h.run();
    h.drop_at(at + Vec2::new(30.0, 0.0));
    h.run();
    let staged = draft(&mut h).diameter;
    assert!((staged - (before + 3.0)).abs() < 0.05, "the drag changes the window's draft");
    assert_eq!(tool(&h, TOOL).diameter, before, "a draft does not write the world");
    click_window_button(&mut h, "Apply");
    assert_eq!(tool(&h, TOOL).diameter, staged, "Apply lands the draft through ChangeTool: {}", h.state().state.status);
    assert!(h.query_by_label(&format!("{} mm", units::len(staged, Unit::Millimeters, 3))).is_some(), "the summary follows the tool");
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(tool(&h, TOOL).diameter, before, "Undo restores the tool's previous numbers");
}

/// #3930: the door opens the window on the selected tool; Properties keeps
/// its subject, and a second click keeps the one window.
#[test]
fn edit_in_tool_library_opens_the_window_on_the_tool() {
    let mut h = shell_harness();
    select_tool(&mut h);
    assert!(h.state().state.tool_library.is_none(), "the window starts closed");
    h.get_by_label(EDIT_IN_LIBRARY).click();
    h.run_steps(3);
    assert_eq!(h.state().state.tool_library.as_ref().and_then(|v| v.open_tool()), Some(TOOL), "the window opens on the tool");
    assert!(library_window(&h).query_all_by_label(&format!("Tool: {TOOL}")).next().is_some(), "the editor names the tool");
    assert_eq!(h.state().state.ui_target, UiTarget::Tool(TOOL.into()), "Properties keeps its subject");
    h.get_by_label(EDIT_IN_LIBRARY).click();
    h.run_steps(3);
    let windows = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .filter(|n| n.accesskit_node().label().as_deref() == Some("Tool library"))
        .count();
    assert_eq!(windows, 1, "a second click keeps the one window");
}

#[test]
fn tools_ribbon_opens_the_tool_library_window_without_retargeting_properties() {
    let mut h = shell_harness();
    h.run_steps(2);
    let before = h.state().state.ui_target.clone();
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Tools").click();
    h.run_steps(3);
    assert_eq!(h.state().state.ui_target, before, "opening the catalog preserves the Properties subject");
    assert!(painted(&h).iter().any(|(t, _)| t == "Tool library"), "the window's title is painted");
    assert!(h.query_by_label("Catalog").is_some(), "the catalog is visible");
    assert_eq!(h.get_by_label("Import tool holders").accesskit_node().role(), egui::accesskit::Role::CheckBox);
    assert_eq!(h.get_by_label("New tool…").accesskit_node().role(), egui::accesskit::Role::Button);
    // #3892: the library is its own window now, not a dock tab.
    assert!(h.query_all_by_role(egui::accesskit::Role::Window).any(|n| n.accesskit_node().label().as_deref() == Some("Tool library")), "the library opens its own window");
    assert!(!crate::egui_app::layout::is_open(&h.state().dock, &Tab::Tools), "the dock gets no Tool library tab");
    h.get_by_role_and_label(egui::accesskit::Role::Button, "Tools").click();
    h.run_steps(3);
    assert!(h.query_by_label("Catalog").is_some(), "opening the library twice keeps it open");
    assert!(h.state().state.tool_library.is_some(), "the second click does not close the window");
    assert_eq!(h.state().state.ui_target, before);
}

/// #2014: the tool settings are a property grid - each label left of its
/// input. #3930: the groups moved from Properties into the tool library
/// window: Geometry is its Tool tab, Holder its folded Body section,
/// Cutting data its Recipes tab and Catalog its Details tab. Each row
/// still puts its label left of its input, and the Body section folds.
/// Before: `tool_settings_form_a_label_left_grid_in_collapsible_groups`.
#[test]
fn tool_settings_form_a_label_left_grid_in_the_window_tabs() {
    let mut h = tall_window(TOOL);
    let in_win = |h: &H, label: &str| library_window(h).query_all_by_label(label).next().is_some();
    for label in ["Type", "Diameter", "Corner radius", "Flute length", "Shank diameter", "Shank length"] {
        assert!(in_win(&h, label), "the Tool tab holds the {label:?} row");
    }
    assert!(!in_win(&h, "Holder diameter"), "a folded Body section hides its rows");
    open_body(&mut h);
    for label in ["Holder diameter", "Holder length"] {
        assert!(in_win(&h, label), "the Body section holds the {label:?} row");
    }
    // Every row puts the label on the left of its input, on one line.
    for (label, value) in [
        ("Diameter", "6.0 mm"),
        ("Flute length", "15.0 mm"),
        ("Holder diameter", "44.0 mm"),
        ("Shank length", "40.0 mm"),
    ] {
        let win = library_window(&h).rect();
        let l = super::structural::at_where(&painted(&h), label, |p| win.contains(p));
        let input = library_window(&h)
            .query_all_by_role(egui::accesskit::Role::SpinButton)
            .find(|n| n.value().as_deref() == Some(value))
            .unwrap_or_else(|| panic!("{label:?} shows its input {value:?}"))
            .rect();
        assert!(l.x < input.center().x, "{label:?} label must sit left of its input: {l:?} vs {input:?}");
        assert!((l.y - input.center().y).abs() < 15.0, "{label:?} label and input must share a row: {l:?} vs {input:?}");
    }
    // The Body section header folds its rows away again.
    open_body(&mut h);
    assert!(!in_win(&h, "Holder diameter"), "a folded Body section hides its rows again");
    assert!(in_win(&h, "Diameter"), "the Tool tab's rows stay");
    click_window_button(&mut h, crate::egui_app::tool_library_dialog::recipes_tab::TAB);
    assert!(in_win(&h, "Centre cutting (may plunge into solid)"), "the Recipes tab holds centre cutting");
    assert!(in_win(&h, "Spindle rpm"), "the Recipes tab holds the spindle rpm");
    click_window_button(&mut h, "Details");
    for label in ["Material group", "Part number", "Tool number"] {
        assert!(in_win(&h, label), "the Details tab holds the {label:?} row");
    }
}
/// #2397: Properties shows no body or profile editor; the tool shape
/// editor has its own window. #3930: Properties shows a read-only summary
/// (no number field), and the holder picker left the shape window. Before:
/// `properties_shows_the_settings_and_the_shape_window_holds_the_shape`.
#[test]
fn properties_shows_a_summary_and_the_shape_window_holds_the_shape() {
    let mut h = shell_harness();
    select_tool(&mut h);
    for label in [
        "Holder library",
        "Library holder",
        "Apply holder",
        "Apply profile",
        "Import body…",
        "Apply body",
        "Profile",
        "Draw holder",
        "Apply",
    ] {
        assert!(h.query_by_label(label).is_none(), "Properties must not show the control {label:?}");
    }
    assert_eq!(props_spins(&h), 0, "the summary has no number field");
    for label in ["Type", "Diameter", "Flute length", "Shank diameter", "Holder", "Tool number"] {
        assert!(here(&h, label), "the summary states {label:?}");
    }
    assert!(h.query_by_label(EDIT_IN_LIBRARY).is_some(), "Properties offers the door to the tool library window");
    assert!(h.query_by_label("Edit tool shape").is_some(), "Properties offers the door to the shape editor");
    h.get_by_label("Edit tool shape").click();
    h.run_steps(3);
    assert_eq!(
        h.state().state.tool_shape_editor.as_deref(),
        Some(TOOL),
        "the door opens the shape window"
    );
    let win = h
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool shape: EM-0600-4FL"))
        .expect("the shape window is titled by the tool");
    for label in ["Body", "Profile", "Close"] {
        assert!(win.query_by_label(label).is_some(), "the shape window shows {label:?}");
    }
    for label in ["Holder library", "Library holder", "Apply holder"] {
        assert!(win.query_by_label(label).is_none(), "the holder pick left the shape window: {label:?}");
    }
    // The summary stays on the page behind it.
    assert!(h.query_by_label("Shank diameter").is_some(), "Properties keeps the summary");
}

/// #2007: the Properties tool page keeps its action row (Add tool beside
/// Close) fixed at the bottom without scrolling, and uses plain labels
/// without "(0 = next free)". #3930: a saved tool's summary has no Apply;
/// its door and Close stay in view. Before:
/// `tool_page_keeps_apply_and_add_tool_beside_close_without_scrolling`.
#[test]
fn tool_page_keeps_its_buttons_and_add_tool_beside_close_without_scrolling() {
    let mut h = shell_harness();
    select_tool(&mut h);
    let leaf = super::super::layout::leaf_rect(&h.state().dock, &Tab::Properties).expect("Properties leaf");
    let door = h.get_by_label(EDIT_IN_LIBRARY).rect();
    let close = h.get_by_label("Close").rect();
    assert!(leaf.expand(1.0).contains_rect(door), "{EDIT_IN_LIBRARY} {door:?} must stay visible in Properties {leaf:?}");
    assert!(leaf.expand(1.0).contains_rect(close), "Close {close:?} must stay visible in Properties {leaf:?} without scrolling");
    assert!(h.query_by_label("Apply").is_none(), "a summary has nothing to apply");

    crate::actions::begin_new_tool(&mut h.state_mut().state);
    h.run_steps(3);
    let add = h.get_by_label("Add tool").rect();
    let close = h.get_by_label("Close").rect();
    assert!(leaf.expand(1.0).contains_rect(add), "Add tool {add:?} must stay visible in Properties {leaf:?} without scrolling");
    assert!(leaf.expand(1.0).contains_rect(close), "Close {close:?} must stay visible in Properties {leaf:?} without scrolling");
    assert!((add.center().y - close.center().y).abs() < 4.0, "Add tool {add:?} and Close {close:?} share a row");
    assert!(add.max.x <= close.min.x, "Add tool sits left of Close: {add:?} vs {close:?}");
    assert!(h.query_by_label("Tool number (0 = next free)").is_none(), "no raw (0 = next free) label on Tool number");
    assert!(here(&h, "Tool number"), "Catalog shows plain Tool number");

    h.state_mut().state.tool_ui.name.clear();
    h.run_steps(3);
    let warn = h.get_by_label("Name the tool first.").rect();
    let add = h.get_by_label("Add tool").rect();
    let close = h.get_by_label("Close").rect();
    assert!(leaf.expand(1.0).contains_rect(warn), "block message {warn:?} stays visible in {leaf:?}");
    assert!(leaf.expand(1.0).contains_rect(add), "Add tool {add:?} stays visible with message in {leaf:?}");
    assert!(leaf.expand(1.0).contains_rect(close), "Close {close:?} stays visible with message in {leaf:?}");
}