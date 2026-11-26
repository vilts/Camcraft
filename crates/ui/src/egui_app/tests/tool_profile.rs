//! #93: the tool editor's Profile section - a milling tool shows Body /
//! Profile tabs (a turning tool the Body section alone), Parametric
//! follows the numbers, Custom keeps hand edits, Apply profile lands one
//! undoable command, and a bad outline is refused by name, live and at
//! Apply. Provenance: our own page law; the outlines are our own.

use super::*;
use crate::state::ErrorScope;
use camcraft_app::spun::SpunPoint;

const TOOL: &str = "EM-0600-4FL";

fn editor() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    // #2397: Properties shows the tool page, so its one side preview is
    // drawn beside the shape window.
    harness.state_mut().state.ui_target = UiTarget::Tool(TOOL.into());
    crate::actions::open_tool_shape_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    harness.run();
    harness
}

/// The editor with the Profile tab open.
fn profile_tab() -> Harness<'static, ShellUi> {
    let mut harness = editor();
    harness.get_by_label("Profile").click();
    harness.run();
    harness.run();
    harness
}

fn drawn(state: &State) -> Option<Vec<SpunPoint>> {
    model::find_by_name(&state.world, TOOL)
        .and_then(|e| camcraft_app::tool_body::body_of(&state.world, &e))
        .and_then(|b| b.spun)
        .map(|s| s.points().to_vec())
}

#[test]
fn a_milling_tool_offers_the_profile_tab_and_a_turning_tool_does_not() {
    let mut harness = profile_tab();
    for label in ["Parametric", "Custom", "Apply profile", "Holder profile"] {
        assert!(harness.query_by_label(label).is_some(), "{label} stands in the Profile tab");
    }
    assert!(harness.query_by_label("Import body…").is_none(), "the Body section is behind its tab");
    // A turning insert: no tabs, the Body section as before.
    harness.state_mut().state.tool_ui.kind = 6;
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Profile").is_none(), "turning tools are not spun profiles");
    assert!(harness.query_by_label("Import body…").is_some());
}

#[test]
fn parametric_follows_the_numbers_and_custom_keeps_hand_edits() {
    let mut harness = profile_tab();
    let d0 = harness.state().state.tool_ui.holder_d;
    let r = |h: &Harness<'static, ShellUi>| h.state().state.tool_profile_ui.points.get(1).map(|p| p.r);
    assert_eq!(r(&harness), Some(d0 * 0.5), "parametric: the holder cylinder");
    harness.state_mut().state.tool_ui.holder_d = d0 + 10.0;
    harness.run();
    assert_eq!(r(&harness), Some((d0 + 10.0) * 0.5), "parametric redraws as the numbers change");
    harness.get_by_label("Custom").click();
    harness.run();
    harness.state_mut().state.tool_profile_ui.points[1].r = 30.0;
    harness.state_mut().state.tool_ui.holder_d = d0;
    harness.run();
    assert_eq!(r(&harness), Some(30.0), "custom keeps the hand edit");
    assert!(harness.state().state.pending.is_empty(), "editing the draft is not a command");
    assert!(drawn(&harness.state().state).is_none());
}

#[test]
fn apply_profile_lands_one_command_and_undo_restores() {
    let mut harness = profile_tab();
    harness.get_by_label("Custom").click();
    harness.run();
    harness.state_mut().state.tool_profile_ui.points[1].r = 25.0;
    harness.state_mut().state.tool_profile_ui.points[2].r = 25.0;
    harness.run();
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    let landed = drawn(&harness.state().state).expect("the drawn holder landed");
    assert_eq!(landed[1].r, 25.0, "status: {}", harness.state().state.status);
    assert!(harness.state().state.tool_profile_ui.custom, "a landed profile re-edits as Custom");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "one undo takes it off");
    assert!(!harness.state().state.tool_profile_ui.custom, "and the draft is parametric again");
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "redo puts it back");
    // Parametric + Apply is the way back: one command, undoable.
    harness.run();
    harness.get_by_label("Parametric").click();
    harness.run();
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "parametric again");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "undo restores the drawn holder");
}

#[test]
fn a_bad_outline_is_refused_by_name_live_and_at_apply() {
    let mut harness = profile_tab();
    harness.get_by_label("Custom").click();
    harness.run();
    let last = harness.state().state.tool_profile_ui.points.len() - 1;
    harness.state_mut().state.tool_profile_ui.points[last].r = 5.0;
    harness.run();
    harness.run();
    assert!(harness.query_all_by_label_contains("must end on the axis").next().is_some(), "the live refusal names it");
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "nothing lands");
    assert!(harness.query_all_by_label_contains("holder profile not set").next().is_some(), "Apply refuses by name");
}

#[test]
fn insert_and_remove_edit_the_table() {
    let mut harness = profile_tab();
    let n = harness.state().state.tool_profile_ui.points.len();
    harness.state_mut().state.insert_profile_point(1);
    harness.run();
    let st = &harness.state().state;
    assert_eq!(st.tool_profile_ui.points.len(), n + 1);
    assert!(st.tool_profile_ui.custom, "an edit makes the draft custom");
    harness.state_mut().state.remove_profile_point(2);
    harness.run();
    assert_eq!(harness.state().state.tool_profile_ui.points.len(), n);
}

/// The to-scale preview's body envelope, as the editor's middle column
/// reads it this frame.
fn preview_reach(harness: &mut Harness<'static, ShellUi>) -> Option<f64> {
    let st = &mut harness.state_mut().state;
    match st.tool_body_preview()? {
        Ok(b) => b.profile().map(|p| p.bands().iter().fold(0.0f64, |m, q| m.max(q.r))),
        Err(_) => None,
    }
}

/// The middle to-scale preview follows Apply profile and its undo while
/// the Profile tab shows - the Body tab is never opened (#475 hands-on).
#[test]
fn the_to_scale_preview_follows_the_profile_without_the_body_tab() {
    let mut harness = profile_tab();
    assert_eq!(preview_reach(&mut harness), None, "no body yet: the plain cylinder");
    harness.get_by_label("Custom").click();
    harness.run();
    harness.state_mut().state.tool_profile_ui.points[1].r = 40.0;
    harness.state_mut().state.tool_profile_ui.points[2].r = 40.0;
    harness.run();
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "status: {}", harness.state().state.status);
    let reach = preview_reach(&mut harness).expect("the preview shows the drawn holder");
    assert!((reach - 40.0).abs() < 1.0, "the preview reaches r {reach}, the drawn holder r 40");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none());
    assert_eq!(preview_reach(&mut harness), None, "undo: the preview is the cylinder again");
}

/// An Apply refusal retires once the outline is corrected and validates.
#[test]
fn a_profile_refusal_retires_when_the_outline_validates() {
    let mut harness = profile_tab();
    harness.get_by_label("Custom").click();
    harness.run();
    let last = harness.state().state.tool_profile_ui.points.len() - 1;
    harness.state_mut().state.tool_profile_ui.points[last].r = 5.0;
    harness.run();
    harness.get_by_label("Apply profile").click();
    harness.run();
    harness.run();
    assert!(harness.state().state.error_for(ErrorScope::Tool).is_some(), "Apply refused");
    harness.state_mut().state.tool_profile_ui.points[last].r = 0.0;
    harness.run();
    harness.run();
    assert!(harness.state().state.error_for(ErrorScope::Tool).is_none(), "the corrected outline retires it");
    assert!(harness.query_all_by_label_contains("holder profile not set").next().is_none());
}

#[test]
fn profile_tab_states_conservative_envelope_and_half_profile_rules_and_dxf_workflow() {
    let mut harness = profile_tab();
    for label in ["Import DXF…", "Export DXF…", "Import STEP…"] {
        assert!(harness.query_by_label(label).is_some(), "{label} sits on the Profile tab");
    }
    assert!(
        harness
            .query_all_by_label_contains("covered by a larger safe shape, never cut away")
            .next()
            .is_some(),
        "Profile tab states that parts that are not round get a larger safe shape"
    );
    assert!(
        harness
            .query_all_by_label_contains("you edit one half of its outline")
            .next()
            .is_some(),
        "Profile tab states that only one half of the outline is edited"
    );

    // Pin the import help beside the control; no screenshot or native picker is needed.
    let page = include_str!("../tool_profile_page.rs");
    for convention in [
        "parallel to Y",
        "DXF +Y is tool +Z",
        "z = 0 is the lower end of the centreline",
        "independent of draw direction",
    ] {
        assert!(
            page.contains(convention),
            "missing import tooltip convention: {convention}"
        );
    }

    // Import an ISO 13399 STEP box as an editable custom half-profile, export to DXF, re-import.
    let step = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/tool_csys_box_iso13399.step");
    let unaligned = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");

    // A STEP without clear CSYS axis refuses by name.
    harness.state_mut().state.import_step_profile_from(&unaligned);
    harness.run();
    assert!(
        harness.state().state.error_for(ErrorScope::Tool).is_some(),
        "unaligned STEP must refuse by name"
    );

    harness.state_mut().state.import_step_profile_from(&step);
    harness.run();
    assert!(harness.state().state.error_for(ErrorScope::Tool).is_none());
    assert!(harness.state().state.tool_profile_ui.custom);
    let points_after_step = harness.state().state.tool_profile_ui.points.clone();
    assert!(points_after_step.len() >= 4);

    let dxf_path = std::env::temp_dir().join(format!("camcraft_ui_dxf_{}.dxf", std::process::id()));
    harness
        .state()
        .state
        .export_profile_dxf_to(&dxf_path)
        .expect("export DXF from profile tab");
    // Reset to Parametric then re-import DXF.
    harness.get_by_label("Parametric").click();
    harness.run();
    harness.state_mut().state.import_profile_dxf_from(&dxf_path);
    harness.run();
    let _ = std::fs::remove_file(&dxf_path);
    assert!(harness.state().state.tool_profile_ui.custom);
    assert_eq!(harness.state().state.tool_profile_ui.points.len(), points_after_step.len());
}

#[test]
fn body_tab_convert_to_profile_refuses_unaligned_step_and_converts_iso_aligned_step() {
    let mut harness = profile_tab();
    harness.get_by_label("Body").click();
    harness.run();

    let unaligned = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/box.step");
    harness.state_mut().state.import_tool_body_from(&unaligned);
    harness.run();
    assert!(harness.query_by_label("Convert to profile").is_some());
    assert!(
        harness
            .query_all_by_label_contains("enveloped conservatively")
            .next()
            .is_some()
    );
    harness.get_by_label("Convert to profile").click();
    harness.run();
    assert!(
        harness
            .state()
            .state
            .error_for(ErrorScope::Tool)
            .is_some_and(|e| e.contains("clear tool axis")),
        "unaligned STEP draft must refuse instead of guessing +Z: {:?}",
        harness.state().state.error_for(ErrorScope::Tool)
    );

    let step = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/tool_csys_box_iso13399.step");
    harness.state_mut().state.import_tool_body_from(&step);
    harness.run();
    harness.get_by_label("Convert to profile").click();
    harness.run();
    assert!(
        harness.state().state.error_for(ErrorScope::Tool).is_none(),
        "aligned ISO 13399 STEP body must convert cleanly: {:?}",
        harness.state().state.error_for(ErrorScope::Tool)
    );
    assert_eq!(
        harness.state().state.tool_profile_ui.column,
        crate::state::ToolColumn::Profile
    );
    assert!(harness.state().state.tool_profile_ui.custom);
    assert!(harness.state().state.editor_tool_body().is_some());
}

/// #2007 / #2397: the Draw holder view stands in the shape window in
/// plain words. The controls column reads top to bottom: what is in use,
/// Parametric / Custom, Apply profile, the file buttons. The table with
/// its headers and the side view stand beside it, in the view column.
#[test]
fn tool_profile_structure() {
    use egui_kittest::kittest::NodeT;
    let harness = profile_tab();
    let win = harness
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool shape: EM-0600-4FL"))
        .expect("the shape window");
    let view_left = win.get_by_label("height mm").rect().min.x;
    let mut last_y = f32::MIN;
    for label in ["Holder profile", "In use: the holder cylinder from the dimensions", "Parametric", "Apply profile", "Import DXF…"] {
        let r = win.get_by_label(label).rect();
        assert!(r.center().y > last_y, "{label:?} must follow the row above it");
        assert!(r.max.x < view_left, "{label:?} sits in the controls column");
        last_y = r.center().y;
    }
    for head in ["height mm", "radius mm", "arc"] {
        let r = win.get_by_label(head).rect();
        assert!(r.min.x >= view_left, "the {head:?} header sits in the view column");
    }
    assert!(win.query_all_by_label_contains("landed").next().is_none(), "no \"landed\" jargon on the page");
}
