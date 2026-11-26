//! Title-bar `"Update available: vX.Y.Z"` notice (#2428 Part B).
//!
//! Shown at the right of the ribbon's top band (directly above the #352 build
//! badge and left of Settings/Home/Donate) only when automatic update checks
//! are enabled and a strictly newer stable GitHub release is recorded.
//! Clicking opens the release-notes link in the browser cleanly; Part B never
//! downloads or installs binaries.
//!
//! Provenance: our own top-band pill widget over `camcraft_app::update_check`;
//! no vendor material consulted.

use camcraft_app::update_check::{ReleaseInfo, is_newer_version, is_safe_release_url};
use egui::{Button, Rect, RichText, Stroke, Ui};

use super::browser_open;
use super::contrast;
use super::theme::Tokens;
use crate::build_info;
use crate::state::State;

fn result_id() -> egui::Id {
    egui::Id::new("update-notes-browser-result")
}

/// The update notice to show in the UI, or `None` when disabled, missing, or
/// not strictly newer than this build's version.
pub(crate) fn visible_update(state: &State) -> Option<&ReleaseInfo> {
    if !state.check_for_updates {
        return None;
    }
    let info = state.update_available.as_ref()?;
    is_newer_version(build_info::VERSION, &info.version).then_some(info)
}

/// User-facing tooltip naming the version and release-notes link.
pub(crate) fn tooltip_text(info: &ReleaseInfo) -> String {
    format!(
        "CamCraft v{} is available.\nRelease notes: {}\nClick to open the release page.",
        info.version, info.release_notes_url
    )
}

/// Draw the `"Update available: vX.Y.Z"` item in `band` right-aligned to `right`,
/// provided it fits cleanly to the right of `min_left`.
pub(super) fn draw(ui: &mut Ui, state: &mut State, pal: &Tokens, band: Rect, min_left: f32, right: f32) {
    if let Some(result) = browser_open::take_result(ui.ctx(), result_id()) {
        match result {
            Ok(()) => state.status = String::from("Update: Opened release notes in the browser"),
            Err(error) => {
                let fallback = visible_update(state)
                    .map(|u| u.release_notes_url.as_str())
                    .unwrap_or("https://github.com/NeoTech/Camcraft/releases");
                state.status = format!("Update: Could not open the browser: {error}. Open {fallback}");
                diag_warn!("{}", state.status);
            }
        }
    }
    let Some(info) = visible_update(state).cloned() else {
        return;
    };
    let _span = crate::frame_cost::span("ui/update_notice");
    let text = format!("Update available: v{}", info.version);
    let fg = contrast::on_fill(pal.selection_bg);
    let label = RichText::new(&text).size(pal.font_label).color(fg);
    let text_w = ui
        .painter()
        .layout_no_wrap(text.clone(), egui::FontId::proportional(pal.font_label), fg)
        .size()
        .x;
    let width = text_w + 2.0 * pal.button_padding_x + 10.0;
    let left = right - width;
    if left < min_left {
        return;
    }
    let rect = Rect::from_min_max(
        egui::pos2(left, band.top() + 2.0),
        egui::pos2(right, band.bottom() - 2.0),
    );
    let button = Button::new(label)
        .fill(pal.selection_bg)
        .stroke(Stroke::new(1.0, pal.accent));
    let tip = tooltip_text(&info);
    let resp = ui.put(rect, button);
    ui.ctx()
        .accesskit_node_builder(resp.id, |b| b.set_description(tip.as_str()));
    let clicked = resp.clicked();
    resp.on_hover_text(tip);
    if clicked && is_safe_release_url(&info.release_notes_url) {
        browser_open::open(
            ui,
            state.browser_build,
            "update-notes-browser",
            result_id(),
            &info.release_notes_url,
        );
    }
}
