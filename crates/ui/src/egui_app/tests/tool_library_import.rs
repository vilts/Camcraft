//! #889: the Tool library page's "Import tool holders" toggle. It
//! stands beside Import library as a checkbox, ticked by default; a
//! click flips the choice the import passes to `ImportTools` as its
//! `holders` parameter and writes it to the per-user preferences (never
//! the document or the history). Unticked, a library carrying a holder
//! body imports its tools bare; ticked, the body lands, and one undo
//! removes the import either way.
//! Provenance: our own page law over our own `camcraft-tools` format.
//!
//! Gate map (M4.4): covers ui::state (state_tool_library, state_prefs),
//! ui::egui_app::pages (tool_page).

use super::*;
use camcraft_app::commands::{AddTool, SetToolBody};
use camcraft_app::prefs::{self, FILE_NAME};
use camcraft_app::tool_library;
use egui::accesskit::Toggled;
use egui_kittest::kittest::NodeT;

const LABEL: &str = "Import tool holders";
const TOOL: &str = "Lib6";

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-toollib-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    d
}

fn library_page() -> Harness<'static, ShellUi> {
    let mut harness = shell_harness();
    harness.run();
    crate::actions::target(&mut harness.state_mut().state, UiTarget::Tool(String::new()));
    harness.run();
    harness.run();
    harness
}

/// A library file holding one 6 mm end mill on a BT30 ER20 holder.
fn library_with_a_holder(dir: &std::path::Path) -> std::path::PathBuf {
    let w = camcraft_app::world::new_world();
    let mut h = camcraft_app::commands::History::default();
    let tool = camcraft_app::tools::catalog()
        .into_iter()
        .find(|(n, _)| *n == "EM-0600-4FL")
        .map(|(_, t)| t)
        .expect("catalog tool");
    h.execute(&w, AddTool::new(&w, TOOL, tool).expect("add"));
    h.execute(&w, SetToolBody::library(&w, TOOL, "BT30 ER20", None, None).expect("holder"));
    let path = dir.join("shop.camtools");
    std::fs::write(&path, tool_library::export_tool_library(&w).expect("export")).expect("write");
    path
}

fn has_body(state: &State) -> Option<bool> {
    model::find_by_name(&state.world, TOOL).map(|e| camcraft_app::tool_body::body_of(&state.world, &e).is_some())
}

#[test]
fn the_toggle_stands_on_the_page_ticked_and_is_remembered_per_user() {
    let mut harness = library_page();
    let store = temp_dir("pref").join(FILE_NAME);
    harness.state_mut().state.load_prefs_from(store.clone());
    harness.run();
    assert!(harness.query_by_label("Import library…").is_some(), "the import button stands");
    let dirty = harness.state().state.history.is_dirty();
    let node = harness.get_by_label(LABEL);
    assert_eq!(node.accesskit_node().role(), egui::accesskit::Role::CheckBox);
    assert_eq!(node.accesskit_node().toggled(), Some(Toggled::True), "ticked by default");
    node.click();
    harness.run();
    harness.run();
    assert_eq!(harness.get_by_label(LABEL).accesskit_node().toggled(), Some(Toggled::False));
    let st = &harness.state().state;
    assert!(!st.import_tool_holders);
    assert_eq!(st.history.is_dirty(), dirty, "a preference is not an unsaved change");
    let (saved, _) = prefs::load(&store);
    assert!(saved.skip_tool_holders, "written to the per-user file at the click");
    let mut restarted = State::new();
    restarted.load_prefs_from(store);
    assert!(!restarted.import_tool_holders, "the choice survives a restart");
}

#[test]
fn unticked_imports_bare_ticked_takes_the_holder_and_undo_restores() {
    let dir = temp_dir("import");
    let lib = library_with_a_holder(&dir);
    let mut state = State::new();
    state.import_tool_holders = false;
    state.import_tool_library_from(&lib);
    assert_eq!(has_body(&state), Some(false), "unticked: the tool arrives bare ({})", state.status);
    assert!(state.status.contains("not imported"), "the status names the left-out holder: {}", state.status);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(has_body(&state), None, "one undo removes the import");

    state.import_tool_holders = true;
    state.import_tool_library_from(&lib);
    assert_eq!(has_body(&state), Some(true), "ticked: the holder lands ({})", state.status);
    state.pending.push_back(CommandKind::Undo);
    state.drain_actions();
    assert_eq!(has_body(&state), None, "one undo removes tool and holder");
    let _ = std::fs::remove_dir_all(&dir);
}
