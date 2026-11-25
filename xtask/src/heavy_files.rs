//! The file-level part of the heavy reach (#3560): a changed source file that only the crate's own unit
//! tests or its own binaries compile reaches only those targets, not the lanes of the crates above it.
//! Provenance: our own build law (AGENTS.md "Heavy lanes at review"); the module and target rules of
//! the public Rust reference (`#[cfg(test)]`, `mod` file lookup) and the Cargo target layout.

use std::path::{Path, PathBuf};

/// Who can compile a changed file `crates/<d>/src/<rel>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Seen {
    /// A module whose declaration (or an ancestor's) is `#[cfg(test)]`: only `d:lib` compiles it.
    UnitTestsOnly(String),
    /// `src/main.rs` or a file under `src/bin/`: only `d:bins` and the integration tests of `d`
    /// (they can run the binary through `CARGO_BIN_EXE_<name>`).
    BinsOnly(String),
    /// Anything else, or anything this module cannot prove: the crate reach (every dependent lane).
    Library,
}

/// Classifies `rel` (the path under `crates/<d>/src/`, `/`-separated). Every doubt answers `Library`:
/// an unreadable file, a module declaration that is missing, inline or found more than once, a
/// `#[path]` attribute or an `include!` that names the file. A narrower answer must be a proof.
/// `UnitTestsOnly` also needs that no other compile root can name the file: each `.rs` file of the
/// crate directory (`src/bin`, `tests/`, `benches/`, `examples/` included) is read for it.
pub fn seen_by(root: &Path, d: &str, rel: &str) -> Seen {
    let krate = root.join("crates").join(d);
    let src = krate.join("src");
    let Some(files) = rust_files(&krate) else { return Seen::Library };
    if !rel.ends_with(".rs") || custom_target_paths(root, d) || named_by_path_or_include(&files, rel) {
        return Seen::Library;
    }
    if rel == "main.rs" || rel.starts_with("bin/") {
        return Seen::BinsOnly(format!("{rel} is built only into the binaries of {d}"));
    }
    if rel == "lib.rs" || rel == "build.rs" {
        return Seen::Library;
    }
    let mut segs: Vec<&str> = rel.trim_end_matches(".rs").split('/').collect();
    if segs.last() == Some(&"mod") {
        segs.pop();
    }
    if segs.is_empty() {
        return Seen::Library;
    }
    for i in 0..segs.len() {
        let Some(parent) = parent_file(&src, &segs[..i]) else { return Seen::Library };
        let Ok(text) = std::fs::read_to_string(&parent) else { return Seen::Library };
        match decl_is_test_only(&text, segs[i]) {
            Some(true) => {
                if declared_elsewhere(&krate, &src, &files, &segs) {
                    return Seen::Library;
                }
                let m = segs[..=i].join("::");
                return Seen::UnitTestsOnly(format!("module {m} is #[cfg(test)]: only the unit tests of {d} build it"));
            }
            Some(false) => {}
            None => return Seen::Library,
        }
    }
    Seen::Library
}

/// The file that declares the module at `path` (empty: the crate root `lib.rs`).
fn parent_file(src: &Path, path: &[&str]) -> Option<PathBuf> {
    if path.is_empty() {
        let lib = src.join("lib.rs");
        return lib.is_file().then_some(lib);
    }
    let dir: PathBuf = path.iter().collect();
    let flat = src.join(&dir).with_extension("rs");
    let nested = src.join(&dir).join("mod.rs");
    match (flat.is_file(), nested.is_file()) {
        (true, false) => Some(flat),
        (false, true) => Some(nested),
        _ => None, // none, or both (a build error): no proof
    }
}

/// `Some(true)` when every declaration `mod <name>;` in `text` carries `#[cfg(test)]`, `Some(false)` when
/// one has no such attribute, `None` when there is no `mod <name>;` line, an inline `mod <name> {`, or a
/// `#[path]` on the declaration.
fn decl_is_test_only(text: &str, name: &str) -> Option<bool> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = false;
    let mut all_test = true;
    for (i, line) in lines.iter().enumerate() {
        let Some(after) = mod_name(line) else { continue };
        let Some(rest) = after.strip_prefix(name) else { continue };
        if rest.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
            continue; // `mod foo_bar` is not `mod foo`
        }
        if line.starts_with(char::is_whitespace) {
            return None; // indented: inside an inline module or a block this reader does not follow
        }
        let rest = rest.trim_start();
        if !rest.starts_with(';') {
            return None; // inline module or a form this reader does not follow
        }
        found = true;
        let mut test = false;
        for prev in lines[..i].iter().rev().map(|l| l.trim()) {
            if prev.is_empty() || prev.starts_with("//") {
                continue;
            }
            if !prev.starts_with("#[") {
                break;
            }
            if prev.contains("path") {
                return None;
            }
            test |= prev.replace(' ', "") == "#[cfg(test)]";
        }
        all_test &= test;
    }
    found.then_some(all_test)
}

/// The text after `mod ` on a declaration line (with an optional `pub`, `pub(crate)`, ... before it).
fn mod_name(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let t = match t.strip_prefix("pub") {
        Some(r) if r.starts_with('(') => r.split_once(')')?.1.trim_start(),
        Some(r) if r.starts_with(' ') => r.trim_start(),
        Some(_) => return None,
        None => t,
    };
    t.strip_prefix("mod ").map(str::trim_start)
}

/// True when the crate manifest sets a target `path` (`[lib]`, `[[bin]]`, ...) or cannot be read: then
/// `src/lib.rs` and `src/main.rs` are not proven to be the target roots.
fn custom_target_paths(root: &Path, d: &str) -> bool {
    let manifest = root.join("crates").join(d).join("Cargo.toml");
    std::fs::read_to_string(manifest).map_or(true, |t| t.lines().any(|l| l.trim_start().starts_with("path")))
}

/// Every `.rs` file under the crate directory, or `None` when a directory or entry cannot be read.
fn rust_files(krate: &Path) -> Option<Vec<(PathBuf, String)>> {
    let mut out = Vec::new();
    let mut stack = vec![krate.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).ok()? {
            let p = e.ok()?.path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "target") {
                    stack.push(p);
                }
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).ok()?;
                out.push((p, text));
            }
        }
    }
    Some(out)
}

/// True when a line of a source file of the crate names the changed file as a path part (in an
/// `include!`, a `#[path]` attribute split over lines, or any other string): then the file can be built
/// where the module walk does not look.
fn named_by_path_or_include(files: &[(PathBuf, String)], rel: &str) -> bool {
    let file = rel.rsplit('/').next().unwrap_or(rel);
    // The name as a whole path part: `"x.rs`, `/x.rs` or `\x.rs` (so `air_x.rs` does not name `x.rs`).
    let names = |l: &str| ['"', '/', '\\'].iter().any(|c| l.contains(&format!("{c}{file}")));
    files.iter().any(|(_, text)| text.lines().any(names))
}

/// True when a file other than the proven parent declares `mod <seg>;` for a segment of `segs` and that
/// declaration can load the same file: a second compile root (`main.rs`, `src/bin`, `tests/`, ...) or a
/// second module tree. An indented declaration (inside an inline module) always counts.
fn declared_elsewhere(krate: &Path, src: &Path, files: &[(PathBuf, String)], segs: &[&str]) -> bool {
    for (f, text) in files {
        for line in text.lines() {
            let Some(after) = mod_name(line) else { continue };
            for (j, seg) in segs.iter().enumerate() {
                let Some(rest) = after.strip_prefix(seg) else { continue };
                if !rest.trim_start().starts_with(';') {
                    continue; // another name (`mod segx;`) or an inline module, which loads no file
                }
                if line.starts_with(char::is_whitespace) {
                    return true;
                }
                if parent_file(src, &segs[..j]).as_deref() == Some(f.as_path()) {
                    continue; // the declaration that the walk proved
                }
                let want: PathBuf = segs[..j].iter().fold(src.to_path_buf(), |p, s| p.join(s));
                if child_dir(krate, f) == want {
                    return true;
                }
            }
        }
    }
    false
}

/// The directory in which `mod x;` of file `f` looks for `x.rs`: the file's own directory for a crate
/// root or a `mod.rs`, else the directory named after the file.
fn child_dir(krate: &Path, f: &Path) -> PathBuf {
    let dir = f.parent().map(Path::to_path_buf).unwrap_or_default();
    let name = f.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let roots = [krate.join("src").join("bin"), krate.join("tests"), krate.join("benches"), krate.join("examples")];
    if matches!(name, "lib.rs" | "main.rs" | "mod.rs" | "build.rs") || roots.contains(&dir) {
        dir
    } else {
        f.with_extension("")
    }
}
