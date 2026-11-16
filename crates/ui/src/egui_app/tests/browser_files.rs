//! #1431 in the shell: the browser build's file commands. Save and Save
//! As hand the project text to a download and mark the document clean;
//! Post hands over the same gated program (a refused post offers
//! nothing); the commands that need a picker or a folder are darkened on
//! the ribbon with the reason as their tip and refused by name on the
//! status line, never a silent no-op. `State::browser_build` stands in for
//! the wasm32 target; the native half of `browser_files` records offers.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::browser_files, ui::state (state_browser), ui::unsaved, ui::egui_app::panes.

use egui_kittest::kittest::NodeT;

use super::*;
use crate::browser_files::{self, NEEDS_PICKER, NO_FOLDER};

fn browser_harness() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.state_mut().state.browser_build = true;
    let _ = browser_files::take_offered();
    h
}

/// The seed's contour as a 0.5 mm top skim, regenerated: a program the
/// gate posts (the seed as shipped is refused, `post_refusal.rs`).
fn make_the_seed_postable(h: &mut Harness<'static, ShellUi>) {
    let mut skim = {
        let st = &h.state().state;
        let e = model::find_by_name(&st.world, "Contour1").expect("the seed's op");
        model::operation_of(&st.world, &e).expect("op").params.clone()
    };
    if let OpParams::Contour { bottom_z, .. } = &mut skim {
        *bottom_z = Height::fixed(29.5);
    }
    let pending = &mut h.state_mut().state.pending;
    pending.push_back(CommandKind::EditOperationParams { name: "Contour1".into(), params: skim });
    pending.push_back(CommandKind::GenerateOperation("Contour1".into()));
    h.run();
}

#[test]
fn the_ribbon_keeps_open_live_in_the_browser() {
    let mut h = browser_harness();
    h.run();
    let st = &h.state().state;
    // #31: Open model and Open project run the browser's async picker.
    for c in [Cmd::Model, Cmd::Project] {
        assert!(panes::enabled(c, st), "{c:?} runs the picker in the browser");
        assert!(panes::browser_refusal(c, st).is_none(), "{c:?}");
    }
    for c in [Cmd::Save, Cmd::SaveAs, Cmd::Post, Cmd::New] {
        assert!(panes::enabled(c, st), "{c:?} runs in the browser");
    }
    let mut desk = State::new();
    desk.browser_build = false;
    for c in [Cmd::Model, Cmd::Project, Cmd::Post] {
        assert!(panes::enabled(c, &desk), "{c:?} is live on the desktop");
        assert!(panes::browser_refusal(c, &desk).is_none());
    }
}

#[test]
fn a_picker_the_browser_lacks_is_refused_by_name_not_ignored() {
    // Open model / Open project are not here: they run the browser's
    // picker (#31; natively they would open rfd's dialog in the test).
    let mut h = browser_harness();
    for action in [CommandKind::LoadMachineDialog, CommandKind::SelectPostDialog] {
        h.state_mut().state.status.clear();
        h.state_mut().state.pending.push_back(action);
        h.run();
        let st = &h.state().state;
        assert_eq!(st.status, NEEDS_PICKER);
        assert!(st.unsaved_prompt.is_none(), "a refusal never asks about unsaved changes");
    }
    // The direct calls (tool library, tool body) refuse the same way.
    h.state_mut().state.import_tool_library_dialog();
    assert_eq!(h.state().state.status, NEEDS_PICKER);
    assert!(browser_files::take_offered().is_empty(), "nothing was downloaded");
}

#[test]
fn save_downloads_the_project_text_and_marks_the_document_clean() {
    let mut h = browser_harness();
    make_the_seed_postable(&mut h);
    assert!(h.state().state.history.is_dirty(), "the edit dirtied the document");
    h.state_mut().state.pending.push_back(CommandKind::SaveProjectDialog);
    h.run();
    let st = &h.state().state;
    let want = camcraft_app::project::save_text(&st.world).expect("project text");
    let offered = browser_files::take_offered();
    assert_eq!(offered.len(), 1, "one download (status: {})", st.status);
    let (name, bytes, mime) = &offered[0];
    assert_eq!(name, "untitled.camproj");
    assert_eq!(mime, "application/json");
    assert_eq!(String::from_utf8_lossy(bytes), want, "the download is what Save writes on the desktop");
    assert!(!st.history.is_dirty(), "the downloaded document is clean");
    assert_eq!(st.status, "saved untitled.camproj (downloaded)");
    assert_eq!(st.window_title(), "CamCraft - untitled.camproj");

    // Save As asks for the name (the native stand-in answers the default).
    h.state_mut().state.pending.push_back(CommandKind::SaveProjectAsDialog);
    h.run();
    let again = browser_files::take_offered();
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].0, "untitled.camproj");
}

#[test]
fn post_downloads_the_gated_program_and_a_refusal_offers_nothing() {
    let mut h = browser_harness();
    // The seed as shipped is refused by the gate: no download, the
    // refusal window holds the reason.
    h.state_mut().state.pending.push_back(CommandKind::PostProgramDialog);
    h.run();
    {
        let st = &h.state().state;
        assert!(st.status.starts_with("post refused: "), "status: {}", st.status);
        assert!(st.post_refusal.is_some(), "the reason gets its window");
        assert!(browser_files::take_offered().is_empty(), "a refused post offers nothing");
    }
    make_the_seed_postable(&mut h);
    h.state_mut().state.pending.push_back(CommandKind::PostProgramDialog);
    h.run();
    let st = &h.state().state;
    let offered = browser_files::take_offered();
    assert_eq!(offered.len(), 1, "one program (status: {})", st.status);
    let (name, bytes, mime) = &offered[0];
    assert!(name.ends_with(".nc"), "{name}");
    assert_eq!(mime, "text/plain");
    let view = st.post_view.as_ref().expect("the viewer shows the program");
    assert_eq!(String::from_utf8_lossy(bytes), view.program, "the viewer shows what was downloaded");
    assert!(st.status.contains("(downloaded)"), "status: {}", st.status);
    assert!(st.post_refusal.is_none(), "a program went out: the old refusal retires");
}

#[test]
fn a_per_op_post_is_refused_in_the_browser_and_the_cell_says_why() {
    let mut h = browser_harness();
    h.state_mut().state.pending.push_back(CommandKind::SetPerOpOutput(true));
    h.run();
    assert!(!panes::enabled(Cmd::Post, &h.state().state));
    assert_eq!(panes::tip_of(Cmd::Post, &h.state().state), NO_FOLDER);
    h.state_mut().state.pending.push_back(CommandKind::PostProgramDialog);
    h.run();
    assert_eq!(h.state().state.status, NO_FOLDER);
    assert!(browser_files::take_offered().is_empty());
}

#[test]
fn a_file_button_is_greyed_with_its_reason_in_the_browser() {
    let mut h = Harness::new_ui(|ui| {
        let _ = browser_files::button(ui, Some(NEEDS_PICKER), "Load machine (.mch)", "");
        let _ = browser_files::button(ui, None, "Built-in", "");
    });
    h.run();
    assert!(h.get_by_label("Load machine (.mch)").accesskit_node().is_disabled(), "greyed in the browser");
    assert!(!h.get_by_label("Built-in").accesskit_node().is_disabled(), "a live button stays live");
}
