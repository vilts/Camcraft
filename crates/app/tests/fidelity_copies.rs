//! #1056: the reduced-fidelity flag (#864) is a property of FACETS, so a
//! stock copied from a coarse model carries it itself - deleting the
//! model, cloning the setup, undoing, saving and loading never make the
//! laws stop tightening on the same facets. When the gate refuses an
//! operation for its model's fidelity, the program sim and the eye's
//! colours read no verdict either (the refusal is what is said), and
//! while a tolerance is left the colours are gated at the tightened
//! verdict tolerance, not the plain 0.01 mm.
//!
//! Gate map (#1056): covers app::model_fidelity (chord_behind,
//! adopt_copies, program_tol, op_problem), app::commands (set_stock,
//! update_model, delete_entity, setup, setup_delete), app::project (the
//! stock's `coarse_mm`), app::posting (verification_report),
//! app::stock_sim (simulate_document), app::stock_view (field_view),
//! app::machined_classes.
//! Lane shape (M4.4): every test here runs in seconds.
//!
//! Provenance: our own pins over our own laws; no vendor material.

use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, DeleteEntity, DeleteSetup, GenerateOperation, History, ImportModel, SetStock, UpdateModel,
};
use camcraft_app::geometry::{cube, Mesh, Point3};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::DEV_TOL;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_fidelity, posting, project, stock_view};
use flecs_ecs::prelude::World;

const MILL: &str = "EM-1000-4FL";
/// Excess 0.007 of 0.01 (a tolerance left), and past every 0.01 law.
const TIGHT: f64 = 0.012;
const TOO_COARSE: f64 = 0.03;

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

/// A full-fidelity part "bar", a second model "blank" imported at
/// `blank_mm`, the stock copied from "blank", and a Face op on "bar"
/// that finishes its flat top exactly.
fn copied(blank_mm: Option<f64>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("bar", brick()));
    h.execute(&world, ImportModel::from_mesh("blank", brick()).with_fidelity(blank_mm));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "blank".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("bar".into()), face()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    (world, h)
}

fn stock_chord(world: &World, name: &str) -> Option<f64> {
    model::find_by_name(world, name).and_then(|e| model_fidelity::chord_of(world, &e))
}

fn op_problem(world: &World) -> Option<String> {
    let op = model::operation_of(world, &model::find_by_name(world, "OP").expect("op")).expect("operation");
    model_fidelity::op_problem(world, "OP", &op)
}

const REFUSAL: &str = "OP: model 'blank' was imported too coarsely to verify; re-import with a larger STEP facet budget (surfaces are approximated to within 0.030 mm)";

#[test]
fn a_stock_copy_keeps_its_flag_when_its_source_is_deleted() {
    let (world, mut h) = copied(Some(TIGHT));
    assert_eq!(stock_chord(&world, "Stock"), Some(TIGHT), "the copy carries the chord error itself");
    h.execute(&world, DeleteEntity::new(&world, "blank").expect("delete"));
    assert!(model::find_by_name(&world, "blank").is_none());
    assert_eq!(stock_chord(&world, "Stock"), Some(TIGHT), "deleting the source leaves the copy's facets as coarse as they were");
    assert!((model_fidelity::excess_of(&world, "Stock") - (TIGHT - 0.005)).abs() < 1e-12, "the laws still tighten");
    h.undo(&world);
    assert_eq!(stock_chord(&world, "Stock"), Some(TIGHT));
    // Undoing the stock itself takes the flag with it; redo brings it back.
    // (generate, create, add tool, then the stock)
    for _ in 0..4 {
        h.undo(&world);
    }
    assert!(model::find_by_name(&world, "Stock").is_none());
    for _ in 0..4 {
        h.redo(&world);
    }
    assert_eq!(stock_chord(&world, "Stock"), Some(TIGHT));

    // Past the bound: the operation is refused before and after the
    // source is deleted, by the source's name.
    let (world, mut h) = copied(Some(TOO_COARSE));
    assert_eq!(op_problem(&world).as_deref(), Some(REFUSAL));
    h.execute(&world, DeleteEntity::new(&world, "blank").expect("delete"));
    assert_eq!(op_problem(&world).as_deref(), Some(REFUSAL), "still refused once the source is gone");
    assert!(posting::verification_gate(&world).contains(&REFUSAL.to_string()));

    // A box stock carries nothing; a copy of a fine model carries nothing.
    let (world, mut h) = copied(None);
    assert_eq!(stock_chord(&world, "Stock"), None);
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 30.0, dz: 12.0 }, Default::default()).expect("box"));
    assert_eq!(stock_chord(&world, "Stock"), None);
}

#[test]
fn the_copy_follows_its_source_through_updates_and_setup_clones() {
    let (world, mut h) = copied(Some(TIGHT));
    // A re-import of the source at full fidelity re-derives the copy and
    // its flag in the same undo step.
    h.execute(&world, UpdateModel::new(&world, "blank", brick()).expect("update"));
    assert_eq!(stock_chord(&world, "Stock"), None);
    h.undo(&world);
    assert_eq!(stock_chord(&world, "Stock"), Some(TIGHT));
    h.execute(&world, UpdateModel::new(&world, "blank", brick()).expect("update").with_fidelity(Some(TOO_COARSE)));
    assert_eq!(stock_chord(&world, "Stock"), Some(TOO_COARSE));
    h.undo(&world);

    // A datum transfer clones the stock with its flag; deleting that
    // setup and undoing restores it.
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize"));
    let first = camcraft_app::setup::active(&world).expect("a setup");
    h.execute(&world, CreateSetup::new(&world, Some("S2"), Some(&first)).expect("clone"));
    assert_eq!(stock_chord(&world, "S2/Stock"), Some(TIGHT), "the clone is as coarse as the copy");
    h.execute(&world, ActivateSetup::new(&world, Some(&first)).expect("activate"));
    h.execute(&world, DeleteEntity::new(&world, "OP").expect("an op machining nowhere else"));
    h.execute(&world, DeleteSetup::new(&world, "S2").expect("delete setup"));
    assert!(model::find_by_name(&world, "S2/Stock").is_none());
    h.undo(&world);
    assert_eq!(stock_chord(&world, "S2/Stock"), Some(TIGHT));
}

#[test]
fn the_copys_flag_rides_the_project_file_and_an_older_copy_adopts_its_source() {
    let dir = std::env::temp_dir();
    let file = |tag: &str| dir.join(format!("camcraft-1056-{tag}-{}.camproj", std::process::id()));
    let (world, _h) = copied(Some(TIGHT));
    let (a, b, c) = (file("a"), file("b"), file("c"));
    project::save(&world, &a).expect("save");
    let text = std::fs::read_to_string(&a).expect("read");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    let entities = doc["entities"].as_array().expect("entities");
    let stock = entities.iter().find(|e| e["name"] == "Stock").expect("stock row");
    assert_eq!(stock["coarse_mm"], serde_json::json!(TIGHT), "the stock row carries the flag");
    // Loaded and saved again: byte for byte.
    let loaded = new_world();
    project::load(&loaded, &a).expect("load");
    project::save(&loaded, &b).expect("save again");
    assert_eq!(text, std::fs::read_to_string(&b).expect("read"));
    let mut h = History::default();
    h.execute(&loaded, DeleteEntity::new(&loaded, "blank").expect("delete"));
    assert_eq!(stock_chord(&loaded, "Stock"), Some(TIGHT), "a loaded copy outlives its source too");

    // A file from before the copy carried its own flag (#911's): the
    // stock row has no key; it adopts its source's on load, so a later
    // delete of the source still cannot clear it.
    let mut old = doc.clone();
    for e in old["entities"].as_array_mut().expect("entities") {
        if e["name"] == "Stock"
            && let Some(o) = e.as_object_mut()
        {
            o.remove("coarse_mm");
        }
    }
    std::fs::write(&c, serde_json::to_string_pretty(&old).expect("json")).expect("write");
    let older = new_world();
    project::load(&older, &c).expect("an older file loads");
    assert_eq!(stock_chord(&older, "Stock"), Some(TIGHT), "adopted from its source");
    let mut h = History::default();
    h.execute(&older, DeleteEntity::new(&older, "blank").expect("delete"));
    assert_eq!(stock_chord(&older, "Stock"), Some(TIGHT));
    for p in [a, b, c] {
        let _ = std::fs::remove_file(p);
    }
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

#[test]
fn the_named_refusal_wins_over_a_gouge_line_the_sim_would_read() {
    // The parts are fine (the program sim alone would judge at 0.01 mm),
    // but the op is judged against a stock copied from a model too
    // coarse for its laws: the gate refuses it by name. The sim must not
    // add a gouge line read off the same material beside that refusal.
    let (world, mut h) = copied(Some(TOO_COARSE));
    h.execute(&world, DeleteEntity::new(&world, "blank").expect("delete"));
    assert_eq!(model_fidelity::verdict_tol(&world), Ok(DEV_TOL), "no part is coarse");
    sink(&world, 0.05);
    let report = posting::verification_report(&world);
    assert_eq!(report.problems, vec![REFUSAL.to_string()], "{:?}", report.problems);
    let said = report.not_judged.expect("the sim says it did not judge");
    assert!(said.contains(REFUSAL), "{said}");
    assert_eq!(model_fidelity::program_tol(&world), Err(REFUSAL.to_string()));
    // The eye paints no verdict either, and says why.
    let fv = stock_view::field_view_with(&world, 2_000.0).expect("the view still draws material");
    assert!(fv.target.is_none());
    assert_eq!(fv.target_note.as_deref(), Some(REFUSAL));
}

/// A part "bar" imported at `coarse_mm` in a box stock, faced.
fn faced(coarse_mm: Option<f64>) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("bar", brick()).with_fidelity(coarse_mm));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 20.0, dz: 12.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("bar".into()), face()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

#[test]
fn the_eyes_colours_are_gated_at_the_tightened_verdict_tolerance() {
    let world = faced(Some(TIGHT));
    let want = DEV_TOL - (TIGHT - 0.005);
    assert_eq!(model_fidelity::program_tol(&world), model_fidelity::verdict_tol(&world));
    let fv = stock_view::field_view_with(&world, 2_000.0).expect("view");
    assert!((fv.tol - want).abs() < 1e-12, "the colours read {} not {want}", fv.tol);
    let play = camcraft_app::stock_play::FieldPlay::with_budget(&world, true, 2_000.0).expect("playback");
    assert!((play.view().tol - want).abs() < 1e-12, "playback's colours too");
    // At full fidelity: the plain tolerance, as before.
    let fv = stock_view::field_view_with(&faced(None), 2_000.0).expect("view");
    assert_eq!(fv.tol, DEV_TOL);
}

/// What the gate says of the Face op once "bar" is of unknown fidelity.
const UNKNOWN_REFUSAL: &str = "OP: model 'bar' was saved before CamCraft recorded mesh fidelity (project v58 or older), so a gouge check against it cannot be trusted; re-import the model to verify its operations";

#[test]
fn a_project_saved_before_fidelity_was_recorded_loads_unknown_and_refuses_until_reimported() {
    // #1056 point 2: a v58 file cannot say whether its meshes met the
    // importer's promise (pre-#911 saves recorded nothing), and the chord
    // cannot be re-measured without the B-rep. Every mesh without a
    // recorded chord loads "fidelity unknown"; a recorded one is kept.
    let dir = std::env::temp_dir();
    let file = |tag: &str| dir.join(format!("camcraft-1056-unknown-{tag}-{}.camproj", std::process::id()));
    let (world, _h) = copied(Some(TIGHT));
    let (a, b, c) = (file("a"), file("b"), file("c"));
    project::save(&world, &a).expect("save");
    let mut doc: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&a).expect("read")).expect("json");
    assert_eq!(doc["version"], serde_json::json!(project::PROJECT_VERSION));
    assert!(!std::fs::read_to_string(&a).expect("read").contains("fidelity_unknown"), "a known document writes no mark");
    doc["version"] = serde_json::json!(58);
    std::fs::write(&b, serde_json::to_string_pretty(&doc).expect("json")).expect("write");

    let old = new_world();
    let notes = project::load_noted(&old, &b).expect("a v58 file loads");
    // "bar" (no record) is unknown; "blank" and its stock copy keep the
    // chord the #911-era file recorded.
    let bar = model::find_by_name(&old, "bar").expect("bar");
    let chord = model_fidelity::chord_of(&old, &bar).expect("bar is flagged");
    assert!(model_fidelity::is_unknown(chord), "never read as full fidelity");
    assert_eq!(stock_chord(&old, "blank"), Some(TIGHT));
    assert_eq!(stock_chord(&old, "Stock"), Some(TIGHT));
    assert!(notes.iter().any(|n| n.starts_with("'bar': saved before CamCraft recorded mesh fidelity")), "{notes:?}");
    assert!(!notes.iter().any(|n| n.starts_with("'blank'") || n.starts_with("'Stock'")), "{notes:?}");
    // Refused by name at the post, and the sim and the eye judge nothing.
    assert_eq!(op_problem(&old).as_deref(), Some(UNKNOWN_REFUSAL));
    let report = posting::verification_report(&old);
    assert_eq!(report.problems, vec![UNKNOWN_REFUSAL.to_string()], "{:?}", report.problems);
    assert_eq!(model_fidelity::program_tol(&old), Err(UNKNOWN_REFUSAL.to_string()));
    assert!(model_fidelity::verdict_tol(&old).is_err_and(|e| e.starts_with("model 'bar' was saved before")));
    assert!(model_fidelity::summary(&old, chord).starts_with("Fidelity unknown: "));
    assert_eq!(model_fidelity::facts(&old, chord)["fidelity_unknown"], serde_json::json!(true));

    // A re-save keeps the mark (never turns into "fine"), at v59.
    project::save(&old, &c).expect("re-save");
    let text = std::fs::read_to_string(&c).expect("read");
    let resaved: serde_json::Value = serde_json::from_str(&text).expect("json");
    let row = resaved["entities"].as_array().expect("entities").iter().find(|e| e["name"] == "bar").expect("bar row");
    assert_eq!(row["fidelity_unknown"], serde_json::json!(true));
    assert!(row.get("coarse_mm").is_none(), "the sentinel is never written as a chord");
    let again = new_world();
    assert_eq!(project::load_noted(&again, &c).expect("load").len(), 0, "a v59 file is not migrated again");
    assert_eq!(op_problem(&again).as_deref(), Some(UNKNOWN_REFUSAL));

    // Only a fresh import clears it; its undo puts the mark back.
    let mut h = History::default();
    h.execute(&again, UpdateModel::new(&again, "bar", brick()).expect("re-import"));
    assert_eq!(model::find_by_name(&again, "bar").and_then(|e| model_fidelity::chord_of(&again, &e)), None);
    assert_eq!(op_problem(&again), None);
    h.undo(&again);
    assert_eq!(op_problem(&again).as_deref(), Some(UNKNOWN_REFUSAL));

    // Both marks at once is a corrupt file, refused before any write.
    let mut both = resaved.clone();
    for e in both["entities"].as_array_mut().expect("entities") {
        if e["name"] == "bar" {
            e["coarse_mm"] = serde_json::json!(TIGHT);
        }
    }
    std::fs::write(&b, serde_json::to_string_pretty(&both).expect("json")).expect("write");
    assert!(project::load(&new_world(), &b).is_err_and(|e| e.to_string().contains("unknown fidelity at once")));
    for p in [a, b, c] {
        let _ = std::fs::remove_file(p);
    }
}
