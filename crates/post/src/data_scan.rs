//! A data folder as it is now (#2380): which names of a name-to-file table
//! have their file in the folder, and which other files of the same
//! extension lie in it. The table only says which name goes with which
//! file; a list built from the table alone shows a removed file and hides
//! an added one.
//!
//! Provenance: our own rule, specced from issue #2380 (the list follows the
//! folder at start and at each refresh); no outside source.
//!
//! A scan reads names only - it opens no file - so a malformed file cannot
//! break it or hide another file; the entry that shows it later carries
//! its own refusal.

use crate::data_dir::list_files;
use std::path::{Path, PathBuf};

/// One folder as read: the table names that have a file, and the other files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderScan {
    /// Table names whose file is in the folder, in table order.
    pub present: Vec<&'static str>,
    /// Files the table does not name: (list name = file stem, path), by file name.
    pub extra: Vec<(String, PathBuf)>,
    /// Why the folder did not read, when it did not (the lists stay empty).
    pub problem: Option<String>,
    /// Entries left out (a name that is not UTF-8), for the caller's log.
    pub skipped: Vec<String>,
}

/// Scan `dir` for `ext` files against `table` (name, file name). A file in
/// `hidden` is neither a table entry nor an extra (for example a fallback
/// file that is not a list entry). File names compare case-insensitively.
/// Never panics, never opens a file.
#[must_use]
pub fn scan_folder<F: AsRef<str>>(dir: &Path, ext: &str, table: &[(&'static str, F)], hidden: &[F]) -> FolderScan {
    let list = match list_files(dir, ext) {
        Ok(l) => l,
        Err(why) => return FolderScan { problem: Some(why), ..FolderScan::default() },
    };
    let has = |n: &str| list.names.iter().any(|f| f.eq_ignore_ascii_case(n));
    let present = table.iter().filter(|(_, f)| has(f.as_ref())).map(|(n, _)| *n).collect();
    let known = |n: &str| table.iter().any(|(_, f)| f.as_ref().eq_ignore_ascii_case(n)) || hidden.iter().any(|h| h.as_ref().eq_ignore_ascii_case(n));
    let extra = list
        .names
        .iter()
        .filter(|n| !known(n))
        .map(|n| (Path::new(n).file_stem().and_then(std::ffi::OsStr::to_str).unwrap_or(n).to_owned(), dir.join(n)))
        .collect();
    FolderScan { present, extra, problem: None, skipped: list.skipped }
}
