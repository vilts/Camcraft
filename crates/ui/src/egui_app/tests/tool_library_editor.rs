//! #3893 (U2 of #3663): the tool editor in the tool library window - one
//! Apply is one undoable `ChangeTool`, the window's draft and Properties'
//! draft stay apart, and the Details tab shows the catalog facts.
//! Gate map: covers ui::egui_app::tool_library_dialog, ui::state, ui::egui_app::tool_settings_grid.
//! Provenance: our own structural tests over the AccessKit tree and the
//! command history; no HSMWorks material involved.

use super::*;
use camcraft_app::tools::Tool;
use egui::accesskit::Role;
use egui_kittest::Node;
use egui_kittest::kittest::NodeT;

pub(super) type H = Harness<'static, ShellUi>;

/// The window's AccessKit node; every query of these tests stays inside
/// it, so a twin row in Properties never matches.
pub(super) fn library_window(h: &H) -> Node<'_> {
    h.query_all_by_role(Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool library"))
        .unwrap_or_else(|| panic!("the Tool library window is not open"))
}

pub(super) fn tool(h: &H, name: &str) -> Tool {
    model::find_by_name(&h.state().state.world, name)
        .and_then(|e| model::tool_of(&h.state().state.world, &e))
        .unwrap_or_else(|| panic!("tool {name} is in the document"))
}

/// The window's draft (the editor must be open).
pub(super) fn draft(h: &mut H) -> &mut crate::state::ToolUi {
    match h.state_mut().state.tool_library.as_mut().and_then(|v| v.editor.as_mut()) {
        Some(e) => &mut e.draft,
        None => panic!("no tool is open in the tool library window"),
    }
}

/// Two different tools: the seeded document's tool, and a second one from
/// the catalog (the `AddTool` command) when the document has only one.
pub(super) fn two_tools(h: &mut H) -> (String, String) {
    if h.state().state.tool_names().len() < 2 {
        let st = &h.state().state;
        let Some((name, tool)) = camcraft_app::tools::catalog().into_iter().find(|(n, t)| {
            model::find_by_name(&st.world, n).is_none() && crate::machine_modes::tool_kind_offered(&st.world, t.kind)
        }) else {
            panic!("the catalog has a second tool for this machine");
        };
        let cmd = crate::state::CommandKind::AddTool { name: name.to_owned(), tool };
        h.state_mut().state.pending.push_back(cmd);
        h.run_steps(2);
    }
    let names = h.state().state.tool_names();
    assert!(names.len() >= 2, "the document has two tools: {names:?}");
    (names[0].clone(), names[1].clone())
}

/// Open the window (the ribbon's Tools cell) and `name` in its editor.
pub(super) fn open_in_window(h: &mut H, name: &str) {
    h.get_by_role_and_label(Role::Button, "Tools").click();
    h.run_steps(2);
    h.state_mut().state.open_tool_in_library(name);
    h.run_steps(3);
}

pub(super) fn click_window_button(h: &mut H, label: &str) {
    library_window(h).get_by_role_and_label(Role::Button, label).click();
    h.run_steps(3);
}

/// Geometry and catalog edits in the window land as ONE `ChangeTool` on
/// Apply, and one undo gives the old tool back.
#[test]
fn apply_is_one_change_tool_and_undo_restores() {
    let mut h = shell_harness();
    h.run_steps(2);
    let (name, _) = two_tools(&mut h);
    open_in_window(&mut h, &name);
    let before = tool(&h, &name);
    let done = h.state().state.history.done_len();
    draft(&mut h).diameter = before.diameter + 0.5;
    draft(&mut h).vendor = "Acme".into();
    h.run_steps(2);
    assert_eq!(tool(&h, &name), before, "a draft does not write the document");
    click_window_button(&mut h, "Apply");
    let after = tool(&h, &name);
    assert_eq!(after.diameter, before.diameter + 0.5, "Apply lands the draft: {}", h.state().state.status);
    assert_eq!(after.catalog.vendor.as_deref(), Some("Acme"), "the Details tab's facts ride the same Apply");
    assert_eq!(h.state().state.history.done_len(), done + 1, "one Apply is one undo step");
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(tool(&h, &name), before, "one undo restores the old tool");
    assert_eq!(draft(&mut h).diameter, before.diameter, "the window's draft follows the undo");
}

/// Properties shows one tool and the window another. Each keeps its own
/// draft over frames (the window's sync does not reset it while the user
/// types), and the window's Apply writes only its own tool.
#[test]
fn the_dialog_draft_and_the_properties_draft_do_not_mix() {
    let mut h = shell_harness();
    h.run_steps(2);
    let (a, b) = two_tools(&mut h);
    crate::actions::target(&mut h.state_mut().state, UiTarget::Tool(a.clone()));
    h.run_steps(3);
    open_in_window(&mut h, &b);
    let (tool_a, tool_b) = (tool(&h, &a), tool(&h, &b));
    assert_eq!(h.state().state.ui_target, UiTarget::Tool(a.clone()), "opening in the window keeps Properties");
    assert_eq!(h.state().state.tool_ui.name, a, "Properties' draft is tool {a}");
    assert_eq!(draft(&mut h).name, b, "the window's draft is tool {b}");
    // Half-typed numbers in both drafts, then frames: neither resets.
    let typed = tool(&h, &b).diameter + 0.5;
    draft(&mut h).diameter = typed;
    h.state_mut().state.tool_ui.diameter = 7.25;
    h.run_steps(4);
    let kept = draft(&mut h).clone();
    assert_eq!((kept.name.as_str(), kept.diameter), (b.as_str(), typed), "the window keeps its draft");
    let props = &h.state().state.tool_ui;
    assert_eq!((props.name.as_str(), props.diameter), (a.as_str(), 7.25), "Properties keeps its draft");
    click_window_button(&mut h, "Apply");
    assert_eq!(tool(&h, &b).diameter, typed, "the window's Apply writes its tool: {}", h.state().state.status);
    assert_eq!(tool(&h, &a), tool_a, "the window's Apply never writes Properties' tool");
    assert_ne!(tool_b.diameter, typed);
    assert_eq!(h.state().state.tool_ui.name, a, "Properties still shows tool {a}");
    // A pick in the rail opens another tool in the window only.
    library_window(&h).get_by_role_and_label(Role::Button, &a).click();
    h.run_steps(3);
    assert_eq!(draft(&mut h).name, a, "the rail opens tool {a} in the window");
    assert_eq!(h.state().state.ui_target, UiTarget::Tool(a.clone()), "the rail does not retarget Properties");
}

/// The Details tab shows the tool's catalog facts (#898), its material
/// group and its tool number; the Tool tab's rows are not on it.
#[test]
fn details_tab_shows_the_catalog_facts() {
    let mut h = shell_harness();
    h.run_steps(2);
    let (name, _) = two_tools(&mut h);
    let mut t = tool(&h, &name);
    t.catalog.vendor = Some("Acme".into());
    t.catalog.part_number = Some("P-77".into());
    t.catalog.coating = Some("TiAlN".into());
    h.state_mut().state.pending.push_back(crate::state::CommandKind::ChangeTool { name: name.clone(), tool: t });
    h.run_steps(2);
    open_in_window(&mut h, &name);
    assert!(library_window(&h).query_all_by_label("Diameter").next().is_some(), "the editor opens on the Tool tab");
    click_window_button(&mut h, "Details");
    let win = library_window(&h);
    for label in ["Material group", "Part number", "Vendor", "Coating", "Helix angle (deg)", "Notes", "Tool number"] {
        assert!(win.query_all_by_label(label).next().is_some(), "the Details tab has the row {label:?}");
    }
    let values: Vec<String> = win.query_all_by_role(Role::TextInput).filter_map(|n| n.value()).collect();
    for fact in ["Acme", "P-77", "TiAlN"] {
        assert!(values.iter().any(|v| v == fact), "the Details tab shows {fact:?}: {values:?}");
    }
    assert!(win.query_all_by_label("Diameter").next().is_none(), "the geometry stays on the Tool tab");
}

/// #3368: a chamfer mill's Tool tab shows the Tip form row (enabled, Sharp
/// for a catalog V-bit) and, for a flat tip, the Tip diameter row; another
/// kind shows neither. A form edit lands as one `ChangeTool` on Apply.
#[test]
fn the_tip_form_row_shows_for_a_taper() {
    let mut h = shell_harness();
    h.run_steps(2);
    let (end_mill, _) = two_tools(&mut h);
    let name = "CM-0600-90";
    let Some((_, cone)) = camcraft_app::tools::catalog().into_iter().find(|(n, _)| *n == name) else {
        panic!("the catalog has {name}");
    };
    h.state_mut().state.pending.push_back(crate::state::CommandKind::AddTool { name: name.into(), tool: cone });
    h.run_steps(2);
    open_in_window(&mut h, name);
    {
        let win = library_window(&h);
        assert!(win.query_all_by_label("Tip form").next().is_some(), "a chamfer mill has the Tip form row");
        let Some(combo) = win.query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some("Sharp")).next() else {
            panic!("the Tip form combo shows Sharp for a catalog chamfer mill");
        };
        assert!(!combo.accesskit_node().is_disabled(), "the tip form can be chosen");
        assert!(win.query_all_by_label("Tip diameter").next().is_none(), "a sharp tip states no diameter");
    }
    let before = tool(&h, name);
    let done = h.state().state.history.done_len();
    draft(&mut h).tip_form = camcraft_app::tools::TaperTip::Flat;
    draft(&mut h).tip_diameter = 1.0;
    h.run_steps(2);
    assert!(library_window(&h).query_all_by_label("Tip diameter").next().is_some(), "a flat tip shows its diameter");
    click_window_button(&mut h, "Apply");
    let after = tool(&h, name);
    assert_eq!((after.tip_form, after.tip_diameter), (camcraft_app::tools::TaperTip::Flat, 1.0), "{}", h.state().state.status);
    assert_eq!(h.state().state.history.done_len(), done + 1, "one Apply is one undo step");
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(tool(&h, name), before, "one undo gives the sharp tip back");
    h.state_mut().state.open_tool_in_library(&end_mill);
    h.run_steps(3);
    assert_eq!(draft(&mut h).name, end_mill);
    assert!(library_window(&h).query_all_by_label("Tip form").next().is_none(), "only a chamfer mill has a tip form");
}

/// #3369: a form mill's Tool tab shows the kind, the Form preset list
/// (Roundover for the catalogue bit) with its number rows and the pilot
/// length; a preset number edit lands as one `ChangeTool` on Apply and
/// the diameter follows the form. Another kind shows no preset row.
#[test]
fn the_form_mill_presets_show() {
    let mut h = shell_harness();
    h.run_steps(2);
    let (end_mill, _) = two_tools(&mut h);
    let name = "FM-RO-R6";
    rack(&mut h.state_mut().state, &[name]);
    h.run_steps(2);
    open_in_window(&mut h, name);
    {
        let win = library_window(&h);
        let combo = |v: &str| win.query_all_by(|n| n.role() == Role::ComboBox && n.value().as_deref() == Some(v)).next().is_some();
        assert!(combo("Form mill"), "the Type list shows the form mill");
        assert!(combo("Roundover"), "the Form preset list shows the catalogue bit's preset");
        for label in ["Form preset", "Radius", "Pilot radius", "Cutting height", "Pilot length"] {
            assert!(win.query_all_by_label(label).next().is_some(), "a form mill has the {label} row");
        }
    }
    let before = tool(&h, name);
    let done = h.state().state.history.done_len();
    match draft(&mut h).form.as_mut().and_then(|f| f.preset.as_mut()) {
        Some(camcraft_app::tools::FormPreset::Roundover { radius, .. }) => *radius = 8.0,
        other => panic!("the catalogue bit is a roundover, not {other:?}"),
    }
    h.run_steps(2);
    click_window_button(&mut h, "Apply");
    let after = tool(&h, name);
    assert_eq!((after.diameter, after.flute_len), (29.0, 18.0), "{}", h.state().state.status);
    assert_eq!(h.state().state.history.done_len(), done + 1, "one Apply is one undo step");
    crate::actions::undo(&mut h.state_mut().state);
    h.run_steps(2);
    assert_eq!(tool(&h, name), before, "one undo gives the old form back");
    h.state_mut().state.open_tool_in_library(&end_mill);
    h.run_steps(3);
    assert!(library_window(&h).query_all_by_label("Form preset").next().is_none(), "only a form mill has a preset");
}

/// #3369: the shape window's Profile tab (Draw cutter) draws a form mill
/// and names its form in place of a tip number.
#[test]
fn a_form_mill_profile_tab_names_its_form() {
    let mut h = Harness::builder().with_size(egui::vec2(1280.0, 1400.0)).build_ui_state(super::frame, ShellUi::seeded());
    h.run();
    let name = "FM-CV-R6";
    rack(&mut h.state_mut().state, &[name]);
    h.run();
    h.state_mut().state.ui_target = UiTarget::Tool(name.into());
    crate::actions::open_tool_shape_editor(&mut h.state_mut().state, name);
    h.run();
    h.run();
    h.get_by_label("Profile").click();
    h.run();
    h.get_by_label("Draw cutter").click();
    h.run();
    h.run();
    assert!(h.query_all_by_label("form").next().is_some(), "the cutter view names the form");
    assert!(h.query_all_by_label("Cove").next().is_some(), "and its preset");
    assert!(h.query_all_by_label("cone angle °").next().is_none(), "a form mill has no tip number");
    // Review of #3946: the form outline sets the diameter and the flute
    // length, so the cutter view shows them and takes no typing there; the
    // shank numbers stay editable.
    // The cell of a row sits between its label and the "form" column (the
    // points table to the right shares the rows' heights).
    let right = h.get_by_label("form").rect().min.x;
    let typed = |label: &str| {
        let at = h.get_by_label(label).rect();
        let cell = |r: egui::Rect| (r.center().y - at.center().y).abs() < 3.0 && r.min.x > at.min.x && r.max.x < right;
        h.query_all_by_role(Role::SpinButton).filter(|n| cell(n.rect())).count()
    };
    assert_eq!((typed("cutter Ø"), typed("flutes")), (0, 0), "a form mill's diameter and flute length are not typed");
    assert_eq!(typed("shank Ø"), 1, "the shank diameter stays editable");
}
