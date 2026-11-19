//! Structural twin of the two Refresh buttons of #2380 (the setup's
//! Machine page and the post picker): each is a named, enabled button,
//! and a press reads the shipped folders again. No position, size or
//! colour is asserted. The post combo finds the document's post by name:
//! a post whose file is gone shows as missing, never as another post.
//!
//! Provenance: our own structural assertion of the folder lists (#2380).

use super::*;
use crate::egui_app::machine_page::REFRESH_MACHINES;
use crate::egui_app::pages::REFRESH_POSTS;
use crate::state::SetupPage;
use egui::accesskit::Role;

/// The shell at the app's default height, with one setup and `page` shown.
fn setup_page(page: SetupPage) -> Harness<'static, ShellUi> {
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 800.0))
        .build_ui_state(frame, ShellUi::seeded());
    h.state_mut().state.pending.push_back(CommandKind::CreateSetup { name: None, datum_from: None });
    h.run_steps(2);
    crate::actions::target(&mut h.state_mut().state, UiTarget::SetupNode("Setup1".into(), page));
    h.run_steps(3);
    h
}

/// The button named `name` is an enabled button, and a press reads the
/// folders again (the process read count goes up).
fn assert_refresh(h: &mut Harness<'static, ShellUi>, name: &str) {
    let button = h.get_by_role_and_label(Role::Button, name);
    assert!(!button.accesskit_node().is_disabled(), "{name:?} is enabled");
    // Read once, so the count below cannot rise from a first use alone.
    let _ = camcraft_app::posting::machine_scan();
    let before = camcraft_app::posting::scan_reads();
    button.click();
    h.run();
    assert!(camcraft_app::posting::scan_reads() > before, "a press of {name:?} reads the folders again");
}

#[test]
fn the_machine_page_refresh_button_rereads_the_folder() {
    let _guard = snapshot_guard();
    let mut h = setup_page(SetupPage::Machine);
    assert_refresh(&mut h, REFRESH_MACHINES);
}

#[test]
fn the_post_picker_refresh_button_rereads_the_folder() {
    let _guard = snapshot_guard();
    let mut h = setup_page(SetupPage::Post);
    assert_refresh(&mut h, REFRESH_POSTS);
}

/// Sets this thread's post folder scan for the life of the guard.
struct ScanFor;

impl ScanFor {
    fn set(scan: camcraft_app::posting::FolderScan) -> Self {
        crate::egui_app::post_pick::SCAN_FOR_TEST.with(|s| *s.borrow_mut() = Some(scan));
        Self
    }
}

impl Drop for ScanFor {
    fn drop(&mut self) {
        crate::egui_app::post_pick::SCAN_FOR_TEST.with(|s| *s.borrow_mut() = None);
    }
}

/// The selected text of each combo box on the page.
fn combo_texts(h: &Harness<'static, ShellUi>) -> Vec<String> {
    h.query_all_by(|n| n.role() == Role::ComboBox).filter_map(|n| n.value()).collect()
}

/// The name of the default document's post (`PostRef::Builtin`).
fn default_post(h: &Harness<'static, ShellUi>) -> String {
    let current = camcraft_app::posting::post_of(&h.state().state.world);
    assert_eq!(current, camcraft_app::schema::PostRef::Builtin, "a new mill document posts with the default post");
    camcraft_app::posting::post_name(&current).map(str::to_owned).unwrap_or_default()
}

#[test]
fn an_empty_post_folder_shows_the_default_post_as_missing() {
    let _guard = snapshot_guard();
    let why = "no such folder";
    let _scan = ScanFor::set(camcraft_app::posting::FolderScan { problem: Some(why.into()), ..Default::default() });
    let h = setup_page(SetupPage::Post);
    let name = default_post(&h);
    let want = format!("{name} {}", crate::egui_app::post_pick::MISSING);
    let texts = combo_texts(&h);
    assert!(texts.contains(&want), "the post combo shows {want:?}, got {texts:?}");
    let line = crate::egui_app::post_pick::folder_problem(why);
    assert!(h.query_by_label(&line).is_some(), "the page shows why the folder did not read");
}

#[test]
fn a_removed_first_post_is_not_shown_as_another_post() {
    let _guard = snapshot_guard();
    let rest = camcraft_app::posting::BUILTINS.iter().skip(1).map(|(n, _)| *n).collect();
    let _scan = ScanFor::set(camcraft_app::posting::FolderScan { present: rest, ..Default::default() });
    let h = setup_page(SetupPage::Post);
    let name = default_post(&h);
    let (label, _) = camcraft_app::posting::script_of(&camcraft_app::schema::PostRef::Builtin).unwrap_or_default();
    assert!(label.starts_with(&name), "script_of uses {name:?}, got {label:?}");
    let want = format!("{name} {}", crate::egui_app::post_pick::MISSING);
    let texts = combo_texts(&h);
    assert!(texts.contains(&want), "the post combo shows the post that script_of uses ({want:?}), got {texts:?}");
}
