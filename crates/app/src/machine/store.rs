//! Where the user's machine files live and how one is written (issue
//! #383): the user machines folder, resolved per the #287 decision, and
//! the validated, atomic save.
//!
//! Provenance: our own resolver, specced from the decision recorded on
//! #287 (`CAMCRAFT_MACHINES`, else `<platform data dir>/camcraft/machines`)
//! and the platforms' published per-user data locations (Windows
//! `%APPDATA%`, macOS `~/Library/Application Support`, the XDG Base
//! Directory spec's `$XDG_DATA_HOME` / `~/.local/share`); the atomic save
//! is the write-temp-then-rename pattern from the `std::fs::rename` docs.
//!
//! A save is all or nothing: the text is validated first and a refused
//! file is never written; a good one goes to a temp file beside the
//! target and is renamed over it, so a crash mid-write leaves the old
//! file whole, never half a machine.

use std::ffi::OsString;
use std::fmt;
use std::path::{Path, PathBuf};

use super::validate::{validate_text, Refusal};

/// The override: an absolute folder of user `.mch` files.
pub const ENV_VAR: &str = "CAMCRAFT_MACHINES";

/// The user machines folder from an environment lookup, for the named
/// platform (`std::env::consts::OS`). Pure, so the order is testable
/// without touching the process environment. `None` = no home to put
/// one in (the pane then lists the shipped library only, and Save As
/// says why it cannot save).
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
    Some(data.join("camcraft").join("machines"))
}

/// The user machines folder for this process.
#[must_use]
pub fn user_dir() -> Option<PathBuf> {
    user_dir_for(std::env::consts::OS, |k| std::env::var_os(k))
}

/// Why a save did not happen. Nothing was written in every case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// The validator refused the file; each refusal by name.
    Refused(Vec<Refusal>),
    /// A shipped machine, or one never saved: it needs Save As.
    NeedsSaveAs,
    /// Save As would replace this file; ask before replacing.
    Exists(PathBuf),
    /// No user machines folder could be resolved.
    NoFolder,
    /// The file name is not a plain name.
    BadName(String),
    /// The disk refused (the OS error, in words).
    Io(String),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::Refused(r) => {
                write!(f, "not saved - {} refusal{}", r.len(), if r.len() == 1 { "" } else { "s" })
            }
            SaveError::NeedsSaveAs => write!(f, "a shipped or new machine is saved with Save As"),
            SaveError::Exists(p) => write!(f, "{} already exists", p.display()),
            SaveError::NoFolder => {
                write!(f, "no user machines folder (set {ENV_VAR} or a home folder)")
            }
            SaveError::BadName(why) => write!(f, "file name {why}"),
            SaveError::Io(e) => write!(f, "not saved - {e}"),
        }
    }
}

/// A Save As name made into a `.mch` file name in `dir`: refused when
/// empty or when it names a folder (`/`, `\`, `..`), so a save can only
/// ever land in the user machines folder.
pub fn file_in(dir: &Path, name: &str) -> Result<PathBuf, SaveError> {
    let n = name.trim();
    if n.is_empty() {
        return Err(SaveError::BadName("is empty".to_owned()));
    }
    if n.contains(['/', '\\', ':']) || n.starts_with('.') {
        return Err(SaveError::BadName(format!("'{n}' must be a plain name, not a path")));
    }
    let file = if n.to_ascii_lowercase().ends_with(".mch") { n.to_owned() } else { format!("{n}.mch") };
    Ok(dir.join(file))
}

/// The temp file that [`write_atomic`] writes before its rename: a hidden
/// file beside `path`, named for the target, this process and the calling
/// thread. Two saves at the same moment run on two threads (an autosave
/// and a save by the user), so they never share a temp file; the saves of
/// one thread run one after another, so its name can repeat. `None` when
/// `path` names no file.
#[must_use]
pub fn temp_path_for(path: &Path) -> Option<PathBuf> {
    let dir = path.parent()?;
    let name = path.file_name()?.to_string_lossy().into_owned();
    Some(dir.join(format!(".{name}.{}.{}.tmp", std::process::id(), thread_number())))
}

/// A number for the calling thread, taken from one counter the first time
/// the thread asks and kept for the thread's life: different threads of
/// this process never get the same number.
fn thread_number() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(1);
    thread_local! {
        static NUMBER: usize = NEXT.fetch_add(1, Ordering::Relaxed);
    }
    NUMBER.with(|n| *n)
}

/// Write `text` to `path` atomically: a temp file in the same folder,
/// flushed, then renamed over the target. The folder is created when
/// missing. On any failure the temp file is removed and the target is
/// as it was. Machine files, preferences, recent files, view settings
/// and project files all save through here.
pub fn write_atomic(path: &Path, text: &str) -> Result<(), SaveError> {
    use std::io::Write;
    let io = |e: std::io::Error| SaveError::Io(e.to_string());
    let dir = path.parent().ok_or_else(|| SaveError::Io(format!("{} has no folder", path.display())))?;
    let tmp = temp_path_for(path).ok_or_else(|| SaveError::Io(format!("{} names no file", path.display())))?;
    std::fs::create_dir_all(dir).map_err(io)?;
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

/// Validate, then write atomically. A refused file is not written.
pub fn save_text(path: &Path, text: &str) -> Result<(), SaveError> {
    let refusals = validate_text(text);
    if !refusals.is_empty() {
        return Err(SaveError::Refused(refusals));
    }
    write_atomic(path, text)
}
