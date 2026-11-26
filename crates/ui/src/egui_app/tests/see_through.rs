//! The "Wireframe, see-through" display mode (#2636 "Done means"): the
//! Display menu has the entry and the entry sets the mode; the mode is kept
//! in the view file; a click in the mode picks a hidden edge.
//!
//! The picking rule of the mode: each edge can be picked, front or back.
//! The edge that draws nearest to the cursor wins. When two edges draw at
//! the same distance from the cursor, the edge nearest to the camera wins.
//! `feature_pick::see_through_takes_the_edge_nearest_the_cursor_then_nearest_the_eye`
//! (`camcraft-app`) pins the order; the tests here pin that the mode asks
//! for it with the "Pick through" switch off.
//! Provenance: our own tests over our own menu and pick law; no vendor
//! material.
//! Gate map: covers ui::display_mode, ui::egui_app::display_menu,
//! ui::feature_click, ui::state_view.

use egui_kittest::kittest::{NodeT, Queryable};

use super::one_pick_rule::{op_click, over_a_hidden_edge};
use super::*;
use crate::state::DisplayMode;

const LABEL: &str = "Wireframe, see-through";
const KEY: &str = "wireframe-see-through";

fn open_menu() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.run();
    let p = hud(h.state()).nav_rect(viewcube::Nav::Display).center();
    click_viewport(&mut h, p);
    assert!(h.state().state.display_menu_open, "a press on Display opens the menu");
    h
}

#[test]
fn the_see_through_entry_exists_sets_its_mode_and_shows_the_check() {
    let mut h = open_menu();
    assert!(h.query_by_label(LABEL).is_some(), "the entry {LABEL:?} exists");
    h.get_by_label(LABEL).click();
    h.run_steps(2);
    let mode = h.state().state.display;
    assert_eq!(mode, DisplayMode::WireframeSeeThrough, "the entry sets its mode");
    assert_eq!(mode.key(), KEY);
    assert_eq!(mode.label(), LABEL);
    assert!(!mode.tooltip().is_empty(), "the entry has a tooltip");
    for other in DisplayMode::ALL {
        let want = if other == mode { egui::accesskit::Toggled::True } else { egui::accesskit::Toggled::False };
        assert_eq!(h.get_by_label(other.label()).accesskit_node().toggled(), Some(want), "{:?}", other.label());
    }
    assert_ne!(mode, DisplayMode::Wireframe, "the hidden-line wireframe stays its own mode");
    assert!(h.state().state.pending.is_empty(), "a display mode is view state, never a command");
}

#[test]
fn the_see_through_mode_is_kept_in_the_view_file() {
    use camcraft_app::view;
    let dir = std::env::temp_dir().join(format!("camcraft_see_through_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("make the test folder");
    let file = dir.join(view::FILE_NAME);
    std::fs::write(&file, format!(r#"{{"format":"camcraft-view/1","view":{{"display":"{KEY}"}}}}"#))
        .expect("write the view file");
    let mut st = State::new();
    st.load_view_from(&file);
    assert_eq!(st.display, DisplayMode::WireframeSeeThrough, "the stored key loads the mode");
    st.persist_view();
    assert_eq!(view::load(&file).0.display, KEY, "the mode is written back");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn in_see_through_a_click_over_a_hidden_edge_picks_that_edge() {
    let mut h = shell_harness();
    h.run();
    h.state_mut().state.display = DisplayMode::WireframeSeeThrough;
    assert!(!h.state().state.pick_through, "the Pick through switch stays off");
    let (p, _) = over_a_hidden_edge(h.state());
    let items = op_click(&mut h, p);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(matches!(items[0].feature, FeatureSel::Chain(_)), "the far-side edge is picked: {items:?}");
}

#[test]
fn the_hidden_line_wireframe_still_picks_only_what_is_in_sight() {
    let mut h = shell_harness();
    h.run();
    h.state_mut().state.display = DisplayMode::Wireframe;
    let (p, _) = over_a_hidden_edge(h.state());
    let items = op_click(&mut h, p);
    assert_eq!(items.len(), 1, "{items:?}");
    assert!(matches!(items[0].feature, FeatureSel::Face(_)), "the near face, never the far edge: {items:?}");
}

#[test]
fn the_wireframe_switch_leaves_see_through_for_the_default_mode() {
    let mut st = State::new();
    st.display = DisplayMode::WireframeSeeThrough;
    crate::egui_app::viewport_bar::act(&mut st, viewcube::Nav::Wire);
    assert_eq!(st.display, DisplayMode::ShadedEdges, "one press leaves each wireframe mode");
    assert!(DisplayMode::WireframeSeeThrough.is_wireframe() && DisplayMode::Wireframe.is_wireframe());
    assert!(!DisplayMode::Wireframe.sees_through(), "only the see-through mode picks hidden edges");
}
