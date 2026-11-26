//! A project open read-only (#3609): a save to the project file that it
//! came from is refused on every save route (the window, the MCP tool
//! `save_project`, the script call `cam.save`). A save to a different file
//! is allowed; the caller then ends read-only for that file. It sits with
//! `save` and its other refusals (a missing folder, a file that this
//! process cannot write).
//! Provenance: our own design and code. The behaviour matches SolidCraft's
//! `--read-only` (SolidCraft #20830); no SolidCraft code or text is used.

use std::path::Path;

/// Refuse a save to `target` when the document is open read-only from the
/// project file `opened`. `Ok` when the document is not read-only, has no
/// file, or `target` is a different file (Save As). The reason names the
/// file.
pub fn refuse_read_only_save(opened: Option<&Path>, read_only: bool, target: &Path) -> Result<(), String> {
    match opened {
        Some(own) if read_only && same_file(own, target) => {
            tracing::warn!(file = %crate::logging::file_name(target), "project not saved: open read-only");
            Err(format!("{} is open read-only; write your copy to a different file", target.display()))
        }
        _ => Ok(()),
    }
}

/// Whether two paths name one file: the same spelling, or the same file
/// once both are made absolute and their links are followed. The Save As
/// dialog gives an absolute path for a file opened by a relative one. A
/// path that does not exist is compared by its spelling only.
fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((std::fs::canonicalize(a), std::fs::canonicalize(b)), (Ok(x), Ok(y)) if x == y)
}
