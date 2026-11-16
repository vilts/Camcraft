//! #692's pick-through toggle: the viewport's nav column carries a
//! "Pick through" button, off by default and shown as off. With it off a
//! click over a hidden edge takes the near face or nothing; with it on the
//! same click reaches the far-side edge. Toggling queues no command (view
//! state, like the axes toggle).
//! Provenance: our own viewport test over our own pick law; no vendor
//! material consulted.

use camcraft_app::operation::GeoSel;

use super::one_pick_rule::{op_click, over_a_hidden_edge};
use super::*;

fn nav_center(harness: &Harness<'static, ShellUi>, kind: viewcube::Nav) -> egui::Pos2 {
    hud(harness.state()).nav_rect(kind).center()
}

#[test]
fn the_pick_through_button_is_off_by_default_and_labelled_in_plain_english() {
    let mut harness = shell_harness();
    harness.run();
    assert!(!harness.state().state.pick_through, "off by default");
    assert_eq!(viewcube::pick_through_label(false), "Pick through: off");
    assert_eq!(viewcube::pick_through_label(true), "Pick through: on");
    assert!(hud(harness.state()).nav_rect(viewcube::Nav::PickThrough) != egui::Rect::ZERO, "the button is drawn");
}

#[test]
fn the_toggle_flips_twice_back_to_off_and_queues_no_command() {
    let mut harness = shell_harness();
    harness.run();
    let p = nav_center(&harness, viewcube::Nav::PickThrough);
    click_viewport(&mut harness, p);
    assert!(harness.state().state.pick_through, "one click turns it on");
    assert!(harness.state().state.pending.is_empty(), "a view toggle is not a command");
    click_viewport(&mut harness, p);
    assert!(!harness.state().state.pick_through, "a second click turns it off");
}

#[test]
fn with_the_toggle_off_a_click_over_a_hidden_edge_never_picks_the_edge() {
    let mut harness = shell_harness();
    harness.run();
    let (p, _) = over_a_hidden_edge(harness.state());
    let items = op_click(&mut harness, p);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(matches!(items[0].feature, FeatureSel::Face(_)), "the near face, never the far edge: {items:?}");
}

#[test]
fn with_the_toggle_on_a_click_over_a_hidden_edge_reaches_the_far_side_edge() {
    let mut harness = shell_harness();
    harness.run();
    let p_nav = nav_center(&harness, viewcube::Nav::PickThrough);
    click_viewport(&mut harness, p_nav);
    assert!(harness.state().state.pick_through, "the toggle is on");
    let (p, _) = over_a_hidden_edge(harness.state());
    let items: Vec<GeoSel> = op_click(&mut harness, p);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(matches!(items[0].feature, FeatureSel::Chain(_)), "the far-side edge is picked: {items:?}");
}
