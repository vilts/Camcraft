//! The disk stamp of a project file (#3608): its size and modification
//! time when the document was opened from it or last saved to it. A
//! different stamp at the next save to that file means that the file on
//! disk changed in between: a second window, a `git pull`, a colleague on
//! a shared folder, or a sync client wrote it. The window asks before it
//! writes over such a file; a script or an agent is refused by name unless
//! it gives `overwrite` (`script::save_guard`).
//!
//! It sits beside `save` and `load` because it describes the same file
//! that they write and read, and every caller of the guard already uses
//! this module. The stamp reads no file content, so it costs the same for
//! a large project as for a small one. The linked-model watch keeps the
//! same pair (`ui::model_link_watch`).
//!
//! Provenance: our own helper over std file metadata.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A project file's size and modification time, and the file they are of.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiskStamp {
    path: PathBuf,
    len: u64,
    /// `None` where the file system keeps no modification time.
    modified: Option<SystemTime>,
}

impl DiskStamp {
    /// The stamp of the file at `path` now. `None`: no file is there, or
    /// it cannot be read (always `None` in the browser, which has no
    /// files). A symbolic link is followed.
    #[must_use]
    pub fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path)
            .ok()
            .filter(std::fs::Metadata::is_file)?;
        Some(Self {
            path: path.to_path_buf(),
            len: meta.len(),
            modified: meta.modified().ok(),
        })
    }

    /// The file that this stamp is of.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// A save to `path` would write over a change that is not ours: `path`
    /// is this stamp's file, and its size or modification time is
    /// different now. A file that is gone has not changed: the save puts
    /// it back and removes no other work. A save to another path is not
    /// this stamp's concern.
    #[must_use]
    pub fn changed(&self, path: &Path) -> bool {
        self.path == path
            && Self::of(path)
                .is_some_and(|now| now.len != self.len || now.modified != self.modified)
    }
}
