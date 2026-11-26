//! #1862 end to end over the MCP tools and the script verb: a setup with
//! only a stock takes a DXF profile headlessly (`import_dxf`), the reply
//! and `get_job` state the units and the skipped counts, a file with no
//! units is refused by name until `units` is given, the profile moves on
//! its stock face (`move_dxf_profile`), undo and redo work, a 2D
//! operation on the profile is accepted and a 3D one is refused by name,
//! and the document saves and reopens.
//!
//! Provenance: our own tests over our own tools; the DXF text is
//! hand-built from Autodesk's public ASCII DXF group conventions.
//!
//! Gate map (M4.4): covers app::mcp (sketch_tools, job get_job, tools),
//! app::script (dxf importDxf), app::commands (ImportDxf, MoveSketch),
//! app::project (sketch_dto), app::sketch_model.

use camcraft_app::mcp::Server;
use camcraft_app::script;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-dxf-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A 4 x 2 rectangle, a circle, a loose line and two skipped entities.
fn write_dxf(dir: &Path, file: &str, insunits: Option<i32>) -> String {
    let header = insunits.map_or(String::new(), |code| format!("9\n$INSUNITS\n70\n{code}\n"));
    let text = format!(
        "0\nSECTION\n2\nHEADER\n{header}0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
         0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n4\n20\n0\n10\n4\n20\n2\n10\n0\n20\n2\n\
         0\nCIRCLE\n8\nholes\n10\n1\n20\n1\n40\n0.25\n\
         0\nLINE\n8\nmarks\n10\n2\n20\n0.5\n11\n3\n21\n0.5\n\
         0\nTEXT\n8\nnotes\n10\n0\n20\n0\n40\n0.1\n1\nhello\n\
         0\nTEXT\n8\nnotes\n10\n0\n20\n1\n40\n0.1\n1\nagain\n\
         0\nENDSEC\n0\nEOF\n"
    );
    let path = dir.join(file);
    std::fs::write(&path, text).expect("write dxf");
    path.display().to_string()
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
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

/// A document with a sheet stock and a tool, and no model.
fn sheet_client() -> Client {
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("set_stock", json!({ "box": { "dx": 300.0, "dy": 200.0, "dz": 6.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c
}

#[test]
fn import_dxf_places_a_profile_and_states_units_and_skipped_counts() {
    let dir = tmp_dir("import");
    let mut c = sheet_client();

    // No stock, no place: a fresh document refuses and says what to do first.
    let mut bare = Client { server: Server::new(), next: 1 };
    let why = bare.refused("import_dxf", json!({ "path": write_dxf(&dir, "bare.dxf", Some(4)) }));
    assert!(why.starts_with("import_dxf:") && why.contains("has no stock - make a stock first"), "{why}");

    // A file with no units is refused by name, never guessed.
    let unitless = write_dxf(&dir, "plate.dxf", None);
    let why = c.refused("import_dxf", json!({ "path": unitless }));
    assert!(why.contains("states no units") && why.contains("\"mm\" or \"in\""), "{why}");
    assert!(why.contains("4 x 2 drawing units"), "the extents, for the caller to judge: {why}");
    assert_eq!(c.ok("get_job", json!({}))["profiles"], json!([]), "nothing landed");
    let why = c.refused("import_dxf", json!({ "path": unitless, "units": "furlongs" }));
    assert!(why.contains("\"units\" must be \"mm\" or \"in\""), "{why}");

    // With the unit stated it lands on the stock top, and the reply says what was read.
    let r = c.ok("import_dxf", json!({ "path": unitless, "units": "in" }));
    assert_eq!(r["profile"], json!("plate"));
    assert_eq!(r["units"], json!({ "stated": null, "used": "in", "from": "caller", "mm_per_unit": 25.4 }));
    assert_eq!(r["skipped"], json!({ "TEXT": 2 }));
    assert_eq!(r["skipped_total"], json!(2));
    assert_eq!(r["chains"], json!({ "closed": 2, "open": 1, "rejected": 0 }));
    assert_eq!(r["layers"], json!(["cut", "holes", "marks"]));
    assert_eq!(r["bounds"]["max"], json!([101.6, 50.8]), "inches read as millimetres");
    assert_eq!(r["placement"], json!({ "x": 0.0, "y": 0.0, "angle": 0.0 }));
    assert_eq!(r["origin"], json!([0.0, 0.0, 6.0]), "the top face of the 6 mm sheet, at the stock origin");
    let summary = r["summary"].as_str().expect("summary");
    assert!(summary.contains("read in in") && summary.contains("2 entities skipped"), "{summary}");

    // A file that states its unit needs no answer; the same stem gets its own name.
    let r2 = c.ok("import_dxf", json!({ "path": write_dxf(&dir, "plate.dxf", Some(4)), "join_tol": 0.05 }));
    assert_eq!(r2["profile"], json!("plate (2)"));
    assert_eq!(r2["units"]["stated"], json!("mm"));
    assert_eq!(r2["units"]["from"], json!("file"));
    assert_eq!(r2["join_tol"], json!(0.05));

    // get_job: the profiles are listed apart from the models, and the setup's model is virtual.
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["models"], json!([]), "a profile is not a model");
    let names: Vec<&str> = job["profiles"].as_array().expect("profiles").iter().filter_map(|p| p["profile"].as_str()).collect();
    assert_eq!(names, ["plate", "plate (2)"]);
    assert_eq!(job["profiles"][0]["units"]["used"], json!("in"));
    assert_eq!(job["profiles"][0]["skipped"], json!({ "TEXT": 2 }));
    assert_eq!(job["setups"][0]["model_reference"], json!({ "kind": "virtual", "profiles": ["plate", "plate (2)"], "solid": false }));

    // Undo removes the second profile; redo puts it back.
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["profiles"].as_array().map(Vec::len), Some(1));
    c.ok("redo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["profiles"].as_array().map(Vec::len), Some(2));
}

#[test]
fn a_profile_moves_on_the_stock_and_takes_a_2d_operation_but_no_3d_one() {
    let dir = tmp_dir("ops");
    let mut c = sheet_client();
    c.ok("import_dxf", json!({ "path": write_dxf(&dir, "plate.dxf", Some(4)) }));

    // The move on the stock face; the angle is kept when it is not given.
    let r = c.ok("move_dxf_profile", json!({ "name": "plate", "position": [25.0, -10.0], "angle": 90.0 }));
    assert_eq!(r["placement"], json!({ "x": 25.0, "y": -10.0, "angle": 90.0 }));
    assert_eq!(r["origin"], json!([25.0, -10.0, 6.0]), "still on the stock top");
    let r = c.ok("move_dxf_profile", json!({ "name": "plate", "position": [5.0, 5.0] }));
    assert_eq!(r["placement"], json!({ "x": 5.0, "y": 5.0, "angle": 90.0 }));
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["profiles"][0]["placement"], json!({ "x": 25.0, "y": -10.0, "angle": 90.0 }));
    let why = c.refused("move_dxf_profile", json!({ "name": "Stock", "position": [0.0, 0.0] }));
    assert!(why.contains("'Stock' is not a DXF profile"), "{why}");
    let why = c.refused("move_model", json!({ "name": "plate", "position": [0.0, 0.0, 0.0] }));
    assert!(why.contains("'plate' is a DXF profile"), "{why}");

    // A 2D operation on the profile is accepted and says what it is checked against.
    c.ok("add_operation", json!({ "kind": "2D Pocket", "tool": "EM-0400-2FL", "source": "plate", "name": "Cut" }));
    let job = c.ok("get_job", json!({}));
    let checks = job["operations"][0]["checks_against"].as_str().expect("an operation on a profile states its checks");
    assert!(checks.contains("the stock 'Stock'") && checks.contains("the chains of the DXF profile 'plate'"), "{checks}");
    assert!(checks.contains("no solid model"), "{checks}");

    // A 3D operation in the setup, and a solid kind on the profile, are refused by name.
    let why = c.refused("add_operation", json!({ "kind": "3D Finish", "tool": "EM-0400-2FL", "name": "Finish" }));
    assert!(why.contains("a 3D Finish operation needs a solid model"), "{why}");
    assert!(why.contains("virtual model reference with no solid"), "{why}");
    let why = c.refused("add_operation", json!({ "kind": "Drill", "tool": "EM-0400-2FL", "source": "plate", "name": "Holes" }));
    assert!(why.contains("a Drill operation needs a solid model - 'plate' is a DXF profile"), "{why}");

    // Nothing is cut for want of a solid: generation refuses by name, and the post gate holds.
    let why = c.refused("generate", json!({ "name": "Cut" }));
    assert!(why.contains("checked against the stock 'Stock'") && why.contains("is not generated yet"), "{why}");
    let report = c.ok("verify", json!({}));
    assert_eq!(report["ok"], json!(false), "an operation with no toolpath never passes the post gate");

    // The setup with only a stock and a profile saves and reopens.
    let file = dir.join("sheet.camproj").display().to_string();
    c.ok("save_project", json!({ "path": file }));
    let mut again = Client { server: Server::new(), next: 1 };
    again.ok("open_project", json!({ "path": file }));
    let job2 = again.ok("get_job", json!({}));
    assert_eq!(job2["profiles"], job["profiles"]);
    assert_eq!(job2["operations"][0]["source"], job["operations"][0]["source"]);
    assert_eq!(job2["setups"][0]["model_reference"]["kind"], json!("virtual"));

    // The script verb takes the same arguments and gives the same refusal.
    let unitless = write_dxf(&dir, "loose.dxf", None);
    let run = |body: &str| script::run(&format!("cam.load({file:?}); {body}"));
    let out = run(&format!("cam.importDxf({unitless:?}, {{ units: \"mm\", name: \"loose\" }}).units.used"));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result, json!("mm"));
    let out = run(&format!("var why = \"\"; try {{ cam.importDxf({unitless:?}) }} catch (e) {{ why = String(e.message) }} why"));
    assert!(out.result.as_str().is_some_and(|w| w.contains("states no units")), "{:?} {:?}", out.result, out.errors);
    let out = run(&format!("cam.importDxf({unitless:?}, {{ unit: \"mm\" }})"));
    assert!(!out.ok && out.errors.iter().any(|e| e.contains("unknown option \"unit\"")), "{:?}", out.errors);
}

#[test]
fn agie_inspection_accepts_listed_chain_and_checks_mounted_travel_in_the_wcs() {
    let dir = tmp_dir("agie-inspection-travel");
    let source: Value = serde_json::from_str(include_str!(
        "../../../assets/machines/agiecut-classic-2s.mch"
    ))
    .unwrap();
    let mut client = Client {
        server: Server::new(),
        next: 1,
    };
    let machine_path = dir.join("machine.mch");
    let mount = |client: &mut Client, x: [f64; 2], y: [f64; 2], x_sign: i32| {
        let mut fixture = source.clone();
        let table = &mut fixture["kinematics"]["default"]["parts"][0];
        table["travel"] = json!({"min": x[0], "max": x[1]});
        table["direction"] = json!(x_sign);
        table["parts"][0]["travel"] = json!({"min": y[0], "max": y[1]});
        std::fs::write(&machine_path, serde_json::to_vec(&fixture).unwrap()).unwrap();
        client.ok("load_machine", json!({"path":machine_path}));
    };
    mount(&mut client, [-8.0, 8.0], [-8.0, 8.0], 1);
    client.ok("set_stock", json!({"box":{"dx":1.0,"dy":1.0,"dz":10.0}}));
    client.ok("set_wcs", json!({"origin":[20.0,0.0,10.0]}));
    client.ok("add_tool", json!({"catalog":"EDM-W025","holder":"none"}));
    let drawing = dir.join("opening.dxf");
    std::fs::write(&drawing,"0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\nopening\n10\n20\n20\n0\n40\n7\n0\nENDSEC\n0\nEOF\n").unwrap();
    client.ok("import_dxf", json!({"path":drawing}));
    let features = client.ok("list_features", json!({"model":"opening"}));
    let id = features["chains"][0]["id"].as_str().expect("listed chain");
    let create = |name: &str, chain: &str| json!({"name":name,"drawing":"opening","tool":"EDM-W025","chain":chain,"start":[20.0,0.0]});
    client.ok("create_agie_inspection", create("Nominal", id));
    client.ok(
        "create_agie_inspection",
        create("Bare", id.strip_prefix("sketch_chain:").unwrap()),
    );
    for invalid in [
        "face:0x12",
        "sketch_layer:opening",
        "sketch_chain:xyz",
        "garbage",
    ] {
        client.refused("create_agie_inspection", create("Invalid", invalid));
    }
    let dest = dir.join("BLOCKED");
    let args = json!({"operation":"Nominal","directory":dest});
    for (x, y, sign, expected) in [
        ([-4.0, 4.0], [-8.0, 8.0], 1, "in X"),
        ([-8.0, 8.0], [-4.0, 4.0], 1, "in Y"), // The two semicircle endpoints have Y=0.
        ([-8.0, 8.0], [-8.0, 8.0], -1, "inverted X"),
    ] {
        mount(&mut client, x, y, sign);
        let why = client.refused("export_agie_inspection", args.clone());
        assert!(why.contains(expected), "{why}");
        assert!(!dest.exists());
    }
    std::fs::remove_dir_all(dir).unwrap();
}
