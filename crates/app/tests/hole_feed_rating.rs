//! #2598: a Drill and a Thread op take their feed from the tool when they
//! are created or their tool is set, and a feed above the tool's own feed
//! is refused at the post by name. The issue's posts: DR-0600 (feed 115)
//! drilled at F300 and TM-0600-P125 (feed 400) milled at 1200 mm/min at
//! the cutting edge, silently.
//!
//! Gate map: covers app::operation::feed_rating (the seed and the post
//! clause), app::cutting_rows (seed_feed_for_tool, retargeted_params),
//! app::commands::operation (EditOperationLinks), app::posting
//! (verification_report), app::mcp::edit (add_operation, set_tool) and the
//! project file round trip of a hand-set feed.
//!
//! Provenance: the issue's own MCP reproduction on a synthetic plate (the
//! catalog drill and thread mill); no vendor material consulted.

use std::path::{Path, PathBuf};

use camcraft_app::mcp::Server;
use camcraft_app::project;
use camcraft_app::tool_cutting_rows::CuttingRow;
use camcraft_app::tools::{Tool, WorkMaterial, catalog};
use serde_json::{json, Value};

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call",
            "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} was expected to refuse: {body}");
        body.as_str().unwrap_or_default().to_owned()
    }

    /// The (feed, plunge_feed) of the named operation, as the job reads it.
    fn feeds(&mut self, op: &str) -> (f64, f64) {
        let job = self.ok("get_job", json!({}));
        let ops = job["operations"].as_array().cloned().unwrap_or_default();
        let o = ops.iter().find(|o| o["name"] == json!(op)).unwrap_or_else(|| panic!("no op {op}: {job}"));
        let fields = o["params"].as_object().and_then(|m| m.values().next()).cloned().unwrap_or_default();
        (fields["feed"].as_f64().unwrap_or(f64::NAN), fields["plunge_feed"].as_f64().unwrap_or(f64::NAN))
    }
}

const H: f64 = 20.0;
const TOP: f64 = 10.0;
/// Sides of the hole polygon: 1.8 um of sagitta at `R`, under the verify's
/// reach (`thread_mill_rim.rs` holds 2.7 um clean).
const N: usize = 96;
/// An M8 x 1.25 tap drill's radius: the thread's minor circle.
const R: f64 = 3.3235;

type P = [f64; 3];

/// A 40 x 40 plate, 10 mm thick, with one through hole of radius `R` at
/// its centre. The hole's vertices are pushed radially onto the square
/// so the mesh closes; facets are wound by their wanted normal.
fn plate() -> Vec<[P; 3]> {
    let mut tris: Vec<[P; 3]> = Vec::new();
    let mut quad = |a: P, b: P, c: P, d: P, want: P| {
        for t in [[a, b, c], [a, c, d]] {
            let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
            let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
            let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
            let dot = n[0] * want[0] + n[1] * want[1] + n[2] * want[2];
            tris.push(if dot >= 0.0 { t } else { [t[0], t[2], t[1]] });
        }
    };
    let ang = |i: usize| std::f64::consts::TAU * (i % N) as f64 / N as f64;
    let bp = |i: usize, z: f64| [R * ang(i).cos(), R * ang(i).sin(), z];
    let sq = |i: usize, z: f64| {
        let (x, y) = (ang(i).cos(), ang(i).sin());
        let s = x.abs().max(y.abs());
        [x * H / s, y * H / s, z]
    };
    for i in 0..N {
        let j = i + 1;
        quad(bp(i, TOP), bp(j, TOP), sq(j, TOP), sq(i, TOP), [0.0, 0.0, 1.0]);
        quad(bp(i, 0.0), bp(j, 0.0), sq(j, 0.0), sq(i, 0.0), [0.0, 0.0, -1.0]);
        let mid = ang(i) + std::f64::consts::PI / N as f64;
        quad(bp(i, TOP), bp(j, TOP), bp(j, 0.0), bp(i, 0.0), [-mid.cos(), -mid.sin(), 0.0]);
        let (a, b) = (sq(i, TOP), sq(j, TOP));
        let out = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5, 0.0];
        quad(a, b, sq(j, 0.0), sq(i, 0.0), out);
    }
    tris
}

fn temp_dir(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-hole-feed-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn write_stl(dir: &Path) -> PathBuf {
    let path = dir.join("plate.stl");
    let mut s = String::from("solid plate\n");
    for t in plate() {
        s.push_str(" facet normal 0 0 0\n  outer loop\n");
        for p in t {
            s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
        }
        s.push_str("  endloop\n endfacet\n");
    }
    s.push_str("endsolid plate\n");
    std::fs::write(&path, s).expect("write stl");
    path
}

/// The issue's sequence: the plate, its stock and material, the catalog
/// drill and thread mill, a Drill op `D` and a Thread op `T` on the hole.
fn setup(dir: &Path) -> Client {
    let stl = write_stl(dir);
    let mut c = Client::new();
    c.ok("import_model", json!({ "path": stl.display().to_string(), "name": "plate" }));
    c.ok("set_stock", json!({ "from_model": "plate" }));
    c.ok("add_tool", json!({ "catalog": "DR-0600" }));
    c.ok("add_tool", json!({ "catalog": "TM-0600-P125" }));
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0600", "source": "plate", "name": "D" }));
    c.ok("detect_holes", json!({ "name": "D" }));
    // The thread is blind, 6 mm deep, in the drilled hole: the shop's
    // tap-drill-deeper-than-thread layout (`thread_support`).
    let hole = json!({ "xy": [0.0, 0.0], "radius": R, "entry_z": TOP, "floor_z": 4.0, "through": false });
    c.ok("add_operation", json!({ "kind": "Thread", "tool": "TM-0600-P125", "source": "plate", "name": "T",
        "params": { "holes": [hole] } }));
    c
}

/// Every F word of a posted program, in order, with its line.
fn f_words(nc: &str) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    for line in nc.lines() {
        for w in line.split_whitespace() {
            if let Some(v) = w.strip_prefix('F').and_then(|v| v.parse::<f64>().ok()) {
                out.push((v, line.to_owned()));
            }
        }
    }
    out
}

#[test]
fn drill_and_thread_mill_take_the_tool_feed_and_post_it() {
    let dir = temp_dir("seed");
    let mut c = setup(&dir);
    assert_eq!(c.feeds("D"), (115.0, 115.0), "the drill feeds at DR-0600's 115 mm/min");
    let (feed, plunge) = c.feeds("T");
    assert_eq!(feed, 400.0, "the thread mill feeds at TM-0600-P125's 400 mm/min at the edge");
    assert!(plunge <= 400.0, "its plunge is not above the tool's feed: {plunge}");
    let g = c.ok("generate", json!({}));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "a seeded program posts: {g} {v}");
    let nc = dir.join("out.nc");
    c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    let text = std::fs::read_to_string(&nc).expect("the program is written");
    let fs = f_words(&text);
    let cycles: Vec<_> = fs.iter().filter(|(_, l)| l.contains("G73") || l.contains("G81") || l.contains("G83")).collect();
    assert!(!cycles.is_empty(), "the drill posts a canned cycle:\n{text}");
    for (f, l) in &cycles {
        assert_eq!(*f, 115.0, "the drill cycle posts the tool's feed: {l}");
    }
    let arcs: Vec<_> = fs.iter().filter(|(_, l)| ["G02", "G03", "G2 ", "G3 "].iter().any(|g| l.contains(g))).collect();
    assert!(!arcs.is_empty(), "the thread mill posts its helix:\n{text}");
    for (f, l) in &fs {
        assert!(*f <= 400.0, "no posted feed is above the tools' feeds: {l}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_feed_above_the_tool_is_refused_at_the_post_by_name_and_kept_on_load() {
    let dir = temp_dir("over");
    let mut c = setup(&dir);
    // The old seed: what every file before #2598 carries.
    c.ok("edit_operation", json!({ "name": "D", "params": { "feed": 1200.0, "plunge_feed": 300.0 } }));
    c.ok("edit_operation", json!({ "name": "T", "params": { "feed": 1200.0 } }));
    // Undo puts the tool's feed back; redo states the old feeds again.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    assert_eq!(c.feeds("D"), (115.0, 115.0), "undo restores the tool's drill feed");
    assert_eq!(c.feeds("T").0, 400.0, "undo restores the tool's thread feed");
    c.ok("redo", json!({}));
    c.ok("redo", json!({}));
    assert_eq!(c.feeds("D"), (1200.0, 300.0));
    c.ok("generate", json!({}));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(false), "a feed above the tool's holds the post: {v}");
    let text = v.to_string();
    assert!(text.contains("D: feeds 300 mm/min") && text.contains("DR-0600") && text.contains("115 mm/min"), "{v}");
    assert!(text.contains("T: feeds 1200 mm/min") && text.contains("TM-0600-P125") && text.contains("400 mm/min"), "{v}");
    let nc = dir.join("out.nc");
    let why = c.refused("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(why.contains("D: feeds 300 mm/min"), "the post names the op and the feed: {why}");
    assert!(!nc.exists(), "nothing is written");

    // A saved file keeps the stated numbers: nothing moves them on load.
    let file = dir.join("old.camproj");
    c.ok("save_project", json!({ "path": file.display().to_string() }));
    let mut d = Client::new();
    d.ok("open_project", json!({ "path": file.display().to_string() }));
    assert_eq!(d.feeds("D"), (1200.0, 300.0), "the stated drill feeds load as they were saved");
    assert_eq!(d.feeds("T").0, 1200.0, "the stated thread feed loads as it was saved");
    d.ok("generate", json!({}));
    let why = d.refused("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(why.contains("D: feeds 300 mm/min") && why.contains("T: feeds 1200 mm/min"), "{why}");

    // Feeds at the tools' own post again.
    c.ok("edit_operation", json!({ "name": "D", "params": { "feed": 115.0, "plunge_feed": 115.0 } }));
    c.ok("edit_operation", json!({ "name": "T", "params": { "feed": 400.0 } }));
    c.ok("generate", json!({}));
    c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(nc.exists(), "the program is written");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn setting_a_new_tool_takes_its_feed_and_undo_restores_the_old_one() {
    let dir = temp_dir("retarget");
    let mut c = setup(&dir);
    c.ok("add_tool", json!({ "catalog": "DR-0300" }));
    c.ok("edit_operation", json!({ "name": "D", "params": { "feed": 90.0, "plunge_feed": 80.0 } }));
    c.ok("set_tool", json!({ "operation": "D", "tool": "DR-0300" }));
    assert_eq!(c.feeds("D"), (120.0, 120.0), "the drill takes DR-0300's 120 mm/min in the same step");
    c.ok("undo", json!({}));
    assert_eq!(c.feeds("D"), (90.0, 80.0), "undo restores the hand-set feeds");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #2724: DR-0600 with a cutting-data row for mild steel at its own 115
/// mm/min, on a mild-steel document. The user lowers the running feed to
/// 80; the row stays at 115. The Drill op at 115 was passed by the post
/// (the row alone was the rating); the lower of the two is the rating now.
#[test]
fn a_running_feed_lowered_below_the_material_row_is_the_rating() {
    let dir = temp_dir("running");
    let mut c = setup(&dir);
    let dr = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t).expect("catalog drill");
    let row = CuttingRow::plain(Some(WorkMaterial::MildSteel), dr.rpm, 115.0);
    let def = project::tool_to_json(&Tool { cutting: vec![row], ..dr });
    c.ok("edit_tool", json!({ "name": "DR-0600", "patch": { "cutting_data": def["cutting_data"].clone() } }));
    c.ok("set_work_material", json!({ "material": "mild_steel" }));
    assert_eq!(c.feeds("D"), (115.0, 115.0), "the drill feeds at the steel row's 115 mm/min");
    c.ok("generate", json!({}));
    assert_eq!(c.ok("verify", json!({}))["ok"], json!(true), "the row feed posts while the tool runs it");

    c.ok("edit_tool", json!({ "name": "DR-0600", "patch": { "feed": 80.0 } }));
    c.ok("generate", json!({}));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(false), "a feed above the lowered running feed holds the post: {v}");
    let text = v.to_string();
    assert!(text.contains("D: feeds 115 mm/min") && text.contains("80 mm/min") && text.contains("DR-0600"), "{v}");
    let nc = dir.join("out.nc");
    let why = c.refused("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(why.contains("D: feeds 115 mm/min, above the 80 mm/min"), "the post names the op and the limit: {why}");
    assert!(!nc.exists(), "nothing is written");

    // A new drill op on the tool seeds the feed that the check passes.
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0600", "source": "plate", "name": "D2" }));
    assert_eq!(c.feeds("D2"), (80.0, 80.0), "the seed is the lowered running feed, not the row");
    c.ok("undo", json!({}));

    c.ok("edit_operation", json!({ "name": "D", "params": { "feed": 80.0, "plunge_feed": 80.0 } }));
    c.ok("generate", json!({}));
    c.ok("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    let posted = std::fs::read_to_string(&nc).expect("the program is written");
    let cycles: Vec<_> = f_words(&posted).into_iter().filter(|(_, l)| l.contains("G73") || l.contains("G81") || l.contains("G83")).collect();
    assert!(!cycles.is_empty(), "the drill posts a canned cycle:\n{posted}");
    for (f, l) in &cycles {
        assert_eq!(*f, 80.0, "the drill cycle posts the lowered feed: {l}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// DR-0600 with `rows`, the document on mild steel, the running feed at
/// `running`: the tool as #2724's two control cases need it.
fn drill_with_rows(c: &mut Client, rows: Vec<CuttingRow>, running: f64) {
    let dr = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t).expect("catalog drill");
    let def = project::tool_to_json(&Tool { cutting: rows, ..dr });
    c.ok("edit_tool", json!({ "name": "DR-0600", "patch": { "cutting_data": def["cutting_data"].clone() } }));
    c.ok("set_work_material", json!({ "material": "mild_steel" }));
    c.ok("edit_tool", json!({ "name": "DR-0600", "patch": { "feed": running } }));
}

/// #2724, the row lower: the user raises the running feed to 200 above the
/// steel row's 115. The rating stays at 115 (the lower of the two). A
/// rule that took the higher number or the running feed alone gives 200:
/// then the op at 200 posts and a new op seeds 200, and the two asserts fail.
#[test]
fn a_running_feed_raised_above_the_material_row_keeps_the_row_rating() {
    let dir = temp_dir("raised");
    let mut c = setup(&dir);
    let rpm = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t.rpm).expect("catalog drill");
    drill_with_rows(&mut c, vec![CuttingRow::plain(Some(WorkMaterial::MildSteel), rpm, 115.0)], 200.0);
    c.ok("edit_operation", json!({ "name": "D", "params": { "feed": 200.0, "plunge_feed": 200.0 } }));
    c.ok("generate", json!({}));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(false), "a feed above the row holds the post when the running feed is higher: {v}");
    let nc = dir.join("out.nc");
    let why = c.refused("post", json!({ "output": nc.display().to_string(), "post": "Fanuc" }));
    assert!(why.contains("D: feeds 200 mm/min, above the 115 mm/min"), "the row is the limit: {why}");
    assert!(!nc.exists(), "nothing is written");
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0600", "source": "plate", "name": "D2" }));
    assert_eq!(c.feeds("D2"), (115.0, 115.0), "the seed is the row, not the raised running feed");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #2724, the unchanged control: the stock states brass, the document
/// mild steel. The running feed (80) follows the document material, so it
/// does not speak for brass: the brass row's 150 stays the rating. A rule
/// that took the lower number here too gives 80, and the asserts fail.
#[test]
fn a_stock_material_other_than_the_document_is_rated_by_its_own_row() {
    let dir = temp_dir("stock");
    let mut c = setup(&dir);
    let rpm = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t.rpm).expect("catalog drill");
    let rows = vec![
        CuttingRow::plain(Some(WorkMaterial::MildSteel), rpm, 115.0),
        CuttingRow::plain(Some(WorkMaterial::Brass), rpm, 150.0),
    ];
    drill_with_rows(&mut c, rows, 80.0);
    c.ok("set_stock_material", json!({ "material": "brass" }));
    c.ok("edit_operation", json!({ "name": "D", "params": { "feed": 150.0, "plunge_feed": 150.0 } }));
    c.ok("generate", json!({}));
    let v = c.ok("verify", json!({}));
    assert_eq!(v["ok"], json!(true), "the brass row's feed posts on brass stock: {v}");
    c.ok("add_operation", json!({ "kind": "drill", "tool": "DR-0600", "source": "plate", "name": "D2" }));
    assert_eq!(c.feeds("D2"), (150.0, 150.0), "the seed is the brass row, not the document's running feed");
    c.ok("undo", json!({}));
    c.ok("edit_operation", json!({ "name": "D", "params": { "feed": 160.0, "plunge_feed": 160.0 } }));
    c.ok("generate", json!({}));
    let v = c.ok("verify", json!({}));
    assert!(v.to_string().contains("D: feeds 160 mm/min, above the 150 mm/min"), "the brass row is the limit: {v}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// #2724, the review's note: a row whose feed is zero or not finite gives
/// no rating of its own, so the running feed is the rating.
#[test]
fn a_row_feed_that_is_not_usable_gives_the_running_feed() {
    use camcraft_app::operation::feed_rating::rated_feed;
    let dr = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t).expect("catalog drill");
    let steel = Some(WorkMaterial::MildSteel);
    for bad in [0.0, f64::NAN, f64::INFINITY, -5.0] {
        let tool = Tool { cutting: vec![CuttingRow::plain(steel, dr.rpm, bad)], feed: 90.0, ..dr.clone() };
        assert_eq!(rated_feed(&tool, steel, steel), Some(90.0), "row feed {bad}");
    }
}
