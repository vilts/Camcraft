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
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewPrefs {
    pub theme: ThemeRef,
    pub color_vision: ColorVision,
    /// #889: the tool library import leaves the file's holder bodies
    /// out (the page's "Import tool holders" unticked). `false` = take them.
    pub skip_tool_holders: bool,
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
        let import_tool_holders = self.skip_tool_holders.then_some(false);
        let disk = OnDisk { format: FORMAT.to_owned(), theme, color_vision, import_tool_holders };
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
        Ok((Self { theme, color_vision, skip_tool_holders }, notes))
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
