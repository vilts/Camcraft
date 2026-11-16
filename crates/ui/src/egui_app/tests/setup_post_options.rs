//! #389 in the shell: a setup's Post page offers its own post options
//! (follow the document's, or this setup's own - one undoable command),
//! the option widgets then edit only that setup's options, and the Tool
//! list page exports the setup's own tool list to a file (no history).
//! Provenance: our own tests over our own shell; no vendor material.

use super::*;
use crate::state::SetupPage;
use camcraft_app::schema::PostOptions;
use camcraft_app::setup_post_options;

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

#[test]
fn the_option_switches_land_on_the_active_setups_own_options() {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::SetupOwnPostOptions { setup: "Setup2".into(), own: true });
    assert_eq!(setup_post_options::own(&state.world, Some("Setup2")), Some(PostOptions::default()));
    run(&mut state, CommandKind::SetSkipStops(true));
    run(&mut state, CommandKind::SetPerOpOutput(true));
    let own = PostOptions { skip_stops: true, per_op: true, ..PostOptions::default() };
    assert_eq!(setup_post_options::own(&state.world, Some("Setup2")), Some(own));
    assert_eq!(setup_post_options::document(&state.world), PostOptions::default(), "the document is untouched");
    assert_eq!(setup_post_options::of(&state.world, Some("Setup1")), PostOptions::default(), "Setup1 follows it");
    // Setup1 active: with two setups the same switch now makes Setup1's
    // own options (#1526) - the document's, which a later setup would
    // follow, never move from a setup's page.
    run(&mut state, CommandKind::ActivateSetup(Some("Setup1".into())));
    run(&mut state, CommandKind::SetSkipStops(true));
    assert!(setup_post_options::own(&state.world, Some("Setup1")).is_some_and(|o| o.skip_stops));
    assert!(!setup_post_options::document(&state.world).skip_stops);
    // Undo walks every step back.
    for _ in 0..5 {
        run(&mut state, CommandKind::Undo);
    }
    assert_eq!(setup_post_options::own(&state.world, Some("Setup2")), None);
    assert_eq!(setup_post_options::own(&state.world, Some("Setup1")), None);
    assert_eq!(setup_post_options::document(&state.world), PostOptions::default());
}

#[test]
fn the_post_page_radio_gives_the_setup_its_own_options() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    harness.run_steps(2);
    assert!(harness.query_by_label("Use the document's post options").is_some());
    harness.get_by_label("This setup has its own post options").click();
    harness.run_steps(2);
    assert!(setup_post_options::own(&harness.state().state.world, Some("Setup1")).is_some());
    harness.get_by_label("Skip pauses and homes").click();
    harness.run_steps(2);
    let world = &harness.state().state.world;
    assert!(setup_post_options::own(world, Some("Setup1")).is_some_and(|o| o.skip_stops), "the setup's own switch");
    assert!(!setup_post_options::document(world).skip_stops, "the document keeps its own");
}

#[test]
fn the_tool_list_page_exports_the_setups_tools() {
    let mut harness = shell_harness();
    harness.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    harness.run_steps(2);
    crate::actions::target(&mut harness.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Tools));
    harness.run_steps(2);
    assert!(harness.query_by_label("Export tool list…").is_some(), "the export button is on the page");
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-389-ui-{}.csv", std::process::id()));
    let version = harness.state().state.doc_version;
    let state = &mut harness.state_mut().state;
    run(state, CommandKind::ToolListExportFile { setup: Some("Setup1".into()), path: path.clone() });
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(text.starts_with("T,Tool,Diameter mm,Kind,Holder,Operations\r\n"), "{text}");
    assert_eq!(text, camcraft_app::tool_list_export::csv(&state.world, Some("Setup1")));
    assert!(state.status.contains("tool list"), "{}", state.status);
    assert_eq!(state.doc_version, version, "an export is not a command");
}
