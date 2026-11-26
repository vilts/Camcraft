//! Native CAD formats and the one table that says which of them an import
//! may convert to STEP (#1873). A native file is never parsed here: a
//! qualified extension goes to the external converter (`cadmpeg`), and
//! each other native extension is refused by its own name.
//!
//! Provenance: our own design, specced from issue #1873. The extension
//! list is from cadmpeg's public crates.io description (read for #1572).

use std::path::Path;

/// The extensions that cadmpeg's public description lists as inputs, in
/// lower case. An import refuses each of them by name while it is not in
/// [`QUALIFIED`]. `prt` is both NX and Creo: a container-only format.
pub const KNOWN: &[&str] =
    &["iges", "igs", "sldprt", "f3d", "fcstd", "ipt", "iam", "3dm", "catpart", "prt", "sat", "sab"];

/// The shared qualified-extension table, in lower case. It is empty until
/// the format qualification (#1875) proves, with real files, that a format
/// converts to a solid that the STEP importer loads whole. The import
/// command, the file dialogs and the MCP tool read this one table.
pub const QUALIFIED: &[&str] = &[];

/// Where an import of a file goes, by its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Not a native CAD extension: the importers of this crate decide.
    Direct,
    /// A qualified native format: converted to STEP, then imported.
    Converted,
    /// A native format that is not qualified: refused by name.
    Unqualified,
}

/// [`route_in`] over [`QUALIFIED`].
#[must_use]
pub fn route(path: &Path) -> Route {
    route_in(QUALIFIED, path)
}

/// The route of `path` under `table` (lower-case extensions). The letter
/// case of the file's extension does not matter.
#[must_use]
pub fn route_in(table: &[&str], path: &Path) -> Route {
    let ext = extension(path);
    if ext.is_empty() {
        Route::Direct
    } else if table.contains(&ext.as_str()) {
        Route::Converted
    } else if KNOWN.contains(&ext.as_str()) {
        Route::Unqualified
    } else {
        Route::Direct
    }
}

/// The extension of `path` in lower case; empty when it has none.
#[must_use]
pub fn extension(path: &Path) -> String {
    path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default()
}
