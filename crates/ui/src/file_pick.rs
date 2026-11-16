//! The browser's file picker (#31): opening a model or a project where no
//! blocking native dialog exists. A browser only offers an async
//! `<input type=file>`: the shell asks for a file, returns at once, and the
//! file's name and bytes arrive frames later. They land in an inbox here;
//! `State::drain_actions` takes them each frame and runs them through the
//! same commands an import or open from disk runs (`state_picked`).
//!
//! The inbox and the `accept` filter are plain Rust on every target, so the
//! bytes-to-document path is pinned by native tests. Only `open` (the DOM
//! input and the awaited read) is wasm32. Native builds never fill the
//! inbox: their dialogs stay rfd's synchronous ones (`file_dialog`).
//! Provenance: our own seam; the DOM API (`HTMLInputElement.files`,
//! `accept`, `Blob.arrayBuffer`) from the WHATWG HTML and File API specs.

use std::cell::RefCell;
use std::collections::VecDeque;

use crate::file_dialog::FileDialog;

/// The largest file the browser build reads into memory, in bytes. A wasm32
/// module has at most 4 GiB of memory and an import needs several times
/// its file, so a bigger file is refused by name before it is read - an
/// allocation that fails in wasm aborts the tab.
pub const BROWSER_READ_LIMIT: u64 = 256 << 20;

/// What a picker was opened for; its answer is routed by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickFor {
    /// A mesh to import (STL, OBJ, STEP) - the Open model dialog.
    Model,
    /// A `.camproj` to open in place of the document.
    Project,
}

/// One file the picker answered with: its name and contents, or why the
/// browser could not read it.
pub struct PickedFile {
    pub purpose: PickFor,
    /// The file's name as the browser gives it (no folder).
    pub name: String,
    pub bytes: Result<Vec<u8>, String>,
}

impl std::fmt::Debug for PickedFile {
    // The bytes can be hundreds of megabytes: say their length only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = match &self.bytes {
            Ok(b) => format!("{} bytes", b.len()),
            Err(why) => format!("unread: {why}"),
        };
        f.debug_struct("PickedFile")
            .field("purpose", &self.purpose)
            .field("name", &self.name)
            .field("bytes", &bytes)
            .finish()
    }
}

thread_local! {
    // The browser runs the shell on one thread; a native test thread has
    // its own inbox, so tests never see each other's files.
    static INBOX: RefCell<VecDeque<PickedFile>> = const { RefCell::new(VecDeque::new()) };
}

/// Hand one picked file to the shell; the next frame takes it.
pub fn deliver(file: PickedFile) {
    INBOX.with(|inbox| match inbox.try_borrow_mut() {
        Ok(mut inbox) => inbox.push_back(file),
        // Only a re-entrant call could hold it; say so, never drop silently.
        Err(_) => diag_warn!("file picker: {} arrived while the inbox was busy - pick it again", file.name),
    });
}

/// Every file picked since the last call, oldest first.
#[must_use]
pub fn take_picked() -> Vec<PickedFile> {
    INBOX.with(|inbox| inbox.try_borrow_mut().map(|mut i| i.drain(..).collect()).unwrap_or_default())
}

/// The `<input accept>` value for a dialog's filters: every extension,
/// dotted and lower-case, in filter order without repeats
/// (".stl,.obj,.step,.stp"). Empty when the dialog has no filter, which
/// the browser reads as "any file".
#[must_use]
pub fn accept_attr(dialog: &FileDialog) -> String {
    let mut seen: Vec<String> = Vec::new();
    for ext in dialog.filters.iter().flat_map(|(_, exts)| exts) {
        let ext = ext.trim().trim_start_matches('.').to_ascii_lowercase();
        if !ext.is_empty() && !seen.iter().any(|s| s.strip_prefix('.') == Some(ext.as_str())) {
            seen.push(format!(".{ext}"));
        }
    }
    seen.join(",")
}

/// The status line while the chooser is up. `in_click` says the chooser
/// was asked for inside the user's click; outside one (a keyboard
/// shortcut, which eframe hands to the app on a later frame) Safari
/// refuses to open it without a word, so the line says what to do
/// instead of leaving a silent no-op.
#[must_use]
pub fn chooser_status(purpose: PickFor, in_click: bool) -> String {
    let (ask, button) = match purpose {
        PickFor::Model => ("choose a model to import…", "File > Model"),
        PickFor::Project => ("choose a project to open…", "File > Open"),
    };
    if in_click {
        ask.to_string()
    } else {
        format!(
            "{ask} if no file chooser opened, this browser (Safari) opens one only from a click - click {button} instead of the shortcut"
        )
    }
}

/// Why a file of `size` bytes is not read, or `None` when it may be.
#[must_use]
pub fn too_big(name: &str, size: f64) -> Option<String> {
    // 256 MiB is exact in an f64.
    let limit = BROWSER_READ_LIMIT as f64;
    (!size.is_finite() || size > limit).then(|| {
        format!(
            "{name} is {:.0} MiB, over the browser build's {} MiB read limit - open it in the desktop app",
            size / 1_048_576.0,
            BROWSER_READ_LIMIT >> 20
        )
    })
}

#[cfg(target_arch = "wasm32")]
pub use web::{open, set_repaint};

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;

    use eframe::web_sys;
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast as _, JsValue};

    use super::{PickFor, PickedFile, accept_attr, deliver, too_big};
    use crate::file_dialog::FileDialog;

    /// The open picker: its input and the change handler the input calls.
    /// Held, never leaked: a cancelled picker never fires, so it stays
    /// here until the next `open` replaces it - at most one at a time.
    struct Pending {
        input: web_sys::HtmlInputElement,
        _on_change: Closure<dyn FnMut()>,
    }

    thread_local! {
        static PENDING: RefCell<Option<Pending>> = const { RefCell::new(None) };
        static REPAINT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
    }

    /// The shell's egui context: a file that arrives wakes the next frame.
    pub fn set_repaint(ctx: &egui::Context) {
        REPAINT.with(|r| {
            if let Ok(mut r) = r.try_borrow_mut() {
                *r = Some(ctx.clone());
            }
        });
    }

    fn repaint() {
        REPAINT.with(|r| {
            if let Ok(r) = r.try_borrow()
                && let Some(ctx) = r.as_ref()
            {
                ctx.request_repaint();
            }
        });
    }

    /// Ask the browser for one file; returns at once. The file (or why it
    /// could not be read) reaches the inbox when the user has chosen;
    /// a cancelled picker delivers nothing, as a cancelled dialog does.
    pub fn open(dialog: &FileDialog, purpose: PickFor) {
        if let Err(why) = try_open(dialog, purpose) {
            let name = dialog.title.clone().unwrap_or_else(|| "the file".to_string());
            diag_warn!("file picker: {why}");
            deliver(PickedFile { purpose, name, bytes: Err(why) });
            repaint();
        }
    }

    fn try_open(dialog: &FileDialog, purpose: PickFor) -> Result<(), String> {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .ok_or_else(|| "this page has no document to open a file picker in".to_string())?;
        let input = document
            .create_element("input")
            .map_err(|e| said("the page would not make a file input", &e))?
            .dyn_into::<web_sys::HtmlInputElement>()
            .map_err(|_| "the page's <input> is not an input element".to_string())?;
        input.set_type("file");
        input.set_accept(&accept_attr(dialog));
        // In the page, hidden: the MDN pattern (a `display:none` input
        // whose `click()` runs from the user's click handler). A detached
        // input is not relied on to fire `change` in every browser.
        input
            .set_attribute("style", "display:none")
            .map_err(|e| said("the page would not hide the file input", &e))?;
        document
            .body()
            .ok_or_else(|| "this page has no <body> to hold a file input".to_string())?
            .append_child(&input)
            .map_err(|e| said("the page would not take the file input", &e))?;
        let held = input.clone();
        let on_change = Closure::<dyn FnMut()>::new(move || {
            if let Some(file) = held.files().and_then(|list| list.get(0)) {
                wasm_bindgen_futures::spawn_local(read(file, purpose));
            }
        });
        input.set_onchange(Some(on_change.as_ref().unchecked_ref()));
        let parked = PENDING.with(|p| match p.try_borrow_mut() {
            Ok(mut p) => {
                // Replacing an earlier (cancelled) picker frees its handler
                // and takes its input out of the page.
                if let Some(old) = p.replace(Pending { input: input.clone(), _on_change: on_change }) {
                    old.input.remove();
                }
                true
            }
            Err(_) => {
                input.remove();
                false
            }
        });
        if !parked {
            return Err("a file picker is already being set up - try again".to_string());
        }
        input.click();
        Ok(())
    }

    async fn read(file: web_sys::File, purpose: PickFor) {
        let name = file.name();
        let bytes = match too_big(&name, file.size()) {
            Some(why) => Err(why),
            None => wasm_bindgen_futures::JsFuture::from(file.array_buffer())
                .await
                .map(|buf| web_sys::js_sys::Uint8Array::new(&buf).to_vec())
                .map_err(|e| said("the browser could not read it", &e)),
        };
        if let Err(why) = &bytes {
            diag_warn!("file picker: {name}: {why}");
        }
        deliver(PickedFile { purpose, name, bytes });
        repaint();
    }

    fn said(what: &str, err: &JsValue) -> String {
        format!("{what} ({})", err.as_string().unwrap_or_else(|| format!("{err:?}")))
    }
}
