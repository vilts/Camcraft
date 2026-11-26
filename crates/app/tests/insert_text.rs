//! #2679: a typed string becomes a profile of engraving strokes on the
//! top face of a setup's stock - through the undoable `InsertText` and
//! through the MCP tool `insert_text`. The placement, undo and redo, the
//! `.camproj` round trip, the refusals by name and the 2D Engrave
//! operation on the text are pinned here.
//!
//! Provenance: our own tests over our own command, tool and stroke font.
//!
//! Gate map (M4.4): covers app::commands (InsertText, MoveSketch),
//! app::mcp (sketch_tools insert_text, job get_job), app::project
//! (sketch_dto), app::sketch, app::sketch_model.

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{Command, CommandError, CreateSetup, History, InsertText, MoveSketch, SetStock, TextOptions};
use camcraft_app::geometry::Point3;
use camcraft_app::mcp::Server;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::world::new_world;
use camcraft_app::{model, project, sketch};

fn options(text: &str) -> TextOptions {
    TextOptions { text: text.into(), height: 6.0, letter_spacing: 0.0, x: 0.0, y: 0.0, angle_deg: 0.0, name: None }
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

fn refusal(world: &World, setup: Option<&str>, o: &TextOptions) -> String {
    match InsertText::new(world, setup, o) {
        Ok(_) => panic!("{o:?} must be refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn typed_text_lands_as_a_placed_profile_and_undoes() {
    let (world, mut history) = plate_job();
    let o = TextOptions { x: 12.0, y: 20.0, angle_deg: 90.0, ..options("SN-0471") };
    run(&world, &mut history, InsertText::new(&world, None, &o), "text");

    let (entity, profile) = sketch::find(&world, "Text SN-0471").expect("the text is a profile");
    assert_eq!(profile.setup.as_deref(), Some("Plate"));
    assert_eq!(profile.data.layers(), ["text"]);
    assert!(profile.data.chains().rejected.is_empty());
    // S, N, -, the slash of 0, 4, 7 and the two strokes of 1 are open; the ring of 0 is closed.
    assert_eq!((profile.data.chains().open.len(), profile.data.chains().closed.len()), (8, 1));
    let (lo, hi) = profile.data.bounds().expect("bounds");
    assert_eq!((lo.x, lo.y, hi.y), (0.0, 0.0, 6.0), "the baseline origin and the cap height");
    assert!(!model::is_model(&world, &entity), "a text profile is not a model");

    // On the stock top (Z 5 + 10), at the stated spot and turn.
    assert_eq!(model::transform_of(&world, &entity).and_then(sketch::face_placement), Some((12.0, 20.0, 90.0)));
    assert_eq!(sketch::world_pose(&world, &entity).map(|p| p.pos), Some(Point3::new(12.0, 20.0, 15.0)));

    // The same string again gets its own name, and `MoveSketch` moves a text profile.
    run(&world, &mut history, InsertText::new(&world, None, &options("SN-0471")), "second");
    assert!(sketch::find(&world, "Text SN-0471 (2)").is_some());
    run(&world, &mut history, MoveSketch::new(&world, "Text SN-0471 (2)", 5.0, 40.0, 0.0), "move");

    assert!(history.undo(&world) && history.undo(&world), "undo the move and the second text");
    assert!(sketch::find(&world, "Text SN-0471 (2)").is_none());
    assert!(history.undo(&world), "undo the first text");
    assert!(sketch::sketches(&world).is_empty(), "undo removes the profile");
    assert!(history.redo(&world));
    let (again, back) = sketch::find(&world, "Text SN-0471").expect("redo puts the text back");
    assert_eq!(back, profile);
    assert_eq!(model::transform_of(&world, &again).and_then(sketch::face_placement), Some((12.0, 20.0, 90.0)));
}

#[test]
fn bad_text_and_bad_places_are_refused_by_name_and_nothing_lands() {
    let (world, _history) = plate_job();
    let why = refusal(&world, None, &options("sn-0471"));
    assert!(why.contains("'s' at character 1 has no glyph") && why.contains("capital letters"), "{why}");
    assert!(refusal(&world, None, &options("  ")).contains("the text is empty"));
    let why = refusal(&world, None, &TextOptions { height: 0.0, ..options("A") });
    assert!(why.contains("text height 0 mm is not usable"), "{why}");
    let why = refusal(&world, None, &TextOptions { letter_spacing: -1.0, ..options("A") });
    assert!(why.contains("letter spacing -1 mm is not usable"), "{why}");
    for bad in [TextOptions { x: f64::NAN, ..options("A") }, TextOptions { angle_deg: f64::INFINITY, ..options("A") }] {
        assert!(refusal(&world, None, &bad).contains("placement must be finite"));
    }
    let why = refusal(&world, None, &TextOptions { y: 2.0e6, ..options("A") });
    assert!(why.contains("from the stock origin"), "{why}");
    assert!(refusal(&world, Some("Nowhere"), &options("A")).contains("no entity named 'Nowhere'"));
    let why = refusal(&world, None, &TextOptions { name: Some("Stock".into()), ..options("A") });
    assert!(why.contains("'Stock' is not a usable name"), "{why}");
    assert!(sketch::sketches(&world).is_empty(), "a refused text leaves the document as it was");

    // No stock, no place.
    let bare = new_world();
    let why = refusal(&bare, None, &options("A"));
    assert!(why.contains("has no stock - make a stock first"), "{why}");
}

#[test]
fn a_text_profile_saves_and_loads_with_its_strokes_and_place() {
    let (world, mut history) = plate_job();
    let o = TextOptions { x: -8.0, y: 3.5, angle_deg: -30.0, letter_spacing: 0.75, name: Some("Serial".into()), ..options("P/N 8B") };
    run(&world, &mut history, InsertText::new(&world, Some("Plate"), &o), "text");
    let text = project::save_text(&world).expect("the document saves");
    let again = new_world();
    project::open(&again, &text).unwrap_or_else(|e| panic!("the file opens: {e}"));

    let (e0, before) = sketch::find(&world, "Serial").expect("profile");
    let (e1, after) = sketch::find(&again, "Serial").expect("the text profile loads");
    assert_eq!(after, before, "setup, strokes, layer, units, tolerance, chains and summary");
    assert_eq!(model::transform_of(&again, &e1), model::transform_of(&world, &e0));
    assert_eq!(sketch::world_pose(&again, &e1), sketch::world_pose(&world, &e0));
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
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

#[test]
fn the_mcp_tool_places_text_reports_it_and_takes_an_engrave_operation() {
    let mut c = Client { server: Server::new(), next: 1 };
    let why = c.refused("insert_text", json!({ "text": "A1", "height": 5.0 }));
    assert!(why.starts_with("insert_text:") && why.contains("has no stock - make a stock first"), "{why}");
    c.ok("set_stock", json!({ "box": { "dx": 120.0, "dy": 80.0, "dz": 6.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0400-2FL" }));

    let r = c.ok("insert_text", json!({ "text": "SN-0471", "height": 6.0, "position": [10.0, 15.0], "angle": 45.0 }));
    assert_eq!(r["profile"], json!("Text SN-0471"));
    assert_eq!(r["text"], json!("SN-0471"));
    assert_eq!(r["height"], json!(6.0));
    assert_eq!(r["layers"], json!(["text"]));
    assert_eq!(r["units"]["used"], json!("mm"));
    assert_eq!(r["chains"], json!({ "closed": 1, "open": 8, "rejected": 0 }));
    assert_eq!(r["bounds"], json!({ "min": [0.0, 0.0], "max": [39.0, 6.0] }), "the last glyph, 1, is 3 units wide");
    assert_eq!(r["placement"], json!({ "x": 10.0, "y": 15.0, "angle": 45.0 }));
    assert_eq!(r["origin"], json!([10.0, 15.0, 6.0]), "on the top face of the 6 mm stock");
    let summary = r["summary"].as_str().expect("summary");
    assert!(summary.contains("\"SN-0471\" at a cap height of 6 mm") && summary.contains("1 closed and 8 open chains"), "{summary}");

    // Refusals are the command's own text; a lower-case letter is not changed.
    let why = c.refused("insert_text", json!({ "text": "sn", "height": 6.0 }));
    assert!(why.contains("'s' at character 1 has no glyph"), "{why}");
    let why = c.refused("insert_text", json!({ "text": "A", "height": "tall" }));
    assert!(why.contains("\"height\" must be a number"), "{why}");
    let why = c.refused("insert_text", json!({ "text": "A" }));
    assert!(why.contains("\"height\" is required"), "{why}");
    let why = c.refused("insert_text", json!({ "text": "A", "height": 6.0, "name": "Text SN-0471" }));
    assert!(why.contains("already exists"), "{why}");

    // get_job lists the text with the profiles; the profile tools move it; undo and redo work.
    let names = |job: &Value| -> Vec<String> {
        job["profiles"].as_array().expect("profiles").iter().filter_map(|p| p["profile"].as_str().map(str::to_owned)).collect()
    };
    assert_eq!(names(&c.ok("get_job", json!({}))), ["Text SN-0471"]);
    let moved = c.ok("move_dxf_profile", json!({ "name": "Text SN-0471", "position": [20.0, 30.0] }));
    assert_eq!(moved["placement"], json!({ "x": 20.0, "y": 30.0, "angle": 45.0 }));
    c.ok("undo", json!({}));
    c.ok("undo", json!({}));
    assert!(names(&c.ok("get_job", json!({}))).is_empty(), "undo removes the text");
    c.ok("redo", json!({}));
    assert_eq!(names(&c.ok("get_job", json!({}))), ["Text SN-0471"]);

    // A 2D Engrave operation takes the text as its source and states what it is checked against.
    c.ok("add_operation", json!({ "kind": "Engrave", "tool": "EM-0400-2FL", "source": "Text SN-0471", "name": "Mark" }));
    let job = c.ok("get_job", json!({}));
    let checks = job["operations"][0]["checks_against"].as_str().expect("an operation on a profile states its checks");
    assert!(checks.contains("'Text SN-0471'") && checks.contains("1 closed, 8 open"), "{checks}");
}

#[test]
fn script_verb_places_text_and_undoes_and_checks_inputs() {
    let dir = std::env::temp_dir().join(format!("camcraft-script-text-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");

    // Start with a project that has a stock.
    let mut c = Client { server: Server::new(), next: 1 };
    c.ok("set_stock", json!({ "box": { "dx": 120.0, "dy": 80.0, "dz": 6.0 } }));
    let file = dir.join("text_script.camproj").display().to_string();
    c.ok("save_project", json!({ "path": file }));

    let run = |body: &str| camcraft_app::script::run(&format!("cam.load({file:?}); {body}"));

    // 1. Success with options object
    let out = run("cam.insertText('SN-0471', { height: 6.0, position: [10.0, 15.0], angle: 45.0 })");
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["profile"], json!("Text SN-0471"));
    assert_eq!(out.result["text"], json!("SN-0471"));
    assert_eq!(out.result["height"].as_f64(), Some(6.0));
    assert_eq!(out.result["layers"], json!(["text"]));
    assert_eq!(out.result["units"]["used"], json!("mm"));
    assert_eq!(out.result["chains"], json!({ "closed": 1, "open": 8, "rejected": 0 }));
    assert_eq!(out.result["placement"]["x"].as_f64(), Some(10.0));
    assert_eq!(out.result["placement"]["y"].as_f64(), Some(15.0));
    assert_eq!(out.result["placement"]["angle"].as_f64(), Some(45.0));
    assert_eq!(out.result["origin"][0].as_f64(), Some(10.0));
    assert_eq!(out.result["origin"][1].as_f64(), Some(15.0));
    assert_eq!(out.result["origin"][2].as_f64(), Some(6.0));
    let summary = out.result["summary"].as_str().expect("summary");
    assert!(summary.contains("\"SN-0471\" at a cap height of 6 mm") && summary.contains("1 closed and 8 open chains"), "{summary}");

    // 2. Success with height as 2nd argument
    let out = run("cam.insertText('REV-B', 8.0, { letter_spacing: 1.0, name: 'PartRev' }).profile");
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result, json!("PartRev"));

    // 3. Success with height as 2nd argument without options object
    let out = run("cam.insertText('A1', 5.0).profile");
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result, json!("Text A1"));

    // 4. Undo and redo work via MCP / history after script verb calls
    let out = run("cam.insertText('BATCH-1', { height: 5.0 }); cam.call('undo'); cam.call('get_job')");
    assert!(out.ok, "{:?}", out.errors);
    let job = out.result;
    let profiles = job["profiles"].as_array().expect("profiles");
    assert!(!profiles.iter().any(|p| p["profile"] == "Text BATCH-1"), "undo removes the profile");

    // 5. Refusals: missing height
    let out = run("cam.insertText('SN-0471', {})");
    assert!(!out.ok && out.errors.iter().any(|e| e.contains("\"height\" is required")), "{:?}", out.errors);

    // 6. Refusals: no arguments
    let out = run("cam.insertText()");
    assert!(!out.ok && out.errors.iter().any(|e| e.contains("requires a text string")), "{:?}", out.errors);

    // 7. Refusals: bad character (lowercase)
    let out = run("cam.insertText('sn-0471', { height: 6.0 })");
    assert!(!out.ok && out.errors.iter().any(|e| e.contains("has no glyph") && e.contains("capital letters")), "{:?}", out.errors);

    // 8. Refusals: unknown option
    let out = run("cam.insertText('SN-0471', { height: 6.0, unknown_opt: true })");
    assert!(!out.ok && out.errors.iter().any(|e| e.contains("unknown option \"unknown_opt\"")), "{:?}", out.errors);

    // 9. Refusals: invalid placement (non-finite or wrong arity)
    let out = run("cam.insertText('SN-0471', { height: 6.0, position: [1.0] })");
    assert!(!out.ok && out.errors.iter().any(|e| e.contains("must be an array of 2 finite numbers")), "{:?}", out.errors);

    // 10. Refusals: no stock in fresh document
    let out = camcraft_app::script::run("cam.insertText('SN-0471', { height: 6.0 })");
    assert!(!out.ok && out.errors.iter().any(|e| e.contains("has no stock - make a stock first")), "{:?}", out.errors);

    let _ = std::fs::remove_dir_all(&dir);
}
