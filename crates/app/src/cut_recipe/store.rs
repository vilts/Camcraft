//! Where the user's cut recipe library lives and how it is edited
//! (#3073): one `recipes.camrecipes` in the user recipes folder,
//! resolved like the fixtures folder: `CAMCRAFT_RECIPES`, else
//! `<platform data dir>/camcraft/recipes`. Saving a recipe is a file
//! edit, not a document command: nothing here touches a world or the
//! undo history, and every write is all or nothing.
//!
//! Provenance: our own resolver from the platforms' published per-user
//! data locations (Windows `%APPDATA%`, macOS `~/Library/Application
//! Support`, the XDG Base Directory spec); the atomic save is the
//! write-temp-then-rename pattern from the `std::fs::rename` docs.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use super::{CutRecipe, parse, to_text};

/// The override: an absolute folder holding the library file.
pub const ENV_VAR: &str = "CAMCRAFT_RECIPES";
/// The library's file name inside the folder.
pub const LIBRARY_FILE: &str = "recipes.camrecipes";

/// The user recipes folder from an environment lookup, for the named
/// platform (`std::env::consts::OS`). Pure, so the order is testable.
/// `None` = no home to put one in.
#[must_use]
pub fn user_dir_for(os: &str, get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let abs = |k: &str| get(k).map(PathBuf::from).filter(|p| p.is_absolute());
    if let Some(dir) = abs(ENV_VAR) {
        return Some(dir);
    }
    let data = match os {
        "windows" => abs("APPDATA"),
        "macos" => abs("HOME").map(|h| h.join("Library").join("Application Support")),
        _ => abs("XDG_DATA_HOME").or_else(|| abs("HOME").map(|h| h.join(".local").join("share"))),
    }?;
    Some(data.join("camcraft").join("recipes"))
}

/// The user recipes folder for this process.
#[must_use]
pub fn user_dir() -> Option<PathBuf> {
    user_dir_for(std::env::consts::OS, |k| std::env::var_os(k))
}

/// The library file in `dir`.
#[must_use]
pub fn library_path(dir: &Path) -> PathBuf {
    dir.join(LIBRARY_FILE)
}

/// Why a library read or edit did not happen. Nothing was written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LibraryError {
    /// The file (or a recipe to add) is refused, in words.
    Refused(String),
    /// This name is already in the library: ask before replacing.
    Exists(String),
    /// No recipe of this name.
    NotFound(String),
    /// The disk refused (the OS error, in words).
    Io(String),
}

impl fmt::Display for LibraryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LibraryError::Refused(why) => write!(f, "{why}"),
            LibraryError::Exists(n) => write!(f, "cut recipe '{n}' is already in the library"),
            LibraryError::NotFound(n) => write!(f, "no cut recipe '{n}' in the library"),
            LibraryError::Io(e) => write!(f, "{e}"),
        }
    }
}

/// The library at `path`. A missing file is the empty library; an
/// unreadable or refused one is an error, never "empty".
pub fn load(path: &Path) -> Result<Vec<CutRecipe>, LibraryError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(LibraryError::Refused),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(LibraryError::Io(format!("{}: {e}", path.display()))),
    }
}

/// The recipe `name` of the library at `path`.
pub fn find(path: &Path, name: &str) -> Result<CutRecipe, LibraryError> {
    load(path)?.into_iter().find(|r| r.name == name).ok_or_else(|| LibraryError::NotFound(name.to_owned()))
}

/// Add `recipe` to the library at `path`. A name already there is
/// refused as `Exists` unless `replace`.
pub fn add(path: &Path, recipe: CutRecipe, replace: bool) -> Result<(), LibraryError> {
    let mut lib = load(path)?;
    if lib.iter().any(|r| r.name == recipe.name) {
        if !replace {
            return Err(LibraryError::Exists(recipe.name));
        }
        lib.retain(|r| r.name != recipe.name);
    }
    lib.push(recipe);
    save(path, &lib)
}

/// Write `recipes` to `path` atomically: a temp file in the same folder,
/// flushed, then renamed over the target. The folder is created when
/// missing; on any failure the temp file is removed.
fn save(path: &Path, recipes: &[CutRecipe]) -> Result<(), LibraryError> {
    use std::io::Write;
    let text = to_text(recipes).map_err(LibraryError::Refused)?;
    let io = |e: std::io::Error| LibraryError::Io(format!("{}: {e}", path.display()));
    let dir = path.parent().ok_or_else(|| LibraryError::Io(format!("{} has no folder", path.display())))?;
    std::fs::create_dir_all(dir).map_err(io)?;
    let name = path.file_name().map_or_else(|| "recipes".into(), |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let wrote = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = wrote {
        let _ = std::fs::remove_file(&tmp);
        return Err(io(e));
    }
    Ok(())
}
