//! #2428 Part B: startup update check UI, top-band `"Settings"` dropdown,
//! and `"Update available: vX.Y.Z"` title-bar notice.
//!
//! Provenance: our own structural and behaviour tests over `ShellUi` and
//! temp `prefs.json` stores; never contacts GitHub or touches user files.

use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use camcraft_app::prefs::{self, FILE_NAME};
use camcraft_app::update_check::ReleaseInfo;
use egui::accesskit::{Role, Toggled};
use egui_kittest::kittest::NodeT;

use super::super::home::HOME;
use super::super::settings_drop::{CHECK_FOR_UPDATES, SETTINGS};
use super::super::update_notice;
use super::*;
use crate::update_check::CheckOutcome;

fn temp_store(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-update-ui-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create test prefs folder");
    dir.join(FILE_NAME)
}

fn sample_release(version: &str) -> ReleaseInfo {
    ReleaseInfo {
        version: version.to_owned(),
        release_notes_url: format!("https://github.com/NeoTech/Camcraft/releases/tag/v{version}"),
    }
}

#[test]
fn title_bar_hides_update_notice_by_default_and_when_up_to_date() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    h.run_steps(2);
    assert!(
        h.query_by_label_contains("Update available").is_none(),
        "default state shows no update notice"
    );
    assert!(
        !super::structural::painted(&h)
            .iter()
            .any(|(t, _)| t.contains("Update available"))
    );

    h.state_mut().state.update_available = Some(sample_release(crate::build_info::VERSION));
    h.run_steps(2);
    assert!(
        h.query_by_label_contains("Update available").is_none(),
        "same version as current build stays hidden"
    );

    h.state_mut().state.update_available = Some(sample_release("0.24.0"));
    h.run_steps(2);
    assert!(
        h.query_by_label_contains("Update available").is_none(),
        "older version stays hidden"
    );

    let delay = crate::egui_app::view_pass::state_redraw_delay(
        &h.state().state,
        Duration::MAX,
        false,
    );
    assert_eq!(delay, None, "idle update state requests no background redraws");
}

#[test]
fn title_bar_shows_update_notice_when_newer_release_is_injected_and_opens_notes_on_click() {
    let _g = snapshot_guard();
    let mut h = shell_harness();
    let info = sample_release("99.0.0");
    h.state_mut().state.browser_build = true;
    h.state_mut().state.update_available = Some(info.clone());
    h.run_steps(3);

    let label = "Update available: v99.0.0";
    let notice = h.get_by_role_and_label(Role::Button, label);
    let notice_rect = notice.rect();
    let desc = notice
        .accesskit_node()
        .description()
        .expect("update notice exposes tooltip description")
        .to_owned();
    assert!(desc.contains("v99.0.0"), "{desc}");
    assert!(desc.contains(&info.release_notes_url), "{desc}");
    assert_eq!(desc, update_notice::tooltip_text(&info));

    let settings = h.get_by_role_and_label(Role::Button, SETTINGS).rect();
    let home = h.get_by_role_and_label(Role::Button, HOME).rect();
    let donate = h.get_by_role_and_label(Role::Button, "Donate").rect();
    assert!(
        notice_rect.top() >= 0.0 && notice_rect.bottom() <= 30.0,
        "notice sits in top band: {notice_rect:?}"
    );
    assert!(
        notice_rect.left() > h.ctx.content_rect().center().x,
        "notice sits on right side of top bar: {notice_rect:?}"
    );
    assert!(
        notice_rect.right() <= settings.left()
            && settings.right() <= home.left()
            && home.right() <= donate.left(),
        "top-right order is notice <= Settings <= Home <= Donate: {notice_rect:?} {settings:?} {home:?} {donate:?}"
    );
    assert!((notice_rect.center().y - donate.center().y).abs() < 1.0);

    let painted = super::structural::painted(&h);
    assert!(
        painted.iter().any(|(t, _)| t == label),
        "painted ribbon shows {label:?}"
    );

    let before_ids = h.state().state.history.done_entry_ids();
    let before_ver = h.state().state.doc_version;
    let before_dirty = h.state().state.history.is_dirty();
    h.get_by_role_and_label(Role::Button, label).click();
    h.step();
    let urls: Vec<_> = h
        .output()
        .platform_output
        .commands
        .iter()
        .filter_map(|cmd| match cmd {
            egui::OutputCommand::OpenUrl(open) => Some(open),
            _ => None,
        })
        .collect();
    assert_eq!(urls.len(), 1, "one click opens release notes URL");
    assert_eq!(urls[0].url, info.release_notes_url);
    assert!(urls[0].new_tab);
    h.run();
    assert_eq!(h.state().state.history.done_entry_ids(), before_ids);
    assert_eq!(h.state().state.doc_version, before_ver);
    assert_eq!(h.state().state.history.is_dirty(), before_dirty);

    let delay = crate::egui_app::view_pass::state_redraw_delay(
        &h.state().state,
        Duration::MAX,
        false,
    );
    assert_eq!(delay, None, "visible update notice still sleeps when idle");
}

#[test]
fn settings_toggle_persists_check_for_updates_and_hides_notice_without_dirtying_document() {
    let _g = snapshot_guard();
    let store = temp_store("toggle");
    let mut h = Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank());
    h.state_mut().state.load_prefs_from(store.clone());
    h.state_mut().state.update_available = Some(sample_release("99.1.0"));
    h.run_steps(2);

    assert!(!h.state().state.history.is_dirty());
    assert!(h.query_by_role_and_label(Role::Button, "Update available: v99.1.0").is_some());
    let before_ids = h.state().state.history.done_entry_ids();
    let before_ver = h.state().state.doc_version;

    h.get_by_role_and_label(Role::Button, SETTINGS).click();
    h.run_steps(2);
    let item = h.get_by_role_and_label(Role::MenuItem, CHECK_FOR_UPDATES);
    assert_eq!(item.accesskit_node().toggled(), Some(Toggled::True));
    item.click();
    h.run_steps(2);

    assert!(!h.state().state.check_for_updates);
    assert!(!h.state().state.ribbon_settings_open);
    assert!(
        h.query_by_role_and_label(Role::Button, "Update available: v99.1.0").is_none(),
        "turning off update checks hides the title-bar notice"
    );
    assert_eq!(h.state().state.history.done_entry_ids(), before_ids);
    assert_eq!(h.state().state.doc_version, before_ver);
    assert!(!h.state().state.history.is_dirty());

    let (saved, note) = prefs::load(&store);
    assert_eq!(note, None);
    assert!(!saved.check_for_updates);

    let mut restarted = State::new();
    restarted.load_prefs_from(store.clone());
    assert!(!restarted.check_for_updates, "disabled setting survives restart");

    h.get_by_role_and_label(Role::Button, SETTINGS).click();
    h.run_steps(2);
    let item = h.get_by_role_and_label(Role::MenuItem, CHECK_FOR_UPDATES);
    assert_eq!(item.accesskit_node().toggled(), Some(Toggled::False));
    item.click();
    h.run_steps(2);
    assert!(h.state().state.check_for_updates);
    assert!(prefs::load(&store).0.check_for_updates);
}

#[test]
fn background_outcome_updates_timestamp_silently_and_shows_available_update() {
    let _g = snapshot_guard();
    let store = temp_store("rx");
    let mut h = shell_harness();
    h.state_mut().state.load_prefs_from(store.clone());
    let initial_status = h.state().state.status.clone();

    let (tx, rx) = mpsc::channel();
    h.state_mut().state.install_update_rx_for_test(rx);
    tx.send(CheckOutcome {
        checked_at_utc: Some(1_795_348_555),
        update: None,
    })
    .expect("send silent outcome");
    h.run_steps(2);
    assert_eq!(h.state().state.status, initial_status, "silent on no-update/error");
    assert_eq!(h.state().state.last_update_check_utc, Some(1_795_348_555));
    assert_eq!(prefs::load(&store).0.last_update_check_utc, Some(1_795_348_555));
    assert!(h.query_by_label_contains("Update available").is_none());

    let (tx, rx) = mpsc::channel();
    h.state_mut().state.install_update_rx_for_test(rx);
    tx.send(CheckOutcome {
        checked_at_utc: Some(1_795_435_000),
        update: Some(sample_release("99.2.0")),
    })
    .expect("send upgrade outcome");
    h.run_steps(2);
    assert_eq!(h.state().state.last_update_check_utc, Some(1_795_435_000));
    assert_eq!(prefs::load(&store).0.last_update_check_utc, Some(1_795_435_000));
    assert!(
        h.query_by_role_and_label(Role::Button, "Update available: v99.2.0")
            .is_some()
    );
}
