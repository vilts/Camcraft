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
//! #2958 adds how the viewport draws its overlay lines: their width in
//! screen points and the multisample anti-aliasing. A word this build
//! does not know reads as that setting's default (a newer build's file
//! keeps the rest of its choices).
//!
//! Provenance: our own design, specced from #645's review ask (a
//! toggle in the view settings, persisted, default on); the folder
//! resolver is `recent`'s (#662) and the atomic write
//! `machine::store`'s (#287); no HSMWorks material.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

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
    /// The viewport's display mode, as the key that the UI names it by
    /// (#2818). A missing key is the default mode, so an old file loads
    /// unchanged; the UI reads an unknown key as the default too.
    #[serde(default = "default_display")]
    pub display: String,
    /// Does the viewport show its ground grid (#2818)? Default ON: an
    /// old file keeps the grid that it always had.
    #[serde(default = "default_names_on")]
    pub show_grid: bool,
    /// Does the viewport draw the ground shadow of the model (#2834)?
    /// Default OFF: an old file keeps the view that it always had.
    #[serde(default)]
    pub ground_shadow: bool,
    /// Does the viewport darken the model faces in their corners (#2834)?
    /// Default OFF, for the same reason.
    #[serde(default)]
    pub ambient_occlusion: bool,
    /// #2958: the overlay line width. Default 1.5 pt.
    #[serde(default)]
    pub line_width: LineWidth,
    /// #2958: the multisample anti-aliasing. Default 4x.
    #[serde(default)]
    pub antialias: Antialias,
}

/// The key of the default display mode: shaded faces with their edges.
pub const DEFAULT_DISPLAY: &str = "shaded-edges";

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            show_gizmo_names: true,
            display: default_display(),
            show_grid: true,
            ground_shadow: false,
            ambient_occlusion: false,
            line_width: LineWidth::default(),
            antialias: Antialias::default(),
        }
    }
}

/// The viewport overlay line width (#2958) in screen points; the shell
/// multiplies it by the display's pixels per point. 2 pt and more turn a
/// dense path into one solid area when the view is far, thus 1.5 pt is
/// the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LineWidth {
    Thin,
    #[default]
    Normal,
    Bold,
    Heavy,
}

impl LineWidth {
    pub const ALL: [Self; 4] = [Self::Thin, Self::Normal, Self::Bold, Self::Heavy];

    #[must_use]
    pub const fn points(self) -> f32 {
        match self {
            Self::Thin => 1.0,
            Self::Normal => 1.5,
            Self::Bold => 2.0,
            Self::Heavy => 3.0,
        }
    }

    /// The file word; the menu shows it with "pt".
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Thin => "1",
            Self::Normal => "1.5",
            Self::Bold => "2",
            Self::Heavy => "3",
        }
    }
}

/// The viewport's multisample anti-aliasing (#2958). The shell builds
/// only a sample count that the graphics device offers; for any other it
/// falls back and says so.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Antialias {
    Off,
    X2,
    #[default]
    X4,
}

impl Antialias {
    pub const ALL: [Self; 3] = [Self::Off, Self::X2, Self::X4];

    /// Samples per pixel.
    #[must_use]
    pub const fn samples(self) -> u32 {
        match self {
            Self::Off => 1,
            Self::X2 => 2,
            Self::X4 => 4,
        }
    }

    /// The file word and the menu text in one.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::X2 => "2x",
            Self::X4 => "4x",
        }
    }
}

/// Write a setting as its file word.
fn put<S: Serializer>(word: &str, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(word)
}

/// Read a file word back; a word (or a type) this build does not know is
/// the default, never a refusal of the whole file.
fn take<'de, D: Deserializer<'de>, T: Copy + Default>(d: D, all: &[T], word: fn(T) -> &'static str) -> Result<T, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    let found = v.as_str().and_then(|w| all.iter().copied().find(|x| word(*x) == w));
    if found.is_none() {
        tracing::warn!("view settings: '{v}' is not known - using {}", word(T::default()));
    }
    Ok(found.unwrap_or_default())
}

impl Serialize for LineWidth {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        put(self.word(), s)
    }
}

impl<'de> Deserialize<'de> for LineWidth {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        take(d, &Self::ALL, Self::word)
    }
}

impl Serialize for Antialias {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        put(self.word(), s)
    }
}

impl<'de> Deserialize<'de> for Antialias {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        take(d, &Self::ALL, Self::word)
    }
}

const fn default_names_on() -> bool {
    true
}

fn default_display() -> String {
    DEFAULT_DISPLAY.to_owned()
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
