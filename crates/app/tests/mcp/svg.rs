//! #1880 end to end: an SVG drawing lands as a profile on a setup's stock through the undoable
//! `ImportSvg` (the `import_svg` MCP tool and the `importSvg` script verb), shares the DXF
//! profile entity (`get_job` "profiles", `move_dxf_profile`, a 2D operation, save and reopen),
//! and an SVG with no physical size is refused by name until the caller states a unit.
//!
//! Provenance: our own tests over our own tools; the SVG text is hand-written from the W3C SVG
//! 1.1 element grammar.
//!
//! Gate map (M4.4): covers app::commands (ImportSvg), app::mcp (sketch_tools import_svg),
//! app::script (dxf importSvg).

use camcraft_app::commands::{Command, CommandError, DxfOptions, History, ImportSvg, SetStock};
use camcraft_app::mcp::Server;
use camcraft_app::schema::Location;
use camcraft_app::script;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-mcp-svg-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// A 60 x 30 mm plate with a 10 mm hole on layer "Cut", and a text label that is skipped.
const PLATE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
     width="80mm" height="40mm" viewBox="0 0 80 40">
  <g inkscape:label="Cut" inkscape:groupmode="layer">
    <path d="M10 5 h60 v30 h-60 z"/>
    <circle cx="25" cy="20" r="5"/>
  </g>
  <text x="40" y="20">part 7</text>
</svg>"#;

/// The same outline with no physical size: one drawing unit is whatever the caller says.
const UNSIZED: &str = r#"<svg viewBox="0 0 4 2"><rect width="4" height="2"/></svg>"#;

fn write(dir: &Path, file: &str, text: &str) -> String {
    let path = dir.join(file);
    std::fs::write(&path, text).expect("write svg");
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

fn sheet_client() -> Client {
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("set_stock", json!({ "box": { "dx": 300.0, "dy": 200.0, "dz": 6.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));
    c
}

#[test]
fn import_svg_places_a_profile_with_chains_nesting_and_skipped_notes() {
    let dir = tmp_dir("import");
    let mut c = sheet_client();

    let mut bare = Client { server: Server::new(), next: 1 };
    let why = bare.refused("import_svg", json!({ "path": write(&dir, "bare.svg", PLATE) }));
    assert!(why.contains("has no stock - make a stock first"), "{why}");

    let r = c.ok("import_svg", json!({ "path": write(&dir, "plate.svg", PLATE) }));
    assert_eq!(r["profile"], json!("plate"));
    assert_eq!(r["units"]["stated"], json!("mm"));
    assert_eq!(r["units"]["from"], json!("file"));
    assert_eq!(r["layers"], json!(["Cut"]));
    assert_eq!(r["chains"], json!({ "closed": 2, "open": 0, "rejected": 0 }));
    assert_eq!(r["skipped"], json!({ "text": 1 }));
    assert_eq!(r["bounds"], json!({ "min": [10.0, 5.0], "max": [70.0, 35.0] }), "Y flipped about the 40 mm page");
    assert_eq!(r["origin"], json!([0.0, 0.0, 6.0]), "on the stock top");
    assert!(r["summary"].as_str().is_some_and(|s| s.contains("2 closed")), "{}", r["summary"]);

    // An SVG with no physical size is refused by name and states its extents.
    let loose = write(&dir, "loose.svg", UNSIZED);
    let why = c.refused("import_svg", json!({ "path": loose }));
    assert!(why.contains("states no physical size") && why.contains("\"mm\" or \"in\""), "{why}");
    assert!(why.contains("4 x 2 drawing units"), "{why}");
    let r = c.ok("import_svg", json!({ "path": loose, "units": "in" }));
    assert_eq!(r["units"], json!({ "stated": null, "used": "in", "from": "caller", "mm_per_unit": 25.4 }));
    assert_eq!(r["bounds"]["max"], json!([101.6, 50.8]));
    // A file that states its size takes no caller unit.
    let why = c.refused("import_svg", json!({ "path": write(&dir, "plate.svg", PLATE), "units": "in" }));
    assert!(why.contains("states its size"), "{why}");
    let why = c.refused("import_svg", json!({ "path": write(&dir, "plate.dxf", PLATE) }));
    assert!(why.contains("is not a .svg file"), "{why}");

    // The profile is a DXF-style profile: listed, moved, and undone like one.
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["profiles"].as_array().map(Vec::len), Some(2));
    let r = c.ok("move_dxf_profile", json!({ "name": "plate", "position": [20.0, 10.0] }));
    assert_eq!(r["placement"], json!({ "x": 20.0, "y": 10.0, "angle": 0.0 }));
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["profiles"].as_array().map(Vec::len), Some(1));
    c.ok("redo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["profiles"].as_array().map(Vec::len), Some(2));

    // A 2D operation takes the profile, and the document saves and reopens with it.
    c.ok("add_operation", json!({ "kind": "2D Contour", "tool": "EM-0400-2FL", "source": "plate", "name": "Cut" }));
    let file = dir.join("sheet.camproj").display().to_string();
    c.ok("save_project", json!({ "path": file }));
    let mut again = Client { server: Server::new(), next: 1 };
    again.ok("open_project", json!({ "path": file }));
    assert_eq!(again.ok("get_job", json!({}))["profiles"], c.ok("get_job", json!({}))["profiles"]);

    // The script verb takes the same options and gives the same refusal.
    let run = |body: &str| script::run(&format!("cam.load({file:?}); {body}"));
    let out = run(&format!("cam.importSvg({loose:?}, {{ units: \"mm\", name: \"loose mm\" }}).bounds.max"));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result, json!([4, 2]));
    let out = run(&format!("var why = \"\"; try {{ cam.importSvg({loose:?}) }} catch (e) {{ why = String(e.message) }} why"));
    assert!(out.result.as_str().is_some_and(|w| w.contains("states no physical size")), "{:?} {:?}", out.result, out.errors);
}

#[test]
fn import_svg_refuses_by_name_and_is_one_undo_step() {
    // No stock in a fresh document: refused before anything is placed.
    let world = new_world();
    let why = ImportSvg::from_bytes(&world, None, "plate.svg", PLATE.as_bytes(), &DxfOptions::default());
    assert!(matches!(why, Err(CommandError::Invalid(ref m)) if m.contains("no stock")), "{:?}", why.err());

    let dir = tmp_dir("command");
    let mut c = sheet_client();
    let broken = write(&dir, "broken.svg", "<svg><path d=\"M0 0 L\"/></svg>");
    let why = c.refused("import_svg", json!({ "path": broken }));
    assert!(why.contains("SVG import:") && why.contains("malformed path"), "{why}");
    let empty = write(&dir, "empty.svg", "<svg width=\"1mm\" height=\"1mm\"><text>hi</text></svg>");
    let why = c.refused("import_svg", json!({ "path": empty }));
    assert!(why.contains("nothing to place") && why.contains("1 skipped"), "{why}");
    assert_eq!(c.ok("get_job", json!({}))["profiles"], json!([]), "nothing landed");

    // The command is one undo step: apply places the profile, invert removes it.
    let mut history = History::default();
    let world = c_world_with_stock();
    let cmd = ImportSvg::from_bytes(&world, None, "plate.svg", PLATE.as_bytes(), &DxfOptions::default()).expect("imports");
    assert_eq!(cmd.name(), "plate");
    history.execute(&world, cmd);
    assert!(camcraft_app::sketch::find(&world, "plate").is_some());
    history.undo(&world);
    assert!(camcraft_app::sketch::find(&world, "plate").is_none());
    history.redo(&world);
    assert!(camcraft_app::sketch::find(&world, "plate").is_some());
}

/// A world with a 100 x 50 x 6 box stock and no model.
fn c_world_with_stock() -> World {
    let world = new_world();
    let shape = StockShape::Box { dx: 100.0, dy: 50.0, dz: 6.0 };
    SetStock::new(&world, shape, Location::default()).expect("stock").apply(&world);
    world
}
