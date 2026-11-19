//! #2358: the setup's Machine page lists the user's own machines - the
//! `.mch` files in the user machines folder - first in its "Library"
//! list, above the shipped ones. Picking one mounts it through the
//! undoable catalogue command, a refused file is listed with its reason
//! and cannot be picked, and "Load other" opens in the user folder.
//! Structural: labels, order, enabled state and commands, no pixels.
//!
//! Provenance: our own tests, specced from issue #2358; no vendor
//! material.
//!
//! Gate map (M4.4): covers ui::egui_app::machine_page, ui::egui_app::machines_pane, ui::state, app::machine::library.

use std::path::{Path, PathBuf};

use egui_kittest::kittest::NodeT;

use super::machines_pane::harness_with_user_dir;
use super::*;
use crate::egui_app::machine_page::library_label;
use crate::state::SetupPage;
use camcraft_app::machine::library::MachineEntry;

type H = Harness<'static, ShellUi>;

/// A valid three-axis mill named `model`.
fn mill(model: &str) -> String {
    format!(
        r#"{{
      "general": {{ "model": "{model}", "kind": "mill" }},
      "kinematics": {{ "default": {{ "units": {{ "length": "mm" }}, "parts": [
        {{ "name": "X", "type": "linear", "travel": {{ "min": 0.0, "max": 300.0 }} }},
        {{ "name": "Y", "type": "linear", "travel": {{ "min": 0.0, "max": 200.0 }} }},
        {{ "name": "Z", "type": "linear", "travel": {{ "min": -100.0, "max": 0.0 }} }}
      ] }} }}
    }}"#
    )
}

/// A mill the validator refuses: X travel runs backwards.
const BACKWARDS: &str = r#"{
  "general": { "model": "BACKWARDS MILL", "kind": "mill" },
  "kinematics": { "default": { "units": { "length": "mm" }, "parts": [
    { "name": "X", "type": "linear", "travel": { "min": 10.0, "max": 5.0 } },
    { "name": "Y", "type": "linear" }, { "name": "Z", "type": "linear" }
  ] } }
}"#;

/// A fresh user machines folder holding `files` (name, text).
fn user_dir(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft_ui_setup_user_machines_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    for (name, text) in files {
        std::fs::write(d.join(name), text).expect("write a machine file");
    }
    d
}

/// A tall shell on `dir`, with Setup1 on the Tormach 770MX and its
/// Machine page open.
fn machine_page_on(dir: &Path) -> H {
    let mut h = harness_with_user_dir(Some(dir.to_path_buf()));
    h.set_size(Vec2::new(1280.0, 800.0));
    let st = &mut h.state_mut().state;
    st.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    st.pending.push_back(CommandKind::LoadBuiltinMachine("Tormach 770MX".into()));
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    h.run_steps(3);
    h
}

fn open_library(h: &mut H) {
    h.get_by_label("Machine library").click();
    h.run_steps(2);
}

fn entry(dir: &Path, file: &str) -> MachineEntry {
    MachineEntry::from_file(&dir.join(file))
}

#[test]
fn the_library_lists_the_user_machines_first() {
    let dir = user_dir("order", &[("my-mill.mch", &mill("MY MILL"))]);
    let mut h = machine_page_on(&dir);
    open_library(&mut h);
    let mine = library_label(&entry(&dir, "my-mill.mch"));
    assert_eq!(mine, "MY MILL (my-mill.mch)", "a user machine names its file");
    let mine_top = h.get_by_label(&mine).rect().top();
    for shipped in ["VMC 3-axis", "Lathe"] {
        let top = h.get_by_label(shipped).rect().top();
        assert!(mine_top < top, "the user machine sits above {shipped}: {mine_top} vs {top}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn picking_a_user_machine_mounts_it_and_undo_restores_the_last() {
    let dir = user_dir("pick", &[("my-mill.mch", &mill("MY MILL"))]);
    let mut h = machine_page_on(&dir);
    let before = camcraft_app::machine::of(&h.state().state.world);
    open_library(&mut h);
    let mine = entry(&dir, "my-mill.mch");
    h.get_by_label(&library_label(&mine)).click();
    h.run_steps(3);
    let st = &h.state().state;
    assert_eq!(camcraft_app::machine::of(&st.world).0, mine.source.mount_path(), "status: {}", st.status);
    // The page names the mounted user machine, not "other".
    assert!(h.query_by_label("other").is_none(), "the combo names the user machine");
    h.state_mut().state.pending.push_back(CommandKind::Undo);
    h.run_steps(3);
    assert_eq!(camcraft_app::machine::of(&h.state().state.world), before, "undo restores the machine");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_refused_user_machine_is_listed_with_its_reason_and_cannot_be_picked() {
    let dir = user_dir("refused", &[("backwards.mch", BACKWARDS)]);
    let mut h = machine_page_on(&dir);
    let before = camcraft_app::machine::of(&h.state().state.world);
    open_library(&mut h);
    let bad = entry(&dir, "backwards.mch");
    let label = library_label(&bad);
    assert!(label.contains("refused: X travel: min 10 must be below max 5"), "{label}");
    let node = h.get_by_label(&label);
    assert!(node.accesskit_node().is_disabled(), "a refused machine cannot be picked");
    node.click();
    h.run_steps(3);
    let st = &h.state().state;
    assert!(st.pending.is_empty(), "nothing queued: {:?}", st.pending);
    assert_eq!(camcraft_app::machine::of(&st.world), before, "the machine did not change");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn load_other_starts_in_the_user_machines_folder_when_it_exists() {
    use crate::egui_app::machines_pane::dialog_start;
    let dir = user_dir("dialog", &[]);
    assert_eq!(dialog_start(Some(&dir)), Some(dir.as_path()), "an existing folder");
    let gone = dir.join("not-there");
    assert_eq!(dialog_start(Some(&gone)), None, "a missing folder: the dialog's own default");
    assert_eq!(dialog_start(None), None, "no user folder");
    let _ = std::fs::remove_dir_all(&dir);
}
