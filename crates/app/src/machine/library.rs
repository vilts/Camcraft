//! The machine catalogue (issue #287): every machine definition the
//! app can name - the shipped library and the user's own `.mch` files -
//! each with a summary, a read-only property sheet and its validation.
//!
//! Provenance: our own design, specced from the issue's workflow (list
//! shipped and user machines with their kind, axes, units and posts) and
//! the `.mch` grammar in `assets/machines/AGENTS.md`; no vendor machine
//! library or its layout was consulted.
//!
//! Reading only. Nothing here writes a file or touches the document:
//! assigning a machine stays the `LoadMachine` command's job
//! (`LoadMachine::from_catalogue`, issue #384), and the catalogue is
//! rebuilt on demand (the pane caches it), never stored. Writing a
//! machine file is `draft` / `store`'s job (#383).

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;
use serde_json::Value;

use super::validate::{validate_text, Refusal};
use super::{kind, parse_mch, Kind};
use crate::posting;

pub use super::sheet::PropGroup;

/// Where a catalogue entry came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MachineSource {
    /// A shipped library entry, by its picker name (`posting::MACHINES`).
    Builtin(&'static str),
    /// A file on disk.
    File(PathBuf),
}

impl MachineSource {
    /// The words the pane shows for the source.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            MachineSource::Builtin(_) => "built-in".to_owned(),
            MachineSource::File(p) => p.display().to_string(),
        }
    }

    /// The path the document records when this machine is mounted -
    /// the same string `machine::of` answers afterwards, so "is this
    /// the setup's machine?" is one comparison.
    #[must_use]
    pub fn mount_path(&self) -> String {
        match self {
            MachineSource::Builtin(n) => posting::builtin_path(n),
            MachineSource::File(p) => p.display().to_string(),
        }
    }
}

/// One machine in the catalogue: what it is, where it came from, how
/// its file reads, and whether it validates.
#[derive(Clone, Debug)]
pub struct MachineEntry {
    /// The list name: the library's picker name, or the file's model
    /// (its file name when it states none or does not read).
    pub name: String,
    pub source: MachineSource,
    /// Declared or inferred kind; `None` when the file does not read.
    pub kind: Option<Kind>,
    /// Linear axis letters, in the file's controller order.
    pub axes: Vec<String>,
    /// Rotary axis letters.
    pub rotaries: Vec<String>,
    /// `mm` or `inch` (the stated length unit; unstated reads mm).
    pub units: &'static str,
    /// The built-in posts that serve this machine's kind
    /// (`posting::post_serves`); empty when the kind is unknown.
    pub posts: Vec<&'static str>,
    /// The read-only property sheet, grouped.
    pub groups: Vec<PropGroup>,
    /// Empty = the file validates clean.
    pub refusals: Vec<Refusal>,
}

impl MachineEntry {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.refusals.is_empty()
    }

    /// Why this entry cannot be made the setup's machine right now, in
    /// words for the pane's disabled button (issue #384); `None` = it
    /// can. A refused file names its first refusal; the machine already
    /// mounted names that (assigning it again would be a no-op command).
    #[must_use]
    pub fn assign_block(&self, world: &World) -> Option<String> {
        if let Some(first) = self.refusals.first() {
            let more = match self.refusals.len() {
                1 => String::new(),
                n => format!(" (and {} more)", n - 1),
            };
            return Some(format!("fails validation - {first}{more}"));
        }
        if super::of(world).0 == self.source.mount_path() {
            return Some("it is already this setup's machine".to_owned());
        }
        None
    }

    /// Build an entry from a file's text. Never panics: a file that
    /// does not read is an entry with its refusals and no facts.
    #[must_use]
    pub fn from_text(name: &str, source: MachineSource, text: &str) -> Self {
        let refusals = validate_text(text);
        let raw: Option<Value> = serde_json::from_str(text).ok();
        let parsed = parse_mch(text).ok();
        let kind = parsed.as_ref().map(|m| kind(Some(m)));
        let (axes, rotaries) = parsed.as_ref().map_or_else(Default::default, |m| {
            let pick = |rot: bool| m.axes.iter().filter(|a| a.rotary == rot).map(|a| a.name.clone()).collect();
            (pick(false), pick(true))
        });
        let units = if parsed.as_ref().and_then(|m| m.units.as_deref()) == Some("inch") { "inch" } else { "mm" };
        let posts: Vec<&'static str> = kind.map_or_else(Vec::new, |k| {
            posting::BUILTINS.iter().map(|(n, _)| *n).filter(|n| posting::post_serves(n, k)).collect()
        });
        let groups = match (&raw, &parsed) {
            (Some(v), Some(m)) => super::sheet::groups(v, m, kind, &posts),
            _ => Vec::new(),
        };
        Self { name: name.to_owned(), source, kind, axes, rotaries, units, posts, groups, refusals }
    }

    /// Read an entry from a `.mch` file. An unreadable file is an
    /// entry naming the read failure, never an error the caller drops.
    #[must_use]
    pub fn from_file(path: &Path) -> Self {
        let file_name = path
            .file_name()
            .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let model = parse_mch(&text)
                    .ok()
                    .map(|m| m.model)
                    .filter(|m| !m.trim().is_empty() && m != "unknown");
                let name = model.unwrap_or(file_name);
                Self::from_text(&name, MachineSource::File(path.to_path_buf()), &text)
            }
            Err(e) => {
                let mut entry = Self::from_text(&file_name, MachineSource::File(path.to_path_buf()), "{}");
                entry.refusals = vec![Refusal { key: "file".to_owned(), rule: format!("cannot be read ({e})") }];
                entry
            }
        }
    }
}

/// Read and VALIDATE a catalogue machine for assignment (#384): the
/// path the document records and the parsed machine, or why not - the
/// file cannot be read, a library name is stale, or the first
/// validation refusal. Read fresh, never trusted from a cached entry:
/// the forgiving reader would otherwise mount a machine whose stated
/// limits it quietly dropped. The commands' `from_catalogue` doors and
/// the ui's routed mount share it.
pub fn read_for_assign(source: &MachineSource) -> Result<(String, super::Machine), String> {
    let text: String = match source {
        MachineSource::Builtin(name) => posting::builtin_machine_text(name)?.to_owned(),
        MachineSource::File(path) => {
            std::fs::read_to_string(path).map_err(|e| format!("machine file {}: {e}", path.display()))?
        }
    };
    if let Some(first) = validate_text(&text).first() {
        return Err(format!("{} fails validation - {first}", source.mount_path()));
    }
    let m = parse_mch(&text)?;
    Ok((source.mount_path(), m))
}

/// The shipped library as the machine folder holds it now (#2380): the
/// `posting::MACHINES` entries that have a file, in table order. A table
/// entry with no file is not listed (it is gone, not an error); the
/// document that names it is still refused by name when it mounts.
#[must_use]
pub fn builtins() -> Vec<MachineEntry> {
    builtins_of(&posting::machine_scan())
}

/// [`builtins`] for a given folder scan.
#[must_use]
pub fn builtins_of(scan: &posting::FolderScan) -> Vec<MachineEntry> {
    posting::MACHINES
        .iter()
        .filter(|(name, _)| scan.present.contains(name))
        .map(|(name, _)| match posting::builtin_machine_text(name) {
            Ok(text) => MachineEntry::from_text(name, MachineSource::Builtin(name), text),
            // The file went between the scan and the read: list it with its refusal.
            Err(why) => {
                let mut entry = MachineEntry::from_text(name, MachineSource::Builtin(name), "{}");
                entry.refusals = vec![Refusal { key: "file".to_owned(), rule: why }];
                entry
            }
        })
        .collect()
}

/// The machine folder's whole list for a scan: the library entries that
/// have a file, then every other `.mch` in the folder as a file entry
/// (a malformed one is listed with its refusals, never skipped silently).
#[must_use]
pub fn shipped_entries(scan: &posting::FolderScan) -> Vec<MachineEntry> {
    let mut out = builtins_of(scan);
    out.extend(scan.extra.iter().map(|(_, path)| MachineEntry::from_file(path)));
    out
}

/// Every `.mch` file directly inside `dir`, sorted by file name. A
/// missing or unreadable folder lists nothing (there is nothing to
/// name); an unreadable FILE is listed with its refusal.
#[must_use]
pub fn scan_dir(dir: &Path) -> Vec<MachineEntry> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = rd
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("mch")))
        .collect();
    paths.sort();
    paths.iter().map(|p| MachineEntry::from_file(p)).collect()
}

/// A catalogue entry's file text: a built-in's shipped text, a file's
/// text read fresh from disk. The editor copies and opens from this.
pub fn text_of(source: &MachineSource) -> Result<String, String> {
    match source {
        MachineSource::Builtin(name) => posting::builtin_machine_text(name).map(str::to_owned),
        MachineSource::File(p) => {
            std::fs::read_to_string(p).map_err(|e| format!("{} cannot be read ({e})", p.display()))
        }
    }
}

/// The catalogue for this document: the shipped library, then every
/// `.mch` in the user machines folder (#383; `store::user_dir`, or the
/// folder the caller names), then the `.mch` the document has mounted
/// from disk (Setup pane, "Load other") when it is not already listed,
/// read fresh from that path.
#[must_use]
pub fn enumerate_with(world: &World, user_dir: Option<&Path>) -> Vec<MachineEntry> {
    let mut out = shipped_entries(&posting::machine_scan());
    if let Some(dir) = user_dir {
        out.extend(scan_dir(dir));
    }
    let (path, _) = super::of(world);
    let listed = |p: &Path| {
        out.iter().any(|e| matches!(&e.source, MachineSource::File(f) if f == p))
    };
    if !path.is_empty() && !path.starts_with(&posting::builtin_path("")) && !listed(Path::new(&path)) {
        out.push(MachineEntry::from_file(Path::new(&path)));
    }
    out
}

/// [`enumerate_with`] the process's user machines folder.
#[must_use]
pub fn enumerate(world: &World) -> Vec<MachineEntry> {
    enumerate_with(world, super::store::user_dir().as_deref())
}
