//! #3072: typed text in a TrueType outline font becomes a profile of
//! closed shapes on the top face of a setup's stock - through the
//! undoable `InsertOutlineText` and through the MCP tool
//! `insert_outline_text`. The placement, undo and redo, the `.camproj`
//! round trip, the refusals by name and a 2D Pocket and a 2D Contour
//! generated on the shapes are pinned here.
//!
//! Provenance: our own tests over our own command and tool, on the
//! generated font of `camcraft_geometry::outline_text::sample_font`.
//!
//! Gate map: covers app::commands (InsertOutlineText), app::mcp
//! (outline_text_tools insert_outline_text), app::project (sketch_dto),
//! app::sketch.

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{
    Command, CommandError, CreateSetup, History, InsertOutlineText, OutlineTextOptions, SetStock,
};
use camcraft_app::geometry::Point3;
use camcraft_app::mcp::Server;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;
use camcraft_app::{project, sketch};
use camcraft_geometry::outline_text::sample_font::sample_font;
use camcraft_geometry::sketch_chains::LoopRole;

fn font_file(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!("camcraft-outline-text-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("sample.ttf");
    std::fs::write(&path, sample_font()).expect("write the font");
    path
}

fn options(font: &Path, text: &str) -> OutlineTextOptions {
    OutlineTextOptions {
        font: font.to_path_buf(),
        text: text.into(),
        height: 7.0,
        letter_spacing: 0.0,
        x: 0.0,
        y: 0.0,
        angle_deg: 0.0,
        name: None,
    }
}

fn run(world: &World, history: &mut History, cmd: Result<impl Command + 'static, CommandError>, what: &str) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e}")));
}

/// One setup `Plate` with a 100 x 60 x 10 stock whose origin is at Z 5.
fn plate_job() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    run(&world, &mut history, CreateSetup::new(&world, Some("Plate"), None), "Plate");
    let at = Location { pos: Point3::new(0.0, 0.0, 5.0), ..Default::default() };
    run(&world, &mut history, SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 60.0, dz: 10.0 }, at), "stock");
    (world, history)
}

fn refusal(world: &World, o: &OutlineTextOptions) -> String {
    match InsertOutlineText::new(world, None, o) {
        Ok(_) => panic!("{o:?} must be refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn outline_text_lands_as_closed_shapes_and_undoes() {
    let font = font_file("lands");
    let (world, mut history) = plate_job();
    let o = OutlineTextOptions { x: 12.0, y: 20.0, angle_deg: 90.0, ..options(&font, "HOD") };
    run(&world, &mut history, InsertOutlineText::new(&world, None, &o), "text");

    let (entity, profile) = sketch::find(&world, "Text HOD").expect("the text is a profile");
    assert_eq!(profile.setup.as_deref(), Some("Plate"));
    assert_eq!(profile.data.layers(), ["text"]);
    let chains = profile.data.chains();
    assert!(chains.open.is_empty() && chains.rejected.is_empty(), "only closed shapes");
    let holes = chains.closed.iter().filter(|c| matches!(c.nesting, Ok(n) if n.role == LoopRole::Hole)).count();
    assert_eq!((chains.closed.len(), holes), (4, 1), "H, O with its hole, D");
    let (lo, hi) = profile.data.bounds().expect("bounds");
    assert_eq!((lo.x, lo.y, hi.y), (0.0, 0.0, 7.0), "the baseline origin and the cap height");
    assert_eq!(sketch::world_pose(&world, &entity).map(|p| p.pos), Some(Point3::new(12.0, 20.0, 15.0)));

    assert!(history.undo(&world), "undo the text");
    assert!(sketch::sketches(&world).is_empty(), "undo removes the profile");
    assert!(history.redo(&world));
    let (_, back) = sketch::find(&world, "Text HOD").expect("redo puts the text back");
    assert_eq!(back, profile);
}

#[test]
fn bad_fonts_and_bad_text_are_refused_by_name_and_nothing_lands() {
    let font = font_file("refusals");
    let (world, _history) = plate_job();
    let missing = font.with_file_name("missing.ttf");
    assert!(refusal(&world, &options(&missing, "H")).contains("cannot read the font"));
    let junk = font.with_file_name("junk.ttf");
    std::fs::write(&junk, b"this is not a font").expect("write");
    let why = refusal(&world, &options(&junk, "H"));
    assert!(why.contains("junk.ttf") && why.contains("not a TrueType or OpenType font"), "{why}");
    let cff = font.with_file_name("cff.otf");
    std::fs::write(&cff, b"OTTO\0\0\0\0\0\0\0\0").expect("write");
    assert!(refusal(&world, &options(&cff, "H")).contains("CFF (PostScript) outlines"));
    assert!(refusal(&world, &options(&font, "HZ")).contains("'Z' at character 2 is not in the font"));
    assert!(refusal(&world, &options(&font, "X")).contains("the text outlines cross or touch"));
    assert!(refusal(&world, &options(&font, " ")).contains("the text is empty"));
    let why = refusal(&world, &OutlineTextOptions { x: f64::NAN, ..options(&font, "H") });
    assert!(why.contains("placement must be finite"), "{why}");
    let why = refusal(&world, &OutlineTextOptions { name: Some("Stock".into()), ..options(&font, "H") });
    assert!(why.contains("'Stock' is not a usable name"), "{why}");
    assert!(sketch::sketches(&world).is_empty(), "a refused text leaves the document as it was");
    let bare = new_world();
    assert!(refusal(&bare, &options(&font, "H")).contains("has no stock - make a stock first"));
}

#[test]
fn an_outline_text_profile_saves_and_loads_with_its_shapes() {
    let font = font_file("save");
    let (world, mut history) = plate_job();
    let o = OutlineTextOptions { x: -8.0, y: 3.5, angle_deg: -30.0, letter_spacing: 0.75, name: Some("Sign".into()), ..options(&font, "8 Do") };
    run(&world, &mut history, InsertOutlineText::new(&world, Some("Plate"), &o), "text");
    let text = project::save_text(&world).expect("the document saves");
    let again = new_world();
    project::open(&again, &text).unwrap_or_else(|e| panic!("the file opens: {e}"));
    let (_, before) = sketch::find(&world, "Sign").expect("profile");
    let (_, after) = sketch::find(&again, "Sign").expect("the text profile loads");
    assert_eq!(after, before, "shapes, layer, units, tolerance, chains and summary");
    assert_eq!(project::save_text(&again).expect("saves again"), text, "save, load, save: the same bytes");
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
        assert!(!err, "{name} failed: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

#[test]
fn the_mcp_tool_places_outline_text_for_a_pocket_and_a_contour() {
    let font = font_file("mcp");
    let path = font.to_string_lossy().into_owned();
    let mut c = Client { server: Server::new(), next: 1 };
    let why = c.refused("insert_outline_text", json!({ "font": path, "text": "HO", "height": 7.0 }));
    assert!(why.starts_with("insert_outline_text:") && why.contains("has no stock - make a stock first"), "{why}");
    c.ok("set_stock", json!({ "box": { "dx": 120.0, "dy": 80.0, "dz": 6.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));

    let r = c.ok("insert_outline_text", json!({ "font": path, "text": "HO", "height": 7.0, "position": [10.0, 15.0] }));
    assert_eq!(r["profile"], json!("Text HO"));
    assert_eq!(r["layers"], json!(["text"]));
    assert_eq!(r["chains"], json!({ "closed": 3, "open": 0, "rejected": 0 }));
    assert_eq!(r["bounds"], json!({ "min": [0.0, 0.0], "max": [12.0, 7.0] }), "H advances 6 mm, O is 6 mm wide");
    let summary = r["summary"].as_str().expect("summary");
    assert!(summary.contains("\"HO\" at a cap height of 7 mm, 3 closed shapes"), "{summary}");
    let why = c.refused("insert_outline_text", json!({ "font": path, "text": "HO" }));
    assert!(why.contains("\"height\" is required"), "{why}");
    let why = c.refused("insert_outline_text", json!({ "text": "HO", "height": 7.0 }));
    assert!(why.contains("font"), "{why}");

    // A 2D Pocket and a 2D Contour take the whole text profile as their source: only closed
    // chains reach them. (A toolpath from profile chains is #1867's work.)
    for (index, (kind, name)) in [("2D Pocket", "Clear"), ("2D Contour", "Cut")].into_iter().enumerate() {
        c.ok("add_operation", json!({ "kind": kind, "tool": "EM-0400-2FL", "source": "Text HO", "name": name }));
        let job = c.ok("get_job", json!({}));
        let checks = job["operations"][index]["checks_against"].as_str().expect("an operation on a profile states its checks");
        assert!(checks.contains("'Text HO'") && checks.contains("3 closed, 0 open"), "{checks}");
    }

    // Undo the two operations, then the text.
    for _ in 0..3 {
        c.ok("undo", json!({}));
    }
    assert!(c.ok("get_job", json!({}))["profiles"].as_array().is_some_and(Vec::is_empty), "undo removes the text");
}
