//! #1339: `VouchModelFidelity` clears the "fidelity unknown" mark (#1056)
//! of one model and of the stocks copied from it, on a person's word. It
//! is one labelled undo step that names who vouched; undo puts each mark
//! back. It never applies to a recorded coarse chord (#864) or to an
//! incomplete mesh (#1610). The record of who vouched rides the project
//! file and speaks only for the facets it was given for.
//!
//! Gate map (#1339): covers app::commands::vouch_fidelity,
//! app::commands::delete_entity, app::model_vouch, app::model_fidelity (op_problem, gate_problems,
//! program_tol), app::project (the model's `fidelity_vouched`),
//! app::posting (verification_report), app::mcp::job (get_job).
//! Lane shape (M4.4): every test here runs in seconds.
//!
//! Provenance: our own pins over our own laws; no vendor material.

use camcraft_app::commands::{
    AddTool, CommandError, CreateOperation, DeleteEntity, GenerateOperation, History, ImportModel, SetStock, UpdateModel,
    VouchModelFidelity,
};
use camcraft_app::geometry::{cube, Mesh, Point3};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{mesh_open, model, model_fidelity, model_vouch, posting, project};
use flecs_ecs::prelude::World;

const MILL: &str = "EM-1000-4FL";
const TOO_COARSE: f64 = 0.03;
const UNKNOWN_REFUSAL: &str = "OP: model 'bar' was saved before CamCraft recorded mesh fidelity (project v58 or older), so a gouge check against it cannot be trusted; re-import the model to verify its operations";

fn brick() -> Mesh {
    cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0))
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_radius: None,
        blend_turns: false,
        picks: Default::default(),
    }
}

/// The part "bar" of unknown fidelity, the stock copied from it (so the
/// copy carries the mark itself), and a Face op that finishes the flat
/// top of "bar" exactly.
fn unknown_bar() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("bar", brick()).with_fidelity(Some(model_fidelity::UNKNOWN_MM)));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "bar".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("bar".into()), face()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    (world, h)
}

fn chord(world: &World, name: &str) -> Option<f64> {
    model::find_by_name(world, name).and_then(|e| model_fidelity::chord_of(world, &e))
}

fn vouched_by(world: &World, name: &str) -> Option<String> {
    model::find_by_name(world, name).and_then(|e| model_vouch::by_of(world, &e))
}

fn refusal(world: &World, name: &str, by: &str) -> String {
    match VouchModelFidelity::new(world, name, by) {
        Ok(_) => panic!("vouching for '{name}' by '{by}' should be refused"),
        Err(e) => e.to_string(),
    }
}

#[test]
fn a_vouched_model_passes_the_gate_and_its_undo_is_refused_again() {
    let (world, mut h) = unknown_bar();
    assert!(chord(&world, "bar").is_some_and(model_fidelity::is_unknown));
    assert!(chord(&world, "Stock").is_some_and(model_fidelity::is_unknown), "the copy carries the mark itself");
    assert_eq!(posting::verification_report(&world).problems, vec![UNKNOWN_REFUSAL.to_string()]);
    assert_eq!(model_fidelity::program_tol(&world), Err(UNKNOWN_REFUSAL.to_string()));

    let cmd = VouchModelFidelity::new(&world, "bar", "  M. Kinney ").expect("vouch");
    assert_eq!(cmd.by(), "M. Kinney", "the name is trimmed");
    assert_eq!(cmd.copies(), ["Stock".to_string()]);
    cmd.run(&mut h, &world);
    assert_eq!(h.top_label(), Some("Vouch for the mesh of 'bar' (by M. Kinney)"), "the history names who vouched");
    assert_eq!(chord(&world, "bar"), None);
    assert_eq!(chord(&world, "Stock"), None, "the stock copy follows its model");
    assert_eq!(vouched_by(&world, "bar").as_deref(), Some("M. Kinney"));
    assert_eq!(posting::verification_report(&world).problems, Vec::<String>::new());
    assert!(model_fidelity::program_tol(&world).is_ok());

    // One undo step: each mark is back and the record is gone.
    assert!(h.undo(&world));
    assert!(chord(&world, "bar").is_some_and(model_fidelity::is_unknown));
    assert!(chord(&world, "Stock").is_some_and(model_fidelity::is_unknown));
    assert_eq!(vouched_by(&world, "bar"), None);
    assert_eq!(posting::verification_report(&world).problems, vec![UNKNOWN_REFUSAL.to_string()]);
    assert_eq!(model_fidelity::program_tol(&world), Err(UNKNOWN_REFUSAL.to_string()));

    assert!(h.redo(&world));
    assert_eq!(chord(&world, "bar"), None);
    assert_eq!(vouched_by(&world, "bar").as_deref(), Some("M. Kinney"));
    assert_eq!(posting::verification_report(&world).problems, Vec::<String>::new());
}

#[test]
fn a_recorded_coarse_chord_is_never_vouched_away() {
    // #864: a recorded chord is a measurement. Too coarse stays refused.
    let (world, mut h) = unknown_bar();
    h.execute(&world, ImportModel::from_mesh("coarse", brick()).with_fidelity(Some(TOO_COARSE)));
    let said = refusal(&world, "coarse", "A");
    assert!(said.contains("'coarse' has a recorded fidelity") && said.contains("0.030 mm") && said.contains("re-import"), "{said}");
    assert!(said.contains("larger STEP facet budget, or export the file again with a tighter tolerance"), "{said}");
    assert_eq!(chord(&world, "coarse"), Some(TOO_COARSE));

    // A stock copied from the unknown model that carries a RECORDED chord
    // keeps it: only the unknown mark leaves with the model's.
    let stock = model::find_by_name(&world, "Stock").expect("stock");
    model_fidelity::set(&world, &stock, Some(TOO_COARSE));
    let cmd = VouchModelFidelity::new(&world, "bar", "A").expect("vouch");
    assert!(cmd.copies().is_empty(), "{:?}", cmd.copies());
    cmd.run(&mut h, &world);
    assert_eq!(chord(&world, "Stock"), Some(TOO_COARSE));
    h.undo(&world);
    assert_eq!(chord(&world, "Stock"), Some(TOO_COARSE), "undo marks only what the command cleared");
}

#[test]
fn every_other_bad_input_is_refused_by_name() {
    let (world, mut h) = unknown_bar();
    let depth_before = h.top_label().map(str::to_owned);
    for by in ["", "   ", "a\nb"] {
        let said = refusal(&world, "bar", by);
        assert!(said.contains("who vouches"), "{by:?}: {said}");
    }
    assert!(refusal(&world, "bar", &"x".repeat(model_vouch::MAX_BY + 1)).contains("at most 120 characters"));
    assert!(matches!(VouchModelFidelity::new(&world, "nope", "A"), Err(CommandError::NotFound(n)) if n == "nope"));
    assert!(refusal(&world, "Stock", "A").contains("'Stock' is not a model"));
    assert!(refusal(&world, "OP", "A").contains("'OP' is not a model"));

    // A model at full fidelity has nothing to vouch for.
    h.execute(&world, ImportModel::from_mesh("fine", brick()));
    assert!(refusal(&world, "fine", "A").contains("'fine' is not marked fidelity unknown"));

    // #1610: an incomplete mesh rides the same sentinel and is never vouched for.
    let hole = "closed shell #9 (4 faces) meshes open";
    h.execute(
        &world,
        UpdateModel::new(&world, "fine", brick()).expect("update").with_fidelity(Some(model_fidelity::UNKNOWN_MM)).with_hole(Some(hole.into())),
    );
    let fine = model::find_by_name(&world, "fine").expect("fine");
    assert_eq!(mesh_open::hole_of(&world, &fine).as_deref(), Some(hole));
    let said = refusal(&world, "fine", "A");
    assert!(said.contains("'fine' was imported incomplete") && said.contains(hole) && said.contains("cannot be vouched for"), "{said}");
    assert!(chord(&world, "fine").is_some_and(model_fidelity::is_unknown));

    // No refusal changed the document or the history.
    assert_eq!(h.top_label().map(str::to_owned), depth_before);
    assert!(chord(&world, "bar").is_some_and(model_fidelity::is_unknown));
    assert_eq!(vouched_by(&world, "bar"), None);
}

#[test]
fn the_record_speaks_only_for_the_facets_it_was_given_for() {
    let (world, mut h) = unknown_bar();
    VouchModelFidelity::new(&world, "bar", "A").expect("vouch").run(&mut h, &world);
    assert_eq!(vouched_by(&world, "bar").as_deref(), Some("A"));

    // Other facets land: the word given for the old ones says nothing.
    h.execute(&world, UpdateModel::new(&world, "bar", brick()).expect("update"));
    assert_eq!(vouched_by(&world, "bar"), None, "a fresh import ends the vouch");
    // Its undo brings back the facets the word was given for.
    h.undo(&world);
    assert_eq!(vouched_by(&world, "bar").as_deref(), Some("A"));

    // A flag on the mesh again silences the record, and a second vouch
    // replaces it; undo restores the first one.
    let bar = model::find_by_name(&world, "bar").expect("bar");
    model_fidelity::set(&world, &bar, Some(model_fidelity::UNKNOWN_MM));
    assert_eq!(vouched_by(&world, "bar"), None);
    VouchModelFidelity::new(&world, "bar", "B").expect("vouch again").run(&mut h, &world);
    assert_eq!(vouched_by(&world, "bar").as_deref(), Some("B"));
    h.undo(&world);
    model_fidelity::set(&world, &bar, None);
    assert_eq!(vouched_by(&world, "bar").as_deref(), Some("A"));
}

#[test]
fn the_vouch_rides_the_project_file() {
    let dir = std::env::temp_dir();
    let file = |tag: &str| dir.join(format!("camcraft-1339-vouch-{tag}-{}.camproj", std::process::id()));
    let (a, b) = (file("a"), file("b"));
    let (world, mut h) = unknown_bar();

    // Before: the mark is saved and nothing speaks of a vouch.
    project::save(&world, &a).expect("save");
    assert!(!std::fs::read_to_string(&a).expect("read").contains("fidelity_vouched"));

    VouchModelFidelity::new(&world, "bar", "M. Kinney").expect("vouch").run(&mut h, &world);
    project::save(&world, &a).expect("save");
    let doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a).expect("read")).expect("json");
    let row = |doc: &serde_json::Value, name: &str| {
        doc["entities"].as_array().expect("entities").iter().find(|e| e["name"] == name).cloned().expect("row")
    };
    let bar = row(&doc, "bar");
    assert_eq!(bar["fidelity_vouched"]["by"], serde_json::json!("M. Kinney"));
    assert!(bar.get("fidelity_unknown").is_none() && bar.get("coarse_mm").is_none(), "{bar}");
    assert!(row(&doc, "Stock").get("fidelity_unknown").is_none(), "the copy is saved unmarked");

    let again = new_world();
    assert_eq!(project::load_noted(&again, &a).expect("load").len(), 0);
    assert_eq!(chord(&again, "bar"), None);
    assert_eq!(chord(&again, "Stock"), None);
    assert_eq!(vouched_by(&again, "bar").as_deref(), Some("M. Kinney"));
    assert_eq!(posting::verification_report(&again).problems, Vec::<String>::new());

    // A corrupt file is refused before any write: a vouch beside a flag,
    // a vouch on a row that is not a model, a vouch with no name.
    let broken = |edit: &dyn Fn(&mut serde_json::Value)| {
        let mut doc = doc.clone();
        edit(&mut doc);
        std::fs::write(&b, serde_json::to_string_pretty(&doc).expect("json")).expect("write");
        project::load(&new_world(), &b).map(|_| ()).map_err(|e| e.to_string())
    };
    let on = |doc: &mut serde_json::Value, name: &str, key: &str, value: serde_json::Value| {
        for e in doc["entities"].as_array_mut().expect("entities") {
            if e["name"] == name {
                e[key] = value.clone();
            }
        }
    };
    let both = broken(&|d| on(d, "bar", "fidelity_unknown", serde_json::json!(true)));
    assert!(both.as_ref().is_err_and(|e| e.contains("a fidelity vouch and a fidelity flag at once")), "{both:?}");
    let coarse = broken(&|d| on(d, "bar", "coarse_mm", serde_json::json!(TOO_COARSE)));
    assert!(coarse.as_ref().is_err_and(|e| e.contains("a fidelity vouch and a fidelity flag at once")), "{coarse:?}");
    let stock = broken(&|d| on(d, "Stock", "fidelity_vouched", serde_json::json!({ "by": "A", "rev": 1 })));
    assert!(stock.as_ref().is_err_and(|e| e.contains("records a fidelity vouch but is not a model")), "{stock:?}");
    let nameless = broken(&|d| on(d, "bar", "fidelity_vouched", serde_json::json!({ "by": " ", "rev": 1 })));
    assert!(nameless.as_ref().is_err_and(|e| e.contains("no usable name")), "{nameless:?}");
    for p in [a, b] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn delete_undo_keeps_the_vouch_and_its_revision_through_save_and_load() {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, ImportModel::from_mesh("bar", brick()));
    history.execute(
        &world,
        UpdateModel::new(&world, "bar", brick())
            .expect("update")
            .with_fidelity(Some(model_fidelity::UNKNOWN_MM)),
    );
    VouchModelFidelity::new(&world, "bar", "M. Kinney")
        .expect("vouch")
        .run(&mut history, &world);
    let bar = model::find_by_name(&world, "bar").expect("bar");
    let revision = model::rev_of(&world, &bar);
    assert!(
        revision > 1,
        "a fresh entity revision must not match this vouch"
    );
    let record = model_vouch::record_of(&world, &bar).expect("vouch record");
    assert_eq!(record.rev, revision);

    // Each undo must recreate the same record for the same facets, even
    // after the deletion has been repeated through redo.
    for cycle in 0..2 {
        if cycle == 0 {
            history.execute(&world, DeleteEntity::new(&world, "bar").expect("delete"));
        } else {
            assert!(history.redo(&world));
        }
        assert!(model::find_by_name(&world, "bar").is_none());
        assert!(history.undo(&world));
        let restored = model::find_by_name(&world, "bar").expect("restored bar");
        let restored_record =
            model_vouch::record_of(&world, &restored).expect("restored vouch record");
        assert_eq!(restored_record.by, record.by);
        assert_eq!(restored_record.rev, record.rev);
        assert_eq!(model::rev_of(&world, &restored), revision);
        assert_eq!(vouched_by(&world, "bar").as_deref(), Some("M. Kinney"));
        assert_eq!(chord(&world, "bar"), None);
    }

    let path = std::env::temp_dir().join(format!(
        "camcraft-2821-delete-vouch-{}.camproj",
        std::process::id()
    ));
    project::save(&world, &path).expect("save after undo");
    let saved: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let row = saved["entities"]
        .as_array()
        .expect("entities")
        .iter()
        .find(|row| row["name"] == "bar")
        .expect("saved bar");
    assert_eq!(
        row["fidelity_vouched"]["by"],
        serde_json::json!("M. Kinney")
    );
    assert_eq!(row["fidelity_vouched"]["rev"], serde_json::json!(revision));

    let loaded = new_world();
    project::load(&loaded, &path).expect("load after undo");
    assert_eq!(vouched_by(&loaded, "bar").as_deref(), Some("M. Kinney"));
    assert_eq!(chord(&loaded, "bar"), None);
    let mut server = Server::new();
    server
        .handle(&serde_json::json!({
            "jsonrpc": "2.0", "id": 0, "method": "initialize",
            "params": { "protocolVersion": "2025-06-18" }
        }))
        .expect("initialize MCP");
    let mut request = |id, name, arguments| {
        server
            .handle(&serde_json::json!({
                "jsonrpc": "2.0", "id": id, "method": "tools/call",
                "params": { "name": name, "arguments": arguments }
            }))
            .expect("MCP response")
    };
    let opened = request(
        1,
        "open_project",
        serde_json::json!({ "path": path.to_string_lossy() }),
    );
    assert_ne!(
        opened["result"]["isError"],
        serde_json::json!(true),
        "{opened}"
    );
    let job = request(2, "get_job", serde_json::json!({}));
    let row = job["result"]["structuredContent"]["models"]
        .as_array()
        .expect("MCP models")
        .iter()
        .find(|row| row["name"] == "bar")
        .expect("MCP bar");
    assert_eq!(
        row["fidelity_vouched"]["by"],
        serde_json::json!("M. Kinney")
    );
    std::fs::remove_file(&path).expect("remove project");
}
