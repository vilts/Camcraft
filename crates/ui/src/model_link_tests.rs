//! Pins for #2463: a linked model whose file changes is imported again as
//! one `UpdateModel` (one undo step) and its operations only go Stale -
//! nothing regenerates or posts by itself; an undo of that import stays;
//! a missing file leaves the model as it is and is said one time; Unlink
//! stops the check; the frame timer comes back while a model is linked.
//!
//! Gate map (M4.4): covers ui::model_link_watch, ui::state (finish_import
//! link landing, UnlinkModel), ui::egui_app::fixture_menu (Unlink row).
//! Provenance: our own tests.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, SetStock};
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, model_source};

use crate::egui_app::fixture_menu::menu_rows;
use crate::model_link_watch::{check, wants_ticks, INTERVAL};
use crate::state::{CommandKind, State, TreeMenuTarget};

const TOOL: &str = "EM-0400-2FL";

/// An ASCII STL of a box from the origin to (`dx`, `dy`, `dz`), wound
/// outward.
fn box_stl(dx: f64, dy: f64, dz: f64) -> String {
    let p = |x: f64, y: f64, z: f64| [x * dx, y * dy, z * dz];
    let quads = [
        ([0.0, 0.0, -1.0], [p(0., 0., 0.), p(0., 1., 0.), p(1., 1., 0.), p(1., 0., 0.)]),
        ([0.0, 0.0, 1.0], [p(0., 0., 1.), p(1., 0., 1.), p(1., 1., 1.), p(0., 1., 1.)]),
        ([0.0, -1.0, 0.0], [p(0., 0., 0.), p(1., 0., 0.), p(1., 0., 1.), p(0., 0., 1.)]),
        ([0.0, 1.0, 0.0], [p(0., 1., 0.), p(0., 1., 1.), p(1., 1., 1.), p(1., 1., 0.)]),
        ([-1.0, 0.0, 0.0], [p(0., 0., 0.), p(0., 0., 1.), p(0., 1., 1.), p(0., 1., 0.)]),
        ([1.0, 0.0, 0.0], [p(1., 0., 0.), p(1., 1., 0.), p(1., 1., 1.), p(1., 0., 1.)]),
    ];
    let mut out = String::from("solid box\n");
    for (n, q) in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            out.push_str(&format!(" facet normal {} {} {}\n  outer loop\n", n[0], n[1], n[2]));
            for v in tri {
                out.push_str(&format!("   vertex {} {} {}\n", v[0], v[1], v[2]));
            }
            out.push_str("  endloop\n endfacet\n");
        }
    }
    out.push_str("endsolid box\n");
    out
}

/// A folder of its own for one test, holding `part.stl`.
struct Dir(PathBuf, Cell<u64>);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("camcraft-ui-2463-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        let d = Dir(dir, Cell::new(0));
        d.write(10.0, 10.0, 5.0);
        d
    }
    fn part(&self) -> PathBuf {
        self.0.join("part.stl")
    }
    fn write(&self, dx: f64, dy: f64, dz: f64) {
        std::fs::write(self.part(), box_stl(dx, dy, dz)).expect("write part");
        // The watcher caches (size, mtime). Same-size edits must move the
        // file clock even when a busy full suite lands two writes in one
        // filesystem timestamp tick. Advance the fixture clock explicitly.
        let tick = self.1.get() + 1;
        self.1.set(tick);
        std::fs::File::options()
            .write(true)
            .open(self.part())
            .expect("open part for timestamp")
            .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000 + tick * 2))
            .expect("advance part timestamp");
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(4.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

/// The part imported from `path` (linked), with a stock, a tool and a
/// Face operation on it, generated.
fn linked_job(path: &Path) -> State {
    let mut state = State::new();
    state.run(CommandKind::ImportFile(path.to_path_buf()));
    state.settle();
    assert!(model::find_by_name(&state.world, "part").is_some(), "{}", state.status);
    let w = &state.world;
    let stock = SetStock::new(w, StockShape::Box { dx: 40.0, dy: 40.0, dz: 20.0 }, Location::default()).expect("stock");
    state.history.execute(w, stock);
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    state.history.execute(w, AddTool::new(w, TOOL, tool).expect("add tool"));
    let op = CreateOperation::new(w, "Face1", TOOL, SrcRef::Entity("part".into()), face()).expect("Face1");
    state.history.execute(w, op);
    state.history.execute(w, GenerateOperation::new(w, "Face1").expect("generate"));
    assert_eq!(status(&state), OpStatus::Ok, "freshly generated");
    state
}

fn status(state: &State) -> OpStatus {
    let e = model::find_by_name(&state.world, "Face1").expect("op");
    operation::status_of(&state.world, &e)
}

fn rev(state: &State) -> u64 {
    model::find_by_name(&state.world, "part").map_or(0, |e| model::rev_of(&state.world, &e))
}

/// Run the check as if `secs` seconds passed, then let the queue settle.
fn tick(state: &mut State, at: &mut Instant, secs: u64) {
    *at += Duration::from_secs(secs);
    check(state, *at);
    state.settle();
}

#[test]
fn a_changed_file_is_imported_again_as_one_undo_step_and_its_operations_only_go_stale() {
    let dir = Dir::new("change");
    let mut state = linked_job(&dir.part());
    let mut at = Instant::now() + INTERVAL;
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 1, "an unchanged file is not imported again");

    dir.write(10.0, 10.0, 8.0);
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 2, "{}", state.status);
    assert!(state.status.starts_with("updated part"), "{}", state.status);
    assert_eq!(status(&state), OpStatus::Stale, "the operation goes Stale and is not generated again");
    assert!(!state.gen_jobs.busy(), "no generation was started");
    let source = model::find_by_name(&state.world, "part").and_then(|e| model_source::of(&state.world, &e));
    assert_eq!(source, model_source::read(&dir.part()).ok(), "the link records the new digest");

    assert!(state.history.undo(&state.world), "one undo step");
    assert_eq!(rev(&state), 1);
    assert_eq!(status(&state), OpStatus::Ok, "undo brings the generated path back");
    tick(&mut state, &mut at, 2);
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 1, "the undo stays: the same file is not imported again");

    let previous = std::fs::metadata(dir.part()).expect("previous file stamp");
    dir.write(12.0, 10.0, 8.0);
    let changed = std::fs::metadata(dir.part()).expect("changed file stamp");
    assert_eq!(previous.len(), changed.len(), "exercise a same-size file change");
    assert_ne!(previous.modified().unwrap(), changed.modified().unwrap(), "the file clock advances independently of suite timing");
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 2, "a new change is imported again: {}", state.status);
}

#[test]
fn a_missing_file_keeps_the_model_and_is_said_one_time() {
    let dir = Dir::new("missing");
    let mut state = linked_job(&dir.part());
    let mut at = Instant::now() + INTERVAL;
    std::fs::remove_file(dir.part()).expect("remove");
    tick(&mut state, &mut at, 2);
    assert!(state.status.starts_with("part kept as it is: its linked file"), "{}", state.status);
    assert_eq!(rev(&state), 1);
    assert_eq!(status(&state), OpStatus::Ok);
    state.status = String::from("other");
    tick(&mut state, &mut at, 2);
    assert_eq!(state.status, "other", "said one time");

    dir.write(10.0, 10.0, 9.0);
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 2, "the file came back changed: {}", state.status);
}

/// Rewrite the link of "part" to `text`, as a project file can hold it.
fn relink(state: &State, text: &str) {
    let w = &state.world;
    let e = model::find_by_name(w, "part").expect("part");
    let mut source = model_source::of(w, &e).expect("linked");
    source.path = text.to_string();
    model_source::set(w, &e, Some(&source));
}

fn link_text(state: &State) -> Option<String> {
    model::find_by_name(&state.world, "part").and_then(|e| model_source::of(&state.world, &e)).map(|s| s.path)
}

/// #3686: a relative link is read against the folder of the open project,
/// not the working directory of the test process (`crates/ui`). A change
/// of the file imports it again and the link stays relative.
#[test]
fn a_relative_link_resolves_against_the_project_folder() {
    let dir = Dir::new("relative");
    let mut state = linked_job(&dir.part());
    relink(&state, "part.stl");
    assert!(!Path::new("part.stl").exists(), "the working directory must not hold the file");
    state.project_path = Some(dir.0.join("job.camproj"));
    state.status = String::from("opened job");
    let mut at = Instant::now() + INTERVAL;
    tick(&mut state, &mut at, 2);
    assert_eq!(state.status, "opened job", "no link error for a file next to the project");
    assert_eq!(rev(&state), 1, "an unchanged file is not imported again");

    dir.write(10.0, 10.0, 8.0);
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 2, "{}", state.status);
    assert!(state.status.starts_with("updated part"), "{}", state.status);
    assert_eq!(link_text(&state).as_deref(), Some("part.stl"), "the link stays relative");
}

/// #3686: without a saved project, a relative link keeps the old reading
/// (the working directory); an absolute link is read as it is wherever the
/// project is.
#[test]
fn an_absolute_link_ignores_the_project_folder_and_an_unsaved_project_keeps_the_path() {
    let dir = Dir::new("absolute");
    let other = Dir::new("absolute-other");
    let mut state = linked_job(&dir.part());
    state.project_path = Some(other.0.join("job.camproj"));
    state.status = String::from("opened job");
    let mut at = Instant::now() + INTERVAL;
    tick(&mut state, &mut at, 2);
    assert_eq!(state.status, "opened job", "an absolute link is read as it is");
    dir.write(10.0, 10.0, 8.0);
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 2, "{}", state.status);
    assert_eq!(link_text(&state), Some(dir.part().to_string_lossy().into_owned()));

    relink(&state, "part.stl");
    state.project_path = None;
    tick(&mut state, &mut at, 2);
    assert!(state.status.starts_with("part kept as it is: its linked file part.stl"), "{}", state.status);
}

#[test]
fn unlink_stops_the_check_and_undo_links_again() {
    let dir = Dir::new("unlink");
    let mut state = linked_job(&dir.part());
    assert!(wants_ticks(&state), "a linked model keeps the frame timer coming back");
    let rows = menu_rows(&state, &TreeMenuTarget::Model { name: "part".into(), row: 0 });
    let unlink = rows.iter().find(|(label, _)| label == "Unlink from part.stl");
    assert!(matches!(unlink, Some((_, Some(CommandKind::UnlinkModel(n)))) if n == "part"), "{rows:?}");

    state.run(CommandKind::UnlinkModel("part".into()));
    assert_eq!(state.status, "unlinked part - its file is no longer checked");
    assert!(!wants_ticks(&state));
    assert!(menu_rows(&state, &TreeMenuTarget::Model { name: "part".into(), row: 0 }).iter().all(|(l, _)| l != "Source"));
    let mut at = Instant::now() + INTERVAL;
    dir.write(10.0, 10.0, 8.0);
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 1, "an unlinked model is never imported again");

    state.run(CommandKind::UnlinkModel("part".into()));
    assert!(state.status.contains("'part' is not linked to a file"), "{}", state.status);
    assert!(state.history.undo(&state.world));
    tick(&mut state, &mut at, 2);
    assert_eq!(rev(&state), 2, "linked again, the changed file is imported: {}", state.status);
}
