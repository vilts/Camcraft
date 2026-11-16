//! #2002: selected tool settings and the catalog occupy separate dock surfaces.
//! Provenance: our own structural tests over the shipping browser and ribbon handlers.
//! Gate map: covers ui::egui_app::pages, ui::egui_app::layout, ui::egui_app::browser_pane.

use super::*;

const TOOL: &str = "EM-0600-4FL";

fn select_tool(h: &mut Harness<'static, ShellUi>) {
    super::super::super::browser_pane::click(
        &mut h.state_mut().state,
        panes::Leaf::Tool(TOOL.into()),
    );
    h.run_steps(3);
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
    assert!(h.state().state.tool_editor.is_none(), "the setup tool list edits Properties directly");
    assert!(h.query_by_label("Diameter").is_some());
    assert!(h.query_by_label("Catalog").is_none());
    assert!(h.query_by_label("Import library…").is_none());
}

#[test]
fn browser_library_door_and_tool_selection_focus_their_own_dock_tabs() {
    let mut h = shell_harness();
    h.get_by_label("View").click();
    h.run_steps(2);
    h.get_by_label("Tool library…").click();
    h.run_steps(3);
    assert!(h.query_by_label("Import library…").is_some(), "the browser door opens the catalog");
    select_tool(&mut h);
    assert!(h.query_by_label("Diameter").is_some(), "selecting a tool brings Properties in front of the catalog");
    assert!(h.query_by_label("Import library…").is_none(), "the catalog stays behind Properties");
    crate::actions::open_tool_editor(&mut h.state_mut().state, TOOL);
    h.run_steps(2);
    assert!(h.state().state.tool_editor.is_some());
    select_tool(&mut h);
    assert!(h.state().state.tool_editor.is_none(), "browser selection closes the explicit tool editor");
    assert!(h.query_by_label("Diameter").is_some(), "the tool's settings return to Properties");
}

#[test]
fn selected_tool_settings_live_in_properties_without_the_catalog() {
    let mut h = shell_harness();
    select_tool(&mut h);
    assert!(h.state().state.tool_editor.is_none(), "selection must edit Properties directly");
    let label = h.query_by_label("Diameter").expect("Properties shows the selected tool's Diameter");
    assert_eq!(label.accesskit_node().role(), egui::accesskit::Role::Label);
    let properties_x = super::at(&painted(&h), "Properties").x;
    assert!(label.rect().max.x > properties_x && label.rect().max.x <= 1280.0, "settings belong to Properties");
    assert!(h.query_all_by_role(egui::accesskit::Role::SpinButton).any(|n| n.value().as_deref() == Some("6.0 mm")), "editable tool diameter uses the document value");
    for label in ["Catalog", "New tool…", "Import library…", "Import tool holders", "Support features"] {
        assert!(h.query_by_label(label).is_none(), "catalog control {label:?} must not occupy Properties");
    }
    assert_eq!(h.state().state.ui_target, UiTarget::Tool(TOOL.into()));
}

#[test]
fn properties_tool_apply_and_undo_use_the_command_history() {
    let mut h = shell_harness();
    select_tool(&mut h);
    let diameter = |h: &Harness<'static, ShellUi>| {
        model::find_by_name(&h.state().state.world, TOOL)
            .and_then(|e| model::tool_of(&h.state().state.world, &e))
            .expect("selected tool").diameter
    };
    assert!(h.query_by_label("Diameter").is_some(), "Properties exposes the editable diameter");
    let before = diameter(&h);
    let at = h.query_all_by_role(egui::accesskit::Role::SpinButton)
        .find(|n| n.value().as_deref() == Some("6.0 mm"))
        .expect("diameter number").rect().center();
    h.drag_at(at);
    h.run();
    h.hover_at(at + Vec2::new(30.0, 0.0));
    h.run();
    h.drop_at(at + Vec2::new(30.0, 0.0));
    h.run();
    let staged = h.state().state.tool_ui.diameter;
    assert!((staged - (before + 3.0)).abs() < 0.05, "the drag changes the draft");
    assert_eq!(diameter(&h), before, "a draft does not write the world");
    h.get_by_label("Apply").scroll_to_me();
    h.run_steps(3);
    h.get_by_label("Apply").click();
    h.run_steps(2);
    assert_eq!(diameter(&h), staged, "Apply lands the draft through ChangeTool: {}", h.state().state.status);
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(diameter(&h), before, "Undo restores the tool's previous numbers");
}

#[test]
fn tools_ribbon_opens_a_docked_catalog_without_retargeting_properties() {
    let mut h = shell_harness();
    h.run_steps(2);
    let before = h.state().state.ui_target.clone();
    let (cells, _) = mill_plan_at(1280.0);
    let at = cells.iter().find(|c| c.cmd == Cmd::Tools).expect("Tools ribbon cell").rect.center();
    set_pos(&mut h, at);
    button_at(&mut h, egui::PointerButton::Primary, at, true);
    button_at(&mut h, egui::PointerButton::Primary, at, false);
    h.run_steps(3);
    assert_eq!(h.state().state.ui_target, before, "opening the catalog preserves the Properties subject");
    assert!(painted(&h).iter().any(|(t, _)| t == "Tool library"), "the catalog has its own dock tab");
    assert!(h.query_by_label("Catalog").is_some(), "the catalog is visible");
    assert_eq!(h.get_by_label("Import tool holders").accesskit_node().role(), egui::accesskit::Role::CheckBox);
    assert_eq!(h.get_by_label("New tool…").accesskit_node().role(), egui::accesskit::Role::Button);
    assert!(!h.query_all_by_role(egui::accesskit::Role::Window).any(|n| n.accesskit_node().label().as_deref() == Some("Tool library")), "the library docks instead of opening a window");
    set_pos(&mut h, at);
    button_at(&mut h, egui::PointerButton::Primary, at, true);
    button_at(&mut h, egui::PointerButton::Primary, at, false);
    h.run_steps(3);
    assert!(h.query_by_label("Catalog").is_some(), "opening the catalog twice keeps it open");
    assert_eq!(h.state().state.ui_target, before);
}
