//! #93 slice 3: the Draw cutter view edits the tool by its parameters -
//! each milling kind shows the numbers its outline is drawn from, an
//! edit redraws a Parametric outline and carries a Custom one (a neck
//! kept above the new flute top), the neck is a parameter refused by
//! name when it cannot fit, and Apply cutter or the editor's Apply lands
//! numbers and outline together, one undo step.
//! Provenance: our own page law; the outlines are our own.

use super::*;
use egui_kittest::kittest::NodeT;
use camcraft_app::cutter_fit::{self, Neck};

const TOOL: &str = "EM-0600-4FL";

/// The editor on the Profile tab, switched to Draw cutter.
fn cutter_tab() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    crate::actions::open_tool_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    harness.run();
    harness.get_by_label("Profile").click();
    harness.run();
    harness.get_by_label("Draw cutter").click();
    harness.run();
    harness.run();
    harness
}

/// The document's tool: (diameter, its drawn cutter's neck, drawn?).
fn landed(state: &State) -> (f64, Option<Neck>, bool) {
    let e = model::find_by_name(&state.world, TOOL).expect("tool");
    let t = model::tool_of(&state.world, &e).expect("a tool");
    let c = camcraft_app::tool_cutter::cutter_of(&state.world, &e);
    let neck = c.as_ref().and_then(|s| cutter_fit::neck_of(&t, s.points()));
    (t.diameter, neck, c.is_some())
}

const NECK: Neck = Neck { d: 5.0, len: 10.0 };

#[test]
fn each_milling_kind_shows_the_parameters_its_outline_is_drawn_from() {
    let mut harness = cutter_tab();
    for label in ["cutter Ø", "flutes", "shank Ø", "shank length", "Neck"] {
        assert!(harness.query_by_label(label).is_some(), "{label} stands in the cutter view");
    }
    let tips = ["ball R", "corner R", "point angle °", "cone angle °"];
    assert!(tips.iter().all(|l| harness.query_by_label(l).is_none()), "a flat end mill has no tip parameter");
    for (kind, tip) in [(1, "ball R"), (2, "corner R"), (3, "point angle °"), (10, "cone angle °")] {
        harness.state_mut().state.tool_ui.kind = kind;
        if kind == 3 {
            harness.state_mut().state.tool_ui.tip_angle = 118.0;
        }
        if kind == 10 {
            harness.state_mut().state.tool_ui.tip_angle = 90.0;
        }
        harness.run();
        harness.run();
        assert!(harness.query_by_label(tip).is_some(), "kind {kind} shows {tip}");
        assert!(tips.iter().filter(|l| **l != tip).all(|l| harness.query_by_label(l).is_none()), "kind {kind} shows only {tip}");
        // The drawn outline keeps agreeing with the numbers it shows.
        assert!(harness.query_all_by_label_contains("cutter Ø 6.00").next().is_some(), "kind {kind} validates live");
    }
}

#[test]
fn a_number_edit_redraws_parametric_and_carries_a_custom_neck() {
    let mut harness = cutter_tab();
    harness.state_mut().state.set_cutter_neck(Some(NECK));
    harness.run();
    assert!(harness.state().state.tool_cutter_ui.custom, "a neck is a drawn outline");
    assert_eq!(harness.state().state.cutter_neck(), Some(NECK));
    assert!(harness.query_by_label("neck Ø").is_some() && harness.query_by_label("neck length").is_some());
    harness.state_mut().state.tool_ui.diameter = 7.0;
    harness.state_mut().state.tool_ui.flute_len = 20.0;
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert_eq!(st.tool_cutter_ui.points.get(1).map(|p| p.r), Some(3.5), "the tip follows the diameter");
    assert_eq!(st.cutter_neck(), Some(NECK), "the neck rode up with the flute top");
    assert!(st.pending.is_empty(), "editing the draft is not a command");
    assert!(harness.query_all_by_label_contains("cutter Ø 7.00").next().is_some(), "and the outline validates live");
    // Neck off: back to the outline the numbers draw.
    harness.state_mut().state.set_cutter_neck(None);
    harness.run();
    assert!(!harness.state().state.tool_cutter_ui.custom);
    assert_eq!(harness.state().state.cutter_neck(), None);
}

#[test]
fn a_neck_that_cannot_fit_is_refused_by_name() {
    let mut harness = cutter_tab();
    let before = harness.state().state.tool_cutter_ui.points.clone();
    harness.state_mut().state.set_cutter_neck(Some(Neck { d: 9.0, len: 10.0 }));
    harness.run();
    harness.run();
    assert_eq!(harness.state().state.tool_cutter_ui.points, before, "the outline stays as it was");
    assert!(harness.query_all_by_label_contains("neck not drawn: the neck is Ø 9.000 mm - it must be above 0 and below the shank's Ø 8.000 mm").next().is_some());
}

#[test]
fn apply_cutter_lands_numbers_and_outline_in_one_undo_step() {
    let mut harness = cutter_tab();
    harness.state_mut().state.set_cutter_neck(Some(NECK));
    harness.state_mut().state.tool_ui.diameter = 7.0;
    harness.run();
    harness.get_by_label("Apply cutter").click();
    harness.run();
    harness.run();
    assert_eq!(landed(&harness.state().state), (7.0, Some(NECK), true), "status: {}", harness.state().state.status);
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert_eq!(landed(&harness.state().state), (6.0, None, false), "one undo takes both back");
}

#[test]
fn the_editors_apply_carries_a_landed_cutter_onto_new_numbers() {
    let mut harness = cutter_tab();
    harness.state_mut().state.set_cutter_neck(Some(NECK));
    harness.run();
    harness.get_by_label("Apply cutter").click();
    harness.run();
    harness.run();
    assert_eq!(landed(&harness.state().state), (6.0, Some(NECK), true));
    // A diameter edit under a drawn cutter used to be refused; now the
    // drawn outline rides along.
    harness.state_mut().state.tool_ui.diameter = 7.0;
    harness.run();
    // The editor window's own Apply (the dock has another).
    harness
        .query_all_by_role(egui::accesskit::Role::Window)
        .find(|n| n.accesskit_node().label().as_deref() == Some("Tool: EM-0600-4FL"))
        .expect("the editor window")
        .get_by_label("Apply")
        .click();
    harness.run();
    harness.run();
    let st = &harness.state().state;
    assert_eq!(landed(st), (7.0, Some(NECK), true), "error: {:?}", st.error_for(crate::state::ErrorScope::Tool));
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert_eq!(landed(&harness.state().state), (6.0, Some(NECK), true), "one undo restores the numbers and the outline");
}
