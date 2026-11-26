//! #2996: a model kept at a chord error past `DEV_TOL`, up to the hard
//! limit `verify::WIDE_CHORD_LIMIT` (0.02 mm), is judged at its own chord
//! error: an exact finish posts, the program header and the model note
//! say what the checks allowed, and a real gouge still blocks the post.
//! Past the limit, the operation is refused by name (#864's law).
//!
//! Gate map (#2996): covers app::fidelity_note, app::model_fidelity
//! (verdict_tol, op_problem, summary, facts), app::posting_program
//! (document_program_id), app::posting (post_setup, verification_gate).
//!
//! Provenance: our own pins over our own laws; no vendor material.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::DEV_TOL;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{fidelity_note, model, model_fidelity, posting, verify};
use flecs_ecs::prelude::World;

const MILL: &str = "EM-1000-4FL";
/// Just under the limit, and just over it.
const UNDER: f64 = 0.019;
const OVER: f64 = 0.021;
/// The limit itself: what #2994's importer meshes an edge-gap file at.
const WIDE: f64 = 0.02;

/// A brick imported at `coarse_mm`, its own stock, and a Face op that
/// finishes its flat top exactly (at zero stock to leave).
fn faced_brick(coarse_mm: Option<f64>) -> World {
    let world = new_world();
    let mut h = History::default();
    let brick = cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0));
    h.execute(&world, ImportModel::from_mesh("brick", brick).with_fidelity(coarse_mm).with_file_tolerance(true));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "brick".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("brick".into()), face_op()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

/// A Face op that finishes the stock top exactly.
fn face_op() -> OpParams {
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

fn post(world: &World, tag: &str) -> Result<String, String> {
    let out = std::env::temp_dir().join(format!("camcraft-2996-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_setup(world, &out);
    let _ = std::fs::remove_file(&out);
    posted.map(|p| p.program)
}

#[test]
fn a_model_just_under_the_limit_posts_an_exact_finish_with_a_header_note() {
    let world = faced_brick(Some(UNDER));
    // The program verdict gives back only the promised chord.
    let tol = model_fidelity::verdict_tol(&world).expect("a tolerance is left under the limit");
    assert!((tol - (DEV_TOL - verify::PROMISED_CHORD)).abs() < 1e-12, "{tol}");
    let gate = posting::verification_gate(&world);
    assert!(gate.is_empty(), "an exact finish on a widened model posts: {gate:?}");
    let program = post(&world, "under").expect("the post goes out");
    let line = program
        .lines()
        .find(|l| l.to_ascii_lowercase().contains("reduced accuracy"))
        .unwrap_or_else(|| panic!("the header names the reduced accuracy:\n{program}"));
    assert!(line.starts_with('(') && line.ends_with(')'), "valid Fanuc header comment: {line}");
    assert!((fidelity_note::surface_bound(UNDER) - 0.024).abs() < 1e-12);
    let low = line.to_ascii_lowercase();
    assert!(low.contains("brick") && low.contains("0.024") && low.contains("0.010"), "{line}");
    let first_move = program.lines().position(|l| l.contains("G1") || l.contains("G0")).unwrap_or(usize::MAX);
    let at = program.lines().position(|l| l == line).unwrap_or(usize::MAX);
    assert!(at < first_move, "the note is in the header, before any motion:\n{program}");
    // The model page and MCP say the same fact in plain words.
    let said = model_fidelity::summary_with_cause(&world, UNDER, true);
    assert!(said.ends_with("checks on this model allow 0.024 mm, not 0.010 mm"), "{said}");
    let note = model_fidelity::facts_with_cause(&world, UNDER, true)["note"].as_str().expect("note").to_string();
    assert!(!note.contains("stricter") && note.contains("header"), "{note}");
}

#[test]
fn a_model_just_over_the_limit_is_refused_by_name() {
    let world = faced_brick(Some(OVER));
    assert!(model_fidelity::verdict_tol(&world).is_err());
    let refusal = "OP: model 'brick' was imported too coarsely to verify; re-import with a larger STEP facet budget, or export the file again with a tighter tolerance (surfaces are approximated to within 0.021 mm)";
    let gate = posting::verification_gate(&world);
    assert!(gate.contains(&refusal.to_string()), "{gate:?}");
    let err = post(&world, "over").expect_err("nothing posts past the limit");
    assert!(err.contains("imported too coarsely to verify"), "{err}");
    assert!(fidelity_note::program_notes(&world).is_empty(), "no widened note past the limit");
}

#[test]
fn a_real_gouge_deeper_than_the_verdict_tolerance_still_blocks_a_widened_model() {
    // 0.02 mm under the face: deeper than the model's own chord error.
    let world = faced_brick(Some(UNDER));
    sink(&world, 0.02);
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|l| l.starts_with("gouge:")), "the gouge is convicted: {gate:?}");
    assert!(gate.iter().all(|l| !l.contains("too coarsely")), "judged, not refused: {gate:?}");
    let err = post(&world, "gouge").expect_err("a gouge blocks the post");
    assert!(err.contains("gouge"), "{err}");
}

#[test]
fn the_edge_gap_step_file_opened_at_reduced_accuracy_posts_a_face_with_the_note() {
    // #2976's fixture: edges 0.018 mm off their surfaces, meshed at the
    // 0.02 mm limit (PR #2994). Before #2996 no program could post on it.
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/edge_gap/edge_gap_018_stated_010.step");
    let cmd = ImportModel::new(&path).expect("imports at reduced accuracy");
    assert!(cmd.coarse_mm.is_some_and(|c| (c - WIDE).abs() < 1e-12), "{:?}", cmd.coarse_mm);
    assert!(cmd.file_tolerance, "the file tolerance, not the budget, set this chord");
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, cmd.renamed("gap"));
    h.execute(&world, SetStock::new(&world, StockShape::FromModel { source: "gap".into() }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("gap".into()), face_op()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    let program = post(&world, "edge-gap").unwrap_or_else(|e| panic!("the post goes out: {e}"));
    let low = program.to_ascii_lowercase();
    assert!(low.contains("reduced accuracy") && low.contains("gap") && low.contains("0.025"), "{program}");
}

#[test]
fn a_full_fidelity_model_and_a_mild_one_say_nothing_new() {
    let full = faced_brick(None);
    assert_eq!(model_fidelity::verdict_tol(&full), Ok(DEV_TOL));
    assert!(fidelity_note::program_notes(&full).is_empty());
    let program = post(&full, "full").expect("posts");
    assert!(!program.to_ascii_lowercase().contains("reduced accuracy"), "{program}");
    // A chord up to DEV_TOL keeps #864's law: the whole excess is taken.
    let mild = faced_brick(Some(0.009));
    let tol = model_fidelity::verdict_tol(&mild).expect("tol");
    assert!((tol - (DEV_TOL - (0.009 - verify::PROMISED_CHORD))).abs() < 1e-12, "{tol}");
    assert!(fidelity_note::program_notes(&mild).is_empty());
    assert_eq!(fidelity_note::clause(&mild, 0.009, true), None);
}

#[test]
fn changing_only_the_import_cause_invalidates_the_cached_verdict() {
    let world = faced_brick(Some(UNDER));
    let e = model::find_by_name(&world, "OP").expect("op");
    assert_eq!(camcraft_app::operation::status_of(&world, &e), camcraft_app::operation::OpStatus::Ok);
    let model = model::find_by_name(&world, "brick").expect("model");
    model_fidelity::set(&world, &model, Some(UNDER));
    assert_eq!(camcraft_app::operation::status_of(&world, &e), camcraft_app::operation::OpStatus::Stale);
    model_fidelity::set_with_cause(&world, &model, Some(UNDER), true);
    assert_eq!(camcraft_app::operation::status_of(&world, &e), camcraft_app::operation::OpStatus::Ok);
    let stock = model::find_by_name(&world, "Stock").expect("stock");
    model_fidelity::set(&world, &stock, Some(UNDER));
    assert_eq!(camcraft_app::operation::status_of(&world, &e), camcraft_app::operation::OpStatus::Stale);
}

#[test]
fn fractional_surface_bounds_are_rounded_up_in_each_document_unit() {
    let world = new_world();
    let exact = fidelity_note::surface_bound(0.0192);
    assert!((exact - 0.0242).abs() < 1e-12, "the numeric bound keeps its precision");
    let mm = fidelity_note::clause(&world, 0.0192, true).expect("widened");
    assert!(mm.contains("allow 0.025 mm"), "the warning never rounds down: {mm}");
    let mut h = History::default();
    h.execute(&world, camcraft_app::commands::SetUnits::new(&world, camcraft_app::units::Unit::Inches).expect("inch units"));
    let inches = fidelity_note::clause(&world, 0.0192, true).expect("widened");
    assert!(inches.contains("allow 0.00096 in"), "the inch warning never rounds down: {inches}");
}
