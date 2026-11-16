//! #1526: a post option changed while one setup is active never changes
//! what another setup posts with. With two or more setups the edit
//! becomes the active setup's own options (`SetSetupPostOptions`, the
//! #1515 law), so a setup that follows the document keeps reading what it
//! read; with one setup the edit still sets the document's options, and
//! the Post page says by name which of the two an edit does.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::setup_mount_pages::two_setups;
use super::*;
use crate::egui_app::setup_post_page::options_target_line;
use crate::state::SetupPage;
use camcraft_app::schema::{EndUnits, PostOptions};
use camcraft_app::setup_post_options::{document, of, own};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

fn setup(state: &mut State) {
    run(state, CommandKind::CreateSetup { name: None, datum_from: None });
}

/// The report: the document skips pauses while it has one setup; a second
/// setup follows it, and turning the skip off on Setup2 leaves Setup1 (and
/// the document) still skipping. Setup2 reads its change; undo / redo walk
/// both.
#[test]
fn an_option_changed_on_setup2_leaves_setup1s_posted_options() {
    let mut state = State::new();
    setup(&mut state);
    run(&mut state, CommandKind::SetSkipStops(true));
    setup(&mut state);
    run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
    let skipping = PostOptions { skip_stops: true, ..PostOptions::default() };
    assert_eq!(of(&state.world, Some("Setup1")), skipping);
    assert_eq!(of(&state.world, Some("Setup2")), skipping, "Setup2 follows the document");

    run(&mut state, CommandKind::SetSkipStops(false));
    let w = &state.world;
    assert_eq!(own(w, Some("Setup2")), Some(PostOptions::default()), "status: {}", state.status);
    assert_eq!(of(w, Some("Setup1")), skipping, "Setup1 still posts without its stops");
    assert_eq!(document(w), skipping, "the document's options are untouched");
    assert!(state.status.contains("(Setup2)"), "the status names the setup: {}", state.status);

    run(&mut state, CommandKind::Undo);
    assert_eq!(own(&state.world, Some("Setup2")), None, "undo: Setup2 follows again");
    assert_eq!(of(&state.world, Some("Setup2")), skipping);
    assert_eq!(of(&state.world, Some("Setup1")), skipping);
    run(&mut state, CommandKind::Redo);
    assert_eq!(of(&state.world, Some("Setup2")), PostOptions::default());
    assert_eq!(of(&state.world, Some("Setup1")), skipping);
}

/// Every option widget routes the same way: Output, end units (the
/// generic `SetPostOptions`) on Setup2 each leave Setup1 on the
/// document's defaults; two undos restore Setup2 to following.
#[test]
fn output_and_options_on_setup2_never_reach_setup1() {
    let mut state = State::new();
    setup(&mut state);
    setup(&mut state);
    run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
    run(&mut state, CommandKind::SetPerOpOutput(true));
    let now = camcraft_app::posting::post_options(&state.world);
    run(&mut state, CommandKind::SetPostOptions(PostOptions { end_units: EndUnits::Inch, ..now }));
    let w = &state.world;
    let mine = PostOptions { per_op: true, end_units: EndUnits::Inch, ..PostOptions::default() };
    assert_eq!(own(w, Some("Setup2")), Some(mine), "status: {}", state.status);
    assert_eq!(of(w, Some("Setup1")), PostOptions::default(), "Setup1 is untouched");
    assert_eq!(document(w), PostOptions::default(), "the document is untouched");
    run(&mut state, CommandKind::Undo);
    run(&mut state, CommandKind::Undo);
    assert_eq!(own(&state.world, Some("Setup2")), None);
    assert_eq!(of(&state.world, Some("Setup1")), PostOptions::default());
}

/// One setup: the edit still sets the document's options (unchanged
/// behaviour - nothing else reads them), and undo takes it back.
#[test]
fn with_one_setup_an_option_still_sets_the_documents() {
    let mut state = State::new();
    setup(&mut state);
    run(&mut state, CommandKind::SetSkipStops(true));
    assert!(document(&state.world).skip_stops, "status: {}", state.status);
    assert_eq!(own(&state.world, Some("Setup1")), None, "Setup1 follows the document");
    run(&mut state, CommandKind::Undo);
    assert_eq!(document(&state.world), PostOptions::default());
}

/// Through the real page: Setup2 follows the document, the page says an
/// edit is Setup2's own, and the Skip switch leaves Setup1 alone.
#[test]
fn the_setup2_post_page_says_and_does_its_own_options() {
    let mut h = shell_harness();
    two_setups(&mut h, SetupPage::Post);
    let line = options_target_line("Setup2", true);
    assert!(h.query_by_label(&line).is_some(), "the page names where an edit lands: {line}");
    h.get_by_label("Skip pauses and homes").click();
    h.run_steps(2);
    let st = &h.state().state;
    assert!(own(&st.world, Some("Setup2")).is_some_and(|o| o.skip_stops), "status: {}", st.status);
    assert_eq!(of(&st.world, Some("Setup1")), PostOptions::default(), "Setup1 is untouched");
}

/// With one setup the page names the document's options as the target.
#[test]
fn the_only_setups_post_page_names_the_documents_options() {
    let mut h = shell_harness();
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    h.run_steps(3);
    let doc = options_target_line("Setup1", false);
    assert!(doc.contains("the document's post options"), "{doc}");
    assert!(h.query_by_label(&doc).is_some(), "the page names the document's options: {doc}");
}
