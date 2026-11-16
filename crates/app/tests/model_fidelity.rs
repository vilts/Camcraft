//! #864: a STEP model imported coarser than the importer promises is
//! flagged (component, project file, MCP), its program verdict is
//! tightened by the excess while that leaves a positive tolerance, and
//! past that the program gate refuses its operations by name instead of
//! emitting gouge verdicts. A model that met the promise - and a project
//! saved before the flag existed - reads exactly as before.
//!
//! Gate map (#864): covers app::model_fidelity, app::operation::fidelity_law,
//! app::geometry::import (graded imports), app::commands (import_model,
//! update_model, repair_model, delete_entity), app::project (the
//! `coarse_mm` key), app::posting (verification_report), app::stock_sim
//! (simulate_document), app::mcp (import_model, get_job).
//!
//! Provenance: our own pins over our own laws; no vendor material.

use camcraft_app::commands::{AddTool, CreateOperation, DeleteEntity, GenerateOperation, History, ImportModel, SetStock, UpdateModel};
use camcraft_app::geometry::import::step;
use camcraft_app::geometry::{cube, Mesh, Point3};
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::DEV_TOL;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_fidelity, posting, project};
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

const MILL: &str = "EM-1000-4FL";
/// A chord error that leaves a positive tolerance (excess 0.007 of 0.01),
/// and one that leaves none.
const TIGHT: f64 = 0.012;
const TOO_COARSE: f64 = 0.03;

fn step_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/Milling-test.step")
}

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
        blend_turns: false,
    }
}

/// The brick imported at `coarse_mm`, its own stock, and a Face op that
/// finishes its flat top exactly.
fn faced_brick(coarse_mm: Option<f64>) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("brick", brick()).with_fidelity(coarse_mm));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "brick".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("brick".into()), face()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

/// Lower every cached move by `dz` (what a generator bug would post).
fn sink(world: &World, dz: f64) {
    let e = model::find_by_name(world, "OP").expect("op");
    let mut t = model::toolpath_of(world, &e).expect("cached");
    for m in &mut t.path.moves {
        m.from.z -= dz;
        m.to.z -= dz;
    }
    e.entity_view(world).set(camcraft_app::schema::Toolpath::new(t.path, t.warnings));
}

fn gouge_line(gate: &[String]) -> Option<&String> {
    gate.iter().find(|l| l.starts_with("gouge:"))
}

#[test]
fn an_over_budget_step_import_is_flagged_and_one_that_fits_is_not() {
    let path = step_path();
    let world = new_world();
    let mut h = History::default();
    let full = ImportModel::new(&path).expect("imports");
    assert_eq!(full.coarse_mm, None, "a model that fits the budget carries no flag");
    h.execute(&world, full.renamed("full"));
    let e = model::find_by_name(&world, "full").expect("landed");
    assert_eq!(model_fidelity::chord_of(&world, &e), None);
    assert_eq!(model_fidelity::excess_of(&world, "full"), 0.0);

    let coarse = step::with_facet_budget(2_000, || ImportModel::new(&path)).expect("imports at reduced fidelity");
    let chord = coarse.coarse_mm.expect("the kept mesh's own chord error rides the command");
    assert!(chord > step::CHORD_CAP && chord.is_finite(), "{chord}");
    h.execute(&world, coarse.renamed("coarse"));
    let e = model::find_by_name(&world, "coarse").expect("landed");
    assert_eq!(model_fidelity::chord_of(&world, &e), Some(chord), "the flag carries the achieved error");
    assert!((model_fidelity::excess_of(&world, "coarse") - (chord - step::CHORD_CAP)).abs() < 1e-12);

    // The flag follows the facets through every command that trades them.
    h.undo(&world);
    assert!(model::find_by_name(&world, "coarse").is_none());
    h.redo(&world);
    let flagged = |w: &World| model::find_by_name(w, "coarse").and_then(|e| model_fidelity::chord_of(w, &e));
    assert_eq!(flagged(&world), Some(chord));
    // An update to a full-fidelity mesh clears it; undo brings it back.
    h.execute(&world, UpdateModel::new(&world, "coarse", brick()).expect("update"));
    assert_eq!(flagged(&world), None);
    h.undo(&world);
    assert_eq!(flagged(&world), Some(chord));
    // A reshaping update (a repair's) keeps it.
    h.execute(&world, UpdateModel::new(&world, "coarse", brick()).expect("update").keeping_fidelity());
    assert_eq!(flagged(&world), Some(chord));
    // Delete and undo: the model comes back flagged.
    h.execute(&world, DeleteEntity::new(&world, "coarse").expect("delete"));
    assert_eq!(flagged(&world), None);
    h.undo(&world);
    assert_eq!(flagged(&world), Some(chord));
    // A stock copied from it is judged as coarse as the model it copies.
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "coarse".into() }, Default::default()).expect("stock"));
    assert!(model_fidelity::excess_of(&world, "Stock") > 0.0);
}

#[test]
fn the_round_trip_of_a_unit_scaled_cap_is_not_a_reduced_fidelity_flag() {
    // An inch file's cap is divided into inches and multiplied back: the
    // result may sit a float's dust above 0.005 and must not be flagged.
    assert_eq!(step::coarse(step::CHORD_CAP), None);
    assert_eq!(step::coarse((step::CHORD_CAP / 25.4) * 25.4), None);
    assert_eq!(step::coarse(step::CHORD_CAP * (1.0 + 1e-12)), None);
    assert_eq!(step::coarse(0.01), Some(0.01), "a metre file stopped at the kernel's 0.01 mm floor");
    assert_eq!(step::coarse(f64::NAN), None);
}

#[test]
fn an_inch_step_is_full_fidelity_and_a_metre_step_is_flagged_at_the_kernels_floor() {
    let model = |n: &str| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(n);
    // The inch twin of the fixture meets the promise: no flag (#839's cap
    // is unit-correct, and the unit round trip is not an excess).
    let inch = ImportModel::new(&model("Milling-test-inch.step")).expect("the inch part imports");
    assert_eq!(inch.coarse_mm, None);
    // A metre file stops at the kernel's least tolerance, 1e-5 m: 0.01 mm,
    // twice the promise. Before #864 that was a log line; now it is the
    // same flag an over-budget model gets - with a tolerance left, so its
    // verdicts are tightened, not refused.
    let metre = ImportModel::new(&model("sample_ap214.step")).expect("the metre sample imports");
    let chord = metre.coarse_mm.expect("flagged");
    assert!((chord - 0.01).abs() < 1e-9, "{chord}");
    let left = camcraft_app::verify::excess_floor(DEV_TOL, model_fidelity::excess(chord)).expect("a tolerance is left");
    assert!((left - 0.005).abs() < 1e-9, "{left}");
}

#[test]
fn a_project_keeps_the_flag_and_one_without_it_saves_back_unchanged() {
    let dir = std::env::temp_dir();
    let file = |tag: &str| dir.join(format!("camcraft-864-{tag}-{}.camproj", std::process::id()));
    // A document with no reduced-fidelity model: the key never appears,
    // so its bytes are what they were before the key existed - and a load
    // and a second save give them back unchanged.
    let (a, b) = (file("plain-a"), file("plain-b"));
    project::save(&faced_brick(None), &a).expect("save");
    let first = std::fs::read_to_string(&a).expect("read");
    assert!(!first.contains("coarse_mm"), "a full-fidelity document writes no fidelity key");
    let loaded = new_world();
    project::load(&loaded, &a).expect("an older-shaped project loads");
    let e = model::find_by_name(&loaded, "brick").expect("model");
    assert_eq!(model_fidelity::chord_of(&loaded, &e), None, "and loads as full fidelity");
    project::save(&loaded, &b).expect("save again");
    assert_eq!(first, std::fs::read_to_string(&b).expect("read"), "saved back unchanged");

    // A flagged model keeps its flag and its number through the file.
    let (c, d) = (file("coarse-a"), file("coarse-b"));
    project::save(&faced_brick(Some(TIGHT)), &c).expect("save");
    let text = std::fs::read_to_string(&c).expect("read");
    assert!(text.contains("\"coarse_mm\""), "the flag is in the file");
    let loaded = new_world();
    project::load(&loaded, &c).expect("load");
    let e = model::find_by_name(&loaded, "brick").expect("model");
    assert_eq!(model_fidelity::chord_of(&loaded, &e), Some(TIGHT));
    project::save(&loaded, &d).expect("save again");
    assert_eq!(text, std::fs::read_to_string(&d).expect("read"));
    for p in [a, b, c, d] {
        let _ = std::fs::remove_file(p);
    }
}

#[test]
fn the_program_verdict_tightens_by_the_excess_while_a_tolerance_is_left() {
    // Full fidelity: today's verdict. A floor cut 0.006 mm too deep is
    // inside the 0.01 mm verdict tolerance; 0.05 mm is a gouge.
    let full = faced_brick(None);
    assert_eq!(model_fidelity::verdict_tol(&full), Ok(DEV_TOL));
    assert!(posting::verification_gate(&full).is_empty(), "{:?}", posting::verification_gate(&full));
    sink(&full, 0.006);
    assert!(posting::verification_gate(&full).is_empty(), "0.006 mm passes at full fidelity");
    sink(&full, 0.044);
    assert!(gouge_line(&posting::verification_gate(&full)).is_some(), "0.05 mm is a gouge at full fidelity");

    // Reduced fidelity with a tolerance left: an exactly finished flat
    // face still posts clean - and the same 0.006 mm is now convicted.
    let tight = faced_brick(Some(TIGHT));
    let tol = model_fidelity::verdict_tol(&tight).expect("a positive tolerance is left");
    assert!((tol - (DEV_TOL - (TIGHT - step::CHORD_CAP))).abs() < 1e-12 && tol > 0.0, "{tol}");
    let e = model::find_by_name(&tight, "OP").expect("op");
    let cached = model::toolpath_of(&tight, &e).expect("cached");
    assert!(cached.warnings.iter().all(|w| !w.blocks_post()), "the generator's tightened laws pass a flat face: {:?}", cached.warnings);
    assert!(posting::verification_gate(&tight).is_empty(), "an exact flat face is no gouge: {:?}", posting::verification_gate(&tight));
    sink(&tight, 0.006);
    let gate = posting::verification_gate(&tight);
    assert!(gouge_line(&gate).is_some(), "the tightened verdict convicts 0.006 mm: {gate:?}");
    assert!(gate.iter().all(|l| !l.contains("too coarsely")), "no refusal while a tolerance is left: {gate:?}");
}

#[test]
fn a_model_too_coarse_to_verify_is_refused_by_name_with_no_gouge_verdict() {
    let world = faced_brick(Some(TOO_COARSE));
    assert!(model_fidelity::verdict_tol(&world).is_err());
    let refusal = "OP: model 'brick' was imported too coarsely to verify; re-import with a larger STEP facet budget (surfaces are approximated to within 0.030 mm)";
    // The exactly finished flat face: refused by name, not called a gouge.
    let report = posting::verification_report(&world);
    assert_eq!(report.problems, vec![refusal.to_string()], "{:?}", report.problems);
    let said = report.not_judged.expect("the sim says it did not judge");
    assert!(said.contains("model 'brick' was imported too coarsely to verify"), "{said}");
    // The generator's laws read untightened there: no gouge on a flat face.
    let e = model::find_by_name(&world, "OP").expect("op");
    let cached = model::toolpath_of(&world, &e).expect("cached");
    assert!(cached.warnings.iter().all(|w| !w.blocks_post()), "{:?}", cached.warnings);
    // A path that does cut deep: still the refusal, still no gouge line.
    sink(&world, 1.0);
    let gate = posting::verification_gate(&world);
    assert!(gate.contains(&refusal.to_string()), "{gate:?}");
    assert!(gouge_line(&gate).is_none(), "no gouge verdict is read off a mesh that cannot carry one: {gate:?}");
    // And nothing posts.
    let out = std::env::temp_dir().join(format!("camcraft-864-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let Err(err) = posted else { panic!("the post went out past the refusal") };
    assert!(err.contains("imported too coarsely to verify"), "{err}");
}

#[test]
fn a_kind_whose_laws_do_not_read_the_excess_is_refused_on_any_reduced_model() {
    // A 2D chamfer's verdict is its generator's own construction law, not
    // yet excess-aware: on a reduced-fidelity model it is refused even
    // where a tolerance would be left; at full fidelity it is untouched.
    let op = |coarse: Option<f64>| {
        let world = new_world();
        let mut h = History::default();
        h.execute(&world, ImportModel::from_mesh("brick", brick()).with_fidelity(coarse));
        let chamfer = camcraft_app::operation::retype_params(&face(), camcraft_app::operation::OpType::Chamfer2D.ordinal());
        let created = camcraft_app::schema::Operation {
            tool: MILL.into(),
            src: SrcRef::Entity("brick".into()),
            params: chamfer,
            ..model::operation_of(&faced_brick(None), &model::find_by_name(&faced_brick(None), "OP").expect("op")).expect("an op to copy")
        };
        model_fidelity::op_problem(&world, "CH", &created)
    };
    assert_eq!(op(None), None);
    let why = op(Some(TIGHT)).expect("refused");
    assert!(why.starts_with("CH: model 'brick' was imported at reduced fidelity (surfaces are approximated to within 0.012 mm)"), "{why}");
    assert!(why.ends_with("re-import with a larger STEP facet budget"), "{why}");
}

#[test]
fn a_coarse_fixture_is_judged_by_a_grown_box_and_never_acquitted_by_its_mesh() {
    use camcraft_app::fixture;
    let steel = |coarse: Option<f64>| {
        let world = new_world();
        let mut h = History::default();
        h.execute(&world, ImportModel::from_mesh("vise", brick()).with_fidelity(coarse));
        let e = model::find_by_name(&world, "vise").expect("landed");
        e.entity_view(&world).set(camcraft_app::schema::FixtureDef::default());
        (fixture::boxes_for(&world, None), fixture::meshes_for(&world, None).len())
    };
    // Full fidelity: the mesh's own box, and the mesh may acquit.
    let (boxes, meshes) = steel(None);
    assert_eq!((boxes[0].min, boxes[0].max), (Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0)));
    assert_eq!(meshes, 1);
    // Reduced fidelity: the box grows by the error on every side (a
    // curved face can stand that far outside its facets) and the mesh is
    // not offered to the law that only ever acquits.
    let (boxes, meshes) = steel(Some(0.5));
    assert_eq!((boxes[0].min, boxes[0].max), (Point3::new(-20.5, -10.5, -0.5), Point3::new(20.5, 10.5, 10.5)));
    assert_eq!(meshes, 0);
}

// ---- MCP ----------------------------------------------------------------

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn ok(&mut self, name: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        let reply = self.server.handle(&msg).expect("a request gets a response");
        let result = &reply["result"];
        assert!(result["isError"] != json!(true), "{name} refused: {result}");
        result["structuredContent"].clone()
    }
}

#[test]
fn mcp_reports_a_reduced_fidelity_import_and_says_nothing_of_a_normal_one() {
    let path = step_path().to_string_lossy().into_owned();
    let mut c = Client { server: Server::new(), next: 1 };
    let full = c.ok("import_model", json!({ "path": path, "name": "full" }));
    assert!(full.get("reduced_fidelity").is_none(), "{full}");
    let coarse = step::with_facet_budget(2_000, || c.ok("import_model", json!({ "path": path, "name": "coarse" })));
    let facts = &coarse["reduced_fidelity"];
    let mm = facts["approximation_mm"].as_f64().expect("the achieved error, in mm");
    assert!(mm > step::CHORD_CAP, "{coarse}");
    assert_eq!(facts["promised_mm"], json!(step::CHORD_CAP));
    let note = facts["note"].as_str().expect("a sentence");
    assert!(note.starts_with("Reduced fidelity: surfaces are approximated to within "), "{note}");
    assert!(!note.contains("chord") && !note.contains("sagitta"), "plain words: {note}");

    // The model read tool carries the same facts, on that model only.
    let job = c.ok("get_job", json!({}));
    let models = job["models"].as_array().expect("models");
    let named = |n: &str| models.iter().find(|m| m["name"] == json!(n)).expect("listed");
    assert!(named("full").get("reduced_fidelity").is_none());
    assert_eq!(named("coarse")["reduced_fidelity"]["approximation_mm"], json!(mm));
}
