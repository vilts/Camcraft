//! View preferences (issue #877): the theme and the colour-vision mode
//! belong to the person at the screen, not to the part, so they are
//! kept per user in the OS config folder beside the recent-files list
//! and survive restarts; no project file carries them.
//!
//! The values live in the world as the `ThemeRef` / `ColorVision`
//! singletons the shell reads every frame; this module only reads and
//! writes the file. A missing file is a quiet default start; an
//! unreadable or corrupt one, or a value this build does not know, is a
//! default start too, with the reason returned for the status line -
//! never a crash. A theme FILE reference is kept as written even when
//! the file is gone (a network drive may come back): the shell refuses
//! it by name at draw and keeps the current look, as before.
//!
//! #889 adds the Tool library page's "Import tool holders" choice: the
//! person's habit for their own machine, not the part's, so it is kept
//! here too (written only when off; absent reads as on).
//!
//! #1945 adds the AI Chat's "Offline only" switch: the person's policy
//! for their own machine, kept here (written only when on). API keys are
//! never kept here - they live in the OS keychain or the environment
//! (`app::ai::secret`), and this file has no field that could hold one.
//!
//! #1873 adds the optional path of the external `cadmpeg` program that
//! converts native CAD files to STEP: where a program is installed is a
//! fact of this machine, not of the part (written only when set).
//!
//! Provenance: our own design, specced from issue #877; the folder and
//! the atomic write are `app::recent`'s (#662).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::schema::{ColorVision, ThemeRef};

/// The preferences file's name inside the config folder.
pub const FILE_NAME: &str = "prefs.json";
/// The format tag the file carries; any other tag is refused.
pub const FORMAT: &str = "camcraft-prefs/1";

/// What the user chose for the screen.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewPrefs {
    pub theme: ThemeRef,
    pub color_vision: ColorVision,
    /// #889: the tool library import leaves the file's holder bodies
    /// out (the page's "Import tool holders" unticked). `false` = take them.
    pub skip_tool_holders: bool,
    /// #1945: the AI Chat reaches loopback model runtimes only.
    pub ai_offline_only: bool,
    /// #1873: where the `cadmpeg` program is, when it is not on `PATH`.
    /// `None` searches `PATH`.
    pub cadmpeg_path: Option<String>,
    /// #2428: check GitHub for a newer release at startup (at most once/day).
    pub check_for_updates: bool,
    /// #2428: UTC Unix timestamp (seconds) of the last startup update check.
    pub last_update_check_utc: Option<u64>,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            theme: ThemeRef::Dark,
            color_vision: ColorVision::Off,
            skip_tool_holders: false,
            ai_offline_only: false,
            cadmpeg_path: None,
            check_for_updates: true,
            last_update_check_utc: None,
        }
    }
}

/// The theme as the file spells it: a built-in name or a path, never one
/// string standing for both (the old project file's `ThemeDto` shape).
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ThemeOnDisk {
    Named(String),
    File(String),
}

/// The file layout. Unknown keys are ignored so a newer build's file
/// still reads; an absent key is that preference's default.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OnDisk {
    format: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    theme: Option<ThemeOnDisk>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color_vision: Option<String>,
    /// #889: written only when false (holders left out of an import).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    import_tool_holders: Option<bool>,
    /// #1945: written only when true (offline-only AI chat).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ai_offline_only: Option<bool>,
    /// #1873: written only when set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cadmpeg_path: Option<String>,
    /// #2428: written only when false (update checks default to on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    check_for_updates: Option<bool>,
    /// #2428: written only when set (epoch seconds).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_update_check_utc: Option<serde_json::Value>,
}

fn vision_word(c: ColorVision) -> &'static str {
    match c {
        ColorVision::Off => "off",
        ColorVision::Protan => "protan",
        ColorVision::Deutan => "deutan",
        ColorVision::Tritan => "tritan",
    }
}

impl ViewPrefs {
    /// The file text: our own `camcraft-prefs/1` JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        let theme = match &self.theme {
            ThemeRef::Dark => None,
            ThemeRef::Named(n) => Some(ThemeOnDisk::Named(n.clone())),
            ThemeRef::File(p) => Some(ThemeOnDisk::File(p.clone())),
        };
        let color_vision =
            (self.color_vision != ColorVision::Off).then(|| vision_word(self.color_vision).to_owned());
        let skip_tool_holders = self.skip_tool_holders;
        let import_tool_holders = skip_tool_holders.then_some(false);
        let ai_offline_only = self.ai_offline_only.then_some(true);
        let cadmpeg_path = self.cadmpeg_path.clone();
        let check_for_updates = (!self.check_for_updates).then_some(false);
        let last_update_check_utc = self.last_update_check_utc.map(serde_json::Value::from);
        let disk = OnDisk {
            format: FORMAT.to_owned(),
            theme,
            color_vision,
            import_tool_holders,
            ai_offline_only,
            cadmpeg_path,
            check_for_updates,
            last_update_check_utc,
        };
        serde_json::to_string_pretty(&disk).unwrap_or_else(|e| {
            tracing::warn!("preferences not serialised: {e}");
            String::new()
        })
    }

    /// Read the file text. A wrong format tag or broken JSON refuses the
    /// whole file; a theme name or colour-vision word this build does not
    /// know falls back to that preference's default, named in the notes.
    pub fn from_json(text: &str) -> Result<(Self, Vec<String>), String> {
        let disk: OnDisk = serde_json::from_str(text).map_err(|e| format!("not a preferences file ({e})"))?;
        if disk.format != FORMAT {
            return Err(format!("format '{}' is not {FORMAT}", disk.format));
        }
        let mut notes = Vec::new();
        let theme = match disk.theme {
            None => ThemeRef::Dark,
            Some(ThemeOnDisk::File(p)) => ThemeRef::File(p),
            Some(ThemeOnDisk::Named(n)) if n == "dark" => ThemeRef::Dark,
            Some(ThemeOnDisk::Named(n)) if crate::commands::theme::BUILTINS.contains(&n.as_str()) => {
                ThemeRef::Named(n)
            }
            Some(ThemeOnDisk::Named(n)) => {
                notes.push(format!("theme '{n}' is not a built-in - using dark"));
                ThemeRef::Dark
            }
        };
        let color_vision = match disk.color_vision.as_deref() {
            None => ColorVision::Off,
            Some(w) => match ColorVision::ALL.into_iter().find(|c| vision_word(*c) == w) {
                Some(c) => c,
                None => {
                    notes.push(format!("colour-vision mode '{w}' is not known - using off"));
                    ColorVision::Off
                }
            },
        };
        let skip_tool_holders = disk.import_tool_holders == Some(false);
        let ai_offline_only = disk.ai_offline_only == Some(true);
        // #1873: a blank path is "not set", never a program named "".
        let cadmpeg_path = disk.cadmpeg_path.filter(|p| !p.trim().is_empty());
        let check_for_updates = disk.check_for_updates != Some(false);
        let last_update_check_utc = match disk.last_update_check_utc {
            None | Some(serde_json::Value::Null) => None,
            Some(v) => match v.as_u64() {
                Some(ts) => Some(ts),
                None => {
                    notes.push(String::from("lastUpdateCheckUtc is not a UTC epoch integer - ignored"));
                    None
                }
            },
        };
        Ok((
            Self {
                theme,
                color_vision,
                skip_tool_holders,
                ai_offline_only,
                cadmpeg_path,
                check_for_updates,
                last_update_check_utc,
            },
            notes,
        ))
    }
}

/// This process's preferences file, if a config folder resolves.
#[must_use]
pub fn store_path() -> Option<PathBuf> {
    crate::recent::config_dir_for(std::env::consts::OS, |k| std::env::var_os(k)).map(|d| d.join(FILE_NAME))
}

/// Read the preferences file. No file yet is a quiet default start; an
/// unreadable or corrupt one is a default start with the reason logged
/// and returned, and a known-bad value keeps the rest of the file.
pub fn load(path: &Path) -> (ViewPrefs, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (ViewPrefs::default(), None),
        Err(e) => {
            let why = format!("preferences not read ({}): {e} - using the defaults", path.display());
            tracing::warn!("{why}");
            return (ViewPrefs::default(), Some(why));
        }
    };
    match ViewPrefs::from_json(&text) {
        Ok((p, notes)) if notes.is_empty() => (p, None),
        Ok((p, notes)) => {
            let why = format!("preferences ({}): {}", path.display(), notes.join("; "));
            tracing::warn!("{why}");
            (p, Some(why))
        }
        Err(e) => {
            let why = format!("preferences ignored ({}): {e} - using the defaults", path.display());
            tracing::warn!("{why}");
            (ViewPrefs::default(), Some(why))
        }
    }
}

/// Write the preferences file atomically (temp file, then rename): a
/// crash mid-write leaves the previous file whole. A refusal is logged
/// here and returned.
pub fn save(path: &Path, prefs: &ViewPrefs) -> Result<(), String> {
    crate::machine::store::write_atomic(path, &prefs.to_json()).map_err(|e| {
        let why = e.to_string();
        tracing::warn!("preferences {why} ({})", path.display());
        why
    })
}

pub use crate::prefs_setups::{
    PanelSetups, SETUP_NAME_MAX, SETUPS_FILE_NAME, SETUPS_FORMAT, SavedSetup, load_setups,
    save_setups, setup_name, setups_path,
};
