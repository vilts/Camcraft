//! #441: the Stock page shows stock only - Machine and Post are edited on
//! their own setup nodes, and the document's theme moved to a Theme door
//! on the ribbon, right of the View door, whose picks ride the history.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use crate::state::SetupPage;
use camcraft_app::commands::{color_vision_of, theme_of};
use camcraft_app::schema::ColorVision;

#[test]
fn the_stock_page_has_no_machine_post_or_theme_controls() {
    let mut harness = shell_harness();
    harness.state_mut().state.ui_target = UiTarget::Stock;
    harness.run();
    assert!(harness.query_by_label("Fit to Models").is_some(), "the stock page is on screen");
    let post = camcraft_app::posting::post_label(&harness.state().state.world);
    let pane = harness
        .state()
        .dock
        .iter_leaves()
        .find(|(_, l)| l.tabs.contains(&Tab::Properties))
        .map(|(_, l)| l.viewport)
        .expect("the properties leaf");
    for gone in ["Library", "Colour vision", "Theme", post.as_str()] {
        assert!(
            harness.query_all_by_label(gone).all(|n| !pane.contains(n.rect().center())),
            "the stock page still shows '{gone}'"
        );
    }
}

#[test]
fn the_theme_door_sits_right_of_panels() {
    let plan = panes::ribbon_plan(camcraft_app::machine::Kind::Mill, &[]);
    let view = plan.iter().find(|g| g.title == "View").expect("the View group");
    assert_eq!(view.cells.as_slice(), &[Cmd::View, Cmd::Theme]);
    let mut harness = shell_harness();
    harness.run();
    let panels = harness.get_by_role_and_label(egui::accesskit::Role::Button, "Panels").rect();
    let theme = harness.get_by_role_and_label(egui::accesskit::Role::Button, "Theme").rect();
    assert!(theme.left() >= panels.right(), "Theme stands right of Panels");
}

#[test]
fn the_theme_door_switches_the_theme_and_undo_leaves_it() {
    // #605: the pick lands through the frame-edge sync, which writes the
    // ONE process-wide live table (`theme::current()`); the structural
    // and snapshot tests assert on that table, so this test shares their
    // lock or a parallel run flips their look mid-frame.
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.run();
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Dark);
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Theme").click();
    harness.run_steps(2);
    assert!(harness.state().state.ribbon_theme_open, "pressing the door opens its list");
    assert!(
        harness
            .query_by_role_and_label(egui::accesskit::Role::MenuItem, "Browse...")
            .is_some(),
        "the list offers Browse for any theme file"
    );
    harness.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Light").click();
    harness.run_steps(2);
    assert!(!harness.state().state.ribbon_theme_open, "a choice closes the list");
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Named("light".into()));
    // #877: the theme is the user's preference, not a document edit, so
    // Ctrl+Z does not take it back.
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(3);
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Named("light".into()), "undo leaves the theme");
    // Leave the house look for whatever frame follows.
    harness.state_mut().state.pending.push_back(CommandKind::SelectTheme(ThemeRef::Dark));
    harness.run_steps(3);
    assert_eq!(theme_of(&harness.state().state.world), ThemeRef::Dark);
}

#[test]
fn the_theme_door_carries_the_colour_vision_modes() {
    let mut harness = shell_harness();
    harness.run();
    harness.get_by_role_and_label(egui::accesskit::Role::Button, "Theme").click();
    harness.run_steps(2);
    harness.get_by_role_and_label(egui::accesskit::Role::MenuItem, "Deuteranopia").click();
    harness.run_steps(2);
    assert_eq!(color_vision_of(&harness.state().state.world), ColorVision::Deutan);
    harness.state_mut().state.pending.push_back(CommandKind::Undo);
    harness.run_steps(3);
    assert_eq!(color_vision_of(&harness.state().state.world), ColorVision::Deutan, "undo leaves the mode (#877)");
}

#[test]
fn machine_and_post_stay_editable_on_their_setup_nodes() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    harness.run_steps(2);
    assert!(harness.query_by_label("Library").is_some(), "the machine node carries the machine library");
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    harness.run_steps(2);
    let post = camcraft_app::posting::post_label(&harness.state().state.world);
    assert!(harness.query_by_label(&post).is_some(), "the post node carries the post choice");
}
