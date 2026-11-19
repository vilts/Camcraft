//! Issue #2385: the Donate door remains available in both shells.
//! Provenance: our own behaviour tests over the real shell harness.

use super::*;

use crate::egui_app::donate;

#[test]
fn donate_link_matches_the_published_project_funding_link() {
    assert!(
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../README.md"))
            .contains(donate::DONATION_URL)
    );
    assert!(
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../.github/FUNDING.yml"
        ))
        .contains(donate::DONATION_URL)
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn donate_browser_failure_is_named_and_recovery_does_not_edit_the_document() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.run_steps(3);
    let before = harness.state().state.history.done_entry_ids();
    let version = harness.state().state.doc_version;
    donate::record_result(&harness.ctx, Err("test launcher failure".into()));
    harness.run();
    let status = &harness.state().state.status;
    assert!(status.starts_with("Donate:"));
    assert!(status.contains("browser") && status.contains("test launcher failure"));
    assert!(
        status.contains(donate::DONATION_URL),
        "the message gives the link for manual use"
    );
    donate::record_result(&harness.ctx, Ok(()));
    harness.run();
    assert!(
        !harness
            .state()
            .state
            .status
            .contains("test launcher failure"),
        "a later success clears the old failure"
    );
    assert_eq!(harness.state().state.history.done_entry_ids(), before);
    assert_eq!(harness.state().state.doc_version, version);
    assert!(harness.state().state.pending.is_empty());
}

#[test]
fn donate_button_is_named_and_enabled_in_both_shells() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    for browser in [false, true] {
        harness.state_mut().state.browser_build = browser;
        harness.run_steps(3);
        let button = harness.get_by_label("Donate");
        assert_eq!(
            button.accesskit_node().role(),
            egui::accesskit::Role::Button
        );
        assert!(
            !button.accesskit_node().is_disabled(),
            "Donate stays enabled in either shell"
        );
    }
}

#[test]
fn donate_in_browser_opens_stripe_in_a_new_tab_without_a_document_edit() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    harness.state_mut().state.browser_build = true;
    harness.run_steps(3);
    let before = harness.state().state.history.done_entry_ids();
    let version = harness.state().state.doc_version;
    harness.get_by_label("Donate").click();
    harness.step();
    let urls: Vec<_> = harness
        .output()
        .platform_output
        .commands
        .iter()
        .filter_map(|command| match command {
            egui::OutputCommand::OpenUrl(url) => Some(url),
            _ => None,
        })
        .collect();
    assert_eq!(urls.len(), 1, "one click opens one browser tab");
    assert_eq!(
        urls[0].url,
        "https://buy.stripe.com/8x2dR93Up10S6eP0FO4c800"
    );
    assert!(urls[0].new_tab);
    harness.run();
    assert_eq!(harness.state().state.history.done_entry_ids(), before);
    assert_eq!(harness.state().state.doc_version, version);
    assert!(harness.state().state.pending.is_empty());
}

#[test]
fn layout_donate_top_right() {
    let _g = snapshot_guard();
    let mut harness = shell_harness();
    for width in [1280.0, 900.0] {
        harness.set_size(Vec2::new(width, 720.0));
        harness.run_steps(3);
        let rect = harness.get_by_label("Donate").rect();
        assert!(
            rect.left() > width * 0.75,
            "Donate stays in the right quarter at {width}"
        );
        assert!(
            rect.top() >= 0.0 && rect.bottom() <= 40.0,
            "Donate stays in the top row at {width}: {rect:?}"
        );
        assert!(
            rect.right() <= width,
            "Donate stays inside the window at {width}"
        );
    }
}
