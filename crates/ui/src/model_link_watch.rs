//! The link check (#2463): while the project is open, the file of each
//! linked model is checked about once a second. When its SHA-256 is not the
//! one the model records, the file is imported again through
//! `CommandKind::ImportFile`, and `finish_import` lands it on the linked
//! model by name (`linked_to`): one `UpdateModel`, one undo step, the
//! operations only go Stale. Nothing here regenerates or posts. A file
//! that is missing or cannot be read leaves the model as it is and is said
//! on the status line one time, by name.
//! Provenance: our own code over our own commands; std file metadata.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

// The clock seam (#31): std's Instant panics in the browser build.
use camcraft_app::clock::Instant;
use camcraft_app::model;
use flecs_ecs::prelude::World;

use crate::state::{CommandKind, State};

/// How often the linked files are checked.
pub const INTERVAL: Duration = Duration::from_secs(1);

/// What the check remembers between frames. View state, not document.
#[derive(Default)]
pub struct LinkWatch {
    next: Option<Instant>,
    /// The size and time of each file when its digest was last read, so an
    /// unchanged file is not read again every second.
    seen: HashMap<String, (u64, Option<SystemTime>)>,
    /// The models whose file problem the status line already said.
    said: HashSet<String>,
    /// The digest each model was last imported again for. An undo of that
    /// import leaves the file as it is; it is not imported again until the
    /// file changes once more, so the undo stays.
    acted: HashMap<String, String>,
}

/// The linked models: name, source path and recorded digest.
fn linked(world: &World) -> Vec<(String, String, String)> {
    model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_stock(world, e))
        .filter_map(|(n, e)| camcraft_app::model_source::of(world, &e).map(|s| (n, s.path, s.sha256)))
        .collect()
}

/// The file a link names (#3686). A relative link is read against the
/// folder of the open project file, not the working directory of the
/// process. An absolute link, or a project not saved yet, keeps the path
/// as it is.
pub(crate) fn resolve(path: &str, project: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    match project.and_then(Path::parent) {
        Some(dir) if p.is_relative() => dir.join(p),
        _ => p.to_path_buf(),
    }
}

/// The model linked to the file at `path`, if one is, and the link text it
/// records: a re-import of that file lands on it, whatever the file's name
/// says, and keeps the link as it was written.
pub(crate) fn linked_to(world: &World, path: &Path, project: Option<&Path>) -> Option<(String, String)> {
    linked(world)
        .into_iter()
        .find(|(_, p, _)| p.as_str() == path.to_string_lossy() || resolve(p, project) == path)
        .map(|(n, p, _)| (n, p))
}

/// True when the frame timer must come back for the next check.
pub(crate) fn wants_ticks(state: &State) -> bool {
    !linked(&state.world).is_empty()
}

/// Check the linked files if `now` is due. Queues at most one import.
pub(crate) fn check(state: &mut State, now: Instant) {
    if state.link_watch.next.is_some_and(|t| now < t) {
        return;
    }
    state.link_watch.next = now.checked_add(INTERVAL);
    if state.import_in_progress() || !state.pending.is_empty() {
        return;
    }
    let project = state.project_path.clone();
    for (name, link, sha256) in linked(&state.world) {
        let path = resolve(&link, project.as_deref());
        match now_digest(&mut state.link_watch, &name, &path) {
            Ok(digest) => {
                state.link_watch.said.remove(&name);
                let Some(digest) = digest else { continue };
                if digest == sha256 || state.link_watch.acted.get(&name) == Some(&digest) {
                    continue;
                }
                state.link_watch.acted.insert(name.clone(), digest);
                state.status = format!("{name}: its file changed - importing it again");
                state.pending.push_back(CommandKind::ImportFile(path));
                return;
            }
            Err(why) => {
                if state.link_watch.said.insert(name.clone()) {
                    state.refuse_drop(format!("{name} kept as it is: its linked file {why}"));
                }
            }
        }
    }
}

/// The digest of the file at `path` when its size or time moved since it
/// was last read (`None`: unchanged, not read again).
fn now_digest(watch: &mut LinkWatch, name: &str, path: &Path) -> Result<Option<String>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{} cannot be read ({e})", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    let stamp = (meta.len(), meta.modified().ok());
    if watch.seen.get(name) == Some(&stamp) {
        return Ok(None);
    }
    let now = camcraft_app::model_source::read(path)?;
    watch.seen.insert(name.to_string(), stamp);
    Ok(Some(now.sha256))
}
