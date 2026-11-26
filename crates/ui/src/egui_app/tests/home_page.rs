//! #2384: the Home button and the recent projects start page. The button
//! is in the top band, left of Donate; a click shows the page in place of
//! the dock. The page lists the recent projects newest first, a gone file
//! greyed as missing; a click on a project opens it through Open's own
//! command (the unsaved-changes prompt included), and the page closes when
//! the document changes. A cancelled prompt keeps the page. The page
//! shows at a start with no file named.
//! Provenance: our own tests over our own shell; no HSMWorks material.
//!
//! Gate map (M4.4): covers ui::egui_app::home, ui::egui_app::donate, ui::egui_app::shell.

use std::path::PathBuf;

use egui::accesskit::Role;
use egui_kittest::kittest::NodeT;

use camcraft_app::recent::RecentKind;

use super::super::home::{self, BACK, HOME, NEW, NO_RECENT, TITLE};
use super::*;
use crate::unsaved::UnsavedChoice;

/// A fresh folder for one test, canonical (macOS temp is a symlink).
fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-home-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("make the test folder");
    std::fs::canonicalize(&d).expect("canonical folder")
}

/// A saved copy of the seeded document at `path`, from another State.
fn saved_project(path: &std::path::Path) {
    let mut other = State::new();
    world::seed(&mut other);
    other.save_project_path(path.to_path_buf());
    assert!(path.is_file(), "fixture project saved: {}", other.status);
}

/// The shell with `a.camproj`, then `gone.camproj` (no file) on the list.
fn with_recent(tag: &str) -> (Harness<'static, ShellUi>, PathBuf) {
    let d = temp_dir(tag);
    let a = d.join("a.camproj");
    saved_project(&a);
    let mut h = shell_harness();
    {
        let st = &mut h.state_mut().state;
        st.recent.add(RecentKind::Project, &a);
        st.recent.add(RecentKind::Project, &d.join("gone.camproj"));
    }
    h.run();
    (h, a)
}

fn press_home(h: &mut Harness<'static, ShellUi>) {
    h.get_by_role_and_label(Role::Button, HOME).click();
    h.run_steps(2);
}

fn page_up(h: &Harness<'static, ShellUi>) -> bool {
    h.query_by_label(TITLE).is_some()
}

#[test]
fn the_home_button_is_in_the_top_band_left_of_donate() {
    let mut h = shell_harness();
    h.run();
    let home = h.get_by_role_and_label(Role::Button, HOME).rect();
    let donate = h.get_by_role_and_label(Role::Button, "Donate").rect();
    assert!(home.right() <= donate.left(), "Home {home:?} is left of Donate {donate:?}");
    assert!((home.center().y - donate.center().y).abs() < 1.0, "one band: {home:?} {donate:?}");
    let ver = crate::build_info::toolbar_version();
    let painted = super::structural::painted(&h);
    let at = painted.iter().find(|(t, _)| t.starts_with(&ver)).map(|(_, p)| *p);
    let at = at.unwrap_or_else(|| panic!("the top bar paints the version {ver:?}"));
    assert!(at.x > h.ctx.content_rect().center().x, "the version stays at the right of the top bar: {at:?}");
    assert!(!page_up(&h), "a fresh shell shows the document, not the page");
}

#[test]
fn home_shows_the_recent_projects_in_place_of_the_dock_and_back_returns() {
    let (mut h, _) = with_recent("show");
    assert!(h.query_by_role_and_label(Role::Pane, "Properties").is_some(), "the dock is up");
    press_home(&mut h);
    assert!(page_up(&h), "the page is up");
    assert!(h.query_by_role_and_label(Role::Pane, "Properties").is_none(), "the page stands in for the dock");
    let rows: Vec<String> = ["gone.camproj (missing)", "a.camproj"]
        .iter()
        .filter(|l| h.query_by_label(l).is_some())
        .map(|l| (*l).to_owned())
        .collect();
    assert_eq!(rows, ["gone.camproj (missing)", "a.camproj"], "both entries, the gone file named missing");
    let gone = h.get_by_label("gone.camproj (missing)");
    assert!(gone.accesskit_node().is_disabled(), "a gone file is greyed and takes no click");
    let a = h.get_by_label("a.camproj");
    assert!(!a.accesskit_node().is_disabled());
    assert!(h.get_by_label("gone.camproj (missing)").rect().top() < a.rect().top(), "newest first");
    h.get_by_role_and_label(Role::Button, BACK).click();
    h.run_steps(2);
    assert!(!page_up(&h), "Back closes the page");
    assert!(h.query_by_role_and_label(Role::Pane, "Properties").is_some(), "the dock is back");
    press_home(&mut h);
    press_home(&mut h);
    assert!(!page_up(&h), "a second press on Home closes the page too");
}

#[test]
fn a_project_click_opens_it_through_open_and_closes_the_page() {
    let (mut h, a) = with_recent("open");
    // A clean document: the open needs no prompt. The seed is unsaved
    // work, so New asks first.
    h.state_mut().state.pending.push_back(CommandKind::NewDocument);
    h.run_steps(2);
    crate::actions::resolve_unsaved(&mut h.state_mut().state, UnsavedChoice::DontSave);
    h.run_steps(2);
    assert!(!h.state().state.history.is_dirty(), "a clean new document");
    press_home(&mut h);
    h.get_by_label("a.camproj").click();
    h.run_steps(3);
    let st = &h.state().state;
    assert_eq!(st.project_path.as_deref(), Some(a.as_path()), "status: {}", st.status);
    assert!(model::find_by_name(&st.world, "Block").is_some(), "the saved document came back");
    assert!(!page_up(&h), "the changed document closes the page");
}

#[test]
fn a_cancelled_unsaved_prompt_keeps_the_page_and_the_document() {
    let (mut h, a) = with_recent("cancel");
    assert!(h.state().state.history.is_dirty(), "the seed is unsaved work");
    press_home(&mut h);
    h.get_by_label("a.camproj").click();
    h.run_steps(2);
    assert!(h.state().state.unsaved_prompt.is_some(), "the open is held for Open's prompt");
    crate::actions::resolve_unsaved(&mut h.state_mut().state, UnsavedChoice::Cancel);
    h.run_steps(2);
    assert_ne!(h.state().state.project_path.as_deref(), Some(a.as_path()), "nothing opened");
    assert!(page_up(&h), "the page stays while the document is the same");
}

#[test]
fn an_empty_list_says_so_and_new_project_closes_the_page() {
    let mut h = shell_harness();
    h.state_mut().state.recent = Default::default();
    h.run();
    press_home(&mut h);
    assert!(h.query_by_label(NO_RECENT).is_some(), "an empty list says so");
    h.get_by_role_and_label(Role::Button, NEW).click();
    h.run_steps(3);
    if h.state().state.unsaved_prompt.is_some() {
        crate::actions::resolve_unsaved(&mut h.state_mut().state, UnsavedChoice::DontSave);
        h.run_steps(3);
    }
    assert!(!page_up(&h), "New project changes the document, so the page closes");
}

#[test]
fn the_page_shows_at_a_start_with_no_file_only() {
    let mut st = State::new();
    home::at_start(&mut st, &["part.stl".to_owned()]);
    assert_eq!(st.home_page, None, "a named file opens over no page");
    home::at_start(&mut st, &[]);
    assert!(home::is_open(&mut st), "no file: the page is up");
    let mut h = shell_harness();
    home::at_start(&mut h.state_mut().state, &[]);
    h.run_steps(4);
    assert!(page_up(&h), "still frames keep the start page up");
}

/// The list keeps canonical paths; on Windows they carry the verbatim
/// prefix, which the page does not show.
#[test]
fn the_page_shows_a_stored_path_without_the_verbatim_prefix() {
    use std::path::Path;
    assert_eq!(home::shown_path(Path::new(r"\\?\C:\jobs\a.camproj")), r"C:\jobs\a.camproj");
    assert_eq!(home::shown_path(Path::new(r"\\?\UNC\shop\jobs\a.camproj")), r"\\shop\jobs\a.camproj");
    assert_eq!(home::shown_path(Path::new("/home/me/a.camproj")), "/home/me/a.camproj");
}

