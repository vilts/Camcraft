//! #1494: each setup row in the browser wears an eye. Only the active setup
//! is drawn by default; another setup's operation paths and work offset
//! come up when its eye is turned on, without making it active. The eye is
//! display only: what posts is the same list with the eye on or off.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::*;
use camcraft_app::setup;
use super::super::browser_pane;
use super::super::browser_tree::Leaf;

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

/// Contour1 generated into Setup1, Setup2 created last (so it is active).
fn two_setups() -> ShellUi {
    let mut shell = ShellUi::seeded();
    let state = &mut shell.state;
    run(state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(state, CommandKind::GenerateOperation("Contour1".into()));
    run(state, CommandKind::CreateSetup { name: None, datum_from: None });
    state.show_all_toolpaths();
    shell
}

fn setup_row(state: &State, name: &str) -> (bool, usize) {
    let (rows, leaves) = panes::browser_rows(state);
    let ix = leaves.iter().position(|l| *l == Leaf::Setup(name.to_owned())).expect("setup row");
    (rows[ix].hidden, ix)
}

#[test]
fn a_setup_row_wears_an_eye_and_only_the_active_setup_is_shown() {
    let shell = two_setups();
    let state = &shell.state;
    assert!(painted::has_eye(painted::Kind::Setup(true)));
    assert!(painted::has_eye(painted::Kind::Setup(false)));
    assert!(!setup_row(state, "Setup2").0, "the active setup is shown");
    assert!(setup_row(state, "Setup1").0, "another setup is hidden by default");
}

#[test]
fn the_eye_shows_another_setups_paths_without_activating_it() {
    let mut shell = two_setups();
    let state = &mut shell.state;
    assert!(interact::toolpath_lines(state).is_empty(), "Setup1's path is not drawn while Setup2 is active");
    let (_, ix) = setup_row(state, "Setup1");
    let (_, leaves) = panes::browser_rows(state);
    browser_pane::eye(state, &leaves[ix]);
    assert!(!setup_row(state, "Setup1").0);
    assert!(!interact::toolpath_lines(state).is_empty(), "the eye turned Setup1's path on");
    assert_eq!(setup::active(&state.world).as_deref(), Some("Setup2"), "the eye never activates");
    browser_pane::eye(state, &leaves[ix]);
    assert!(interact::toolpath_lines(state).is_empty(), "off again");
}

#[test]
fn activating_a_setup_draws_it_and_the_active_setups_eye_does_nothing() {
    let mut shell = two_setups();
    let state = &mut shell.state;
    run(state, CommandKind::ActivateSetup(Some("Setup1".into())));
    assert!(!interact::toolpath_lines(state).is_empty());
    assert!(!setup_row(state, "Setup1").0);
    state.toggle_setup_display("Setup1");
    assert!(state.shown_setups.is_empty(), "the active setup's eye is a no-op");
    assert!(!interact::toolpath_lines(state).is_empty());
}

#[test]
fn another_setups_work_offset_follows_its_eye() {
    let mut shell = two_setups();
    let state = &mut shell.state;
    let wcs = setup::wcs_entity_name(&state.world, Some("Setup1"));
    assert!(state.wcs_hidden_by_setup(&wcs));
    assert!(!state.wcs_hidden_by_setup(&setup::wcs_entity_name(&state.world, Some("Setup2"))));
    state.toggle_setup_display("Setup1");
    assert!(!state.wcs_hidden_by_setup(&wcs));
}

#[test]
fn hiding_a_setup_never_changes_what_posts() {
    let mut shell = two_setups();
    let state = &mut shell.state;
    let ops = |s: &State| {
        camcraft_app::posting::program_ops(&s.world)
            .map(|v| v.into_iter().map(|(n, _)| n).collect::<Vec<_>>())
    };
    let hidden = ops(state);
    assert_eq!(hidden.as_ref().map(|v| v.contains(&"Contour1".to_owned())), Ok(true), "Setup1's op posts while hidden");
    state.toggle_setup_display("Setup1");
    assert_eq!(ops(state), hidden, "the eye is display only");
    assert!(model::find_by_name(&state.world, "Contour1").and_then(|e| model::toolpath_of(&state.world, &e)).is_some(), "the cached path stays");
}
