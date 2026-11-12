//! The file-dialog seam (#31, M13.3): every open/save/folder picker in the
//! UI goes through `FileDialog` here instead of naming `rfd::FileDialog`.
//! Native builds forward to rfd's synchronous dialog, unchanged. A
//! wasm32 build has no synchronous dialog (the browser only offers an
//! async `<input type=file>`), so there the pickers answer `None` - the
//! same "cancelled" the user gets from closing the dialog - and log why,
//! until the browser shell brings an async picker.
//! Provenance: our own seam; rfd's builder API from its public docs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use camcraft_app::clock::Instant;

/// Microseconds the UI thread spent waiting on a native dialog since the
/// last [`take_waited`] (#866): the dialogs are modal and synchronous, so
/// the user's time browsing for a file would otherwise read as a frozen
/// "commands" phase in the frame watch.
static WAITED_US: AtomicU64 = AtomicU64::new(0);

/// Run one modal dialog, adding its open time to [`WAITED_US`].
fn timed<T>(open: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = open();
    let us = u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX);
    // Drained every frame: u64 microseconds wrap only after ~584k years.
    WAITED_US.fetch_add(us.min(u64::MAX / 2), Ordering::Relaxed);
    out
}

/// The time spent in modal dialogs since the last call, and reset it.
/// The frame watch takes it off the frame it happened in (#866).
#[must_use]
pub fn take_waited() -> Duration {
    Duration::from_micros(WAITED_US.swap(0, Ordering::Relaxed))
}

/// The picker's settings, kept on every target so the call sites (and
/// the tests) see one shape whatever the platform does with it.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FileDialog {
    pub(crate) title: Option<String>,
    pub(crate) filters: Vec<(String, Vec<String>)>,
    pub(crate) directory: Option<PathBuf>,
    pub(crate) file_name: Option<String>,
}

impl FileDialog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn set_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    #[must_use]
    pub fn add_filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
        self.filters
            .push((name.into(), extensions.iter().map(ToString::to_string).collect()));
        self
    }

    #[must_use]
    pub fn set_directory(mut self, dir: impl AsRef<Path>) -> Self {
        self.directory = Some(dir.as_ref().to_path_buf());
        self
    }

    #[must_use]
    pub fn set_file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// Open one existing file; `None` when cancelled (or no dialog exists).
    #[must_use]
    pub fn pick_file(self) -> Option<PathBuf> {
        timed(|| imp::pick_file(self))
    }

    /// Choose a file to write; `None` when cancelled (or no dialog exists).
    #[must_use]
    pub fn save_file(self) -> Option<PathBuf> {
        timed(|| imp::save_file(self))
    }

    /// Choose a folder; `None` when cancelled (or no dialog exists).
    #[must_use]
    pub fn pick_folder(self) -> Option<PathBuf> {
        timed(|| imp::pick_folder(self))
    }
}

// The shell's window, published once it exists: rfd's dialogs take it as
// their owner, so a picker opens in front of the app instead of behind it.
// The window is `!Send`, so this is a thread-local rather than a `OnceLock`
// - every picker runs on the thread that owns it.
#[cfg(not(target_arch = "wasm32"))]
thread_local! {
    static PARENT: std::cell::RefCell<Option<std::sync::Arc<winit::window::Window>>> =
        const { std::cell::RefCell::new(None) };
}

/// Publish the window the native dialogs belong to, once it exists. Later
/// calls are ignored: the shell opens one window.
#[cfg(not(target_arch = "wasm32"))]
pub fn set_parent(window: &std::sync::Arc<winit::window::Window>) {
    PARENT.with(|p| {
        let mut p = p.borrow_mut();
        if p.is_none() {
            *p = Some(window.clone());
        }
    });
}

/// The published window, if the shell has opened one on this thread.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn parent() -> Option<std::sync::Arc<winit::window::Window>> {
    PARENT.with(|p| p.borrow().clone())
}

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::FileDialog;
    use std::path::PathBuf;

    fn native(d: FileDialog) -> rfd::FileDialog {
        let mut out = rfd::FileDialog::new();
        if let Some(title) = d.title {
            out = out.set_title(title);
        }
        for (name, exts) in &d.filters {
            out = out.add_filter(name.as_str(), exts);
        }
        if let Some(dir) = d.directory {
            out = out.set_directory(dir);
        }
        if let Some(name) = d.file_name {
            out = out.set_file_name(name);
        }
        // #1067: an unowned dialog can open behind the app window, and an
        // owned one still follows an owner that is not the foreground window
        // (a shell launched from a terminal). Raise the owner first, then
        // hand the dialog to it.
        if let Some(window) = super::parent() {
            window.focus_window();
            out = out.set_parent(&*window);
        }
        out
    }

    pub(super) fn pick_file(d: FileDialog) -> Option<PathBuf> {
        native(d).pick_file()
    }

    pub(super) fn save_file(d: FileDialog) -> Option<PathBuf> {
        native(d).save_file()
    }

    pub(super) fn pick_folder(d: FileDialog) -> Option<PathBuf> {
        native(d).pick_folder()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::FileDialog;
    use std::path::PathBuf;

    fn unavailable(what: &str, d: &FileDialog) -> Option<PathBuf> {
        diag_warn!(
            "{what} dialog ({}) is not available in the browser build yet (#31): treated as cancelled",
            d.title.as_deref().unwrap_or("untitled")
        );
        None
    }

    pub(super) fn pick_file(d: FileDialog) -> Option<PathBuf> {
        unavailable("open", &d)
    }

    pub(super) fn save_file(d: FileDialog) -> Option<PathBuf> {
        unavailable("save", &d)
    }

    pub(super) fn pick_folder(d: FileDialog) -> Option<PathBuf> {
        unavailable("folder", &d)
    }
}

#[cfg(test)]
mod tests {
    use super::FileDialog;
    use std::path::PathBuf;

    #[test]
    fn file_dialog_builder_keeps_every_setting() {
        let d = FileDialog::new()
            .set_title("Post one file per operation into")
            .add_filter("CSV", &["csv"])
            .add_filter("Text", &["txt", "text"])
            .set_directory("some/dir")
            .set_file_name("part-tools.csv");
        assert_eq!(d.title.as_deref(), Some("Post one file per operation into"));
        assert_eq!(
            d.filters,
            vec![
                ("CSV".to_owned(), vec!["csv".to_owned()]),
                ("Text".to_owned(), vec!["txt".to_owned(), "text".to_owned()]),
            ]
        );
        assert_eq!(d.directory, Some(PathBuf::from("some/dir")));
        assert_eq!(d.file_name.as_deref(), Some("part-tools.csv"));
    }
}
