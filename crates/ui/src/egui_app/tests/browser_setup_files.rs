//! #2950: the browser build loads the user's own machine (`.mch`) and post
//! (`.cps`) files as bytes - from the picker, a folder picker or a drop -
//! through the same machine mount and post choice the desktop runs. A
//! loaded file is a session entry in the machine and post lists; a file
//! that does not parse is refused by name and changes nothing. The drop is
//! fed as egui's raw dropped-file input with a bare name (what a browser
//! hands over); `State::browser_build` stands in for the wasm32 target.
//! The session lists belong to the thread that fills them (each test runs
//! on its own thread); the tests still use file names of their own.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (#2950): covers ui::egui_app::setup_file_drop, ui::state_picked, ui::browser_files, app::session_files.

use std::path::{Path, PathBuf};

use camcraft_app::machine::library;
use camcraft_app::posting;
use camcraft_app::schema::PostRef;
use camcraft_app::session_files;
use egui_kittest::kittest::NodeT;

use super::*;
use crate::file_pick::{PickFor, PickedFile};
use crate::state::SetupPage;

fn asset(rel: &str) -> Vec<u8> {
    std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets").join(rel)).expect("a shipped asset")
}

#[derive(Debug)]
struct BareDrop {
    name: PathBuf,
    data: Result<Vec<u8>, String>,
}

impl egui::DroppedFile for BareDrop {
    fn path(&self) -> &Path {
        &self.name
    }
    fn bytes(&self) -> Result<Vec<u8>, String> {
        self.data.clone()
    }
}

fn drop_bare(h: &mut Harness<'static, ShellUi>, files: Vec<(&str, Vec<u8>)>) {
    for (name, data) in files {
        h.input_mut().dropped_files.push(std::sync::Arc::new(BareDrop { name: PathBuf::from(name), data: Ok(data) }));
    }
    h.run_steps(2);
    h.state_mut().state.settle();
    h.run();
}

fn with_setup() -> Harness<'static, ShellUi> {
    let mut h = shell_harness();
    h.state_mut().state.browser_build = true;
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    h
}

fn machine_path(h: &Harness<'static, ShellUi>) -> String {
    camcraft_app::machine::of(&h.state().state.world).0
}

fn picked(purpose: PickFor, name: &str, bytes: Result<Vec<u8>, String>) -> CommandKind {
    CommandKind::OpenPicked(PickedFile { purpose, name: name.into(), bytes })
}

#[test]
fn a_dropped_mch_mounts_by_its_name_and_one_undo_takes_it_away() {
    let mut h = with_setup();
    let before = machine_path(&h);
    drop_bare(&mut h, vec![("drop-2950-vmc.mch", asset("machines/tormach-770mx.mch"))]);
    let st = &h.state().state;
    assert_eq!(machine_path(&h), "drop-2950-vmc.mch", "status: {}", st.status);
    assert!(camcraft_app::machine::of(&st.world).1.is_some(), "the parsed machine is mounted");
    assert!(session_files::machine_names().contains(&"drop-2950-vmc.mch".to_owned()));
    let listed = library::enumerate(&st.world);
    assert!(
        listed.iter().any(|e| e.source.mount_path() == "drop-2950-vmc.mch" && e.is_valid()),
        "the machine list holds the session entry"
    );
    let st = &mut h.state_mut().state;
    st.history.undo(&st.world);
    assert_eq!(machine_path(&h), before, "one undo takes the mount away");
}

#[test]
fn a_dropped_cps_is_selected_and_posts_from_its_bytes() {
    let mut h = with_setup();
    let text = asset("posts/grbl.cps");
    drop_bare(&mut h, vec![("drop-2950-mine.cps", text.clone())]);
    let st = &h.state().state;
    assert_eq!(posting::post_of(&st.world), PostRef::File("drop-2950-mine.cps".into()), "status: {}", st.status);
    let (label, script) = posting::script_source(&st.world).expect("the session post reads");
    assert_eq!(label, "drop-2950-mine.cps");
    assert_eq!(script.as_bytes(), text.as_slice(), "the post runs the dropped bytes");
    assert!(session_files::post_names().contains(&"drop-2950-mine.cps".to_owned()));
}

#[test]
fn several_dropped_files_land_and_each_bad_one_is_refused_by_name() {
    let mut h = with_setup();
    let machine_before = machine_path(&h);
    let post_before = posting::post_of(&h.state().state.world);
    drop_bare(
        &mut h,
        vec![
            ("multi-2950-good.mch", asset("machines/tormach-770mx.mch")),
            ("multi-2950-good.cps", asset("posts/grbl.cps")),
            ("multi-2950-bad.mch", b"{ not a machine".to_vec()),
            ("multi-2950-bad.cps", b"function onOpen( {".to_vec()),
        ],
    );
    let st = &h.state().state;
    for bad in ["multi-2950-bad.mch", "multi-2950-bad.cps"] {
        assert!(st.status.contains(bad), "{bad} is refused by name: {}", st.status);
    }
    assert!(session_files::machine_names().contains(&"multi-2950-good.mch".to_owned()));
    assert!(session_files::post_names().contains(&"multi-2950-good.cps".to_owned()));
    assert!(!session_files::machine_names().contains(&"multi-2950-bad.mch".to_owned()));
    assert!(!session_files::post_names().contains(&"multi-2950-bad.cps".to_owned()));
    assert_eq!(machine_path(&h), machine_before, "several files only fill the lists");
    assert_eq!(posting::post_of(&st.world), post_before);
}

#[test]
fn a_malformed_pick_is_refused_by_name_and_changes_nothing() {
    let mut h = with_setup();
    let machine_before = machine_path(&h);
    let post_before = posting::post_of(&h.state().state.world);
    let dirty = h.state().state.history.is_dirty();
    let st = &mut h.state_mut().state;
    st.run(picked(PickFor::Machine, "pick-2950-bad.mch", Ok(b"[]".to_vec())));
    assert!(st.status.contains("pick-2950-bad.mch"), "{}", st.status);
    st.run(picked(PickFor::Machine, "pick-2950-wrong.cps", Ok(asset("posts/grbl.cps"))));
    assert!(st.status.contains("pick-2950-wrong.cps is not a machine (.mch) file"), "{}", st.status);
    st.run(picked(PickFor::Post, "pick-2950-bad.cps", Ok(b"throw 1;".to_vec())));
    assert!(st.status.contains("pick-2950-bad.cps"), "{}", st.status);
    st.run(picked(PickFor::Post, "pick-2950-gone.cps", Err("browser read error".into())));
    assert!(st.status.contains("pick-2950-gone.cps") && st.status.contains("browser read error"), "{}", st.status);
    assert_eq!(machine_path(&h), machine_before);
    let st = &h.state().state;
    assert_eq!(posting::post_of(&st.world), post_before);
    assert_eq!(st.history.is_dirty(), dirty, "a refused file changes nothing");
    assert!(!session_files::post_names().contains(&"pick-2950-wrong.cps".to_owned()));
}

#[test]
fn a_picked_folder_fills_both_lists_and_a_picked_file_mounts_or_selects() {
    let mut h = with_setup();
    let st = &mut h.state_mut().state;
    for (name, rel) in [("folder-2950.mch", "machines/tormach-770mx.mch"), ("folder-2950.cps", "posts/grbl.cps")] {
        st.run(picked(PickFor::Folder, name, Ok(asset(rel))));
    }
    assert!(session_files::machine_names().contains(&"folder-2950.mch".to_owned()), "{}", st.status);
    assert!(session_files::post_names().contains(&"folder-2950.cps".to_owned()), "{}", st.status);
    st.run(picked(PickFor::Machine, "pick-2950.mch", Ok(asset("machines/tormach-770mx.mch"))));
    st.run(picked(PickFor::Post, "pick-2950.cps", Ok(asset("posts/grbl.cps"))));
    assert_eq!(machine_path(&h), "pick-2950.mch");
    assert_eq!(posting::post_of(&h.state().state.world), PostRef::File("pick-2950.cps".into()));
}

#[test]
fn the_machine_and_post_load_buttons_are_live_in_the_browser() {
    for action in [CommandKind::LoadMachineDialog, CommandKind::SelectPostDialog, CommandKind::LoadSetupFolderDialog] {
        assert_eq!(crate::browser_files::refusal(&action, false), None, "{action:?} runs the browser's picker");
        assert!(crate::state::opens_a_chooser(&action), "{action:?} runs inside the click (Safari)");
    }
    let mut h = Harness::builder().with_size(Vec2::new(1280.0, 800.0)).build_ui_state(frame, ShellUi::seeded());
    h.state_mut().state.browser_build = true;
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    h.run_steps(3);
    for label in ["Load machine (.mch)", crate::egui_app::machine_page::LOAD_FOLDER] {
        let node = h.get_by_label(label);
        assert_eq!(node.accesskit_node().role(), egui::accesskit::Role::Button, "{label}");
        assert!(!node.accesskit_node().is_disabled(), "{label} is live in the browser");
    }
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Post));
    h.run_steps(3);
    let post = h.get_by_label("Load post (.cps)");
    assert_eq!(post.accesskit_node().role(), egui::accesskit::Role::Button);
    assert!(!post.accesskit_node().is_disabled(), "Load post is live in the browser");
    // The desktop has its folders: no folder button there.
    h.state_mut().state.browser_build = false;
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), SetupPage::Machine));
    h.run_steps(3);
    assert!(h.query_by_label(crate::egui_app::machine_page::LOAD_FOLDER).is_none());
}

#[test]
fn same_name_folder_and_picker_leave_the_selected_post_and_history_stable() {
    let mut h = with_setup();
    let st = &mut h.state_mut().state;
    let before = posting::post_of(&st.world);
    let original = asset("posts/grbl.cps");
    st.run(picked(
        PickFor::Post,
        "stable-3160.cps",
        Ok(original.clone()),
    ));
    let selected = posting::post_of(&st.world);
    let mut changed = original.clone();
    changed.extend_from_slice(b"\n// a different post file\n");
    for purpose in [PickFor::Folder, PickFor::Post] {
        st.run(picked(purpose, "stable-3160.cps", Ok(changed.clone())));
        assert!(
            st.status.contains("stable-3160.cps") && st.status.contains("different post text"),
            "{}",
            st.status
        );
        assert_eq!(posting::post_of(&st.world), selected);
        assert_eq!(
            posting::script_source(&st.world)
                .expect("original selected post")
                .1
                .as_bytes(),
            original
        );
    }
    st.history.undo(&st.world);
    assert_eq!(
        posting::post_of(&st.world),
        before,
        "the refused folder and picker add no history step"
    );
}

#[test]
fn a_folder_machine_import_invalidates_the_populated_catalogue() {
    let mut h = with_setup();
    let st = &mut h.state_mut().state;
    crate::egui_app::machines_pane::refresh(st);
    let before_mount = camcraft_app::machine::of(&st.world).0;
    assert!(
        st.machines_pane
            .list
            .as_ref()
            .expect("populated list")
            .iter()
            .all(|entry| entry.source.mount_path() != "catalogue-3160.mch")
    );
    st.run(picked(
        PickFor::Folder,
        "catalogue-3160.mch",
        Ok(asset("machines/tormach-770mx.mch")),
    ));
    assert_eq!(camcraft_app::machine::of(&st.world).0, before_mount);
    crate::egui_app::machines_pane::refresh(st);
    assert!(
        st.machines_pane
            .list
            .as_ref()
            .expect("refreshed list")
            .iter()
            .any(|entry| entry.source.mount_path() == "catalogue-3160.mch" && entry.is_valid()),
        "{}",
        st.status
    );
}

#[test]
fn an_unloaded_session_post_is_refused_by_name_at_post_time() {
    let post = PostRef::File("unit-3160-never.cps".to_owned());
    assert!(
        posting::script_of(&post)
            .expect_err("not loaded")
            .contains("unit-3160-never.cps")
    );
}
