//! #163: the app's seams speak to the terminal. A project save names its
//! file name (never its folder) and size, a failed open says why, a generation names its kind,
//! move count and time, a written NC program names its file and size, a
//! refused post says why, and commands are logged at debug.
//! Provenance: our own fixtures over our own log lines.
//!
//! Gate map (#163): covers app::logging, app::project (save/load),
//! app::posting (post_script), app::operation (generate), app::commands
//! (History dispatch).
//! Lane shape (M4.4): all tests here run in seconds.

use std::io::Write;
use std::sync::{Arc, Mutex};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project};

/// Everything the subscriber writes, kept for the assertions.
#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Ok(mut v) = self.0.lock() {
            v.extend_from_slice(buf);
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for Captured {
    type Writer = Captured;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

impl Captured {
    fn text(&self) -> String {
        self.0.lock().map(|v| String::from_utf8_lossy(&v).into_owned()).unwrap_or_default()
    }
}

/// Run `f` with a debug-level subscriber writing into a buffer (this
/// thread only), and return what it logged.
fn logged(f: impl FnOnce()) -> String {
    let out = Captured::default();
    let sub = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("debug"))
        .with_writer(out.clone())
        .with_ansi(false)
        .finish();
    tracing::subscriber::with_default(sub, f);
    out.text()
}

fn face_params() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// The file name alone: what the log shows for a path (#163 review).
fn file_of(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("camcraft-logging-{tag}-{}.{ext}", std::process::id()))
}

#[test]
fn save_generate_and_post_each_name_what_they_did() {
    let project_path = tmp("save", "camproj");
    let nc_path = tmp("post", "nc");
    let text = logged(|| {
        let world = new_world();
        let mut h = History::default();
        let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
        h.execute(&world, stock.expect("stock"));
        h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 19.0))));
        let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog tool");
        h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
        let op = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face_params());
        h.execute(&world, op.expect("op"));
        h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
        project::save(&world, &project_path).expect("save");
        let posted = posting::post_setup(&world, &nc_path);
        assert!(posted.is_ok(), "the face posts: {posted:?}");
    });
    let _ = std::fs::remove_file(&project_path);
    let _ = std::fs::remove_file(&nc_path);
    for want in [
        "project saved".to_owned(),
        file_of(&project_path),
        "generated".to_owned(),
        "op=Face1".to_owned(),
        "moves=".to_owned(),
        "NC program written".to_owned(),
        file_of(&nc_path),
        "bytes=".to_owned(),
        "command=\"SetStock\"".to_owned(),
    ] {
        assert!(text.contains(&want), "missing {want:?} in the log:\n{text}");
    }
    for full in [&project_path, &nc_path] {
        let full = full.display().to_string();
        assert!(!text.contains(&full), "the log names the folder {full:?}:\n{text}");
    }
}

#[test]
fn a_failed_open_and_a_refused_post_say_why() {
    let missing = tmp("missing", "camproj");
    let nc_path = tmp("refused", "nc");
    let text = logged(|| {
        let world = new_world();
        assert!(project::load(&world, &missing).is_err(), "a missing file does not open");
        assert!(posting::post_setup(&world, &nc_path).is_err(), "an empty document does not post");
    });
    assert!(!nc_path.exists(), "nothing written");
    assert!(text.contains("project not opened") && text.contains(&file_of(&missing)), "{text}");
    assert!(text.contains("NC program not written"), "{text}");
}

#[test]
fn the_default_filter_is_used_when_the_environment_names_none() {
    // A smoke check of the shipped default: it parses and keeps info.
    let f = tracing_subscriber::EnvFilter::new(camcraft_app::logging::DEFAULT_FILTER);
    assert!(f.to_string().contains("info"), "{f}");
    camcraft_app::logging::init();
    camcraft_app::logging::init(); // a second install is a no-op, never a panic
}

/// #163: a generator that refuses on a limit says which limit and its
/// value, at warn - not just "refused".
#[test]
fn a_budget_refusal_names_the_limit_that_fired_at_warn() {
    let text = logged(|| {
        let world = new_world();
        let mut h = History::default();
        let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
        h.execute(&world, stock.expect("stock"));
        h.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 19.0))));
        let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog tool");
        h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("tool"));
        let mut params = face_params();
        if let OpParams::Face { stepover, .. } = &mut params {
            *stepover = 1e-7; // millions of passes: past the pass budget
        }
        let op = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
        h.execute(&world, op.expect("op"));
        if let Ok(cmd) = GenerateOperation::new(&world, "Face1") {
            h.execute(&world, cmd);
        }
    });
    let refusal = text.lines().find(|l| l.contains("refused")).unwrap_or_default();
    assert!(refusal.contains("WARN"), "a refusal is a warning:\n{text}");
    assert!(refusal.contains("passes"), "names the limit that fired:\n{text}");
}
