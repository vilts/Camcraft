//! #1224, the MCP leg of the trace: a setup's own machine picked through
//! the MCP `load_machine` tool reaches the posted files of the MCP `post`
//! tool - one file per mount, each naming its own machine and holding its
//! own setup's work - and the MCP `undo` gives the setup back to the
//! document. A setup's own POST has no MCP tool yet (#1233): the post
//! half of the trace is pinned app-side in `setup_mount_trace.rs`.
//! Provenance: our own fixture over our own commands and tools; no vendor
//! material.
//!
//! Gate map (#1224): covers app::mcp::setup_tools (load_machine with a
//! setup), app::mcp::tools (open_project, generate, post, undo),
//! app::posting (post_mounts).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, History, LoadMachine, SetSetupWorkOffset, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::Point3;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use serde_json::{json, Value};
use std::path::PathBuf;

const TOOL: &str = "EM-0400-2FL";

fn tmp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-1224-mcp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A saved project: the document on the shipped VMC 3-axis with the
/// built-in Fanuc post; Setup1 (G54) and Setup2 (G55) each face a box
/// from its top, at 8000 rpm (inside every machine here). No setup owns
/// a mount.
fn saved_project(dir: &std::path::Path) -> PathBuf {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, LoadMachine::builtin(&world, "VMC 3-axis").expect("machine"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, camcraft_app::tools::Tool { rpm: 8000.0, ..tool }).expect("tool"));
    for (i, op) in ["Face1", "Face2"].into_iter().enumerate() {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Location::default());
        h.execute(&world, stock.expect("box"));
        let top = Location { pos: Point3::new(0.0, 0.0, 20.0), ..Location::default() };
        h.execute(&world, SetWcsOrigin::new(&world, top).expect("wcs"));
        if i == 1 {
            h.execute(&world, SetSetupWorkOffset::new(&world, "Setup2", Some(1)).expect("G55"));
        }
        let face = OpParams::Face {
            holes: Default::default(),
            blend_turns: false,
            tilt: None,
            stepover: 3.0,
            clearance_z: Height::fixed(25.0),
            cut_z: Height::fixed(19.5),
            top_z: None,
            stepdown: None,
            feed: 1200.0,
            plunge_feed: 300.0,
            climb: true,
        };
        h.execute(&world, CreateOperation::new(&world, op, TOOL, SrcRef::Stock, face).expect("face"));
    }
    let path = dir.join("two-setups.ccam");
    camcraft_app::project::save(&world, &path).expect("save");
    path
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("a request gets a response")
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result["isError"] != json!(true), "{name} refused: {reply}");
        result["structuredContent"].clone()
    }

    /// Post to `out`; each written file's (name, text), removed again.
    fn post(&mut self, out: &std::path::Path) -> Vec<(String, String)> {
        let r = self.ok("post", json!({ "output": out.display().to_string() }));
        let files = r["files"].as_array().cloned().unwrap_or_default();
        files
            .iter()
            .map(|f| {
                let p = PathBuf::from(f.as_str().expect("a path"));
                let text = std::fs::read_to_string(&p).expect("written");
                let _ = std::fs::remove_file(&p);
                (p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), text)
            })
            .collect()
    }
}

#[test]
fn a_setups_own_machine_picked_over_mcp_reaches_its_own_posted_file_and_undoes() {
    let dir = tmp_dir("machine");
    let project = saved_project(&dir);
    let out = dir.join("job.nc");
    let mut c = Client::new();
    c.ok("open_project", json!({ "path": project.display().to_string() }));
    c.ok("generate", json!({}));
    let before = c.post(&out);
    assert_eq!(before.len(), 1, "one mount, one file: {before:?}");

    let own = c.ok("load_machine", json!({ "builtin": "Tormach 770M", "setup": "Setup2" }));
    assert_eq!((own["setup"].clone(), own["machine"].clone()), (json!("Setup2"), json!("Tormach 770M")), "{own}");
    c.ok("generate", json!({}));
    let files = c.post(&out);
    assert_eq!(files.len(), 2, "one file per mount: {files:?}");
    let ((n1, a), (n2, b)) = (&files[0], &files[1]);
    assert!(n1.ends_with("_01_Setup1.nc") && n2.ends_with("_02_Setup2.nc"), "{n1} {n2}");
    assert!(a.starts_with("%\nO0001 (VMC 3-AXIS)") && a.contains("Face1 - FACE - G54"), "{a}");
    assert!(!a.contains("Face2") && !a.contains("G55") && !a.contains("770M"), "{a}");
    assert!(b.starts_with("%\nO0001 (Tormach 770M)") && b.contains("Face2 - FACE - G55"), "{b}");
    assert!(!b.contains("Face1") && !b.contains("G54") && !b.contains("VMC"), "{b}");

    // One MCP undo gives Setup2 back to the document: one file, as before.
    c.ok("undo", json!({}));
    assert_eq!(c.post(&out), before, "the undo restores the earlier output");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #1233, the post half of the trace: two setups given two different
/// posts over MCP post one file per mount, each in its own post's dialect;
/// the MCP `undo` of the pick gives the mount back to the document's post.
#[test]
fn a_setups_own_post_picked_over_mcp_reaches_its_own_posted_file_and_undoes() {
    let dir = tmp_dir("post");
    let project = saved_project(&dir);
    let out = dir.join("job.nc");
    let mut c = Client::new();
    c.ok("open_project", json!({ "path": project.display().to_string() }));
    c.ok("set_setup_post", json!({ "setup": "Setup1", "post": "Siemens 840D" }));
    c.ok("set_setup_post", json!({ "setup": "Setup2", "post": "Haas" }));
    c.ok("generate", json!({}));
    let files = c.post(&out);
    assert_eq!(files.len(), 2, "one file per mount: {files:?}");
    let ((_, a), (_, b)) = (&files[0], &files[1]);
    assert!(a.starts_with("; VMC 3-AXIS\n") && a.contains("N10 G17 G710") && a.contains("Face1"), "{a}");
    assert!(!a.contains("M30") && !a.contains("Face2"), "{a}");
    assert!(b.starts_with("%\n(VMC 3-AXIS)") && b.contains("Face2") && b.contains("G55"), "{b}");
    assert!(!b.contains("G710") && !b.contains("Face1"), "{b}");

    // Undo steps back through the generate to Setup2's pick (bounded) and
    // takes it: Setup2 follows the document's Fanuc again, Setup1 keeps its own.
    let own = |c: &mut Client, i: usize| c.ok("get_job", json!({}))["setups"][i]["post"].clone();
    for _ in 0..4 {
        if own(&mut c, 1) == Value::Null {
            break;
        }
        c.ok("undo", json!({}));
    }
    assert_eq!(own(&mut c, 1), Value::Null, "the pick is undone");
    assert!(own(&mut c, 0).as_str().unwrap_or_default().contains("Siemens"), "Setup1 keeps its pick");
    c.ok("generate", json!({}));
    let after = c.post(&out);
    assert!(after[0].1 == *a, "Setup1 keeps its own post");
    assert!(after[1].1.starts_with("%\nO0001 (VMC 3-AXIS)") && after[1].1.contains("Face2"), "{}", after[1].1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// #1233: `set_setup_post` sets the field only. A milling post on a setup
/// carrying milling work, given a lathe post, is accepted by the tool; the
/// later `post` refuses it by name (#1213) and writes nothing.
#[test]
fn a_post_that_does_not_serve_the_setups_machine_is_accepted_then_refused_at_post() {
    let dir = tmp_dir("kind");
    let project = saved_project(&dir);
    let out = dir.join("job.nc");
    let mut c = Client::new();
    c.ok("open_project", json!({ "path": project.display().to_string() }));
    let r = c.ok("set_setup_post", json!({ "setup": "Setup2", "post": "Fanuc Lathe" }));
    assert_eq!(r["own_post"], json!(true), "{r}");
    c.ok("generate", json!({}));
    let reply = c.request("tools/call", json!({ "name": "post", "arguments": { "output": out.display().to_string() } }));
    assert_eq!(reply["result"]["isError"], json!(true), "{reply}");
    let text = reply.to_string();
    assert!(text.contains("lathe post") && text.contains("Face2"), "refusal names the post and setup: {text}");
    assert!(!dir.join("job_02_Setup2.nc").exists() && !out.exists(), "nothing written");
    let _ = std::fs::remove_dir_all(&dir);
}
