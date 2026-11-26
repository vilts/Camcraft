//! Open model / Open project where the file arrives as bytes (#31): the
//! browser's async picker (`crate::file_pick`) delivers a name and the
//! file's contents, and they run through the same commands a file opened
//! from disk runs - `ImportModel` (one undo step) for a model, the project
//! loader for a `.camproj`. A file the browser could not read, or one the
//! importer refuses, is said on the status line by name; nothing changes.
//! Native builds keep their synchronous dialogs; the inbox they drain each
//! frame stays empty there.
//! Provenance: our own routing over the app's import and project laws.

use std::path::{Path, PathBuf};

use camcraft_app::clock::Instant;
use camcraft_app::commands::ImportModel;
use camcraft_app::schema::PostRef;
use camcraft_app::{project, session_files};

use super::{CommandKind, State};
use crate::file_pick::{PickFor, PickedFile, SETUP_EXTENSIONS};

impl State {
    /// File > Open model: rfd's dialog natively; in a browser, the async
    /// picker - its answer comes back through [`State::take_picked_files`].
    pub(crate) fn open_model_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = super::mesh_dialog().pick_file() {
            self.import_path(path);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = crate::file_pick::chooser_status(PickFor::Model, self.picker_in_click);
            crate::file_pick::open(&super::mesh_dialog(), PickFor::Model);
        }
    }

    /// File > Open project, after the unsaved-changes prompt: as above.
    pub(crate) fn open_project_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = super::project_dialog().pick_file() {
            self.open_project_path(path);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = crate::file_pick::chooser_status(PickFor::Project, self.picker_in_click);
            crate::file_pick::open(&super::project_dialog(), PickFor::Project);
        }
    }

    /// Run the queue now, inside the browser's click, when it holds an
    /// action that opens a file chooser (#31). Safari opens a chooser
    /// only while the user's click is being handled; the shell's usual
    /// drain runs at the start of the NEXT frame, from
    /// `requestAnimationFrame`, where Safari ignores the request without
    /// a word. eframe runs the frame that sees a mouse release inside its
    /// `pointerup` handler, so the web shell calls this at the end of that
    /// frame (`in_click`); anything else waits for the usual drain.
    /// Returns whether it ran anything.
    pub fn run_click_actions(&mut self, in_click: bool) -> bool {
        if !in_click || !self.pending.iter().any(opens_a_chooser) {
            return false;
        }
        self.picker_in_click = true;
        while let Some(action) = self.pending.pop_front() {
            self.run(action);
        }
        self.picker_in_click = false;
        true
    }

    /// Queue every file the picker delivered since the last frame as one
    /// `OpenPicked` command each, so it runs where every action runs. The
    /// files of one folder pick (or one drop of several) land together,
    /// with one status line that names each refusal (#2950).
    pub(crate) fn take_picked_files(&mut self) {
        let mut batch = Vec::new();
        for file in crate::file_pick::take_picked() {
            if file.purpose == PickFor::Folder {
                batch.push((file.name, file.bytes));
            } else {
                self.run(CommandKind::OpenPicked(file));
            }
        }
        if !batch.is_empty() {
            self.add_setup_files(batch);
        }
    }

    /// Land one picked file, or refuse it by name.
    pub(crate) fn open_picked(&mut self, file: PickedFile) {
        let PickedFile { purpose, name, bytes } = file;
        if purpose == PickFor::Folder {
            self.add_setup_files(vec![(name, bytes)]);
            return;
        }
        let data = match bytes {
            Ok(data) => data,
            Err(why) => {
                self.status = match purpose {
                    PickFor::Model => format!("import refused: {name} could not be read: {why}"),
                    PickFor::Project => format!("open failed: {name} could not be read: {why}"),
                    PickFor::Machine => format!("machine not loaded: {name} could not be read: {why}"),
                    PickFor::Post | PickFor::Folder => format!("post not selected: {name} could not be read: {why}"),
                };
                return;
            }
        };
        match purpose {
            PickFor::Model => self.import_bytes(&name, &data),
            PickFor::Project => self.open_project_bytes(&name, &data),
            // #2950: kept for the session by name, then mounted or chosen
            // through the same commands a file from disk runs.
            PickFor::Machine => match add_setup_file(&name, &data, &["mch"]) {
                Ok(()) => {
                    self.machines_pane.list = None;
                    self.load_machine_path(PathBuf::from(&name));
                },
                Err(why) => self.status = format!("machine not loaded: {why}"),
            },
            PickFor::Post | PickFor::Folder => match add_setup_file(&name, &data, &["cps"]) {
                Ok(()) => self.select_post(PostRef::File(name)),
                Err(why) => self.status = format!("post not selected: {why}"),
            },
        }
    }

    /// Load machine (#2950): rfd's dialog natively; in a browser, the
    /// async picker - the file lands through [`State::open_picked`].
    pub(crate) fn load_machine_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dir = crate::egui_app::machines_pane::user_dir(self);
            if let Some(path) = super::pick_machine_file(dir.as_deref()) {
                self.load_machine_path(path);
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = crate::file_pick::chooser_status(PickFor::Machine, self.picker_in_click);
            crate::file_pick::open(&machine_dialog(), PickFor::Machine);
        }
    }

    /// Load post (#2950): as above, for a `.cps`.
    pub(crate) fn select_post_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = super::pick_post_file() {
            self.select_post(PostRef::File(path.display().to_string()));
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = crate::file_pick::chooser_status(PickFor::Post, self.picker_in_click);
            crate::file_pick::open(&post_dialog(), PickFor::Post);
        }
    }

    /// Load folder (#2950): the browser's folder picker. The desktop
    /// reads its machine and post folders itself, so it says so.
    pub(crate) fn load_setup_folder_dialog(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.status = String::from(NO_FOLDER_ON_DESKTOP);
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.status = crate::file_pick::chooser_status(PickFor::Folder, self.picker_in_click);
            crate::file_pick::open(&setup_folder_dialog(), PickFor::Folder);
        }
    }

    /// Add machine and post files to the session's lists without a mount
    /// or a choice (a folder pick, a drop of several files): one status
    /// line that counts what landed and names each refusal.
    pub(crate) fn add_setup_files(&mut self, files: Vec<(String, Result<Vec<u8>, String>)>) {
        let mut added = 0usize;
        let mut refused = Vec::new();
        for (name, bytes) in files {
            let landed = bytes
                .map_err(|why| format!("{name} could not be read: {why}"))
                .and_then(|data| add_setup_file(&name, &data, SETUP_EXTENSIONS));
            match landed {
                Ok(()) => {
                    added += 1;
                    if Path::new(&name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("mch")) {
                        self.machines_pane.list = None;
                    }
                },
                Err(why) => refused.push(why),
            }
        }
        let plural = if added == 1 { "" } else { "s" };
        self.status = if refused.is_empty() {
            format!("added {added} file{plural} to the machine and post lists")
        } else {
            format!("added {added} file{plural} to the machine and post lists; refused: {}", refused.join("; "))
        };
    }

    /// The bytes twin of `import_path`: parsed here, on this thread (a
    /// browser has no worker to hand it to), then landed by
    /// `finish_import` like any import - fresh, update or refusal.
    fn import_bytes(&mut self, name: &str, data: &[u8]) {
        let budget = self.step_budget_for(Path::new(name));
        let result = ImportModel::from_bytes(name, data, budget);
        // A disk import may be in flight on native: its file, clock and
        // fixture binding are its own, so they are set aside around ours.
        let held = (self.import_file.take(), self.import_started.take(), self.import_as_fixture.take());
        self.import_started = Some(Instant::now());
        self.finish_import(result);
        (self.import_file, self.import_started, self.import_as_fixture) = held;
    }

    /// The bytes twin of `open_project_path`. The document has no path
    /// afterwards (a browser file has none), so Save asks where.
    fn open_project_bytes(&mut self, name: &str, data: &[u8]) {
        let Ok(text) = std::str::from_utf8(data) else {
            self.status = format!("open failed: {name} is not a CamCraft project (not text)");
            return;
        };
        let loaded = camcraft_app::world::new_world();
        match project::open_noted(&loaded, text) {
            Ok(notes) => {
                self.reset_for_open(loaded);
                self.history.mark_clean();
                self.doc_version += 1;
                self.status = if notes.is_empty() {
                    format!("opened {name}")
                } else {
                    format!("opened {name} - {}", notes.join("; "))
                };
            }
            Err(err) => self.status = format!("open failed: {name}: {err}"),
        }
    }
}

/// What Load folder says on the desktop, which has no folder picker for it.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const NO_FOLDER_ON_DESKTOP: &str =
    "Load folder is for the browser: the desktop app lists its machine and post folders itself";

/// Keep one machine or post file for the session, checked as a file from
/// disk is checked; the error names the file and why. `allowed` is the
/// extensions this door takes (a machine picker takes no post).
fn add_setup_file(name: &str, data: &[u8], allowed: &[&str]) -> Result<(), String> {
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    if !allowed.contains(&ext.as_str()) {
        let what = match allowed {
            ["mch"] => "a machine (.mch) file",
            ["cps"] => "a post (.cps) file",
            _ => "a machine (.mch) or post (.cps) file",
        };
        return Err(format!("{name} is not {what}"));
    }
    let text = std::str::from_utf8(data).map_err(|_| format!("{name} is not text"))?;
    if ext == "mch" { session_files::add_machine(name, text, |text| {
        if let Some(first) = camcraft_app::machine::validate::validate_text(text).first() {
            return Err(format!("fails validation - {first}"));
        }
        camcraft_app::machine::parse_mch(text).map_err(|e| format!("is not a machine file: {e}"))
    }).map(drop) } else { session_files::add_post(name, text) }
}

#[cfg(target_arch = "wasm32")]
fn machine_dialog() -> crate::file_dialog::FileDialog {
    crate::file_dialog::FileDialog::new().add_filter("Machine Definition", &["mch"])
}

#[cfg(target_arch = "wasm32")]
fn post_dialog() -> crate::file_dialog::FileDialog {
    crate::file_dialog::FileDialog::new().add_filter("Post Script", &["cps"])
}

#[cfg(target_arch = "wasm32")]
fn setup_folder_dialog() -> crate::file_dialog::FileDialog {
    crate::file_dialog::FileDialog::new().add_filter("Machines and posts", SETUP_EXTENSIONS)
}

/// The actions that may open the browser's file chooser: the Open and
/// Load dialogs, and the unsaved-changes answer that lets a held Open
/// project go on. They must run inside the click that asked for them.
#[must_use]
pub(crate) fn opens_a_chooser(action: &CommandKind) -> bool {
    matches!(
        action,
        CommandKind::OpenModelDialog
            | CommandKind::OpenProjectDialog
            | CommandKind::ResolveUnsaved(_)
            | CommandKind::LoadMachineDialog
            | CommandKind::SelectPostDialog
            | CommandKind::LoadSetupFolderDialog
    )
}
