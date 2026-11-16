//! #389 in the shell: a setup's Post page offers its own post options
//! (follow the document's, or this setup's own - one undoable command),
//! the option widgets then edit only that setup's options, and the Tool
//! list page exports the setup's own tool list to a file (no history).
//! Provenance: our own tests over our own shell; no vendor material.

use super::*;
use egui_kittest::kittest::NodeT;
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

// The UI switch reaches the post engine through the same options object as a real post.
fn explain_program(options: PostOptions) -> String {
    let cps = r#"explains = "paren";
function onOpen() {
    var words = ["G21"];
    var comment = explain(words);
    writeln(words.join(" ") + (comment ? " " + comment : ""));
}
"#;
    camcraft_app::post::engine::run_with(
        cps,
        &serde_json::Value::Null,
        "",
        &camcraft_app::posting::options_value(options),
    )
    .expect("the original fixture posts")
    .program
}

fn assert_explain_checked(h: &Harness<'static, ShellUi>, checked: bool) {
    let node = h.get_by(|n| {
        n.role() == egui::accesskit::Role::CheckBox
            && n.label().as_deref() == Some("Explain G-code (add a comment to each block)")
    });
    let expected = if checked { egui::accesskit::Toggled::True } else { egui::accesskit::Toggled::False };
    assert_eq!(node.accesskit_node().toggled(), Some(expected));
}

/// #1390: the document page exposes Explain through the undoable options command.
#[test]
fn explain_on_the_document_post_page_is_undoable() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode(String::new(), SetupPage::Post));
    h.run_steps(3);
    let label = "Explain G-code (add a comment to each block)";
    assert!(h.query_by(|n| n.role() == egui::accesskit::Role::CheckBox && n.label().as_deref() == Some(label)).is_some(), "the document post page offers Explain");
    assert!(h.query_by_label("The program gets longer.").is_some(), "the page states the size cost");
    let before = setup_post_options::document(&h.state().state.world);
    let plain = explain_program(before);
    assert_explain_checked(&h, false);
    h.get_by(|n| {
        n.role() == egui::accesskit::Role::CheckBox && n.label().as_deref() == Some(label)
    })
    .click();
    h.run_steps(2);
    assert_explain_checked(&h, true);
    let after = setup_post_options::document(&h.state().state.world);
    assert_eq!(after, PostOptions { explain: true, ..before });
    let explained = explain_program(after);
    assert_ne!(explained, plain, "the checkbox changes posted text");
    assert!(explained.contains(" ("), "an explanation comment is appended: {explained}");
    assert_eq!(explained.split(" (").next().unwrap_or_default(), plain.trim_end(), "executable words stay identical");
    run(&mut h.state_mut().state, CommandKind::Undo);
    h.run_steps(2);
    assert_explain_checked(&h, false);
    assert_eq!(setup_post_options::document(&h.state().state.world), before);
    assert_eq!(explain_program(setup_post_options::document(&h.state().state.world)), plain);
    run(&mut h.state_mut().state, CommandKind::Redo);
    assert!(setup_post_options::document(&h.state().state.world).explain);
}

/// #1390: changing Setup2 explains only its output, and undo restores inheritance.
#[test]
fn explain_on_the_setup_post_page_leaves_other_setups_unchanged() {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 1100.0))
        .build_ui_state(frame, ShellUi::seeded());
    super::setup_mount_pages::two_setups(&mut h, SetupPage::Post);
    let label = "Explain G-code (add a comment to each block)";
    assert!(h.query_by(|n| n.role() == egui::accesskit::Role::CheckBox && n.label().as_deref() == Some(label)).is_some(), "the setup post page offers Explain");
    assert!(h.query_by_label("The program gets longer.").is_some(), "the page states the size cost");
    let before = setup_post_options::of(&h.state().state.world, Some("Setup2"));
    assert_explain_checked(&h, false);
    h.get_by(|n| {
        n.role() == egui::accesskit::Role::CheckBox && n.label().as_deref() == Some(label)
    })
    .click();
    h.run_steps(2);
    assert_explain_checked(&h, true);
    let world = &h.state().state.world;
    assert_eq!(setup_post_options::own(world, Some("Setup2")), Some(PostOptions { explain: true, ..before }));
    assert_eq!(setup_post_options::document(world), before, "the document keeps its options");
    assert_eq!(setup_post_options::of(world, Some("Setup1")), before, "Setup1 keeps its options");
    run(&mut h.state_mut().state, CommandKind::Undo);
    h.run_steps(2);
    assert_explain_checked(&h, false);
    assert_eq!(setup_post_options::own(&h.state().state.world, Some("Setup2")), None);
    run(&mut h.state_mut().state, CommandKind::Redo);
    assert!(setup_post_options::of(&h.state().state.world, Some("Setup2")).explain);
}
