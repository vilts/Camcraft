//! The Wire Contour operation over MCP (#1108, part of #290): a wire EDM
//! mounted headlessly from a `.mch` path with a synthetic condition
//! table, a Wire Contour added with no tool, its side and condition
//! edited, generated, undone and redone; a tool named on the kind, an
//! unknown condition and an unnamed one come back as tool errors in the
//! command's own words, and `list_operation_types` says the kind
//! mounts no tool. A taper the machine cannot post is refused by the
//! edit door (#1800).
//!
//! Gate map (#1108): covers app::mcp (edit, job, tools, setup_tools),
//! app::commands (CreateOperation, EditOperationParams tool and wire
//! doors), app::operation (wire_op, wire_sketch_arcs).
//! Provenance: our own tests over our own fixtures. The conditions are
//! SYNTHETIC - invented numbers of a plausible size, from no machine's
//! manual (#290 D4, D10).

mod pocket_support;
// #3991: the tangent arc lead over MCP.
#[path = "mcp_wire_contour/arc_lead.rs"]
mod arc_lead;

use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;
use pocket_support::{rounded_rect, write_stl};

use serde_json::{json, Value};
use std::path::PathBuf;

const WIRE: &str = include_str!("edm/wire-edm.mch");

/// Synthetic: a complete rough condition (`C110`) and one with no gap.
const TABLE: &str = r#"[
    { "id": "C110", "name": "rough", "gap": 0.04, "wire_diameter": 0.25, "role": "rough" },
    { "id": "S9", "wire_diameter": 0.25 }
]"#;

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-wire-contour-1108-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
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
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }

    fn side(&mut self, op: &str) -> Value {
        let job = self.ok("get_job", json!({}));
        let ops = job["operations"].as_array().cloned().unwrap_or_default();
        let found = ops.iter().find(|o| o["name"] == json!(op) || o["key"] == json!(op)).cloned().unwrap_or(Value::Null);
        found["params"]["wire_contour"]["side"].clone()
    }
}

/// The outline extruded from z = 0 to 12: a slug whose through
/// silhouette is the outline.
fn prism(pts: &[(f64, f64)]) -> Mesh {
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        f.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    Mesh::from_facets(f)
}

/// A document with the synthetic wire EDM mounted from its file and a
/// 40 x 20 slug imported.
fn wire_job(tag: &str) -> Client {
    wire_job_on(tag, WIRE)
}

fn wire_job_on(tag: &str, template: &str) -> Client {
    let dir = tmp_dir(tag);
    let mut v: Value = serde_json::from_str(template).expect("template json");
    v["edm"]["conditions"] = serde_json::from_str(TABLE).expect("conditions json");
    let mch = dir.join("wire.mch");
    std::fs::write(&mch, v.to_string()).expect("write mch");
    let stl = dir.join("slug.stl");
    write_stl(&prism(&rounded_rect(20.0, 10.0, 2.0, 8)), &stl);
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("load_machine", json!({ "path": mch.display().to_string() }));
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("add_tool", json!({ "catalog": "EDM-W025", "holder": "none" }));
    c
}

fn wire_args(condition: &str) -> Value {
    json!({ "kind": "Wire Contour", "name": "W", "tool": "EDM-W025", "source": "slug",
        "params": { "condition": condition, "side": "die", "picks": { "start": [[0.0, 0.0]] } } })
}

#[test]
fn wire_contour_is_added_with_a_wire_tool_edited_generated_undone_and_redone() {
    let mut c = wire_job("flow");
    let added = c.ok("add_operation", wire_args("C110"));
    assert_eq!(added["kind"], json!("Wire Contour"));
    assert_eq!(c.side("W"), json!("die"));

    let edited = c.ok("edit_operation", json!({ "name": "W", "params": { "side": "punch" } }));
    assert_eq!(edited["params"]["wire_contour"]["side"], json!("punch"));
    c.ok("edit_operation", json!({ "name": "W", "params": { "picks": { "start": [[30.0, 0.0]] } } }));
    // The condition is patched too; an unknown one is refused by name.
    c.ok("edit_operation", json!({ "name": "W", "params": { "condition": "C110" } }));
    let bad = c.refused("edit_operation", json!({ "name": "W", "params": { "condition": "NOPE" } }));
    assert!(bad.contains("NOPE"), "{bad}");

    let g = c.ok("generate", json!({ "name": "W" }));
    assert!(g["generated"].is_array(), "{g}");
    let job = c.ok("get_job", json!({}));
    let status = job["operations"][0]["status"].clone();
    assert_ne!(status, json!("never"), "generated: {job}");

    // Undo walks the history back to the die the op was added with, and
    // redo forward again to the punch.
    let mut undone = 0;
    while c.side("W") != json!("die") {
        c.ok("undo", json!({}));
        undone += 1;
        assert!(undone < 8, "undo never reached the die");
    }
    for _ in 0..undone {
        c.ok("redo", json!({}));
    }
    assert_eq!(c.side("W"), json!("punch"));
}

#[test]
fn agiecut_template_refuses_a_wire_job_without_an_installed_condition() {
    let mut client = wire_job("agiecut-template");
    client.ok("load_machine", json!({ "builtin": "AGIECUT Classic 2S (template)" }));
    let why = client.refused("add_operation", wire_args("C110"));
    assert!(why.contains("states no EDM condition") && why.contains("edm.conditions"), "{why}");
    assert_eq!(client.ok("get_job", json!({}))["operations"], json!([]));
}

#[test]
fn wire_contour_refusals_arrive_as_tool_errors_in_the_commands_words() {
    let mut c = wire_job("refuse");
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL", "holder": "none" }));
    let wrong_tool = c.refused("add_operation", {
        let mut a = wire_args("C110");
        a["tool"] = json!("EM-0600-4FL");
        a
    });
    assert!(wrong_tool.contains("cuts with a wire"), "{wrong_tool}");
    let unknown = c.refused("add_operation", wire_args("NOPE"));
    assert!(unknown.contains("NOPE"), "{unknown}");
    let nogap = c.refused("add_operation", wire_args("S9"));
    assert!(nogap.to_lowercase().contains("gap"), "{nogap}");
    let none = c.refused("add_operation", wire_args(""));
    assert!(none.to_lowercase().contains("condition"), "{none}");
    // A mill kind still needs its tool.
    let mill = c.refused("add_operation", json!({ "kind": "Face" }));
    assert!(mill.contains("tool"), "{mill}");
    assert_eq!(c.ok("get_job", json!({}))["operations"].as_array().map_or(0, Vec::len), 0);
}

/// #3332: `edit_operation` sets the wire's exit pick beside its start,
/// `list_operation_types` says that the kind takes both, the pick the
/// wire cannot hold is refused by name, and undo takes the pick away.
#[test]
fn edit_operation_sets_the_wire_exit_pick_and_refuses_one_it_cannot_hold() {
    let mut c = wire_job("exit");
    c.ok("add_operation", wire_args("C110"));
    let types = c.ok("list_operation_types", json!({}));
    let kinds = types["kinds"].as_array().cloned().unwrap_or_default();
    let wire = kinds.iter().find(|k| k["label"] == json!("Wire Contour")).expect("kind listed");
    assert_eq!(wire["start_exit"]["start"], json!(true), "{wire}");
    assert_eq!(wire["start_exit"]["exit"], json!(true), "{wire}");

    let picks = |c: &mut Client| c.ok("get_job", json!({}))["operations"][0]["params"]["wire_contour"]["picks"].clone();
    let before = picks(&mut c);
    assert!(before.get("exit").is_none_or(|e| e == &json!([])), "a new wire cut has no exit pick: {before}");
    let patch = json!({ "name": "W", "params": { "picks": { "start": [[0.0, 0.0]], "exit": [[5.0, 1.0]] } } });
    c.ok("edit_operation", patch);
    assert_eq!(picks(&mut c)["exit"], json!([[5.0, 1.0]]));
    assert_eq!(picks(&mut c)["start"], json!([[0.0, 0.0]]));
    let g = c.ok("generate", json!({ "name": "W" }));
    assert!(g["generated"].is_array(), "{g}");

    // An exit beside a taper is refused at the door, and nothing changes.
    let why = c.refused("edit_operation", json!({ "name": "W", "params": { "taper_deg": 2.0 } }));
    assert!(why.contains("Wire Contour") && why.contains("exit point") && why.contains("taper"), "{why}");
    assert_eq!(picks(&mut c)["exit"], json!([[5.0, 1.0]]));
    // An exit outside a die's profile is refused by name at generation.
    c.ok("edit_operation", json!({ "name": "W", "params": { "picks": { "start": [[0.0, 0.0]], "exit": [[50.0, 0.0]] } } }));
    let (err, body) = c.call("generate", json!({ "name": "W" }));
    let text = body.to_string();
    assert!(text.contains("exit point at X50.000 Y0.000") && text.contains("outside the profile"), "{err}: {text}");

    // Undo: the bad pick, then the good pick.
    c.ok("undo", json!({}));
    assert_eq!(picks(&mut c)["exit"], json!([[5.0, 1.0]]));
    let mut undone = 0;
    while picks(&mut c).get("exit").is_some_and(|e| e != &json!([])) {
        c.ok("undo", json!({}));
        undone += 1;
        assert!(undone < 6, "undo never took the exit pick away");
    }
    assert_eq!(picks(&mut c)["start"], json!([[0.0, 0.0]]));
}

#[test]
fn list_operation_types_says_the_wire_kind_mounts_a_wire_tool() {
    let mut c = Client { server: Server::new(), next: 1 };
    let types = c.ok("list_operation_types", json!({}));
    let kinds = types["kinds"].as_array().cloned().unwrap_or_default();
    let wire = kinds.iter().find(|k| k["label"] == json!("Wire Contour")).expect("kind listed");
    assert_eq!(wire["mounts_tool"], json!(true));
    assert_eq!(wire["tool_kinds"], json!(["Wire"]));
    let face = kinds.iter().find(|k| k["label"] == json!("Face")).expect("face");
    assert_eq!(face["mounts_tool"], json!(true));
    assert!(face.get("note").is_none());
}

#[test]
fn a_wire_contour_cannot_post_over_mcp_and_writes_no_file() {
    let mut c = wire_job("post");
    c.ok("add_operation", wire_args("C110"));
    c.ok("generate", json!({ "name": "W" }));
    let out = tmp_dir("post-out").join("wire.nc");
    let why = c.refused("post", json!({ "output": out.display().to_string() }));
    let low = why.to_lowercase();
    assert!(low.contains("wire") || low.contains("edm"), "refusal names the wire cut: {why}");
    assert!(!out.exists(), "a refused post writes no file");
}

/// #1800: a taper the mounted machine cannot post is refused over MCP by
/// the same command door the GUI page passes - this machine file states
/// no edm.max_taper_deg - and the op keeps its vertical cut.
#[test]
fn a_taper_edit_the_machine_cannot_post_is_a_tool_error() {
    let mut c = wire_job("taper");
    c.ok("add_operation", wire_args("C110"));
    let why = c.refused("edit_operation", json!({ "name": "W", "params": { "taper_deg": 3.0 } }));
    assert!(why.contains("3.000 degree taper is asked") && why.contains("states no edm.max_taper_deg"), "{why}");
    let job = c.ok("get_job", json!({}));
    let op = job["operations"][0].to_string();
    assert!(!op.contains("\"taper_deg\":3"), "the refused taper never lands: {op}");
}

#[test]
fn wire_contour_accepts_an_explicit_zero_taper_on_add_and_edit() {
    let mut c = wire_job_on("zero-taper", include_str!("edm/wire-iso-fixture.mch"));
    let mut args = wire_args("C110");
    args["params"]["taper_deg"] = json!(0);
    c.ok("add_operation", args);
    let tapered = c.ok("edit_operation", json!({ "name": "W", "params": { "taper_deg": 3 } }));
    assert_eq!(tapered["params"]["wire_contour"]["taper_deg"], json!(3.0));
    let vertical = c.ok("edit_operation", json!({ "name": "W", "params": { "taper_deg": 0.0 } }));
    assert!(vertical["params"]["wire_contour"].as_object().expect("wire parameters").get("taper_deg").is_none());

    let why = c.refused("edit_operation", json!({ "name": "W", "params": { "taper_dge": 0 } }));
    assert!(why.contains("unknown field taper_dge"), "{why}");
    let why = c.refused("add_operation", json!({ "kind": "Face", "tool": "EDM-W025", "params": { "taper_deg": 0 } }));
    assert!(why.contains("unknown field taper_deg"), "{why}");
}

#[test]
fn list_operation_types_and_skill_match_wire_contour_dto_and_tool_law() {
    use camcraft_app::project::{
        WireContourDto, WireFourAxisDto, WirePairingDto, WireSideDto, WireSkimDto, WireSyncDto, WireTabsDto,
        WireWideningDto,
    };
    use std::collections::BTreeSet;

    let mut c = Client { server: Server::new(), next: 1 };
    let reply = c.ok("list_operation_types", json!({ "kind": "wire_contour" }));
    assert!(reply.to_string().chars().count() <= 4_000, "compact budget: {reply}");
    let wire = &reply["kinds"][0];
    assert_eq!(wire["key"], json!("wire_contour"));
    assert_eq!(wire["mounts_tool"], json!(true));
    assert_eq!(wire["tool_kinds"], json!(["Wire"]));

    let listed: BTreeSet<String> = wire["fields"]
        .as_array()
        .expect("fields array")
        .iter()
        .map(|f| f["name"].as_str().expect("field name").to_owned())
        .collect();

    // Populate every WireContourDto field with a non-default value so
    // `skip_serializing_if` omits none of them.
    let populated = WireContourDto {
        top_z: serde_json::from_value(json!({ "reference": "fixed", "offset": 12.0 })).expect("top_z"),
        bottom_z: serde_json::from_value(json!({ "reference": "fixed", "offset": 0.0 })).expect("bottom_z"),
        condition: "C110".into(),
        side: WireSideDto::Die,
        start: None,
        picks: camcraft_app::project::PathPicksDto {
            start: vec![camcraft_app::project::PathPickDto(camcraft_app::path_picks::PathPick::xy(
                camcraft_app::geo2d::Point2::new(0.0, 0.0),
            ))],
            exit: Vec::new(),
        },
        tabs: Some(WireTabsDto {
            count: Some(1),
            width: Some(1.0),
            at: vec![[5.0, 0.0]],
            cutoff_condition: "C120".into(),
        }),
        capture: true,
        both_pieces_restrained: true,
        stock_centered_in_xy_travel: true,
        taper_deg: 2.0,
        burnout_clearance: Some(1.0),
        clearance: 2.5,
        skims: vec![WireSkimDto { condition: "C111".into(), reverse: false }],
        four_axis: Some(WireFourAxisDto {
            pairing: WirePairingDto::ArcLength,
            sync: vec![WireSyncDto { upper: Some([0.0, 0.0]), lower: Some([0.0, 0.0]) }],
            tolerance: 0.05,
            widening: WireWideningDto::Uniform,
            verify_tolerance: 0.01,
        }),
        sharp_corners: true,
        lead_arc_radius: Some(0.5),
    };
    let dto_keys: BTreeSet<String> = serde_json::to_value(&populated)
        .expect("serialize WireContourDto")
        .as_object()
        .expect("object")
        .keys()
        .cloned()
        .collect();
    assert_eq!(listed, dto_keys, "list_operation_types wire_contour fields must match WireContourDto");

    // Pin the skill documentation against the live fields, tool law, machines and posts.
    let skill_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.claude/skills/camcraft-mcp/SKILL.md");
    let skill = std::fs::read_to_string(&skill_path).expect("read camcraft-mcp SKILL.md");
    for field in &dto_keys {
        assert!(skill.contains(&format!("`{field}`")), "SKILL.md missing Wire Contour field `{field}`");
    }
    for required in [
        "EDM-W025",
        "Sodick ALC400G",
        "AGIECUT Classic 2S (template)",
        "Generic ISO Wire EDM",
        "Sodick Wire EDM (Mark 21)",
        "Native wire EDM job files",
        "AGIE_QN2",
    ] {
        assert!(skill.contains(required), "SKILL.md missing `{required}`");
    }
    assert!(!skill.contains("today the Wire Contour"), "SKILL.md still claims Wire Contour mounts no tool");
    assert!(!skill.contains("the wire post is #533"), "SKILL.md still claims wire post does not exist");
    // #3537: the old post name is a hidden alias, never a listed name.
    assert!(!skill.contains("Agie native wire EDM"), "SKILL.md still lists the hidden alias post name");

    // Omitting `tool` on add_operation for Wire Contour must refuse by name,
    // and listed fields are accepted by edit_operation.
    let mut job = wire_job("missing-tool");
    let missing = job.refused(
        "add_operation",
        json!({ "kind": "Wire Contour", "source": "slug", "params": { "condition": "C110", "side": "die" } }),
    );
    assert!(missing.contains("tool"), "{missing}");
    job.ok("add_operation", wire_args("C110"));
    job.ok(
        "edit_operation",
        json!({ "name": "W", "params": { "burnout_clearance": 1.0, "sharp_corners": true, "capture": true, "clearance": 2.0 } }),
    );
}
