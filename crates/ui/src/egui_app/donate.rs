//! Project donation link in the ribbon's top row (#2385).
//! Provenance: our own UI; the link comes from .github/FUNDING.yml.

#[cfg(not(target_arch = "wasm32"))]
use egui::Context;
use egui::{Button, Image, Rect, RichText, Ui, Vec2};

use super::icons::{Icon, IconSet};
use super::theme::Tokens;
use crate::state::State;

/// The project funding link, also published in README.md and FUNDING.yml.
pub(super) const DONATION_URL: &str = "https://buy.stripe.com/8x2dR93Up10S6eP0FO4c800";

fn result_id() -> egui::Id {
    egui::Id::new("donate-browser-result")
}

/// The native worker reports back without holding or changing the document.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn record_result(ctx: &Context, result: Result<(), String>) {
    ctx.data_mut(|data| data.insert_temp(result_id(), Some(result)));
    ctx.request_repaint();
}

#[cfg(not(target_arch = "wasm32"))]
fn open_system_browser() -> Result<(), String> {
    use std::process::Command;

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", DONATION_URL]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(DONATION_URL);
        command
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(DONATION_URL);
        command
    };
    run_launcher(&mut command)
}

#[cfg(not(target_arch = "wasm32"))]
fn run_launcher(command: &mut std::process::Command) -> Result<(), String> {
    let status = command.status().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("the browser launcher returned {status}"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn open_native(ctx: &Context) {
    let worker_ctx = ctx.clone();
    if let Err(error) = std::thread::Builder::new()
        .name("donate-browser".into())
        .spawn(move || record_result(&worker_ctx, open_system_browser()))
    {
        record_result(ctx, Err(error.to_string()));
    }
}

/// The button stays available for an empty document and in the browser.
pub(super) fn draw(
    ui: &mut Ui,
    state: &mut State,
    pal: &Tokens,
    icons: Option<&IconSet>,
    band: Rect,
) {
    if let Some(result) = ui.ctx().data_mut(|data| {
        data.remove_temp::<Option<Result<(), String>>>(result_id())
            .flatten()
    }) {
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
        if state.browser_build || cfg!(target_arch = "wasm32") {
            ui.ctx().open_url(egui::OpenUrl::new_tab(DONATION_URL));
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            open_native(ui.ctx());
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::run_launcher;
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
