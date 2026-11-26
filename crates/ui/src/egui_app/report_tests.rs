//! Issue #1696: the status strip's Report door opens a window that fills
//! GitHub's new-issue form in (title, body, label) and copies a picture
//! of the whole application window to the clipboard.
//! Provenance: our own tests over our own window.
//!
//! The picture itself is the shell's work (shell_window/capture.rs) and
//! needs a real GPU: that half rides the gpu_render lane. The harness
//! covers the door's own half: the request it sends, the reply answer,
//! the token gate, and the clipboard command.

use egui::accesskit::Role;
use egui::Vec2;
use egui_kittest::kittest::Queryable;
use egui_kittest::Harness;

use super::gpu_test_lock::gpu_guard;
use super::{debug_info, panes, report, theme};
use crate::state::State;

fn frame(ui: &mut egui::Ui, st: &mut State) {
    let pal = theme::current();
    panes::status(ui, st, &pal, None);
    report::window(ui.ctx(), st);
}

fn harness() -> Harness<'static, State> {
    Harness::builder()
        .with_size(Vec2::new(900.0, 500.0))
        .build_ui_state(frame, State::new())
}

/// A picture to answer a request with: two pixels, no GPU.
fn sample_shot() -> egui::ColorImage {
    egui::ColorImage::new([2, 2], vec![egui::Color32::RED; 4])
}

/// The event a real shell sends when its capture is ready.
fn shot_event(token: egui::UserData) -> egui::Event {
    egui::Event::Screenshot {
        viewport_id: egui::ViewportId::ROOT,
        user_data: token,
        image: std::sync::Arc::new(sample_shot()),
    }
}

/// Did this frame ask the shell to copy a picture?
fn copied_a_picture(h: &Harness<'static, State>) -> bool {
    h.output()
        .platform_output
        .commands
        .iter()
        .any(|c| matches!(c, egui::OutputCommand::CopyImage(_)))
}

#[test]
fn url_encode_keeps_safe_words_and_plains_the_rest() {
    assert_eq!(report::url_encode("Contour-1_2.3~ok"), "Contour-1_2.3~ok");
    assert_eq!(report::url_encode("a b"), "a+b");
    // A reserved byte and a non-ASCII byte both go through as UTF-8 %XX.
    assert_eq!(report::url_encode("a?b#c"), "a%3Fb%23c");
    assert_eq!(report::url_encode("é"), "%C3%A9");
}

#[test]
fn the_url_prefills_the_new_issue_with_title_body_and_label() {
    let url = report::issue_url(report::ReportKind::Bug, "Crash on post", "line one\nline two");
    assert!(url.starts_with("https://github.com/NeoTech/Camcraft/issues/new?title="), "{url}");
    assert!(url.contains("&body=line+one%0Aline+two"), "{url}");
    assert!(url.ends_with("&labels=bug"), "{url}");
    let feat = report::issue_url(report::ReportKind::Feature, "", "");
    assert!(feat.ends_with("&labels=enhancement"), "{feat}");
}

#[test]
fn the_body_carries_the_questions_and_the_debug_report() {
    let state = State::new();
    let body = report::report_body(&state);
    assert!(body.contains("**What happened**"), "{body}");
    assert!(body.contains("**Steps to reproduce**"), "{body}");
    // Same fields, one source: the Debug info window's report, in the form
    // that leaves the machine (file names only, #2744).
    assert!(body.contains(&debug_info::shared_report(&state)), "the body must embed the debug report");
    assert!(body.contains("Project: unsaved"), "an unsaved project still reads: {body}");
    assert!(body.contains("clipboard"), "the body tells the reporter to paste the picture");
}

/// A project path inside a user's home directory, in this platform's form.
fn user_project_path() -> (std::path::PathBuf, &'static str) {
    let home = if cfg!(windows) { r"C:\Users\jane.doe" } else { "/home/jane.doe" };
    (std::path::Path::new(home).join("Parts").join("bracket.camproj"), home)
}

#[test]
fn the_report_door_url_has_the_project_file_name_and_no_directory() {
    // #2744: the browser sends the URL to GitHub, so the user's directory
    // (and the user name in it) must not be in it - neither as typed nor
    // percent-encoded. The file name is enough for a triage.
    let mut state = State::new();
    let (path, home) = user_project_path();
    state.project_path = Some(path.clone());
    // The status strip can repeat the path after a save.
    state.status = format!("saved {}", path.display());
    let body = report::report_body(&state);
    let url = report::issue_url(report::ReportKind::Bug, "Crash on post", &body);
    assert!(body.contains("Project: bracket.camproj\n"), "{body}");
    assert!(body.contains("Status: saved bracket.camproj"), "{body}");
    assert!(url.contains(&report::url_encode("Project: bracket.camproj")), "{url}");
    let dir = path.parent().map(|p| p.display().to_string()).unwrap_or_default();
    for secret in [dir.as_str(), home, "jane.doe", "Parts"] {
        assert!(!body.contains(secret), "the body leaks {secret:?}: {body}");
        assert!(!url.contains(secret), "the URL leaks {secret:?}: {url}");
        assert!(!url.contains(&report::url_encode(secret)), "the URL leaks encoded {secret:?}: {url}");
    }
    // The in-app Debug info stays on the machine and keeps the full path.
    let local = debug_info::report(&state);
    assert!(local.contains(&format!("Project: {}", path.display())), "{local}");
}

#[test]
fn the_report_door_scrubs_a_status_path_that_is_not_the_project() {
    // #2744 review: with no saved project, a status such as `open failed:
    // <path>` still names a full path. The shared report keeps only its
    // file name, in the Windows and the POSIX form, on every system.
    for failed in [r"C:\Users\someone\Documents\secret\part.camproj", "/home/someone/secret/part.camproj"] {
        let mut state = State::new();
        state.status = format!("open failed: {failed}: The system cannot find the file");
        let body = report::report_body(&state);
        let url = report::issue_url(report::ReportKind::Bug, "Open fails", &body);
        assert!(body.contains("Project: unsaved"), "{body}");
        assert!(body.contains("Status: open failed: part.camproj: The system cannot find the file"), "{body}");
        assert!(url.contains(&report::url_encode("part.camproj")), "{url}");
        for secret in ["someone", "secret", "Documents", "Users", "home"] {
            assert!(!body.contains(secret), "the body leaks {secret:?}: {body}");
            assert!(!url.contains(secret), "the URL leaks {secret:?}: {url}");
            assert!(!url.contains(&report::url_encode(secret)), "the URL leaks encoded {secret:?}: {url}");
        }
        // The status strip and the in-app Debug info keep the full text.
        assert!(state.status.contains(failed));
        assert!(debug_info::report(&state).contains(failed));
    }
}

#[test]
fn the_door_opens_the_window_and_a_kind_button_files_the_url() {
    let _gpu = gpu_guard(); // #3589: the screenshot renderer makes a wgpu instance
    let mut h = harness();
    h.run();
    assert!(h.query_by_label("Report a bug").is_none(), "closed on a fresh state");
    h.get_by_label("Report an issue").click();
    h.run();
    assert!(h.state().report_open);
    h.get_by_label("Request a feature").click();
    h.step(); // the click lands this frame; its output is this frame's
    let opened = h.output().platform_output.commands.iter().any(|c| {
        matches!(c, egui::OutputCommand::OpenUrl(u)
            if u.url.starts_with("https://github.com/NeoTech/Camcraft/issues/new")
            && u.url.ends_with("&labels=enhancement"))
    });
    assert!(opened, "the kind button opened the prefilled new-issue URL");
    h.run();
    assert!(h.state().status.contains("clipboard"), "status: {}", h.state().status);
    // The door closes it again. The window's title row reads "Report an
    // issue" too, so only the button answers (the Debug info window's law).
    h.get_by(|n| n.role() == Role::Button && n.label().as_deref() == Some("Report an issue")).click();
    h.run();
    assert!(!h.state().report_open);
    assert!(h.query_by_label("Report a bug").is_none(), "the door closed it");
}

#[test]
fn the_title_field_travels_into_the_url() {
    let _gpu = gpu_guard(); // #3589: the screenshot renderer makes a wgpu instance
    let mut h = harness();
    h.get_by_label("Report an issue").click();
    h.run();
    let s = h.state_mut();
    s.report_title = "Pocket leaves a witness line".to_owned();
    h.run();
    h.get_by_label("Report a bug").click();
    h.step();
    // The builder puts the title first, so the separator before it is "?"
    // and the ones after it are "&" (GitHub's blank-issue query order).
    let opened = h.output().platform_output.commands.iter().any(|c| {
        matches!(c, egui::OutputCommand::OpenUrl(u) if u.url.contains("?title=Pocket+leaves+a+witness+line"))
    });
    assert!(opened, "the typed title rides into the URL");
}

#[test]
fn a_kind_button_asks_the_shell_for_the_whole_window() {
    let _gpu = gpu_guard(); // #3589: the screenshot renderer makes a wgpu instance
    // The request is the whole-window picture, not a model-area picture:
    // egui's viewport screenshot command, aimed at the root window, with
    // the door's own token (#1696, the maintainer's GUI finding).
    let mut h = harness();
    h.get_by_label("Report an issue").click();
    h.run();
    h.get_by_label("Report a bug").click();
    h.step();
    let asked = h
        .output()
        .viewport_output
        .get(&egui::ViewportId::ROOT)
        .map(|v| {
            v.commands.iter().any(|c| {
                matches!(c, egui::ViewportCommand::Screenshot(t) if t == report::shot_token())
            })
        })
        .unwrap_or(false);
    assert!(asked, "the kind button asked the shell for the root window");
}

#[test]
fn the_answer_copies_the_picture_and_says_the_word() {
    let _gpu = gpu_guard(); // #3589: the screenshot renderer makes a wgpu instance
    let mut h = harness();
    h.get_by_label("Report an issue").click();
    h.run();
    h.get_by_label("Report a bug").click();
    h.step();
    assert!(!copied_a_picture(&h), "the shell has not answered yet");
    h.input_mut().events.push(shot_event(report::shot_token().clone()));
    h.step();
    assert!(copied_a_picture(&h), "the answer went to the clipboard");
    assert!(h.state().status.contains("clipboard"), "status: {}", h.state().status);
}

#[test]
fn the_picture_is_still_copied_after_the_window_is_closed() {
    let _gpu = gpu_guard(); // #3589: the screenshot renderer makes a wgpu instance
    // The answer lands a frame after the press. A user who closes the
    // window in that moment must still get the clipboard picture they
    // were told was coming.
    let mut h = harness();
    h.get_by_label("Report an issue").click();
    h.run();
    h.get_by_label("Report a bug").click();
    h.step();
    h.state_mut().report_open = false;
    h.step();
    h.input_mut().events.push(shot_event(report::shot_token().clone()));
    h.step();
    assert!(copied_a_picture(&h), "a late answer still reaches the clipboard");
}

#[test]
fn an_answer_for_another_token_is_left_alone() {
    // A reply for another door's request must not be read here: the token
    // is the only thing that tells the requests apart. No request is made
    // in this test on purpose: the harness (wgpu feature) fulfils every
    // real request with the door's own token, which would copy first and
    // say nothing about the foreign answer.
    let mut h = harness();
    h.get_by_label("Report an issue").click();
    h.run();
    h.state_mut().status = "keep me".to_owned();
    h.step();
    h.input_mut().events.push(shot_event(egui::UserData::new("some-other-door")));
    h.step();
    assert!(!copied_a_picture(&h), "another door's answer must not be copied");
    assert_eq!(h.state().status, "keep me", "an unread answer says nothing");
}

#[test]
fn no_answer_copies_nothing() {
    let _gpu = gpu_guard(); // #3589: the screenshot renderer makes a wgpu instance
    // No shell, no picture: the report still opens (the URL and the text
    // are the whole point), and the door must not claim a copy it never
    // made (Prime Directive: log or say, never silence).
    let mut h = harness();
    h.get_by_label("Report an issue").click();
    h.run();
    h.get_by_label("Report a bug").click();
    h.run();
    assert!(!copied_a_picture(&h), "nothing to copy with no answer from the shell");
    assert!(h.state().report_open, "the report stays open for the story");
}
