//! View preferences (issue #645, the toggle asked on top of #727):
//! whether the viewport's origin gizmos wear their names, kept per
//! user in the OS config folder next to the recents file so the
//! viewport starts the way the user left it.
//!
//! Like the recents (#662) this is app PREFERENCE state, not document
//! state: nothing here touches the world or the command history
//! (AGENTS.md "Everything Is a Command" - flipping the checkbox is
//! view state like the camera, outside undo; unlike the camera it
//! outlives the run). A missing, unreadable or corrupt file starts at
//! the defaults - names ON, the #727 decision - and says why, never a
//! crash.
//!
//! Provenance: our own design, specced from #645's review ask (a
//! toggle in the view settings, persisted, default on); the folder
//! resolver is `recent`'s (#662) and the atomic write
//! `machine::store`'s (#287); no HSMWorks material.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::recent::config_dir_for;

/// Sibling of `recent::FILE_NAME` in the same config folder.
pub const FILE_NAME: &str = "view.json";

/// The format tag of the file we read and write.
pub const FORMAT: &str = "camcraft-view/1";

/// The viewport preferences we keep. A new preference is a new
/// `#[serde(default = ...)]` field here: old files keep reading and
/// unknown keys stay ignored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewPrefs {
    /// Do the origin gizmos wear their names (#727's names, #645's
    /// toggle)? Default ON: the names are what made several triads
    /// tell apart, so shipping the toggle must not change the viewport
    /// a user already has.
    #[serde(default = "default_names_on")]
    pub show_gizmo_names: bool,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self { show_gizmo_names: true }
    }
}

const fn default_names_on() -> bool {
    true
}

/// The file layout. Unknown keys are ignored so a newer build's file
/// still reads; a missing `view` section is the all-defaults view.
#[derive(Serialize, Deserialize)]
struct OnDisk {
    format: String,
    #[serde(default)]
    view: ViewPrefs,
}

/// This process's view file, if a config folder resolves (the same
/// folder and resolver as the recents file, #662).
#[must_use]
pub fn store_path() -> Option<PathBuf> {
    config_dir_for(std::env::consts::OS, |k| std::env::var_os(k)).map(|d| d.join(FILE_NAME))
}

/// Read the view file. No file yet is a quiet default start; an
/// unreadable or corrupt one is a default start too, with the reason
/// logged and returned so the caller can show it - never a crash.
pub fn load(path: &Path) -> (ViewPrefs, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (ViewPrefs::default(), None),
        Err(e) => {
            let why = format!("view settings not read ({}): {e} - starting with the defaults", path.display());
            tracing::warn!("{why}");
            return (ViewPrefs::default(), Some(why));
        }
    };
    let disk: OnDisk = match serde_json::from_str(&text) {
        Ok(d) => d,
        Err(e) => {
            let why = format!("view settings ignored ({}): not a view file ({e}) - starting with the defaults", path.display());
            tracing::warn!("{why}");
            return (ViewPrefs::default(), Some(why));
        }
    };
    if disk.format != FORMAT {
        let why = format!(
            "view settings ignored ({}): format '{}' is not {FORMAT} - starting with the defaults",
            path.display(),
            disk.format
        );
        tracing::warn!("{why}");
        return (ViewPrefs::default(), Some(why));
    }
    (disk.view, None)
}

/// Write the view file atomically (`machine::store`, folder created
/// when missing): a crash mid-write leaves the previous file whole. A
/// refusal is logged here and returned.
pub fn save(path: &Path, prefs: &ViewPrefs) -> Result<(), String> {
    let disk = OnDisk { format: FORMAT.to_owned(), view: prefs.clone() };
    let text = serde_json::to_string_pretty(&disk)
        .map_err(|e| format!("view settings not serialised: {e}"))?;
    super::machine::store::write_atomic(path, &text).map_err(|e| {
        let why = e.to_string();
        tracing::warn!("view settings {why} ({})", path.display());
        why
    })
}
