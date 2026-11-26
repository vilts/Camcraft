//! The Report door (#1696): one place to file a bug or ask for a feature.
//! It opens the repository's issue form in the browser with the title,
//! the body and the label filled in, and it puts a picture of the whole
//! application window on the clipboard so the user can paste it in.
//!
//! Provenance: our own. GitHub's prefill query (`issues/new?title=…&body=
//! …&labels=…`) is its documented new-issue URL form; it applies only to a
//! blank issue template, so the repository also gained `blank-issue.md`.
//! The picture is the whole window, not only the 3D view: egui's viewport
//! screenshot command (`ViewportCommand::Screenshot`), answered by the
//! native shell's capture module (shell_window/capture.rs), specced from
//! egui's public capture contract and the shape of the report dialog the
//! maintainer pointed at (SolidCraft, #1696; his GUI finding: a model-area
//! picture is not what a triage needs). Their code was not read; the
//! mechanism is egui's public API. No vendor material consulted.

use egui::{Button, Context, Id, Image, Response, TextEdit, Ui, Vec2, Window};

use super::icons::{Icon, IconSet};
use crate::state::State;

/// The repository's blank-issue page: prefill only works on the blank
/// template, not on an issue form.
const ISSUE_NEW_URL: &str = "https://github.com/NeoTech/Camcraft/issues/new";

/// The one token this window answers. The shell replies to every
/// screenshot request with it; a reply carrying any other token belongs
/// to another door and stays unread here. `UserData` compares by Arc
/// identity, so cloning keeps the match. The payload is deliberately not
/// a zero-sized type: Rust never allocates for those, so every
/// `UserData::new(())` shares one dangling pointer and would compare
/// equal to every other - a `()` token answers foreign requests too.
pub(crate) fn shot_token() -> &'static egui::UserData {
    static TOKEN: std::sync::OnceLock<egui::UserData> = std::sync::OnceLock::new();
    TOKEN.get_or_init(|| egui::UserData::new("camcraft-report-shot"))
}

/// What the user files: a defect, or an enhancement. The two GitHub
/// labels are the repository's existing `bug` and `enhancement`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReportKind {
    Bug,
    Feature,
}

impl ReportKind {
    /// The window's button for this kind.
    #[must_use]
    pub(crate) fn button(self) -> &'static str {
        match self {
            Self::Bug => "Report a bug",
            Self::Feature => "Request a feature",
        }
    }

    /// The label GitHub puts on the new issue.
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Bug => "bug",
            Self::Feature => "enhancement",
        }
    }
}

/// Percent-encode one query value. Only ASCII unreserved bytes pass
/// through; a space takes `+`, as GitHub's own query forms read.
pub(crate) fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The prefilled new-issue URL for one kind of report. GitHub opens the
/// blank template with the title and body already typed and the right
/// label ticked; the user adds the story and pastes the picture.
#[must_use]
pub(crate) fn issue_url(kind: ReportKind, title: &str, body: &str) -> String {
    format!(
        "{ISSUE_NEW_URL}?title={}&body={}&labels={}",
        url_encode(title),
        url_encode(body),
        url_encode(kind.label()),
    )
}

/// The body: the two questions a triage needs answered, then the plain
/// text the Debug info window reports (#157) - same fields, one source.
/// The body travels to GitHub in the URL, so it takes the shared form of
/// that report: file names only, never a directory of the user (#2744).
#[must_use]
pub(crate) fn report_body(state: &State) -> String {
    let mut out = String::new();
    out.push_str("<!-- CamCraft filled in the title, this text and the label. -->\n\n");
    out.push_str("**What happened**\n\n\n\n**Steps to reproduce**\n\n1. \n\n");
    out.push_str("---\n\n");
    out.push_str(&super::debug_info::shared_report(state));
    if cfg!(not(target_arch = "wasm32")) {
        out.push_str("\n\nScreenshot: CamCraft is copying the whole window to the clipboard - paste it into this description.");
    } else {
        out.push_str("\n\nScreenshot: paste a picture of the window into this description.");
    }
    out
}

/// Ask the shell for a picture of the whole window (#1696). The native
/// shell answers one frame later, with the dialog on screen; the browser
/// shell cannot serve a window capture, so there the report stays text.
pub(crate) fn request_screenshot(ctx: &Context) {
    if cfg!(target_arch = "wasm32") {
        let _ = ctx;
        return;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(shot_token().clone()));
}

/// A picture that answers our request, if one arrived this frame.
fn taken_shot(ctx: &Context) -> Option<std::sync::Arc<egui::ColorImage>> {
    ctx.input(|i| {
        i.events.iter().find_map(|event| match event {
            egui::Event::Screenshot { viewport_id, user_data, image }
                if *viewport_id == egui::ViewportId::ROOT && user_data == shot_token() =>
            {
                Some(image.clone())
            }
            _ => None,
        })
    })
}

/// The status strip's door: the glyph when the icon set is up, the words
/// when it is not (the Debug info door's degrade law). Both answer to
/// "Report an issue" - the name the issue forms promise.
pub(super) fn button(ui: &mut Ui, icons: Option<&IconSet>) -> Response {
    let resp = match icons.and_then(|s| s.texture(Icon::Report)) {
        Some(tex) => {
            let ink = ui.visuals().text_color();
            let glyph = Image::new(tex).fit_to_exact_size(Vec2::splat(16.0)).tint(ink);
            ui.add(Button::image(glyph).frame(false)).on_hover_text("Report an issue")
        }
        None => ui.small_button("Report an issue"),
    };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Report an issue"));
    resp
}

/// The window: a title field and the two kinds. One press opens the
/// prefilled GitHub form and asks the shell for the window picture; when
/// the picture arrives the window puts it on the clipboard and answers
/// with the status strip's word.
/// Allocates nothing while closed; closed by default on a fresh State,
/// the shell's window law.
pub(crate) fn window(ctx: &Context, state: &mut State) {
    // The picture is taken with this window up and lands a frame later,
    // so read it even when the dialog has been closed since: a user who
    // clicks and closes still gets the clipboard picture they were told
    // was coming.
    if let Some(image) = taken_shot(ctx) {
        ctx.output_mut(|o| {
            o.commands.push(egui::OutputCommand::CopyImage((*image).clone()));
        });
        state.status = "Screenshot: CamCraft copied the whole window to the clipboard".to_owned();
    }
    if !state.report_open {
        return;
    }
    let id = Id::new("report-issue");
    let folded = ctx.data(|d| d.get_temp::<bool>(id.with("collapsed"))).unwrap_or(false);
    let mut open = true;
    let mut filed: Option<ReportKind> = None;
    Window::new("Report an issue")
        .id(id)
        .title_bar(false)
        .resizable(!folded)
        .default_size([440.0, 180.0])
        .show(ctx, |ui| {
            if super::window_chrome::title_row(ui, id, "Report an issue", &mut open) {
                return;
            }
            ui.separator();
            ui.label("CamCraft fills the title, the description and the label into GitHub, and copies the whole window to the clipboard. Type a short title:");
            ui.add_sized(
                [ui.available_width(), 0.0],
                TextEdit::singleline(&mut state.report_title).hint_text("e.g. Contour leaves a witness line"),
            );
            ui.horizontal(|ui| {
                for kind in [ReportKind::Bug, ReportKind::Feature] {
                    if ui.button(kind.button()).clicked() {
                        filed = Some(kind);
                    }
                }
            });
            ui.label("Before you file: check the open issues. A duplicate wastes both our times.");
        });
    if let Some(kind) = filed {
        // The shell answers the request a frame later, once the window
        // has repainted with the dialog up: the shot shows the whole
        // app - panels, tabs, status strip, 3D view - the picture the
        // triage needs (the maintainer's GUI finding, #1696).
        request_screenshot(ctx);
        let body = report_body(state);
        let url = issue_url(kind, &state.report_title, &body);
        ctx.open_url(egui::OpenUrl::new_tab(&url));
        state.status = format!(
            "{}: camcraft filled the new issue and is copying the window to the clipboard - add the story and paste the picture",
            kind.button()
        );
    }
    state.report_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shot_token_matches_only_itself() {
        // The token identity the Event::Screenshot reply is checked with:
        // a clone matches, a fresh request never matches another token.
        // The full round trip (request, shell answer, clipboard) rides
        // report_tests: the shell-side half needs a real GPU.
        let token = shot_token();
        assert_eq!(*token, token.clone());
        let other = egui::UserData::new("some-other-door");
        assert_ne!(*token, other);
        // The law the payload choice protects: two tokens of the same
        // non-zero payload are still two different tokens.
        let third = egui::UserData::new("some-other-door");
        assert_ne!(other, third);
    }
}
