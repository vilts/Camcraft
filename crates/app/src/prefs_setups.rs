//! Saved panel setups (#2339): per-user dock arrangements stored in
//! `panel-setups.json` beside `prefs.json`, split from `prefs.rs` along the
//! domain seam for the 400-line law.
//!
//! Provenance: our own design specced from issue #2339; the folder and
//! atomic write are `app::recent`'s (#662).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The saved panel-setup file's name inside the config folder. It is a
/// file of its own: a corrupt or older setup never takes the theme and
/// colour-vision preferences down with it.
pub const SETUPS_FILE_NAME: &str = "panel-setups.json";
/// The format tag the setups file carries; any other tag is refused.
pub const SETUPS_FORMAT: &str = "camcraft-panel-setups/1";
/// The longest setup name the store keeps, in characters.
pub const SETUP_NAME_MAX: usize = 48;

/// One named arrangement: the dock's own JSON, kept opaque here. The
/// surface owns what a dock looks like (splits, tab order, a float rect);
/// this module only keeps the value, writes it and gives it back.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedSetup {
    pub name: String,
    pub dock: serde_json::Value,
}

/// The person's saved panel setups (#2339): which panels are open, where
/// each is attached and how. This is the screen's, not the part's, so it
/// lives beside the theme and colour-vision preferences and never in the
/// project file.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelSetups {
    /// Show `last` at the next start.
    pub auto_load: bool,
    /// The setup the user saved or loaded most recently.
    pub last: Option<String>,
    /// In the order the user saved them.
    pub setups: Vec<SavedSetup>,
}

impl Default for PanelSetups {
    fn default() -> Self {
        Self { auto_load: true, last: None, setups: Vec::new() }
    }
}

/// The setups as the file spells them. A dock this module cannot judge is
/// kept as it reads; the surface refuses it by name at the load.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetupsOnDisk {
    format: String,
    /// Written only when off; absent reads as on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    auto_load: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last: Option<String>,
    #[serde(default)]
    setups: Vec<SavedOnDisk>,
}

#[derive(Serialize, Deserialize)]
struct SavedOnDisk {
    name: String,
    dock: serde_json::Value,
}

/// Trim and check a setup name as the user typed it. The refusal is one
/// sentence and reads on the status line.
pub fn setup_name(raw: &str) -> Result<&str, String> {
    let name = raw.trim();
    if name.is_empty() {
        return Err(String::from("the setup needs a name"));
    }
    if name.chars().count() > SETUP_NAME_MAX {
        return Err(format!("the setup name is longer than {SETUP_NAME_MAX} characters"));
    }
    if name.chars().any(char::is_control) {
        return Err(String::from("the setup name has a control character"));
    }
    Ok(name)
}

impl PanelSetups {
    /// The file text: our own `camcraft-panel-setups/1` JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let disk = SetupsOnDisk {
            format: SETUPS_FORMAT.to_owned(),
            auto_load: (!self.auto_load).then_some(false),
            last: self.last.clone(),
            setups: self
                .setups
                .iter()
                .map(|s| SavedOnDisk { name: s.name.clone(), dock: s.dock.clone() })
                .collect(),
        };
        serde_json::to_string_pretty(&disk).unwrap_or_else(|e| {
            tracing::warn!("panel setups not serialised: {e}");
            String::new()
        })
    }

    /// Read the file text. A wrong format tag or broken JSON refuses the
    /// whole file. A setup with a bad name, a dock that is not an object,
    /// or a name that stands twice is dropped, named in the notes.
    pub fn from_json(text: &str) -> Result<(Self, Vec<String>), String> {
        let disk: SetupsOnDisk =
            serde_json::from_str(text).map_err(|e| format!("not a panel setup file ({e})"))?;
        if disk.format != SETUPS_FORMAT {
            return Err(format!("format '{}' is not {SETUPS_FORMAT}", disk.format));
        }
        let mut notes = Vec::new();
        let mut setups: Vec<SavedSetup> = Vec::new();
        for raw in disk.setups {
            let name = match setup_name(&raw.name) {
                Ok(n) => n.to_owned(),
                Err(why) => {
                    notes.push(format!("setup '{}' is dropped: {why}", raw.name));
                    continue;
                }
            };
            if !raw.dock.is_object() {
                notes.push(format!("setup '{name}' is dropped: its arrangement is not a saved dock"));
                continue;
            }
            if setups.iter().any(|s| s.name == name) {
                notes.push(format!("setup '{name}' stands twice - the second one is dropped"));
                continue;
            }
            setups.push(SavedSetup { name, dock: raw.dock });
        }
        let auto_load = disk.auto_load.unwrap_or(true);
        let last = match disk.last.as_deref().map(setup_name) {
            None => None,
            Some(Ok(n)) if setups.iter().any(|s| s.name == n) => Some(n.to_owned()),
            Some(Ok(n)) => {
                notes.push(format!("the last setup '{n}' is not in the file - starting with the default"));
                None
            }
            Some(Err(why)) => {
                notes.push(format!("the last setup name is dropped: {why}"));
                None
            }
        };
        Ok((Self { auto_load, last, setups }, notes))
    }

    /// The setup under `name`, if it is saved.
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&SavedSetup> {
        self.setups.iter().find(|s| s.name == name)
    }

    /// Save an arrangement under `name`, replacing one of that name, and
    /// remember it as the last one used.
    pub fn upsert(&mut self, name: &str, dock: serde_json::Value) {
        match self.setups.iter_mut().find(|s| s.name == name) {
            Some(saved) => saved.dock = dock,
            None => self.setups.push(SavedSetup { name: name.to_owned(), dock }),
        }
        self.last = Some(name.to_owned());
    }

    /// Forget one setup. The last-used name is forgotten with it.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.setups.len();
        self.setups.retain(|s| s.name != name);
        let removed = self.setups.len() != before;
        if removed && self.last.as_deref() == Some(name) {
            self.last = None;
        }
        removed
    }
}

/// This process's saved panel setups file, if a config folder resolves.
#[must_use]
pub fn setups_path() -> Option<PathBuf> {
    crate::recent::config_dir_for(std::env::consts::OS, |k| std::env::var_os(k))
        .map(|d| d.join(SETUPS_FILE_NAME))
}

/// Read the saved panel setups. No file yet is a quiet empty store; an
/// unreadable or corrupt one is an empty store with the reason logged and
/// returned - never a crash.
pub fn load_setups(path: &Path) -> (PanelSetups, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return (PanelSetups::default(), None);
        }
        Err(e) => {
            let why = format!("panel setups not read ({}): {e} - the defaults are used", path.display());
            tracing::warn!("{why}");
            return (PanelSetups::default(), Some(why));
        }
    };
    match PanelSetups::from_json(&text) {
        Ok((p, notes)) if notes.is_empty() => (p, None),
        Ok((p, notes)) => {
            let why = format!("panel setups ({}): {}", path.display(), notes.join("; "));
            tracing::warn!("{why}");
            (p, Some(why))
        }
        Err(e) => {
            let why = format!("panel setups ignored ({}): {e} - the defaults are used", path.display());
            tracing::warn!("{why}");
            (PanelSetups::default(), Some(why))
        }
    }
}

/// Write the saved panel setups atomically, like the preferences. A
/// refusal is logged here and returned.
pub fn save_setups(path: &Path, setups: &PanelSetups) -> Result<(), String> {
    crate::machine::store::write_atomic(path, &setups.to_json()).map_err(|e| {
        let why = e.to_string();
        tracing::warn!("panel setups {why} ({})", path.display());
        why
    })
}
