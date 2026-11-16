//! #1450: a model file dropped on the window loads through the Model
//! dialog's own import command (one Ctrl+Z takes it away); a corrupt file,
//! an unsupported one and a multi-file drop are refused by name and leave
//! the project unchanged. The drop is fed as egui's raw dropped-file input.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (#1450): covers ui::egui_app::file_drop, ui::state (refuse_drop, import_path).

use std::path::{Path, PathBuf};

use super::*;

fn cube_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/cube.stl")
}

fn temp_file(tag: &str, name: &str, bytes: &[u8]) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-drop-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    let p = d.join(name);
    std::fs::write(&p, bytes).expect("write the fixture");
    p
}

/// egui's drop handle is a trait the integration owns; the test's own.
#[derive(Debug)]
struct Fake(PathBuf);

impl egui::DroppedFile for Fake {
    fn path(&self) -> &Path {
        &self.0
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        std::fs::read(&self.0).map_err(|e| e.to_string())
    }
}

fn drop_files(h: &mut Harness<'static, ShellUi>, paths: &[&Path]) {
    for p in paths {
        h.input_mut().dropped_files.push(std::sync::Arc::new(Fake(p.to_path_buf())));
    }
    h.run_steps(2);
    h.state_mut().state.settle();
    h.run();
}

fn names(h: &Harness<'static, ShellUi>) -> Vec<String> {
    let mut v: Vec<String> = model::named_entities(&h.state().state.world).into_iter().map(|(n, _)| n).collect();
    v.sort();
    v
}

#[test]
fn a_dropped_mesh_imports_through_the_model_command_and_undoes() {
    let mut h = shell_harness();
    let before = names(&h);
    assert!(!before.iter().any(|n| n == "cube"));
    drop_files(&mut h, &[&cube_path()]);
    assert!(names(&h).iter().any(|n| n == "cube"), "the dropped model is in the document: {}", h.state().state.status);
    let st = &mut h.state_mut().state;
    st.history.undo(&st.world);
    assert_eq!(names(&h), before, "one undo removes the dropped model");
}

#[test]
fn a_corrupt_file_is_refused_by_name_and_changes_nothing() {
    let mut h = shell_harness();
    let before = names(&h);
    let bad = temp_file("corrupt", "broken.step", b"this is not a STEP file");
    drop_files(&mut h, &[&bad]);
    let said = h.state().state.status.clone();
    assert!(said.contains("import refused"), "{said}");
    assert_eq!(names(&h), before);
}

#[test]
fn an_unsupported_extension_is_refused_by_name_and_changes_nothing() {
    let mut h = shell_harness();
    let before = names(&h);
    let txt = temp_file("ext", "notes.txt", b"hello");
    drop_files(&mut h, &[&txt]);
    assert!(h.state().state.status.contains("notes.txt is not a model file"), "{}", h.state().state.status);
    assert_eq!(names(&h), before);
}

#[test]
fn several_files_at_once_are_refused_and_change_nothing() {
    let mut h = shell_harness();
    let before = names(&h);
    let other = temp_file("multi", "other.stl", b"x");
    drop_files(&mut h, &[&cube_path(), &other]);
    assert!(h.state().state.status.contains("2 files were dropped"), "{}", h.state().state.status);
    assert_eq!(names(&h), before);
}
