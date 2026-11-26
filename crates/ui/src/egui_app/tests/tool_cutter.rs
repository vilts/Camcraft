//! #93 slice 2: the Profile tab's Draw cutter view - Parametric follows
//! the tool's numbers, Custom keeps hand edits, Apply cutter lands one
//! undoable command, a cutter that disagrees with the numbers is refused
//! by name live and at Apply, and the holder view is still the default.
//! Provenance: our own page law; the outlines are our own.

use super::*;
use crate::state::ErrorScope;
use camcraft_app::spun::SpunPoint;
use egui_kittest::kittest::NodeT;

const TOOL: &str = "EM-0600-4FL";

/// The editor on the Profile tab, switched to Draw cutter.
fn cutter_tab() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    harness.state_mut().state.ui_target = UiTarget::Tool(TOOL.into());
    crate::actions::open_tool_shape_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    harness.run();
    harness.get_by_label("Profile").click();
    harness.run();
    harness.get_by_label("Draw cutter").click();
    harness.run();
    harness.run();
    harness
}

fn drawn(state: &State) -> Option<Vec<SpunPoint>> {
    model::find_by_name(&state.world, TOOL)
        .and_then(|e| camcraft_app::tool_cutter::cutter_of(&state.world, &e))
        .map(|s| s.points().to_vec())
}

#[test]
fn the_profile_tab_offers_the_cutter_beside_the_holder() {
    let mut harness = shell_harness();
    harness.run();
    harness.state_mut().state.ui_target = UiTarget::Tool(TOOL.into());
    crate::actions::open_tool_shape_editor(&mut harness.state_mut().state, TOOL);
    harness.run();
    harness.run();
    harness.get_by_label("Profile").click();
    harness.run();
    harness.run();
    assert!(harness.query_by_label("Holder profile").is_some(), "the holder is the default view");
    assert!(harness.query_by_label("Draw holder").is_some() && harness.query_by_label("Draw cutter").is_some());
    harness.get_by_label("Draw cutter").click();
    harness.run();
    harness.run();
    for label in ["Cutter profile", "Parametric", "Custom", "Apply cutter"] {
        assert!(harness.query_by_label(label).is_some(), "{label} stands in the cutter view");
    }
    assert!(harness.query_by_label("Holder profile").is_none());
    assert!(harness.query_all_by_label_contains("cutter Ø 6.00").next().is_some(), "the summary states the numbers");
}

#[test]
fn parametric_follows_the_numbers_and_custom_keeps_hand_edits() {
    let mut harness = cutter_tab();
    let r = |h: &Harness<'static, ShellUi>| h.state().state.tool_cutter_ui.points.get(1).map(|p| p.r);
    assert_eq!(r(&harness), Some(3.0), "parametric: the Ø6 cutter");
    harness.state_mut().state.tool_ui.diameter = 8.0;
    harness.run();
    assert_eq!(r(&harness), Some(4.0), "parametric redraws as the numbers change");
    harness.state_mut().state.tool_ui.diameter = 6.0;
    harness.run();
    harness.get_by_label("Custom").click();
    harness.run();
    // A Ø5 neck from the flute top to 25 mm.
    harness.state_mut().state.insert_cutter_point(3);
    harness.state_mut().state.insert_cutter_point(3);
    let st = &mut harness.state_mut().state;
    st.tool_cutter_ui.points[3] = SpunPoint::line(15.0, 2.5);
    st.tool_cutter_ui.points[4] = SpunPoint::line(25.0, 2.5);
    st.tool_cutter_ui.points[5] = SpunPoint::line(25.0, 4.0);
    harness.run();
    assert_eq!(harness.state().state.tool_cutter_ui.points.len(), 8, "custom keeps the hand edits");
    assert!(harness.state().state.pending.is_empty(), "editing the draft is not a command");
    assert!(drawn(&harness.state().state).is_none());
    assert!(harness.query_all_by_label_contains("cutter Ø").next().is_some(), "the necked cutter validates live");
}

#[test]
fn apply_cutter_lands_one_command_and_undo_restores() {
    let mut harness = cutter_tab();
    harness.get_by_label("Custom").click();
    harness.run();
    harness.state_mut().state.insert_cutter_point(3);
    harness.state_mut().state.insert_cutter_point(3);
    let st = &mut harness.state_mut().state;
    st.tool_cutter_ui.points[3] = SpunPoint::line(15.0, 2.5);
    st.tool_cutter_ui.points[4] = SpunPoint::line(30.0, 2.5);
    st.tool_cutter_ui.points[5] = SpunPoint::line(30.0, 4.0);
    harness.run();
    harness.get_by_label("Apply cutter").click();
    harness.run();
    harness.run();
    let landed = drawn(&harness.state().state).unwrap_or_else(|| panic!("landed; status: {}", harness.state().state.status));
    assert_eq!(landed.len(), 8);
    assert!(harness.state().state.tool_cutter_ui.custom, "a landed cutter re-edits as Custom");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "one undo takes it off");
    assert!(!harness.state().state.tool_cutter_ui.custom, "and the draft is parametric again");
    harness.state_mut().state.pending.push_back(CommandKind::Redo);
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "redo puts it back");
    // Parametric + Apply is the way back: one undoable command.
    harness.get_by_label("Parametric").click();
    harness.run();
    harness.get_by_label("Apply cutter").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "drawn from its numbers again");
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run();
    assert!(drawn(&harness.state().state).is_some(), "undo restores the drawn cutter");
}

#[test]
fn a_cutter_off_the_numbers_is_refused_by_name_live_and_at_apply() {
    let mut harness = cutter_tab();
    harness.get_by_label("Custom").click();
    harness.run();
    // Ø7 within the flutes against the stated Ø6.
    harness.state_mut().state.tool_cutter_ui.points[1].r = 3.5;
    harness.state_mut().state.tool_cutter_ui.points[2].r = 3.5;
    harness.run();
    harness.run();
    assert!(harness.query_all_by_label_contains("the tool states Ø 6.000").next().is_some(), "the live refusal names it");
    harness.get_by_label("Apply cutter").click();
    harness.run();
    harness.run();
    assert!(drawn(&harness.state().state).is_none(), "nothing lands");
    let err = harness.state().state.error_for(ErrorScope::Tool).map(str::to_owned).unwrap_or_default();
    assert!(err.starts_with("cutter profile not set: ") && err.contains("Ø 7.000"), "{err}");
}

// #2003: pointer repaints must not feed the previous window width back into layout.
fn assert_editor_width_stays_put(subject: &str, title: &str, unit: Unit, feed: Option<f64>) {
    let _guard = snapshot_guard();
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::SetUnits(unit));
    harness.state_mut().state.drain_actions();
    crate::actions::open_tool_shape_editor(&mut harness.state_mut().state, subject);
    if let Some(feed) = feed {
        harness.state_mut().state.tool_ui.feed = feed;
    }
    // The first pass measures an invisible egui window; the next paints it.
    harness.run_steps(2);
    let width = |h: &Harness<'static, ShellUi>| {
        h.query_all_by_role(egui::accesskit::Role::Window)
            .find(|n| n.accesskit_node().label().as_deref() == Some(title))
            .expect("tool editor is visible")
            .rect()
            .width()
    };
    let first = width(&harness);
    for frame in 0..100 {
        harness.input_mut().events.push(Event::PointerMoved(egui::pos2(
            20.0 + frame as f32,
            20.0,
        )));
        harness.run_steps(1);
    }
    let last = width(&harness);
    assert!(
        (last - first).abs() <= 1.0,
        "tool editor width changed after 100 pointer frames: first={first}, last={last}"
    );
}

#[test]
fn existing_tool_editor_width_stays_put_after_100_pointer_frames() {
    assert_editor_width_stays_put(TOOL, "Tool shape: EM-0600-4FL", Unit::Millimeters, None);
}

#[test]
fn blank_tool_shape_width_stays_put_after_100_pointer_frames() {
    assert_editor_width_stays_put("", "Tool shape", Unit::Millimeters, None);
}

#[test]
fn wide_metric_feed_tool_editor_width_stays_put_after_100_pointer_frames() {
    assert_editor_width_stays_put(TOOL, "Tool shape: EM-0600-4FL", Unit::Millimeters, Some(1.0e12));
}

#[test]
fn wide_inch_feed_tool_editor_width_stays_put_after_100_pointer_frames() {
    assert_editor_width_stays_put(TOOL, "Tool shape: EM-0600-4FL", Unit::Inches, Some(1.0e12));
}
