//! #547 (part of #290): `electrode_from_faces` over MCP - a pocket's
//! faces, picked by their `list_features` ids, yield an electrode tool
//! through the app's undoable `ElectrodeFromFaces`; its answer seeds the
//! Sinker Burn that burns the pocket; one `undo` removes the tool; and
//! every bad argument is a tool error by name, never a panic. A
//! rounded pocket yields a body electrode (#1152) whose section rides
//! the reply, `get_job`, `add_tool` and `edit_tool`.
//!
//! Gate map (#547): covers app::mcp::sinker_tools (electrode_from_faces),
//! app::mcp::tools (the tool table), app::mcp::tool_edit (the outline
//! key), app::commands::electrode,
//! app::operation::sinker_faces.
//! Provenance: our own SYNTHETIC fixtures - the sinker template of #530
//! with invented conditions (no machine's manual) and a block built in
//! code. Not exercised on a real EDM control.

mod pocket_support;

use camcraft_app::geometry::Mesh;
use pocket_support::height_field;

const SINKER: &str = include_str!("edm/sinker-edm.mch");
/// The cavity: 20 x 12, off-centre at (6, -4), floor Z 14 under the
/// 20 mm work top.
const LO: (f64, f64) = (-4.0, -10.0);
const HI: (f64, f64) = (16.0, 2.0);
const FLOOR: f64 = 14.0;
const TOP: f64 = 20.0;

/// The sinker template with SYNTHETIC conditions (no manual's values).
fn sinker_machine() -> String {
    let mut v: serde_json::Value = serde_json::from_str(SINKER).expect("template json");
    v["edm"]["conditions"] = serde_json::json!([
        { "id": "F1", "name": "synthetic finish", "gap": 0.02, "undersize": 0.02, "role": "finish" }
    ]);
    v.to_string()
}

/// A 60 x 60 x 20 block with the cavity sunk in it.
fn block() -> Mesh {
    height_field(&[-30.0, LO.0, HI.0, 30.0], &[-30.0, LO.1, HI.1, 30.0], |i, j| if (i, j) == (1, 1) { FLOOR } else { TOP })
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn the_electrode_is_generated_and_burned_over_mcp_with_the_gui_commands() {
    use camcraft_app::mcp::Server;
    use serde_json::{json, Value};
    fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Result<Value, String> {
        *id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = s.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        if result["isError"] == json!(true) {
            Err(result["content"][0]["text"].as_str().unwrap_or_default().to_owned())
        } else {
            Ok(result["structuredContent"].clone())
        }
    }
    let dir = std::env::temp_dir().join(format!("camcraft-547-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (stl, mch) = (dir.join("part.stl"), dir.join("sinker.mch"));
    pocket_support::write_stl(&block(), &stl);
    std::fs::write(&mch, sinker_machine()).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let list = s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {} })).expect("listed");
    let listed = list["result"]["tools"].as_array().expect("tools").iter().any(|t| t["name"] == json!("electrode_from_faces"));
    assert!(listed, "electrode_from_faces is a listed tool");
    let mut ok = |s: &mut Server, name: &str, args: Value| call(s, &mut id, name, args).unwrap_or_else(|e| panic!("{name} refused: {e}"));
    ok(&mut s, "import_model", json!({ "path": stl.display().to_string() }));
    ok(&mut s, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } }));
    ok(&mut s, "load_machine", json!({ "path": mch.display().to_string() }));
    let _ = std::fs::remove_dir_all(&dir);
    // The cavity's faces by their listed bounds: inside the pocket's box.
    let rows = ok(&mut s, "list_features", json!({ "model": "part", "kinds": ["faces"] }));
    let inside = |b: &Value| {
        let n = |k: &str, i: usize| b[k][i].as_f64().unwrap_or(f64::NAN);
        n("min", 0) >= LO.0 - 1e-6 && n("max", 0) <= HI.0 + 1e-6 && n("min", 1) >= LO.1 - 1e-6 && n("max", 1) <= HI.1 + 1e-6 && n("min", 2) >= FLOOR - 1e-6
    };
    let picks: Vec<Value> = rows["faces"]
        .as_array()
        .expect("faces")
        .iter()
        .filter(|f| inside(&f["bounds"]))
        .map(|f| json!({ "model": "part", "id": f["id"] }))
        .collect();
    assert_eq!(picks.len(), 5, "the floor and four walls: {picks:?}");

    let got = ok(&mut s, "electrode_from_faces", json!({ "name": "E-F", "picks": picks, "condition": "F1", "stub": 10, "pickup": 15 }));
    assert_eq!(got["tool"], json!("E-F"));
    let num = |v: &Value| v.as_f64().unwrap_or(f64::NAN);
    assert!(near(num(&got["electrode"]["x"]), 19.96) && near(num(&got["electrode"]["y"]), 11.96), "{got}");
    assert_eq!((got["electrode"]["length"].clone(), got["electrode"]["pickup"].clone()), (json!(16.0), json!(15.0)));
    // #1152: a rectangular cavity's electrode says so and states no section.
    assert_eq!((got["electrode"]["shape"].clone(), got["electrode"]["outline"].clone()), (json!("rectangle"), Value::Null));
    assert_eq!(got["cavity"]["center"], json!([6.0, -4.0]));
    assert_eq!((got["undersize"].clone(), got["condition"].clone(), got["note"].clone()), (json!(0.02), json!("F1"), Value::Null));
    assert_eq!(got["warning"], Value::Null, "an undersize equal to the gap burns to size: {got}");
    let mut params = got["burn"].clone();
    params["condition"] = json!("F1");
    params["pass"] = json!("finish");
    ok(&mut s, "add_operation", json!({ "kind": "Sinker Burn", "tool": "E-F", "source": "stock", "name": "Finish", "params": params }));
    let generated = ok(&mut s, "generate", json!({ "name": "Finish" }));
    assert!(generated["generated"].as_array().is_some_and(|g| !g.is_empty()), "{generated}");

    // An orbit is taken off the electrode too, and said.
    let orbit = ok(&mut s, "electrode_from_faces", json!({ "name": "E-O", "picks": picks, "undersize": 0.1, "orbit": 0.25, "stub": 10, "pickup": 0 }));
    assert!(near(num(&orbit["electrode"]["x"]), 19.3) && near(num(&orbit["inset"]), 0.35), "{orbit}");
    assert!(orbit["note"].as_str().is_some_and(|n| n.contains("#544") && n.contains("#1355")), "{orbit}");

    let tools = |s: &mut Server, id: &mut i64| -> Vec<String> {
        let job = call(s, id, "get_job", json!({})).expect("job");
        job["tools"].as_array().map(|t| t.iter().filter_map(|x| x["name"].as_str().map(str::to_owned)).collect()).unwrap_or_default()
    };
    assert!(tools(&mut s, &mut id).contains(&"E-O".to_owned()));
    call(&mut s, &mut id, "undo", json!({})).expect("undo");
    assert!(!tools(&mut s, &mut id).contains(&"E-O".to_owned()), "one undo removes the electrode");

    // Review of #1156: a stated undersize of 0 (a full-size electrode)
    // is warned by name; a condition never needs it, since the machine
    // file refuses a condition undersized below its own gap.
    let zero = call(&mut s, &mut id, "electrode_from_faces", json!({ "name": "E-0", "picks": picks, "undersize": 0, "stub": 10, "pickup": 0 })).expect("electrode_from_faces");
    assert!(zero["warning"].as_str().is_some_and(|w| w.contains("an undersize of 0 makes the electrode the cavity's full size")), "{zero}");
    let sound = call(&mut s, &mut id, "electrode_from_faces", json!({ "name": "E-S", "picks": picks, "undersize": 0.1, "stub": 10, "pickup": 0 })).expect("electrode_from_faces");
    assert_eq!(sound["warning"], Value::Null, "a stated undersize with no condition warns only at 0: {sound}");

    // Every bad argument is a tool error by name, never a panic.
    let mut bad = |args: Value| call(&mut s, &mut id, "electrode_from_faces", args).expect_err("refused");
    let base = json!({ "name": "E-X", "picks": picks, "undersize": 0.1, "stub": 10, "pickup": 15 });
    let with = |k: &str, v: Value| {
        let mut b = base.clone();
        b[k] = v;
        b
    };
    assert!(bad(with("condition", json!("F1"))).contains("not both"));
    assert!(bad(with("undersize", Value::Null)).contains("\"undersize\" (mm per side) or the EDM \"condition\""));
    assert!(bad(with("setup", json!("Setup1"))).contains("Setup1") || bad(with("setup", json!("Setup1"))).contains("goes with"));
    assert!(bad(with("stub", Value::Null)).contains("\"stub\" is required"));
    assert!(bad(with("pickup", json!("high"))).contains("finite number of mm"));
    assert!(bad(with("undersize", json!(7))).contains("leaves no electrode"));
    assert!(bad(with("picks", json!([]))).contains("at least one pick"));
    assert!(bad(with("picks", json!([picks[0], picks[1]]))).contains("electrode from faces"));
    assert!(bad(with("name", json!("E-F"))).contains("already exists"));
    assert!(bad(with("depth", json!(3))).contains("unknown argument"));
    let no_such = json!({ "name": "E-X", "picks": picks, "condition": "ZZ", "stub": 10, "pickup": 15 });
    assert!(bad(no_such).contains("states no EDM condition"));
}

/// #1152: a rounded-corner pocket's faces yield a body electrode over
/// MCP. Its answer names the shape and the section; `get_job` shows the
/// section in the tool's definition; `add_tool` takes that definition
/// back; and `edit_tool` holds the record's door (sizes off the outline
/// are refused by name, an emptied outline is a rectangular face).
#[test]
fn a_rounded_pocket_yields_a_body_electrode_over_mcp() {
    use camcraft_app::mcp::Server;
    use pocket_support::{block_with_pocket, rounded_rect};
    use serde_json::{json, Value};
    fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Result<Value, String> {
        *id += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": *id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = s.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        if result["isError"] == json!(true) {
            Err(result["content"][0]["text"].as_str().unwrap_or_default().to_owned())
        } else {
            Ok(result["structuredContent"].clone())
        }
    }
    let dir = std::env::temp_dir().join(format!("camcraft-1152-mcp-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (stl, mch) = (dir.join("part.stl"), dir.join("sinker.mch"));
    pocket_support::write_stl(&block_with_pocket(30.0, 30.0, &rounded_rect(10.0, 6.0, 2.0, 4), FLOOR, TOP), &stl);
    std::fs::write(&mch, sinker_machine()).expect("write the machine");
    let (mut s, mut id) = (Server::new(), 0);
    let _ = s.handle(&json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {} }));
    let mut ok = |s: &mut Server, name: &str, args: Value| call(s, &mut id, name, args).unwrap_or_else(|e| panic!("{name} refused: {e}"));
    ok(&mut s, "import_model", json!({ "path": stl.display().to_string() }));
    ok(&mut s, "set_stock", json!({ "box": { "dx": 60, "dy": 60, "dz": 20 } }));
    ok(&mut s, "load_machine", json!({ "path": mch.display().to_string() }));
    let _ = std::fs::remove_dir_all(&dir);
    let rows = ok(&mut s, "list_features", json!({ "model": "part", "kinds": ["faces"] }));
    let inside = |b: &Value| {
        let n = |k: &str, i: usize| b[k][i].as_f64().unwrap_or(f64::NAN);
        n("min", 0) >= -10.0 - 1e-6 && n("max", 0) <= 10.0 + 1e-6 && n("min", 1) >= -6.0 - 1e-6 && n("max", 1) <= 6.0 + 1e-6 && n("min", 2) >= FLOOR - 1e-6
    };
    let picks: Vec<Value> =
        rows["faces"].as_array().expect("faces").iter().filter(|f| inside(&f["bounds"])).map(|f| json!({ "model": "part", "id": f["id"] })).collect();
    assert_eq!(picks.len(), 21, "the floor, four flat walls and sixteen corner strips");

    let got = ok(&mut s, "electrode_from_faces", json!({ "name": "E-F", "picks": picks, "condition": "F1", "stub": 10, "pickup": 15 }));
    let num = |v: &Value| v.as_f64().unwrap_or(f64::NAN);
    assert_eq!(got["electrode"]["shape"], json!("outline"), "{got}");
    let section = got["electrode"]["outline"].as_array().expect("the section").clone();
    assert!(section.len() >= 20 && section.iter().all(|p| p.as_array().is_some_and(|xy| xy.len() == 2)), "{got}");
    assert!((num(&got["electrode"]["x"]) - 19.96).abs() < 1e-5 && (num(&got["electrode"]["y"]) - 11.96).abs() < 1e-5, "{got}");
    // The section is about the burn point: inside the two sizes.
    assert!(section.iter().all(|p| num(&p[0]).abs() <= 9.98 + 1e-5 && num(&p[1]).abs() <= 5.98 + 1e-5), "{section:?}");

    // get_job shows the section in the tool's definition, and add_tool
    // takes the definition back as it stands.
    let job = ok(&mut s, "get_job", json!({}));
    let def = job["tools"].as_array().expect("tools").iter().find(|t| t["name"] == json!("E-F")).expect("the electrode")["definition"].clone();
    assert_eq!(def["outline"].as_array().map(Vec::len), Some(section.len()), "{def}");
    ok(&mut s, "add_tool", json!({ "name": "E-copy", "definition": def, "holder": "none" }));

    // The burn it seeds is judged clean by the program verdict.
    let mut params = got["burn"].clone();
    params["condition"] = json!("F1");
    params["pass"] = json!("finish");
    ok(&mut s, "add_operation", json!({ "kind": "Sinker Burn", "tool": "E-F", "source": "stock", "name": "Finish", "params": params }));
    let generated = ok(&mut s, "generate", json!({ "name": "Finish" }));
    assert!(generated["generated"].as_array().is_some_and(|g| !g.is_empty()), "{generated}");

    // edit_tool: a size edited off the outline is refused by name; an
    // emptied outline makes the copy a rectangular face.
    let why = call(&mut s, &mut id, "edit_tool", json!({ "name": "E-copy", "patch": { "diameter": 25 } })).expect_err("refused");
    assert!(why.contains("the outline's X and Y extents"), "{why}");
    let why = call(&mut s, &mut id, "edit_tool", json!({ "name": "E-copy", "patch": { "outline": [[0, 0], [1, 0]] } })).expect_err("refused");
    assert!(why.contains("at least 3 points"), "{why}");
    let flat = call(&mut s, &mut id, "edit_tool", json!({ "name": "E-copy", "patch": { "outline": [] } })).expect("an emptied outline");
    assert!(flat["definition"].get("outline").is_none(), "{flat}");
    assert_eq!(got["cavity"]["center"], json!([0.0, 0.0]), "{got}");
}

/// #2430 review: the electrode reply names the frame that supplies its
/// burn coordinates, including a named aligned setup that is not active.
#[test]
fn electrode_reply_names_the_effective_setup_and_its_aligned_coordinates() {
    use camcraft_app::mcp::Server;
    use serde_json::{Value, json};

    fn call(s: &mut Server, id: &mut i64, name: &str, args: Value) -> Value {
        *id += 1;
        let reply = s.handle(&json!({
            "jsonrpc": "2.0", "id": *id, "method": "tools/call",
            "params": { "name": name, "arguments": args }
        })).expect("MCP reply");
        assert_ne!(reply["result"]["isError"], json!(true), "{name}: {reply}");
        reply["result"]["structuredContent"].clone()
    }
    let dir = std::env::temp_dir().join(format!("camcraft-2430-electrode-frame-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (stl, mch) = (dir.join("part.stl"), dir.join("sinker.mch"));
    pocket_support::write_stl(&block(), &stl);
    std::fs::write(&mch, sinker_machine()).expect("machine");
    let (mut s, mut id) = (Server::new(), 0);
    call(&mut s, &mut id, "import_model", json!({ "path": stl.display().to_string() }));
    call(&mut s, &mut id, "load_machine", json!({ "path": mch.display().to_string() }));
    std::fs::remove_dir_all(&dir).expect("remove fixtures");
    let rows = call(&mut s, &mut id, "list_features", json!({ "model": "part", "kinds": ["faces"] }));
    let faces: Vec<&Value> = rows["faces"].as_array().expect("faces").iter().filter(|f| {
        let b = &f["bounds"];
        let n = |key: &str, i: usize| b[key][i].as_f64().unwrap_or(f64::NAN);
        n("min", 0) >= LO.0 - 1e-6 && n("max", 0) <= HI.0 + 1e-6
            && n("min", 1) >= LO.1 - 1e-6 && n("max", 1) <= HI.1 + 1e-6
            && n("min", 2) >= FLOOR - 1e-6
    }).collect();
    assert_eq!(faces.len(), 5);
    let picks: Vec<Value> = faces.iter().map(|f| json!({ "model": "part", "id": f["id"] })).collect();
    let floor = faces.iter().find(|f| near(f["bounds"]["max"][2].as_f64().expect("Z"), FLOOR)).expect("floor");
    let args = |name: &str| json!({ "name": name, "picks": picks, "condition": "F1", "stub": 10, "pickup": 0 });
    let check = |got: &Value, frame: &str, setup: Value, floor: f64, top: f64| {
        assert_eq!(got["frame"], json!(frame), "{got}");
        assert_eq!(got["setup"], setup, "{got}");
        assert_eq!(got["cavity"]["center"], json!([6.0, -4.0]), "{got}");
        assert_eq!((got["burn"]["x"].clone(), got["burn"]["y"].clone()), (json!(6.0), json!(-4.0)), "{got}");
        assert!(near(got["cavity"]["floor_z"].as_f64().expect("floor"), floor), "{got}");
        assert!(near(got["cavity"]["top_z"].as_f64().expect("top"), top), "{got}");
        assert_eq!(got["burn"]["floor_z"], json!({ "reference": "fixed", "offset": floor }), "{got}");
        assert_eq!(got["burn"]["top_z"], json!({ "reference": "fixed", "offset": top }), "{got}");
    };
    let legacy = call(&mut s, &mut id, "electrode_from_faces", args("E-world"));
    check(&legacy, "world", Value::Null, FLOOR, TOP);
    call(&mut s, &mut id, "add_setup", json!({ "name": "Aligned" }));
    call(&mut s, &mut id, "lay_part_on_face", json!({ "setup": "Aligned", "face": { "model": "part", "id": floor["id"] } }));
    call(&mut s, &mut id, "add_setup", json!({ "name": "Other" }));
    // Other is active. The named setup supplies the aligned heights.
    let mut named_args = args("E-named");
    named_args["setup"] = json!("Aligned");
    let named = call(&mut s, &mut id, "electrode_from_faces", named_args);
    check(&named, "setup", json!("Aligned"), 0.0, TOP - FLOOR);
    let job = call(&mut s, &mut id, "get_job", json!({}));
    assert_eq!(job["active_setup"], json!("Other"));
    let other = call(&mut s, &mut id, "electrode_from_faces", args("E-other"));
    check(&other, "setup", json!("Other"), FLOOR, TOP);
    call(&mut s, &mut id, "activate_setup", json!({ "name": "Aligned" }));
    let active = call(&mut s, &mut id, "electrode_from_faces", args("E-active"));
    check(&active, "setup", json!("Aligned"), 0.0, TOP - FLOOR);
    // Feature IDs and bounds continue to use document/world coordinates.
    let listed_again = call(&mut s, &mut id, "list_features", json!({ "model": "part", "kinds": ["faces"] }));
    assert_eq!(listed_again["frame"], json!("world"));
    assert_eq!(listed_again["faces"], rows["faces"]);
}
