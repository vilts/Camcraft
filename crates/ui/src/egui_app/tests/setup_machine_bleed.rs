//! #1515: a machine (or post) picked while one setup is active never
//! re-mounts another setup. With two or more setups the pick becomes the
//! active setup's own (`SetSetupMachine` / `SetSetupPost`), so a setup
//! that follows the document - or owns its machine - keeps reading what
//! it read; with one setup the pick still sets the document's machine,
//! and the page says by name which of the two a pick does.
//! Provenance: our own tests over our own shell; no HSMWorks material.

use super::setup_mount_pages::{library_label, pick, two_setups};
use super::*;
use crate::state::SetupPage;
use camcraft_app::schema::PostRef;
use camcraft_app::{machine, posting, setup_mount};

fn run(state: &mut State, k: CommandKind) {
    state.pending.push_back(k);
    state.drain_actions();
}

/// Two declared setups, Setup2 active (CreateSetup activates the new one).
fn two(state: &mut State) {
    run(state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(state, CommandKind::ActivateSetup(Some("Setup2".into())));
}

fn lathe() -> String {
    posting::builtin_path("Lathe")
}

/// The report: Setup1 with no machine, a library pick on Setup2 - Setup1
/// still has none, Setup2 reads the pick, and undo / redo walk both back.
#[test]
fn a_machine_picked_on_setup2_leaves_setup1_without_one() {
    let mut state = State::new();
    two(&mut state);
    assert_eq!(machine::of_setup(&state.world, Some("Setup1")), (String::new(), None));
    run(&mut state, CommandKind::LoadBuiltinMachine("Lathe".into()));
    let w = &state.world;
    assert_eq!(machine::of_setup(w, Some("Setup2")).0, lathe(), "status: {}", state.status);
    assert!(setup_mount::own_machine(w, Some("Setup2")).is_some(), "the pick is Setup2's own");
    assert_eq!(machine::of_setup(w, Some("Setup1")), (String::new(), None), "Setup1 still has no machine");
    assert_eq!(machine::document_of(w), (String::new(), None), "the document's machine is untouched");
    assert!(state.status.contains("(Setup2)"), "the status names the setup: {}", state.status);

    run(&mut state, CommandKind::Undo);
    assert_eq!(setup_mount::own_machine(&state.world, Some("Setup2")), None, "undo: Setup2 follows again");
    assert_eq!(machine::of_setup(&state.world, Some("Setup1")), (String::new(), None));
    run(&mut state, CommandKind::Redo);
    assert_eq!(machine::of_setup(&state.world, Some("Setup2")).0, lathe());
    assert_eq!(machine::of_setup(&state.world, Some("Setup1")), (String::new(), None));
}

/// Setup1 following the document's 3-axis mill: a Clear and a pick
/// on Setup2 each leave Setup1 and the document alone.
#[test]
fn verbs_on_setup2_never_move_setup1s_or_the_documents_machine() {
    let mut state = State::new();
    // The document mounts a 3-axis mill while it has one setup.
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::LoadBuiltinMachine("VMC 3-axis".into()));
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::ActivateSetup(Some("Setup2".into())));
    let vmc = posting::builtin_path("VMC 3-axis");
    assert_eq!(machine::of_setup(&state.world, Some("Setup1")).0, vmc);
    // Clear on Setup2 (following the document) states its own "no machine".
    run(&mut state, CommandKind::ClearMachine);
    assert_eq!(machine::of_setup(&state.world, Some("Setup2")), (String::new(), None), "status: {}", state.status);
    assert_eq!(machine::of_setup(&state.world, Some("Setup1")).0, vmc, "Setup1 keeps the document's mill");
    assert_eq!(machine::document_of(&state.world).0, vmc, "the document keeps its mill");
    run(&mut state, CommandKind::LoadBuiltinMachine("Lathe".into()));
    assert_eq!(machine::of_setup(&state.world, Some("Setup2")).0, lathe());
    assert_eq!(machine::of_setup(&state.world, Some("Setup1")).0, vmc);
    // Two undos: Setup2 follows the document's mill again; Setup1 never moved.
    run(&mut state, CommandKind::Undo);
    run(&mut state, CommandKind::Undo);
    assert_eq!(setup_mount::own_machine(&state.world, Some("Setup2")), None);
    assert_eq!(machine::of_setup(&state.world, Some("Setup2")).0, vmc);
    assert_eq!(machine::of_setup(&state.world, Some("Setup1")).0, vmc);
}

/// One setup: the pick still sets the document's machine (unchanged
/// behaviour - nothing else reads it), and undo takes it back.
#[test]
fn with_one_setup_a_pick_still_sets_the_documents_machine() {
    let mut state = State::new();
    run(&mut state, CommandKind::CreateSetup { name: None, datum_from: None });
    run(&mut state, CommandKind::LoadBuiltinMachine("Lathe".into()));
    assert_eq!(machine::document_of(&state.world).0, lathe(), "status: {}", state.status);
    assert_eq!(setup_mount::own_machine(&state.world, Some("Setup1")), None, "Setup1 follows the document");
    run(&mut state, CommandKind::Undo);
    assert_eq!(machine::document_of(&state.world), (String::new(), None));
}

/// The post verbs keep the same law: a post picked on Setup2 is its own,
/// Setup1 keeps the document's post, and undo gives it back.
#[test]
fn a_post_picked_on_setup2_leaves_setup1s_post() {
    let mut state = State::new();
    two(&mut state);
    run(&mut state, CommandKind::SelectPostNamed("Haas".into()));
    let w = &state.world;
    assert_eq!(setup_mount::own_post(w, Some("Setup2")), Some(PostRef::Named("Haas".into())), "status: {}", state.status);
    assert_eq!(setup_mount::post_of(w, Some("Setup1")), PostRef::Builtin, "Setup1 keeps the document's post");
    assert_eq!(posting::document_post_of(w), PostRef::Builtin, "the document's post is untouched");
    run(&mut state, CommandKind::Undo);
    assert_eq!(setup_mount::own_post(&state.world, Some("Setup2")), None);
    assert_eq!(setup_mount::post_of(&state.world, Some("Setup1")), PostRef::Builtin);
}

/// Through the real page: Setup2 follows the document, the page says a
/// pick is Setup2's own, and a Library combo pick leaves Setup1 alone.
#[test]
fn the_setup2_machine_page_says_and_does_its_own_pick() {
    let mut h = shell_harness();
    two_setups(&mut h, SetupPage::Machine);
    let own = crate::egui_app::setup_pages::target_line("machine", "Setup2", true);
    assert!(h.query_by_label(&own).is_some(), "the page names where a pick lands: {own}");
    let shown = library_label(&h);
    pick(&mut h, &shown, "Lathe");
    let st = &h.state().state;
    assert_eq!(machine::of_setup(&st.world, Some("Setup2")).0, lathe(), "status: {}", st.status);
    assert_eq!(machine::of_setup(&st.world, Some("Setup1")), (String::new(), None), "Setup1 is untouched");
}

/// With one setup the page names the document's machine as the target.
#[test]
fn the_only_setups_machine_page_names_the_documents_machine() {
    let mut h = shell_harness();
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    h.run_steps(3);
    let doc = crate::egui_app::setup_pages::target_line("machine", "Setup1", false);
    assert!(doc.contains("the document's machine"), "{doc}");
    assert!(h.query_by_label(&doc).is_some(), "the page names the document's machine: {doc}");
}

/// #1527 review: the Post page reads the POST's target, not the
/// machine's. One setup that owns its machine but follows the document's
/// post: its Post page names the document's post (a machine-target
/// mix-up would call the pick Setup1's own).
#[test]
fn a_setup_owning_only_its_machine_names_the_documents_post_on_its_post_page() {
    let mut h = shell_harness();
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    h.state_mut().state.pending.push_back(CommandKind::SetupOwnMachine { setup: "Setup1".into(), own: true });
    h.run_steps(2);
    let st = &h.state().state;
    assert_eq!(st.machine_target().as_deref(), Some("Setup1"), "status: {}", st.status);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    h.run_steps(3);
    let doc = crate::egui_app::setup_pages::target_line("post", "Setup1", false);
    assert!(h.query_by_label(&doc).is_some(), "the Post page names the document's post: {doc}");
    let own = crate::egui_app::setup_pages::target_line("post", "Setup1", true);
    assert!(h.query_by_label(&own).is_none(), "never the machine's target: {own}");
}
