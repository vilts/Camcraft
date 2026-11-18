//! The shipped data directories (post scripts, machine files) read from
//! disk at RUN time, with one cached read per file (#2282).
//!
//! Each release package ships `assets/<kind>/` beside the executable
//! (`cargo xtask release` `ASSET_DIRS`, `scripts/package-*.sh`; the
//! v0.22.0 Windows zip holds `assets/{machines,posts,scripts,themes,ui}`
//! beside `camcraft-ui.exe`). The lookup is a candidate walk, and the
//! first existing directory wins for the whole kind (files are never
//! mixed from two directories):
//!
//! 1. `<exe dir>/assets/<kind>` - the zip, the installer, the Linux
//!    tarball, deb/rpm (`/usr/lib/camcraft`), AppImage, flatpak, snap;
//! 2. `<app>/Contents/Resources/assets/<kind>` - the macOS app bundle;
//! 3. `<profile>/../../assets/<kind>` - a dev run from `target/<profile>/`;
//! 4. `<this crate>/../../assets/<kind>` - the source tree, for the test
//!    binaries in `target/*/deps/` (a path, not a compiled-in copy).
//!
//! Candidates 3 and 4 apply only when the exe is in a cargo build tree
//! of the old or the new cargo build-dir layout ([`cargo_profile_dir`]).
//! An installed exe without its `assets/<kind>` folder
//! is refused by name: it never reads the build machine's checkout, nor
//! `/usr/assets` or `C:\assets`. There is no per-user copy: it could
//! shadow the shipped files with no warning (review of #2293).
//!
//! No directory found, or a file missing or unreadable in it, is an
//! `Err` that names the file and the directory - never a panic, never a
//! silent substitute. The browser build has no disk: its callers keep a
//! compiled-in copy for `wasm32` only.
//!
//! Provenance: our own resolver, specced from the package layouts of
//! this repository and cargo's documented build-directory layout.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The largest data file read (a hostile-input cap): the biggest shipped
/// post script is far below it.
pub const MAX_BYTES: u64 = 4 * 1024 * 1024;

/// The profile folder of a cargo build tree that holds `exe_dir`, or
/// `None` for an installed exe. Cargo has two layouts (#2316):
///
/// - old: the exe is in `target/<profile>` or its `deps` / `examples`
///   child, and the profile folder holds cargo's `.fingerprint`;
/// - new build-dir layout: a test or example exe is in
///   `target/<profile>/build/<pkg>/<hash>/out`, the uplifted binaries
///   stay in `target/<profile>`, and there is no `.fingerprint` there.
///   The profile folder then holds `build/camcraft-post`, the unit folder
///   of this crate, which every exe that reads data files links.
pub fn cargo_profile_dir<'a>(exe_dir: &'a Path, is_dir: &dyn Fn(&Path) -> bool) -> Option<&'a Path> {
    let name = |p: &Path| p.file_name().and_then(std::ffi::OsStr::to_str).map(str::to_owned);
    let profile = match name(exe_dir).as_deref() {
        Some("deps" | "examples") => exe_dir.parent()?,
        Some("out") => {
            // out -> <hash> -> <pkg> -> build -> <profile>
            let build = exe_dir.parent()?.parent()?.parent()?;
            if name(build).as_deref() == Some("build") { build.parent()? } else { exe_dir }
        }
        _ => exe_dir,
    };
    let in_tree =
        is_dir(&profile.join(".fingerprint")) || is_dir(&profile.join("build").join(env!("CARGO_PKG_NAME")));
    in_tree.then_some(profile)
}

/// The directory candidates for one kind (`posts`, `machines`) in
/// precedence order (see the module doc). `is_dir` decides if the exe is
/// in a cargo build tree, which alone allows the dev candidates.
#[must_use]
pub fn candidates(kind: &str, exe: Option<&Path>, manifest: Option<&Path>, is_dir: &dyn Fn(&Path) -> bool) -> Vec<PathBuf> {
    let mut out = Vec::with_capacity(4);
    let exe_dir = exe.and_then(Path::parent);
    if let Some(dir) = exe_dir {
        out.push(dir.join("assets").join(kind));
        if dir.file_name().is_some_and(|n| n == std::ffi::OsStr::new("MacOS"))
            && let Some(contents) = dir.parent()
        {
            out.push(contents.join("Resources").join("assets").join(kind));
        }
    }
    let Some(profile) = exe_dir.and_then(|d| cargo_profile_dir(d, is_dir)) else {
        return out;
    };
    if let Some(repo) = profile.parent().and_then(Path::parent) {
        out.push(repo.join("assets").join(kind));
    }
    if let Some(repo) = manifest.and_then(Path::parent).and_then(Path::parent) {
        out.push(repo.join("assets").join(kind));
    }
    out
}

/// The first candidate `is_dir` accepts, or a sentence that names each
/// place looked at.
pub fn resolve(kind: &str, exe: Option<&Path>, manifest: Option<&Path>, is_dir: impl Fn(&Path) -> bool) -> Result<PathBuf, String> {
    let all = candidates(kind, exe, manifest, &is_dir);
    if let Some(dir) = all.iter().find(|p| is_dir(p)) {
        return Ok(dir.clone());
    }
    let looked: Vec<String> = all.iter().map(|p| p.display().to_string()).collect();
    Err(format!(
        "no {kind} folder found - the program needs its assets/{kind} directory beside the executable (looked in: {})",
        if looked.is_empty() { "nowhere: no executable path".to_owned() } else { looked.join(", ") }
    ))
}

/// One kind's directory and its read cache. A file is read once; later
/// calls (and failures) come from the cache, so no caller reads per frame.
#[derive(Debug)]
pub struct DataFiles {
    dir: Result<PathBuf, String>,
    cache: Mutex<HashMap<String, Result<Arc<str>, String>>>,
}

impl DataFiles {
    /// Files of `dir`, or the reason there is no directory.
    #[must_use]
    pub fn new(dir: Result<PathBuf, String>) -> Self {
        Self { dir, cache: Mutex::new(HashMap::new()) }
    }

    /// [`resolve`] wired to the running process: the real exe path, and
    /// this crate's source tree as the last resort of a cargo build tree.
    #[must_use]
    pub fn from_process(kind: &str) -> Self {
        let exe = std::env::current_exe().ok();
        let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
        Self::new(resolve(kind, exe.as_deref(), Some(manifest), Path::is_dir))
    }

    /// The resolved directory, or why none.
    pub fn dir(&self) -> Result<&Path, String> {
        self.dir.as_deref().map_err(Clone::clone)
    }

    /// The full path of `file` in the directory.
    pub fn path_of(&self, file: &str) -> Result<PathBuf, String> {
        self.dir().map(|d| d.join(file))
    }

    /// The text of `file`, read on first use and cached. The error names
    /// the file and the directory.
    pub fn read(&self, file: &str) -> Result<Arc<str>, String> {
        let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(hit) = cache.get(file) {
            return hit.clone();
        }
        let got = self.dir().and_then(|dir| read_file(dir, file)).map(Arc::from);
        cache.insert(file.to_owned(), got.clone());
        got
    }
}

/// One uncached read of `file` in `dir`: capped at [`MAX_BYTES`], UTF-8
/// only, and every error names the file and the directory.
pub fn read_file(dir: &Path, file: &str) -> Result<String, String> {
    let path = dir.join(file);
    let where_ = || format!("{file} in {}", dir.display());
    let f = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", where_()))?;
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(f, MAX_BYTES + 1), &mut buf)
        .map_err(|e| format!("{}: read failed: {e}", where_()))?;
    if buf.len() as u64 > MAX_BYTES {
        return Err(format!("{}: over the {MAX_BYTES}-byte limit - refused", where_()));
    }
    let text = String::from_utf8(buf).map_err(|_| format!("{}: not UTF-8 text", where_()))?;
    Ok(text)
}

#[cfg(test)]
#[path = "data_dir_tests.rs"]
mod tests;
