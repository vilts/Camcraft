//! #1871: plasma setup, editing, verification and posting through headless MCP.
//! Gate map (#1871): covers app::mcp::tools, app::mcp::edit, app::mcp::job,
//! app::mcp::tool_edit, app::mcp::setup_tools, app::posting_plasma.
//! Provenance: our own MCP tests; the holed plate follows plasma_gate.rs.

mod pocket_support;

use camcraft_app::cl::{ContourRole, MoveKind, PathBuilder, TorchSpan};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use camcraft_app::{model, project};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const POST: &str = "Tormach PathPilot Plasma";
const TORCH: &str = "PT-KERF-150";

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn new() -> Self {
        Self { server: Server::new(), next: 1 }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.server.handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .expect("a request gets a response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let err = result["isError"] == json!(true);
        (err, if err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused: {body}");
        body.as_str().expect("tool error text").to_owned()
    }

    fn params(&mut self) -> Value {
        self.ok("get_job", json!({}))["operations"][0]["params"]["plasma_profile"].clone()
    }
}

fn tmp_dir(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("camcraft-mcp-plasma-1871-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("temp directory");
    path
}

/// A 40 x 40 x 5 mm plate with a 10 x 10 mm through hole.
fn holed_plate() -> Mesh {
    let ring = |h: f64, z: f64| [Point3::new(-h, -h, z), Point3::new(h, -h, z), Point3::new(h, h, z), Point3::new(-h, h, z)];
    let quad = |a, b, c, d| [Triangle::new(a, b, c), Triangle::new(a, c, d)];
    let mut facets = Vec::new();
    for (z, up) in [(5.0, true), (0.0, false)] {
        let (outer, inner) = (ring(20.0, z), ring(5.0, z));
        for i in 0..4 {
            let j = (i + 1) % 4;
            facets.extend(if up { quad(outer[i], outer[j], inner[j], inner[i]) } else { quad(outer[i], inner[i], inner[j], outer[j]) });
        }
    }
    for (radius, outward) in [(20.0, true), (5.0, false)] {
        let (lo, hi) = (ring(radius, 0.0), ring(radius, 5.0));
        for i in 0..4 {
            let j = (i + 1) % 4;
            facets.extend(if outward { quad(lo[i], lo[j], hi[j], hi[i]) } else { quad(lo[i], hi[i], hi[j], lo[j]) });
        }
    }
    Mesh::from_facets(facets)
}

fn plasma_job(dir: &Path) -> Client {
    let stl = dir.join("plate.stl");
    pocket_support::write_stl(&holed_plate(), &stl);
    let mut c = Client::new();
    c.ok("load_machine", json!({ "builtin": "Tormach 1300PL" }));
    c.ok("add_tool", json!({ "catalog": TORCH }));
    c.ok("import_model", json!({ "path": stl }));
    let added = c.ok("add_operation", json!({
        "kind": "Plasma Profile", "name": "Plasma1", "tool": TORCH, "source": "plate",
        "params": { "clearance_z": 25.0, "top_z": 5.0, "bottom_z": 0.0,
            "lead_in": 4.0, "lead_out": 3.0, "overburn": 1.0 }
    }));
    assert_eq!(added["kind"], json!("Plasma Profile"));
    c
}

#[test]
fn post_schema_advertises_the_builtin_plasma_post() {
    let mut c = Client::new();
    let reply = c.request("tools/list", json!({}));
    let tools = reply["result"]["tools"].as_array().expect("tool list");
    let post = tools.iter().find(|t| t["name"] == "post").expect("post tool");
    let description = post["inputSchema"]["properties"]["post"]["description"].as_str().expect("post description");
    assert!(description.contains(POST), "the shipped plasma post must be discoverable: {description}");
}

#[test]
fn plasma_profile_is_edited_undone_redone_generated_verified_and_posted_over_mcp() {
    let dir = tmp_dir("workflow");
    let mut c = plasma_job(&dir);
    let before = c.params();
    let edited = c.ok("edit_operation", json!({ "name": "Plasma1", "params": { "pierce_delay": 0.5 } }));
    assert_eq!(edited["params"]["plasma_profile"]["pierce_delay"], json!(0.5));
    c.ok("undo", json!({}));
    assert_eq!(c.params(), before);
    c.ok("redo", json!({}));
    assert_eq!(c.params()["pierce_delay"], json!(0.5));
    let generated = c.ok("generate", json!({ "name": "Plasma1" }));
    assert_eq!(generated["generated"][0]["status"], json!("ok"), "{generated}");
    let verified = c.ok("verify", json!({}));
    assert_eq!(verified["ok"], json!(true), "{verified}");
    assert_eq!(verified["gate"]["problems"], json!([]));
    let clause = verified["gate"]["not_judged"].as_str().expect("explicit plasma judgment scope");
    assert!(clause.contains("Plasma1") && clause.contains("2D plasma laws") && clause.ends_with("clean"), "{clause}");
    let out = dir.join("plate.nc");
    let posted = c.ok("post", json!({ "post": POST, "output": out }));
    assert!(posted["lines"].as_u64().is_some_and(|n| n > 0), "{posted}");
    let nc = std::fs::read_to_string(&out).expect("posted file");
    for word in ["M200", "G15", "G16", "M205", "M209 P0.5"] {
        assert_eq!(nc.lines().filter(|line| *line == word).count(), 2, "{word} for hole and outline: {nc}");
    }
    assert!(nc.ends_with("G30\nM30\n"), "{nc}");
    std::fs::remove_dir_all(dir).expect("clean temporary files");
}

#[test]
fn plasma_mcp_refuses_a_non_torch_nonzero_rpm_and_a_milling_post() {
    let dir = tmp_dir("refusals");
    let mut c = plasma_job(&dir);
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    let before = c.ok("get_job", json!({}));
    let why = c.refused("set_tool", json!({ "operation": "Plasma1", "tool": "EM-0600-4FL" }));
    assert!(why.contains("EM-0600-4FL") && why.contains("Plasma Profile") && why.contains("plasma torch"), "{why}");
    assert_eq!(c.ok("get_job", json!({})), before, "a refused tool assignment changes nothing");
    let why = c.refused("edit_tool", json!({ "name": TORCH, "patch": { "rpm": 3000.0 } }));
    assert!(why.contains(TORCH) && why.contains("plasma torch turns no spindle"), "{why}");
    assert_eq!(c.ok("get_job", json!({})), before, "a refused rpm changes nothing");
    c.ok("generate", json!({ "name": "Plasma1" }));
    let out = dir.join("refused.nc");
    let why = c.refused("post", json!({ "post": "Fanuc", "output": out }));
    assert!(why.contains("Fanuc") && why.contains("plasma cutting"), "{why}");
    assert!(!out.exists(), "a refused post writes no NC file");
    std::fs::remove_dir_all(dir).expect("clean temporary files");
}

/// Corrupt only the saved path cache: keep sane torch spans, but lay the
/// outline before its hole. This is a post-gate backstop, not an MCP edit.
fn save_bad_cut_order(path: &Path) {
    let world = camcraft_app::world::new_world();
    project::load(&world, path).expect("load generated document");
    let entity = model::find_by_name(&world, "Plasma1").expect("operation");
    let cache = model::toolpath_of(&world, &entity).expect("cached path");
    let record = cache.path.plasma.as_ref().expect("plasma record");
    let (hole, outline) = (record.torches[0], record.torches[1]);
    assert_eq!((hole.role, outline.role), (ContourRole::Inner, ContourRole::Outer));
    let moves = &cache.path.moves;
    let mut builder = PathBuilder::new(Point3::new(moves[0].from.x, moves[0].from.y, 25.0));
    let mut torches = Vec::new();
    for span in [outline, hole] {
        let pierce = moves[span.on].from;
        builder.push(MoveKind::Rapid, Point3::new(builder.at().x, builder.at().y, 25.0), None);
        builder.push(MoveKind::Rapid, Point3::new(pierce.x, pierce.y, 25.0), None);
        builder.push(MoveKind::Rapid, pierce, None);
        let on = builder.moves_len();
        for m in &moves[span.on..=span.off] {
            match m.arc {
                Some(a) => builder.push_arc(MoveKind::Feed, m.to, m.feed, a.center, a.ccw),
                None => builder.push(MoveKind::Feed, m.to, m.feed),
            }
        }
        torches.push(TorchSpan { on, off: builder.moves_len() - 1, ..span });
    }
    builder.push(MoveKind::Rapid, Point3::new(builder.at().x, builder.at().y, 25.0), None);
    let mut bad = builder.finish();
    let mut swapped = record.clone();
    swapped.torches = torches;
    assert!(swapped.is_sane(&bad.moves), "fixture is structurally sane");
    bad.plasma = Some(swapped);
    entity.entity_view(&world).set(camcraft_app::schema::Toolpath::new(bad, Vec::new()));
    project::save(&world, path).expect("save corrupted cache");
}

#[test]
fn plasma_mcp_reports_bad_cut_order_and_refuses_to_write_nc() {
    let dir = tmp_dir("order");
    let mut c = plasma_job(&dir);
    c.ok("generate", json!({ "name": "Plasma1" }));
    let file = dir.join("bad-order.camproj");
    c.ok("save_project", json!({ "path": file }));
    save_bad_cut_order(&file);
    c.ok("open_project", json!({ "path": file }));
    assert_eq!(c.ok("get_job", json!({}))["operations"][0]["status"], json!("ok"), "fixture cache stays fresh");
    let verified = c.ok("verify", json!({}));
    assert_eq!(verified["ok"], json!(false), "{verified}");
    assert_eq!(verified["gate"]["ok"], json!(false), "{verified}");
    let problems = verified["gate"]["problems"].as_array().expect("gate problems");
    assert!(problems.iter().any(|p| p.as_str().is_some_and(|s| s.contains("Plasma1") && s.contains("plasma cut order"))), "{verified}");
    let findings = verified["findings"].as_array().expect("findings");
    assert!(findings.iter().any(|f| f["kind"] == "post_gate" && f["blocks_post"] == true
        && f["message"].as_str().is_some_and(|s| s.contains("plasma cut order"))), "{verified}");
    let out = dir.join("bad-order.nc");
    let why = c.refused("post", json!({ "post": POST, "output": out }));
    assert!(why.contains("Plasma1") && why.contains("plasma cut order") && why.contains("C0 is cut after contour C1"), "{why}");
    assert!(!out.exists(), "a refused post writes no NC file");
    std::fs::remove_dir_all(dir).expect("clean temporary files");
}

/// #3456: replace the saved cache with two parts, C0 (0..20) then C1
/// (40..60), where the rapid into C1 runs over the freed C0 - a tip-up.
fn save_tip_up(path: &Path) {
    let world = camcraft_app::world::new_world();
    project::load(&world, path).expect("load generated document");
    let entity = model::find_by_name(&world, "Plasma1").expect("operation");
    let record = model::toolpath_of(&world, &entity).expect("cached path").path.plasma.expect("plasma record");
    let parts: [(u32, &[(f64, f64)]); 2] = [
        (0, &[(-3.0, 10.0), (0.0, 10.0), (0.0, 0.0), (20.0, 0.0), (20.0, 20.0), (0.0, 20.0), (0.0, 10.0), (-3.0, 13.0)]),
        (1, &[(63.0, 15.0), (60.0, 15.0), (60.0, 25.0), (40.0, 25.0), (40.0, 5.0), (60.0, 5.0), (60.0, 15.0), (63.0, 12.0)]),
    ];
    let mut b = PathBuilder::new(Point3::new(-3.0, 10.0, 25.0));
    let mut torches = Vec::new();
    for (contour, pts) in parts {
        let (px, py) = pts[0];
        b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, 25.0), None);
        b.push(MoveKind::Rapid, Point3::new(px, py, 25.0), None);
        b.push(MoveKind::Rapid, Point3::new(px, py, 0.0), None);
        let on = b.moves_len();
        for &(x, y) in &pts[1..] {
            b.push(MoveKind::Feed, Point3::new(x, y, 0.0), Some(2500.0));
        }
        torches.push(TorchSpan { contour, role: ContourRole::Outer, parent: None, on, off: b.moves_len() - 1, small: None });
    }
    b.push(MoveKind::Rapid, Point3::new(b.at().x, b.at().y, 25.0), None);
    let mut t = b.finish();
    let rec = camcraft_app::cl::PlasmaCut { torches, ..record };
    assert!(rec.is_sane(&t.moves), "fixture: a sane record");
    t.plasma = Some(rec);
    entity.entity_view(&world).set(camcraft_app::schema::Toolpath::new(t, Vec::new()));
    project::save(&world, path).expect("save the tip-up cache");
}

/// #3456: a tip-up the plasma laws find is a finding and an operation
/// warning, not only a clause inside `gate.not_judged`. The law warns:
/// it does not block the post, so `ok` stays true.
#[test]
fn plasma_mcp_verify_lists_a_tip_up_as_a_finding_and_an_operation_warning() {
    let dir = tmp_dir("tip-up");
    let mut c = plasma_job(&dir);
    c.ok("generate", json!({ "name": "Plasma1" }));
    let file = dir.join("tip-up.camproj");
    c.ok("save_project", json!({ "path": file }));
    save_tip_up(&file);
    c.ok("open_project", json!({ "path": file }));
    let verified = c.ok("verify", json!({}));
    let tip = |s: &str| s.contains("tip-up") && s.contains("contour C1") && s.contains("part C0");
    let findings = verified["findings"].as_array().expect("findings");
    let found: Vec<&Value> = findings.iter().filter(|f| f["kind"] == "plasma_tip_up").collect();
    assert_eq!(found.len(), 1, "one tip-up finding: {verified}");
    assert_eq!(found[0]["op"], json!("Plasma1"), "{verified}");
    assert_eq!(found[0]["blocks_post"], json!(false), "the tip-up law warns: {verified}");
    assert!(found[0]["message"].as_str().is_some_and(tip), "{verified}");
    let warnings = verified["operations"][0]["warnings"].as_array().expect("operation warnings");
    assert!(warnings.iter().any(|w| w.as_str().is_some_and(tip)), "the op's own warnings name it: {verified}");
    assert_eq!(verified["gate"]["problems"], json!([]), "{verified}");
    assert_eq!(verified["held_by"], json!([]), "a warning holds nothing: {verified}");
    assert_eq!(verified["ok"], json!(true), "{verified}");
    std::fs::remove_dir_all(dir).expect("clean temporary files");
}
