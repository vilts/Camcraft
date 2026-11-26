//! #1162: the MCP `edit_machine` tool - a merge patch over the mounted
//! sinker machine's `edm` section, one undo step, every refusal named
//! by key with nothing changed, and `get_job` reading the result back.
//!
//! Gate map (#1162): covers app::mcp::machine_tools, app::mcp::job (machine),
//! post::machine::edm_named.

mod pocket_support;

use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::mcp::Server;

use serde_json::{json, Value};
use std::path::PathBuf;

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

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
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
}

fn sinker() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/edm/sinker-edm.mch")
}

fn edm(c: &mut Client) -> Value {
    c.ok("get_job", json!({}))["machine"]["definition"]["edm"].clone()
}

/// #2602: a kind edit cannot give a mill a turning spindle.
#[test]
fn kind_edit_cannot_relabel_a_mill_for_turning() {
    let mill = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tormach-770mx.mch");
    for setup in [None, Some("Turning")] {
        let mut c = Client::new();
        if let Some(name) = setup {
            c.ok("add_setup", json!({ "name": name }));
        }
        let mut load = json!({ "path": mill.display().to_string() });
        let mut edit = json!({ "patch": { "model": "Edited mill model" } });
        if let Some(name) = setup {
            load["setup"] = json!(name);
            edit["setup"] = json!(name);
        }
        c.ok("load_machine", load);
        let machine = c.ok("edit_machine", edit.clone())["machine"].clone();
        let before = c.ok("get_job", json!({}));
        for kind in ["lathe", "turning", "mill-turn"] {
            edit["patch"] = json!({ "kind": kind });
            let refusal = c.refused("edit_machine", edit.clone());
            assert!(refusal.contains("kind") && refusal.contains("turning"), "{refusal}");
            assert_eq!(c.ok("get_job", json!({})), before, "a refusal changes nothing");
        }
        c.ok("undo", json!({}));
        // This must change the model again: a refusal adds no undo step.
        edit["patch"] = json!({ "model": "Edited mill model" });
        assert_eq!(c.ok("edit_machine", edit)["machine"], machine, "the setup keeps its mill definition too");
    }
}

#[test]
fn an_existing_lathe_keeps_its_machine_edits() {
    let lathe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/machines/tormach-15l-slant-pro.mch");
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": lathe.display().to_string() }));
    let result = c.ok("edit_machine", json!({ "patch": { "kind": "lathe", "model": "Edited lathe model" } }));
    assert_eq!(result["machine"]["model"], json!("Edited lathe model"));
}

#[test]
fn an_edm_edit_is_one_undo_step_and_get_job_reads_it_back() {
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": sinker().display().to_string() }));
    let cond = json!({ "id": "C110", "gap": 0.04, "undersize": 0.1, "role": "rough" });
    let r = c.ok("edit_machine", json!({ "patch": { "edm": {
        "max_orbit_radius": 0.5, "pickup": "chuck", "orbits": [{ "kind": "circular", "word": "OC" }], "conditions": [cond] } } }));
    assert_eq!(r["machine"]["edm"]["max_orbit_radius"], json!(0.5), "{r}");
    let e = edm(&mut c);
    assert_eq!(e["conditions"][0]["id"], json!("C110"), "{e}");
    assert_eq!(e["orbits"][0]["word"], json!("OC"));
    c.ok("undo", json!({}));
    assert!(edm(&mut c).get("conditions").is_none(), "undo takes the whole edit back");
    c.ok("redo", json!({}));
    assert_eq!(edm(&mut c)["max_orbit_radius"], json!(0.5));
    // A null removes a key.
    c.ok("edit_machine", json!({ "patch": { "edm": { "pickup": null } } }));
    assert!(edm(&mut c).get("pickup").is_none());
}

#[test]
fn a_bad_edit_is_refused_by_name_and_changes_nothing() {
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": sinker().display().to_string() }));
    c.ok("edit_machine", json!({ "patch": { "edm": { "max_orbit_radius": 0.5 } } }));
    let before = c.ok("get_job", json!({}))["machine"].clone();

    let neg = c.refused("edit_machine", json!({ "patch": { "edm": { "max_orbit_radius": -1.0 } } }));
    assert!(neg.contains("edm.max_orbit_radius"), "{neg}");
    let kind = c.refused("edit_machine", json!({ "patch": { "edm": { "orbits": [{ "kind": "spiral", "word": "X" }] } } }));
    assert!(kind.contains("edm.orbits[0].kind"), "{kind}");
    let cond = c.refused("edit_machine", json!({ "patch": { "edm": { "conditions": [{ "id": " ", "gap": 0.04 }] } } }));
    assert!(cond.contains("edm.conditions[0]"), "{cond}");
    let wire = c.refused("edit_machine", json!({ "patch": { "edm": { "max_taper_deg": 5.0 } } }));
    assert!(wire.contains("edm: states a key this machine kind"), "a wire key on a sinker: {wire}");
    let unknown = c.refused("edit_machine", json!({ "patch": { "edm": { "orbit_radius": 1.0 } } }));
    assert!(unknown.contains("unknown key 'edm.orbit_radius'"), "{unknown}");
    let top = c.refused("edit_machine", json!({ "patch": { "bogus": 1 } }));
    assert!(top.contains("unknown key 'bogus'"), "{top}");
    let same = c.refused("edit_machine", json!({ "patch": { "edm": { "max_orbit_radius": 0.5 } } }));
    assert!(same.contains("changes nothing"), "{same}");

    assert_eq!(c.ok("get_job", json!({}))["machine"], before, "nothing changed");
    // Only the one good edit is on the undo stack.
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
}

#[test]
fn edit_machine_needs_a_machine_and_a_known_setup() {
    let mut c = Client::new();
    assert!(c.refused("edit_machine", json!({ "patch": { "edm": {} } })).contains("no machine"));
    c.ok("load_machine", json!({ "path": sinker().display().to_string() }));
    assert!(c.refused("edit_machine", json!({ "patch": {}, "setup": "Nope" })).contains("no setup"));
    let tools = c.request("tools/list", json!({}))["result"]["tools"].to_string();
    assert!(tools.contains("edit_machine"));
}

/// #1499: a wire's tank and flushing are edited by the same undoable
/// mount; an unknown word is refused by name and changes nothing, and a
/// sinker does not take them.
#[test]
fn a_wire_tank_and_flushing_edit_is_one_undo_step_and_an_unknown_word_is_refused() {
    let wire = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/edm/wire-iso-fixture.mch");
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": wire.display().to_string() }));
    c.ok("edit_machine", json!({ "patch": { "edm": { "tank": "submerged", "flushing": "high" } } }));
    let e = edm(&mut c);
    assert_eq!((e["tank"].clone(), e["flushing"].clone()), (json!("submerged"), json!("high")), "{e}");
    let before = c.ok("get_job", json!({}))["machine"].clone();
    let bad = c.refused("edit_machine", json!({ "patch": { "edm": { "flushing": "medium" } } }));
    assert!(bad.contains("patch refused") && bad.contains("medium"), "{bad}");
    let bad = c.refused("edit_machine", json!({ "patch": { "edm": { "tank": "flooded" } } }));
    assert!(bad.contains("patch refused") && bad.contains("flooded"), "{bad}");
    assert_eq!(c.ok("get_job", json!({}))["machine"], before, "nothing changed");
    // #2686: the skim passes' flushing is the same kind of edit; taking
    // the rough cut's flushing back while it is stated is refused.
    c.ok("edit_machine", json!({ "patch": { "edm": { "skim_flushing": "low" } } }));
    assert_eq!(edm(&mut c)["skim_flushing"], json!("low"));
    let bad = c.refused("edit_machine", json!({ "patch": { "edm": { "flushing": null } } }));
    assert!(bad.contains("patch refused: edm.skim_flushing: needs edm.flushing"), "{bad}");
    assert_eq!(edm(&mut c)["flushing"], json!("high"), "nothing changed");
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["machine"], before, "undo takes the skim flushing back");
    c.ok("undo", json!({}));
    let e = edm(&mut c);
    assert!(e.get("tank").is_none() && e.get("flushing").is_none(), "undo takes both back: {e}");

    let mut s = Client::new();
    s.ok("load_machine", json!({ "path": sinker().display().to_string() }));
    let k = s.refused("edit_machine", json!({ "patch": { "edm": { "tank": "submerged" } } }));
    assert!(k.contains("edm: states a key this machine kind"), "a wire key on a sinker: {k}");
}

/// #3212: the MCP view and merge patch use the machine file's nested codes.
#[test]
fn nested_edm_codes_merge_read_back_undo_and_post_through_wire_iso() {
    let fixture: Value = serde_json::from_str(include_str!("edm/wire-iso-fixture.mch")).expect("fixture");
    let codes = fixture["edm"]["codes"].clone();
    let wire = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/edm/wire-iso-fixture.mch");
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": wire.display().to_string() }));
    let changed = c.ok("edit_machine", json!({ "patch": { "edm": { "codes": { "thread": "FX-EDIT-THREAD" } } } }));
    assert_eq!(changed["machine"]["edm"]["codes"]["thread"], json!("FX-EDIT-THREAD"));
    assert!(changed["machine"].get("edm_codes").is_none(), "one code key: {changed}");
    assert_eq!(edm(&mut c)["codes"]["condition"], codes["condition"], "merge keeps other codes");
    assert_eq!(edm(&mut c)["codes"]["thread"], json!("FX-EDIT-THREAD"));
    c.ok("undo", json!({}));
    assert_eq!(edm(&mut c)["codes"], codes);
    c.ok("redo", json!({}));
    assert_eq!(edm(&mut c)["codes"]["thread"], json!("FX-EDIT-THREAD"));
    c.ok("edit_machine", json!({ "patch": { "edm": { "codes": { "thread": null } } } }));
    assert!(edm(&mut c)["codes"].get("thread").is_none());
    assert_eq!(edm(&mut c)["codes"]["condition"], codes["condition"]);
    c.ok("edit_machine", json!({ "patch": { "edm": { "codes": null } } }));
    assert!(edm(&mut c).get("codes").is_none(), "null removes the code map");
    c.ok("edit_machine", json!({ "patch": { "edm": { "codes": codes } } }));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["machine"]["definition"]["edm"]["codes"], codes);
    assert!(job["machine"]["definition"].get("edm_codes").is_none());

    let dir = std::env::temp_dir().join(format!("camcraft-mcp-edm-codes-3212-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temporary directory");
    let stl = dir.join("slug.stl");
    let pts = [(20.0, 10.0), (-20.0, 10.0), (-20.0, -10.0), (20.0, -10.0)];
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut facets = Vec::new();
    for i in 0..pts.len() {
        let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
        facets.push(Triangle::new(Point3::new(0.0, 0.0, 12.0), p(a, 12.0), p(b, 12.0)));
        facets.push(Triangle::new(Point3::new(0.0, 0.0, 0.0), p(b, 0.0), p(a, 0.0)));
        facets.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 12.0)));
        facets.push(Triangle::new(p(a, 0.0), p(b, 12.0), p(a, 12.0)));
    }
    pocket_support::write_stl(&Mesh::from_facets(facets), &stl);
    c.ok("import_model", json!({ "path": stl.display().to_string() }));
    c.ok("add_tool", json!({ "catalog": "EDM-W025", "holder": "none" }));
    c.ok("add_operation", json!({ "kind": "Wire Contour", "name": "W", "tool": "EDM-W025", "source": "slug",
        "params": { "condition": "C110", "side": "punch", "start": [-30.0, 0.0], "capture": true,
            "top_z": { "reference": "fixed", "offset": 12.0 }, "bottom_z": { "reference": "fixed", "offset": 0.0 } } }));
    c.ok("generate", json!({ "name": "W" }));
    let output = dir.join("wire.nc");
    c.ok("post", json!({ "output": output.display().to_string(), "post": "Generic ISO Wire EDM" }));
    let nc = std::fs::read_to_string(output).expect("posted NC");
    for word in ["FX-THREAD", "FX-COND C110", "FX-CUT", "FX-COMP-OFF", "FX-TIP-OUT"] {
        assert!(nc.contains(word), "the edited code reaches the wire post: {word}\n{nc}");
    }
    std::fs::remove_dir_all(dir).expect("remove temporary files");
}

#[test]
fn bad_nested_edm_codes_are_named_and_leave_the_machine_unchanged() {
    let wire = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/edm/wire-iso-fixture.mch");
    let mut c = Client::new();
    c.ok("load_machine", json!({ "path": wire.display().to_string() }));
    let before = c.ok("get_job", json!({}));
    for bad in [json!([]), json!(true), json!(1), json!({ "Thread": "FX-THREAD" }), json!({ "thread": "" }),
        json!({ "thread": "(comment)" }), json!({ "thread": 42 }), json!({ "thread": "X".repeat(65) }),
        json!({ "thread": "FX\nTHREAD" }), json!({ "thread": "é" })] {
        let refusal = c.refused("edit_machine", json!({ "patch": { "edm": { "codes": bad } } }));
        assert!(refusal.contains("edm.codes"), "{refusal}");
        assert_eq!(c.ok("get_job", json!({})), before, "a refused code changes nothing");
    }
    for patch in [json!({ "edm_codes": { "thread": "FX-LEGACY" } }), json!({ "edm_codes": null }),
        json!({ "edm_codes": { "thread": "FX-LEGACY" }, "edm": { "codes": { "thread": "FX-NESTED" } } })] {
        let legacy = c.refused("edit_machine", json!({ "patch": patch }));
        assert!(legacy.contains("edm_codes"), "the old storage key is named: {legacy}");
        assert_eq!(c.ok("get_job", json!({})), before);
    }
}
