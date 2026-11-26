//! Shared external-browser URL launcher for top-bar links (#2385, #2428).
//!
//! Browser builds ask egui (`OpenUrl::new_tab`); native builds spawn a short
//! worker thread running the platform launcher and report failures via egui
//! temp data so `State` stays untouched until the next frame.
//!
//! Provenance: extracted from `donate.rs` (#2385); no vendor material consulted.

#[cfg(not(target_arch = "wasm32"))]
use egui::Context;
use egui::Ui;

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn record_result(ctx: &Context, id: egui::Id, result: Result<(), String>) {
    ctx.data_mut(|data| data.insert_temp(id, Some(result)));
    ctx.request_repaint();
}

pub(super) fn take_result(ctx: &egui::Context, id: egui::Id) -> Option<Result<(), String>> {
    ctx.data_mut(|data| {
        data.remove_temp::<Option<Result<(), String>>>(id)
            .flatten()
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn has_unsafe_shell_chars(url: &str) -> bool {
    url.chars()
        .any(|c| c.is_control() || matches!(c, '&' | '|' | '<' | '>' | '^' | '"' | '`' | '\''))
}

#[cfg(not(target_arch = "wasm32"))]
fn open_system_browser(url: &str) -> Result<(), String> {
    use std::process::Command;

    if !url.starts_with("https://") || has_unsafe_shell_chars(url) {
        return Err(String::from("refused an unexpected link URL"));
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };
    run_launcher(&mut command)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn run_launcher(command: &mut std::process::Command) -> Result<(), String> {
    let status = command.status().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("the browser launcher returned {status}"))
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn open_native(ctx: &Context, thread_name: &str, id: egui::Id, url: String) {
    let worker_ctx = ctx.clone();
    if let Err(error) = std::thread::Builder::new()
        .name(thread_name.into())
        .spawn(move || record_result(&worker_ctx, id, open_system_browser(&url)))
    {
        record_result(ctx, id, Err(error.to_string()));
    }
}

/// Open an external HTTPS URL in the user's browser on either target.
pub(super) fn open(
    ui: &Ui,
    browser_build: bool,
    thread_name: &str,
    id: egui::Id,
    url: &str,
) {
    if browser_build || cfg!(target_arch = "wasm32") {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        open_native(ui.ctx(), thread_name, id, url.to_owned());
        #[cfg(target_arch = "wasm32")]
        let _ = (thread_name, id);
    }
}
