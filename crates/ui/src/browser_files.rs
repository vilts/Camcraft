//! The browser build's file commands (#1431): a page has no paths, so
//! Save, Save As and Post hand their bytes to a download, and every file
//! command that still needs a native picker or a folder is refused by
//! name (and greyed with the same reason) instead of doing nothing.
//! Provenance: our own law over the WHATWG File API (`Blob`,
//! `URL.createObjectURL`) and the HTML `a[download]` attribute, from the
//! public specifications; no vendor material consulted.

use crate::state::CommandKind;

/// Opening this file needs a picker the browser build does not have yet:
/// its async picker opens models and projects only (#31).
pub const NEEDS_PICKER: &str =
    "Not available in the browser yet: the browser file picker opens models and projects only (#31)";
/// The browser keeps no list of files it opened.
pub const NO_RECENT: &str = "Not available in the browser: it keeps no list of recent files";
/// The fixture library lives in a folder on disk.
pub const NO_LIBRARY: &str = "Not available in the browser: the fixture library is a folder on disk";
/// Exports other than the project and the program are not downloads yet.
pub const NO_EXPORT: &str = "Not available in the browser yet: this export does not download yet (#1437)";
/// A per-operation post writes into a folder, which a page cannot pick.
pub const NO_FOLDER: &str =
    "Not available in the browser: one file per operation writes into a folder; switch Output to one combined program";

/// Why `action` cannot run in the browser build, or `None` when it can
/// (`per_op`: the post writes one file per operation).
#[must_use]
pub fn refusal(action: &CommandKind, per_op: bool) -> Option<&'static str> {
    match action {
        // Open model and Open project run the browser's async picker (#31).
        CommandKind::ImportFixtureDialog
        | CommandKind::LoadMachineDialog
        | CommandKind::SelectPostDialog
        | CommandKind::SelectThemeDialog => Some(NEEDS_PICKER),
        CommandKind::OpenRecent(..) => Some(NO_RECENT),
        CommandKind::FixtureLibrarySaveDialog(_) | CommandKind::FixtureLibraryInsertDialog(_) => Some(NO_LIBRARY),
        CommandKind::SetupSheetDialog | CommandKind::ExportStockDialog | CommandKind::ToolListExportDialog(_) => {
            Some(NO_EXPORT)
        }
        CommandKind::PostProgramDialog if per_op => Some(NO_FOLDER),
        _ => None,
    }
}

/// A download name from what the user typed: the last path segment, with
/// characters a file system refuses replaced, and `ext` added when missing.
/// An empty or all-dots answer falls back to `untitled.<ext>`.
#[must_use]
pub fn file_name(input: &str, ext: &str) -> String {
    let last = input.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let clean: String = last
        .chars()
        .map(|c| if c.is_control() || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c })
        .collect();
    let stem = if clean.trim_matches('.').is_empty() { "untitled".to_owned() } else { clean };
    let dotted = format!(".{ext}");
    if stem.to_ascii_lowercase().ends_with(&dotted.to_ascii_lowercase()) { stem } else { format!("{stem}{dotted}") }
}

/// A file button: live when `why` is `None` (with its `hover` tip), else
/// greyed with `why` as its tip. `true` when clicked.
pub fn button(ui: &mut egui::Ui, why: Option<&str>, label: &str, hover: &str) -> bool {
    let r = ui.add_enabled(why.is_none(), egui::Button::new(label));
    match why {
        Some(why) => {
            r.on_disabled_hover_text(why);
            false
        }
        None if hover.is_empty() => r.clicked(),
        None => r.on_hover_text(hover).clicked(),
    }
}

/// A file button in a page row's slot (the `even_row` layout, #1440):
/// the same greying as [`button`], sized like the surrounding buttons.
pub fn button_sized(ui: &mut egui::Ui, why: Option<&str>, label: &str, hover: &str, size: egui::Vec2) -> bool {
    let r = ui.add_enabled(why.is_none(), egui::Button::new(label).min_size(size));
    match why {
        Some(why) => {
            r.on_disabled_hover_text(why);
            false
        }
        None if hover.is_empty() => r.clicked(),
        None => r.on_hover_text(hover).clicked(),
    }
}

/// Hand `bytes` to the browser as a download named `name`.
pub fn offer(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    imp::offer(name, bytes, mime)
}

/// Ask for a download name (Save As), starting from `default`; `None`
/// when the user cancels.
#[must_use]
pub fn ask_name(default: &str) -> Option<String> {
    imp::ask_name(default)
}

/// The downloads offered on this thread since the last call (native
/// builds only, where a test stands in for the browser).
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn take_offered() -> Vec<(String, Vec<u8>, String)> {
    imp::OFFERED.with(|o| std::mem::take(&mut *o.borrow_mut()))
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::cell::RefCell;

    thread_local! {
        pub(super) static OFFERED: RefCell<Vec<(String, Vec<u8>, String)>> = const { RefCell::new(Vec::new()) };
    }

    // A native build has no page: the shell runs these only when a test
    // stands in for the browser (`State::browser_build`), so record them.
    pub(super) fn offer(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
        OFFERED.with(|o| o.borrow_mut().push((name.to_owned(), bytes.to_vec(), mime.to_owned())));
        Ok(())
    }

    pub(super) fn ask_name(default: &str) -> Option<String> {
        Some(default.to_owned())
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::cell::RefCell;

    use eframe::wasm_bindgen::{self, JsCast, JsValue, prelude::wasm_bindgen};
    use eframe::web_sys::{self, js_sys};

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(catch, js_namespace = URL, js_name = createObjectURL)]
        fn create_object_url(blob: &web_sys::Blob) -> Result<String, JsValue>;
        #[wasm_bindgen(catch, js_namespace = URL, js_name = revokeObjectURL)]
        fn revoke_object_url(url: &str) -> Result<(), JsValue>;
    }

    thread_local! {
        // The last download's URL, revoked at the next one: revoking at
        // once can cancel a download the browser has not started yet.
        static LAST_URL: RefCell<Option<String>> = const { RefCell::new(None) };
    }

    fn js(what: &str, e: JsValue) -> String {
        format!("download failed: {what}: {}", e.as_string().unwrap_or_else(|| format!("{e:?}")))
    }

    pub(super) fn offer(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
        let options = web_sys::BlobPropertyBag::new();
        options.set_type(mime);
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
            .map_err(|e| js("blob", e))?;
        let url = create_object_url(&blob).map_err(|e| js("object URL", e))?;
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| "download failed: the page has no document".to_owned())?;
        let body = document.body().ok_or_else(|| "download failed: the page has no body".to_owned())?;
        let link = document.create_element("a").map_err(|e| js("link", e))?;
        link.set_attribute("href", &url).map_err(|e| js("link", e))?;
        link.set_attribute("download", name).map_err(|e| js("link", e))?;
        link.set_attribute("style", "display:none").map_err(|e| js("link", e))?;
        body.append_child(&link).map_err(|e| js("link", e))?;
        let clicked = link
            .clone()
            .dyn_into::<web_sys::HtmlElement>()
            .map(|a| a.click())
            .map_err(|_| "download failed: the link is not an HTML element".to_owned());
        link.remove();
        if let Some(old) = LAST_URL.with(|l| l.borrow_mut().replace(url)) {
            if let Err(e) = revoke_object_url(&old) {
                diag_warn!("{}", js("revoke", e));
            }
        }
        clicked
    }

    pub(super) fn ask_name(default: &str) -> Option<String> {
        let window = web_sys::window()?;
        match window.prompt_with_message_and_default("Save as (downloads to the browser's folder)", default) {
            Ok(answer) => answer,
            Err(e) => {
                diag_warn!("{}", js("name prompt", e));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn every_picker_and_folder_command_is_refused_by_name_in_the_browser() {
        for (action, why) in [
            (CommandKind::ImportFixtureDialog, NEEDS_PICKER),
            (CommandKind::LoadMachineDialog, NEEDS_PICKER),
            (CommandKind::SelectPostDialog, NEEDS_PICKER),
            (CommandKind::SelectThemeDialog, NEEDS_PICKER),
            (CommandKind::OpenRecent(camcraft_app::recent::RecentKind::Project, PathBuf::from("a.camproj")), NO_RECENT),
            (CommandKind::FixtureLibrarySaveDialog("S".into()), NO_LIBRARY),
            (CommandKind::FixtureLibraryInsertDialog("S".into()), NO_LIBRARY),
            (CommandKind::SetupSheetDialog, NO_EXPORT),
            (CommandKind::ExportStockDialog, NO_EXPORT),
            (CommandKind::ToolListExportDialog(None), NO_EXPORT),
        ] {
            assert_eq!(refusal(&action, false), Some(why), "{action:?}");
        }
        assert_eq!(refusal(&CommandKind::PostProgramDialog, true), Some(NO_FOLDER));
        for action in [CommandKind::SaveProjectDialog, CommandKind::SaveProjectAsDialog, CommandKind::PostProgramDialog] {
            assert_eq!(refusal(&action, false), None, "{action:?} downloads in the browser");
        }
        for action in [CommandKind::OpenModelDialog, CommandKind::OpenProjectDialog] {
            assert_eq!(refusal(&action, false), None, "{action:?} runs the browser's picker (#31)");
        }
    }

    #[test]
    fn a_download_name_is_one_clean_segment_with_its_extension() {
        assert_eq!(file_name("part", "camproj"), "part.camproj");
        assert_eq!(file_name("part.CAMPROJ", "camproj"), "part.CAMPROJ");
        assert_eq!(file_name("C:\\work\\part.camproj", "camproj"), "part.camproj");
        assert_eq!(file_name("../a/b?.nc", "nc"), "b_.nc");
        assert_eq!(file_name("  ", "nc"), "untitled.nc");
        assert_eq!(file_name("..", "nc"), "untitled.nc");
    }
}
