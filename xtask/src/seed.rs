//! Seeds a new worktree's `target/debug` from another checkout of the same repository (#2575), so cargo
//! finds the dependency units (solidcore, the build-script crates) Fresh instead of compiling them again.
//! Provenance: our own build tooling (#900, #2575); Cargo's target layouts (deps/, .fingerprint/, and build/<package>/<hash>/).
//!
//! Why not sccache: its Rust key hashes the `OUT_DIR` value and the cwd, and both name the worktree, so a
//! build-script crate misses in each new worktree and every crate above it misses too. Cargo's own
//! fingerprints do not name the worktree, so a copied unit reads as Fresh. Only units of packages with a
//! registry or git source are copied: a local package (a workspace member or a path dependency) has its
//! sources in the checkout, and an older copy of them could read as Fresh with other code.

use std::fs::{self, File, TryLockError};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::json::Json;

/// The marker in `target/debug` of a seeded worktree: it is seeded once, never again.
pub const MARKER: &str = ".camcraft-seeded";
/// The unit directories, in copy order: the outputs first, the fingerprints last, so that a copy that
/// stops half way leaves outputs without a fingerprint (cargo builds them again), never the reverse.
const PARTS: [&str; 3] = ["build", "deps", ".fingerprint"];
/// Cargo's locks of a target profile directory; a build in progress holds them.
const LOCKS: [&str; 2] = [".cargo-lock", ".cargo-build-lock"];

/// The names of the packages that have no `source` in `cargo metadata` (workspace members and path
/// dependencies). Their sources differ between checkouts, so their units are never copied.
pub fn local_packages(metadata: &Json) -> Vec<String> {
    let Some(packages) = metadata.get("packages") else { return Vec::new() };
    packages
        .items()
        .iter()
        .filter(|p| matches!(p.get("source"), None | Some(Json::Null)))
        .filter_map(|p| p.text("name").map(str::to_string))
        .collect()
}

/// True when the unit entry `entry` (`<name>-<hash>`, `lib<crate>-<hash>.rlib`, ...) can belong to a
/// local package. It matches by prefix and may match too much: that only costs a compile.
pub fn is_local(entry: &str, local: &[String]) -> bool {
    let bare = entry.strip_prefix("lib");
    local.iter().any(|name| {
        let snake = name.replace('-', "_");
        [name.as_str(), snake.as_str()].iter().any(|n| {
            let head = format!("{n}-");
            *n == entry || entry.starts_with(&head) || bare.is_some_and(|b| b.starts_with(&head))
        })
    })
}

/// Counts legacy fingerprints and the units in `build/<package>/<hash>/fingerprint`.
fn units(checkout: &Path) -> usize {
    let debug = checkout.join("target").join("debug");
    let legacy = fs::read_dir(debug.join(".fingerprint")).map(|d| d.count()).unwrap_or(0);
    let nested = fs::read_dir(debug.join("build"))
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .flat_map(|package| fs::read_dir(package.path()).into_iter().flatten())
        .filter_map(Result::ok)
        .filter(|unit| unit.path().join("fingerprint").is_dir())
        .count();
    legacy + nested
}

/// Takes cargo's locks of `dir` without a wait. `Ok(None)` when a lock is held (a build runs there).
fn lock(dir: &Path, create: bool) -> Result<Option<Vec<File>>, String> {
    let mut held = Vec::new();
    for name in LOCKS {
        let path = dir.join(name);
        if !create && !path.is_file() {
            continue;
        }
        let file = File::options().read(true).write(true).create(create).truncate(false).open(&path);
        let file = file.map_err(|e| format!("open {}: {e}", path.display()))?;
        match file.try_lock() {
            Ok(()) => held.push(file),
            Err(TryLockError::WouldBlock) => return Ok(None),
            Err(TryLockError::Error(e)) => return Err(format!("lock {}: {e}", path.display())),
        }
    }
    Ok(Some(held))
}

/// Removes the file or tree at `path`, if there is one.
fn remove(path: &Path) {
    if fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()) {
        let _ = fs::remove_dir_all(path);
    } else {
        let _ = fs::remove_file(path);
    }
}

/// Copies `from` to `to` (a file or a tree) with the modification times of `from`: cargo compares them.
/// `fs::copy` clones the data where the file system can (APFS, btrfs, XFS, ReFS). Symbolic links are skipped.
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(from).map_err(|e| format!("read {}: {e}", from.display()))?;
    if meta.is_dir() {
        fs::create_dir_all(to).map_err(|e| format!("create {}: {e}", to.display()))?;
        let list = fs::read_dir(from).map_err(|e| format!("read {}: {e}", from.display()))?;
        for item in list {
            let item = item.map_err(|e| format!("read {}: {e}", from.display()))?;
            copy_tree(&item.path(), &to.join(item.file_name()))?;
        }
    } else if meta.is_file() {
        fs::copy(from, to).map_err(|e| format!("copy {}: {e}", from.display()))?;
        let modified = meta.modified().map_err(|e| format!("mtime {}: {e}", from.display()))?;
        let file = File::options().write(true).open(to).map_err(|e| format!("open {}: {e}", to.display()))?;
        file.set_modified(modified).map_err(|e| format!("mtime {}: {e}", to.display()))?;
    }
    Ok(())
}

/// Copies the units of the checkout in `sources` with the most units into `dest_root/target/debug`. `Ok(None)` when the worktree was seeded before; `Ok(Some(note))` says what was seeded or why not.
pub fn seed(dest_root: &Path, sources: &[PathBuf], local: &[String]) -> Result<Option<String>, String> {
    let dest = dest_root.join("target").join("debug");
    if dest.join(MARKER).exists() {
        return Ok(None);
    }
    let me = fs::canonicalize(dest_root).unwrap_or_else(|_| dest_root.to_path_buf());
    let mut found: Vec<(usize, &PathBuf)> = sources
        .iter()
        .filter(|s| fs::canonicalize(s).unwrap_or_else(|_| s.to_path_buf()) != me)
        .map(|s| (units(s), s))
        .filter(|(n, _)| *n > 0)
        .collect();
    if found.is_empty() {
        return Ok(Some("target seed: no other checkout has built units; this worktree builds them".into()));
    }
    found.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    fs::create_dir_all(&dest).map_err(|e| format!("create {}: {e}", dest.display()))?;
    let Some(_mine) = lock(&dest, true)? else {
        return Ok(Some("target seed: a build runs in this worktree; not seeded".into()));
    };
    // The checkout with the most units first; one where a build runs now is passed over.
    for (_, source) in found {
        let src = source.join("target").join("debug");
        let Some(_theirs) = lock(&src, false)? else { continue };
        let t0 = std::time::Instant::now();
        let copied = copy_units(&src, &dest, local)?;
        let secs = t0.elapsed().as_secs();
        let note = format!("target seed: seeded {copied} unit entries from {} in {secs}s", source.display());
        fs::write(dest.join(MARKER), format!("{note}\n")).map_err(|e| format!("write {MARKER}: {e}"))?;
        return Ok(Some(note));
    }
    Ok(Some("target seed: a build runs in each other checkout; not seeded this time".into()))
}

/// Copies each unit entry of `src` that is not of a local package into `dest`; the number copied. The
/// entries of one part are copied on several threads; a part starts when the part before it is complete.
fn copy_units(src: &Path, dest: &Path, local: &[String]) -> Result<usize, String> {
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 16);
    let mut copied = 0usize;
    for part in PARTS {
        let (from, to) = (src.join(part), dest.join(part));
        let Ok(list) = fs::read_dir(&from) else { continue };
        fs::create_dir_all(&to).map_err(|e| format!("create {}: {e}", to.display()))?;
        let mut names = Vec::new();
        for item in list {
            let item = item.map_err(|e| format!("read {}: {e}", from.display()))?;
            let name = item.file_name().to_string_lossy().into_owned();
            if !name.starts_with(".seed-") && !is_local(&name, local) {
                names.push(name);
            }
        }
        let next = std::sync::atomic::AtomicUsize::new(0);
        let results: Vec<Result<(), String>> = std::thread::scope(|s| {
            let jobs: Vec<_> = (0..workers)
                .map(|_| {
                    s.spawn(|| loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(name) = names.get(i) else { return Ok(()) };
                        copy_unit(&from.join(name), &to, name)?;
                    })
                })
                .collect();
            jobs.into_iter().map(|j| j.join().unwrap_or_else(|_| Err("a copy thread stopped".into()))).collect()
        });
        results.into_iter().collect::<Result<Vec<()>, String>>()?;
        copied += names.len();
    }
    Ok(copied)
}

/// Copies one entry `from` into the directory `to` as `name`. In the nested layout this is a whole
/// package tree: its outputs and fingerprints become visible together, only after the copy completes.
fn copy_unit(from: &Path, to: &Path, name: &str) -> Result<(), String> {
    // A copy under a temporary name, then one rename: a unit is complete or absent, never half there.
    let tmp = to.join(format!(".seed-{name}"));
    remove(&tmp);
    copy_tree(from, &tmp)?;
    // A unit that is here already (`cargo xtask` built its own dependencies just now) has the same hash,
    // so the same inputs; it is replaced, else its newer time makes each copied unit above it dirty.
    remove(&to.join(name));
    fs::rename(&tmp, to.join(name)).map_err(|e| format!("rename {}: {e}", tmp.display()))
}

/// The checkouts of this repository (`git worktree list`): the main checkout and its worktrees.
fn checkouts(root: &Path) -> Vec<PathBuf> {
    let list = crate::exec::git(root, &["worktree", "list", "--porcelain"]).unwrap_or_default();
    list.lines().filter_map(|l| l.strip_prefix("worktree ")).map(PathBuf::from).collect()
}

/// Seeds the gate's worktree before its first cargo step. Never fails the gate: a problem is a note.
/// `CAMCRAFT_NO_SEED=1` opts out; a dry run seeds nothing.
pub fn before_gate(root: &Path, dry: bool) {
    if dry || std::env::var_os("CAMCRAFT_NO_SEED").is_some_and(|v| v == "1") {
        return;
    }
    if root.join("target").join("debug").join(MARKER).exists() {
        return;
    }
    let out = Command::new(crate::exec::cargo()).args(["metadata", "--format-version", "1"]).current_dir(root).output();
    let metadata = match out {
        Ok(o) if o.status.success() => crate::json::parse(&String::from_utf8_lossy(&o.stdout)),
        _ => None,
    };
    let Some(metadata) = metadata else {
        println!("target seed: cargo metadata failed; not seeded");
        return;
    };
    match seed(root, &checkouts(root), &local_packages(&metadata)) {
        Ok(Some(note)) => println!("{note}"),
        Ok(None) => {}
        Err(e) => println!("target seed: {e}; not seeded"),
    }
}
