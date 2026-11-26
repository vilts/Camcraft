//! Project donation link in the ribbon's top row (#2385).
//! Provenance: our own UI; the link comes from .github/FUNDING.yml.

use egui::{Button, Image, Rect, RichText, Ui, Vec2};

use super::browser_open;
use super::icons::{Icon, IconSet};
use super::theme::Tokens;
use crate::state::State;

/// The project funding link, also published in README.md and FUNDING.yml.
pub(super) const DONATION_URL: &str = "https://buy.stripe.com/8x2dR93Up10S6eP0FO4c800";

fn result_id() -> egui::Id {
    egui::Id::new("donate-browser-result")
}

/// The native worker reports back without holding or changing the document.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(super) fn record_result(ctx: &egui::Context, result: Result<(), String>) {
    browser_open::record_result(ctx, result_id(), result);
}

/// The button stays available for an empty document and in the browser.
/// Returns its left edge: the Home button (#2384) sits left of it.
pub(super) fn draw(
    ui: &mut Ui,
    state: &mut State,
    pal: &Tokens,
    icons: Option<&IconSet>,
    band: Rect,
) -> f32 {
    if let Some(result) = browser_open::take_result(ui.ctx(), result_id()) {
        match result {
            Ok(()) => state.status = "Donate: Opened the donation link in the browser".into(),
            Err(error) => {
                state.status =
                    format!("Donate: Could not open the browser: {error}. Open {DONATION_URL}");
                diag_warn!("{}", state.status);
            }
        }
    }
    let label = RichText::new("Donate").size(pal.font_label).color(pal.text);
    let button = match icons.and_then(|set| set.texture(Icon::Donate)) {
        Some(texture) => Button::image_and_text(
            Image::new(texture)
                .fit_to_exact_size(Vec2::splat(16.0))
                .tint(pal.text),
            label,
        ),
        None => Button::new(label),
    };
    let width = ui
        .painter()
        .layout_no_wrap(
            "Donate".into(),
            egui::FontId::proportional(pal.font_label),
            pal.text,
        )
        .size()
        .x
        + 24.0
        + 2.0 * pal.button_padding_x;
    let rect = Rect::from_min_max(
        egui::pos2(band.right() - pal.item_spacing_x - width, band.top() + 2.0),
        egui::pos2(band.right() - pal.item_spacing_x, band.bottom() - 2.0),
    );
    if ui
        .put(rect, button)
        .on_hover_text("Support CamCraft")
        .clicked()
    {
        browser_open::open(
            ui,
            state.browser_build,
            "donate-browser",
            result_id(),
            DONATION_URL,
        );
    }
    rect.left()
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::super::browser_open::run_launcher;
    use std::process::{Command, Stdio};

    fn test_launcher() -> Command {
        let mut command = Command::new(std::env::current_exe().expect("test executable"));
        command.stdout(Stdio::null()).stderr(Stdio::null());
        command
    }

    #[test]
    fn donate_launcher_refuses_a_missing_executable() {
        let missing = std::env::temp_dir().join(format!(
            "camcraft-missing-donate-launcher-{}",
            std::process::id()
        ));
        assert!(!missing.exists(), "the fixture path must not be executable");
        assert!(run_launcher(&mut Command::new(missing)).is_err());
    }

    #[test]
    fn donate_launcher_reports_a_nonzero_exit() {
        let error = run_launcher(test_launcher().arg("--bad-option-2385"))
            .expect_err("the launcher rejected its arguments");
        assert!(error.contains("browser launcher"));
    }

    #[test]
    fn donate_launcher_accepts_a_successful_exit() {
        assert!(run_launcher(test_launcher().arg("--list")).is_ok());
    }
}
