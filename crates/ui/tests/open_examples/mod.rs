//! Installed example menu tests for issue #2043.
//! Provenance: our own UI contract tests.
//! Gate map: covers ui::egui_app::recent_drop, ui::unsaved.

use super::*;
use egui_kittest::{Harness, kittest::Queryable};
use std::path::PathBuf;
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};

// An override keeps fixtures local to their test thread during parallel UI suites.
thread_local! {
    static DIRECTORY: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}
static NEXT: AtomicUsize = AtomicUsize::new(0);
pub(super) fn directory_override() -> Option<PathBuf> {
    DIRECTORY.with(|directory| directory.borrow().clone())
}
struct Package(PathBuf);
impl Package {
    fn new() -> Self {
        let package = Self::absent();
        std::fs::create_dir(&package.0).expect("create owned example folder");
        package
    }
    fn absent() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "camcraft-open-examples-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        assert!(!directory.exists(), "fixture path starts absent");
        DIRECTORY.with(|slot| *slot.borrow_mut() = Some(directory.clone()));
        Self(directory)
    }
    fn project(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        let mut state = State::new();
        state.save_project_path(path.clone());
        assert!(path.is_file(), "fixture saved: {}", state.status);
        path
    }
}
impl Drop for Package {
    fn drop(&mut self) {
        DIRECTORY.with(|slot| *slot.borrow_mut() = None);
        if self.0.exists() {
            std::fs::remove_dir_all(&self.0).expect("remove owned example folder");
        }
    }
}
fn menu(mut state: State, kind: RecentKind) -> Harness<'static, State> {
    press(&mut state, kind);
    let mut harness = Harness::builder().with_size(Vec2::new(800.0, 600.0)).build_ui_state(
        |ui, state| {
            let button = Rect::from_min_size(egui::pos2(20.0, 20.0), Vec2::new(100.0, 30.0));
            let cells = [Cell { cmd: Cmd::Project, rect: button, salt: 0 },
                         Cell { cmd: Cmd::Model, rect: button, salt: 1 }];
            draw(ui, state, &Tokens::DARK, egui::Id::new("example-menu"), &cells, ui.max_rect());
        }, state);
    harness.run();
    harness
}

#[test]
fn open_example_entry_is_available_without_recent_projects() {
    let package = Package::new();
    package.project("2043-fixture.camproj");
    let harness = menu(State::new(), RecentKind::Project);
    assert!(harness.query_by_label("Open example").is_some(),
        "File Open must offer Open example without a recent project");
    assert!(harness.state().pending.is_empty(), "opening the menu must not queue a file dialog");
}

#[test]
fn open_example_lists_only_direct_projects_in_name_order() {
    let package = Package::new();
    package.project("z-project.camproj");
    let a = package.project("a-project.camproj");
    std::fs::write(package.0.join("notes.txt"), "not a project").expect("text fixture");
    let nested = package.0.join("folder.camproj");
    std::fs::create_dir(&nested).expect("nested folder");
    std::fs::write(nested.join("nested.camproj"), "nested file").expect("nested fixture");
    let mut harness = menu(State::new(), RecentKind::Project);
    harness.get_by_label("Open example").click();
    harness.run();
    assert!(harness.get_by_label("a-project.camproj").rect().top()
        < harness.get_by_label("z-project.camproj").rect().top(), "sort project names");
    for excluded in ["notes.txt", "folder.camproj", "nested.camproj"] {
        assert!(harness.query_by_label(excluded).is_none(), "exclude {excluded}");
    }
    harness.get_by_label("a-project.camproj").click();
    harness.run();
    assert!(matches!(harness.state().pending.front(),
        Some(CommandKind::OpenRecent(RecentKind::Project, path)) if path == &a),
        "selection uses the existing project-open command");
    harness.state_mut().drain_actions();
    assert_eq!(harness.state().project_path.as_ref(), Some(&a), "the existing loader opens the selected project");
}

#[test]
fn open_example_selection_keeps_the_unsaved_project_guard() {
    let package = Package::new();
    let project = package.project("guarded.camproj");
    let mut state = State::new();
    state.run(CommandKind::CreateSetup { name: Some("Unsaved".into()), datum_from: None });
    assert!(state.history.is_dirty(), "fixture contains unsaved work");
    let mut harness = menu(state, RecentKind::Project);
    harness.get_by_label("Open example").click();
    harness.run();
    harness.get_by_label("guarded.camproj").click();
    harness.run();
    harness.state_mut().drain_actions();
    assert!(matches!(&harness.state().unsaved_prompt,
        Some(CommandKind::OpenRecent(RecentKind::Project, path)) if path == &project),
        "example selection must ask before replacing unsaved work");
    assert!(harness.state().history.is_dirty());
    assert!(harness.state().project_path.is_none());
}

#[test]
fn open_example_is_hidden_without_the_installed_directory() {
    let _package = Package::absent();
    let mut state = State::new();
    state.recent.add(RecentKind::Project, &PathBuf::from("missing.camproj"));
    let harness = menu(state, RecentKind::Project);
    harness.get_by_label(BROWSE);
    assert!(harness.query_by_label("Open example").is_none());
}

#[test]
fn open_example_is_hidden_in_browser_and_model_menus() {
    let package = Package::new();
    package.project("available.camproj");
    let mut browser = State::new();
    browser.browser_build = true;
    browser.recent.add(RecentKind::Project, &PathBuf::from("missing.camproj"));
    let harness = menu(browser, RecentKind::Project);
    assert!(harness.query_by_label("Open example").is_none());
    let mut models = State::new();
    models.recent.add(RecentKind::Model, &PathBuf::from("missing.stl"));
    let harness = menu(models, RecentKind::Model);
    harness.get_by_label(BROWSE);
    assert!(harness.query_by_label("Open example").is_none());
}

#[test]
fn open_example_empty_directory_has_a_clear_message() {
    let _package = Package::new();
    let mut harness = menu(State::new(), RecentKind::Project);
    harness.get_by_label("Open example").click();
    harness.run();
    harness.get_by_label("No example projects");
    assert!(harness.state().pending.is_empty());
}
