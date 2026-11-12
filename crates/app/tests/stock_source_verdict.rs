//! #1094: a casting copied into a "from model" stock and left in the tree
//! is judged as the part unless the user ticks "The source model is raw
//! stock only" (`SetStockSourceOnly`, default off). These tests pin the
//! explicit rule: off (every older document) judges every model exactly
//! as before - the review's two reproductions of the dropped inference
//! (a stock made from the part with an unrelated second model, live or
//! suppressed) must refuse; on, the casting leaves the verdict and a legal
//! program over a separate part posts; an op that names the marked model
//! (source or boundary pick) puts it back; a mark that leaves nothing to
//! judge refuses by name; a gouge on the stock's source says how to mark
//! it; undo/redo, a new stock source, the project file and an older file.
//!
//! Provenance: our own fixtures (`ladder_entry_height.rs`'s blind hole
//! under a casting), specced from the Prime Directive; no vendor material.
//!
//! Gate map (#1094): covers app::stock_source, app::commands
//! (stock_source_only), app::stock_sim (the part set), app::setup_align
//! (parts_facets_in), app::posting_sim (the clause and hint),
//! app::project (raw_stock_only).
//! Lane shape: all tests here run in seconds.

use flecs_ecs::prelude::*;

use camcraft_app::commands::{
    AddTool, CreateOperation, DeleteEntity, DeleteFixture, EditOperationParams, GenerateOperation, History, ImportModel,
    MoveModel, SetStock, SetStockSourceOnly, TagFixture, UpdateModel,
};
use camcraft_app::schema::StockSourceOnly;
use camcraft_app::stock_source::raw_stock_only_of;
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::{BoundaryMode, BoundaryShape, ExtraBoundary, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_source::{RAW_STOCK_ONLY, stock_only_model};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

/// The part's top and the hole's mouth.
const MOUTH: f64 = 20.0;
/// The modelled hole's floor.
const FLOOR: f64 = 10.0;
const TOOL: &str = "EM-0800-4FL";

fn quad(a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>) {
    f.push(Triangle::new(a, b, c));
    f.push(Triangle::new(a, c, d));
}

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 60 x 60 casting, Z -10..20 - the stock's source.
fn casting() -> Mesh {
    cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, MOUTH))
}

/// A 40 x 40 x [0, 20] block with a 20 x 20 blind hole from its top to Z 10.
fn part() -> Mesh {
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, MOUTH));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-10.0, -10.0), (10.0, -10.0), (10.0, 10.0), (-10.0, 10.0)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, MOUTH);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-10.0, -10.0, FLOOR), Point3::new(10.0, 10.0, MOUTH));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// A milled bore of a 16 mm hole at the origin, from the mouth to `floor`.
fn bore(floor: f64) -> OpParams {
    OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(MOUTH),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 5.0,
        feed: 500.0,
        plunge_feed: 150.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![Hole { xy: Point2::new(0.0, 0.0), radius: 8.0, entry_z: MOUTH, floor_z: floor, through: false, stepped: false }],
    }
}

fn add_tool(world: &World, h: &mut History) {
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(world, AddTool::new(world, TOOL, tool).expect("tool"));
}

fn op(world: &World, h: &mut History, name: &str, src: SrcRef, floor: f64) {
    h.execute(world, CreateOperation::new(world, name, TOOL, src, bore(floor)).expect("create op"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate op"));
}

fn mark(world: &World, h: &mut History, on: bool) {
    h.execute(world, SetStockSourceOnly::new(world, on).expect("mark"));
}

/// The issue's job: the casting left in the tree as the stock's source,
/// a separate part, and a Bore (to `floor`) on the part.
fn casting_job(floor: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("casting", casting()));
    h.execute(&world, ImportModel::from_mesh("part", part()));
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    add_tool(&world, &mut h);
    op(&world, &mut h, "Op1", SrcRef::Entity("part".into()), floor);
    (world, h)
}

/// The review's reproduction: the stock IS the part, a block "other"
/// well beside it, Op1 on the stock bores 5 mm under the part's floor.
fn part_stock_job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part()));
    let other = cube(Point3::new(80.0, -10.0, 0.0), Point3::new(100.0, 10.0, 20.0));
    h.execute(&world, ImportModel::from_mesh("other", other));
    let shape = StockShape::FromModel { source: "part".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    add_tool(&world, &mut h);
    op(&world, &mut h, "Op1", SrcRef::Stock, FLOOR - 5.0);
    op(&world, &mut h, "Op2", SrcRef::Entity("other".into()), FLOOR);
    (world, h)
}

fn post(world: &World, tag: &str) -> Result<(), String> {
    let out = std::env::temp_dir().join(format!("camcraft-1094-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(world, &out).map(|_| ());
    let _ = std::fs::remove_file(&out);
    posted
}

fn gouge_line(world: &World) -> Option<String> {
    posting::verification_gate(world).into_iter().find(|l| l.starts_with("gouge"))
}

fn entity(world: &World, name: &str) -> flecs_ecs::prelude::Entity {
    model::find_by_name(world, name).expect("entity")
}

#[test]
fn unmarked_the_casting_is_judged_and_the_refusal_says_how_to_mark_it() {
    let (world, _) = casting_job(FLOOR);
    assert_eq!(stock_only_model(&world), None, "off is the default");
    let line = gouge_line(&world).expect("the cut into the judged casting is a gouge, as before");
    assert!(line.contains("'casting'") && line.contains(RAW_STOCK_ONLY), "no hint: {line}");
    assert!(post(&world, "unmarked").is_err());
}

#[test]
fn marked_raw_stock_only_the_casting_leaves_the_verdict_and_the_program_posts() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    assert_eq!(stock_only_model(&world), Some(entity(&world, "casting")));
    assert_eq!(gouge_line(&world), None);
    let posted = post(&world, "marked");
    assert!(posted.is_ok(), "a legal program over a separate part was refused: {posted:?}");
}

#[test]
fn marked_the_separate_part_stays_judged_a_cut_below_its_floor_blocks() {
    let (world, mut h) = casting_job(FLOOR - 5.0);
    mark(&world, &mut h, true);
    let why = post(&world, "deep").expect_err("a bore 5 mm into the part's floor posted");
    assert!(why.contains("gouge"), "refused, but not as a gouge: {why}");
}

#[test]
fn a_stock_made_from_the_part_with_an_unrelated_model_beside_it_refuses_the_gouge() {
    let (world, _) = part_stock_job();
    assert_eq!(stock_only_model(&world), None);
    assert!(gouge_line(&world).is_some(), "the 5 mm gouge into the part went unjudged");
    assert!(post(&world, "repro1").is_err(), "the review's reproduction posted");
}

#[test]
fn the_same_with_the_unrelated_op_suppressed_still_refuses() {
    let (world, mut h) = part_stock_job();
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Op2", true).expect("suppress"));
    assert_eq!(stock_only_model(&world), None);
    assert!(gouge_line(&world).is_some(), "the 5 mm gouge into the part went unjudged");
    assert!(post(&world, "repro2").is_err(), "the review's suppressed reproduction posted");
}

#[test]
fn marked_an_op_naming_the_model_puts_it_back_in_the_verdict() {
    let (world, mut h) = part_stock_job();
    mark(&world, &mut h, true);
    assert_eq!(stock_only_model(&world), Some(entity(&world, "part")), "the user's word, no op names it");
    // Any op naming the part as its source - suppressed too - makes it the part again.
    let op3 = CreateOperation::new(&world, "Op3", TOOL, SrcRef::Entity("part".into()), bore(FLOOR)).expect("op3");
    h.execute(&world, op3);
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Op3", true).expect("suppress"));
    assert_eq!(stock_only_model(&world), None);
    let line = gouge_line(&world).expect("the part is judged again");
    assert!(line.contains("an operation machines it"), "the hint says why the mark did not hold: {line}");
    assert!(post(&world, "named").is_err());
}

#[test]
fn marked_a_boundary_pick_on_the_model_puts_it_back_in_the_verdict() {
    let (world, mut h) = part_stock_job();
    mark(&world, &mut h, true);
    // Op1 machines the stock, but its boundary is picked on the part.
    let e = entity(&world, "Op1");
    let mut o = model::operation_of(&world, &e).expect("op");
    let pick = GeoSel { entity: "part".into(), feature: FeatureSel::Face(1) };
    o.area.extra.push(ExtraBoundary { mode: BoundaryMode::Include, shape: BoundaryShape::Picks(vec![pick]) });
    e.entity_view(&world).set(o);
    assert_eq!(stock_only_model(&world), None, "a boundary pick on the part names it");
}

#[test]
fn a_mark_that_leaves_nothing_to_judge_refuses_by_name() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("casting", casting()));
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    add_tool(&world, &mut h);
    op(&world, &mut h, "Op1", SrcRef::Stock, FLOOR);
    mark(&world, &mut h, true);
    let gate = posting::verification_gate(&world);
    assert!(
        gate.iter().any(|l| l.starts_with("nothing is judged as the part") && l.contains("'casting'")),
        "a program judged against nothing passed: {gate:?}"
    );
    assert!(post(&world, "nothing").is_err());
}

#[test]
fn the_mark_undoes_redoes_and_a_new_stock_source_drops_its_hold() {
    let (world, mut h) = casting_job(FLOOR);
    let casting_e = entity(&world, "casting");
    mark(&world, &mut h, true);
    assert_eq!(stock_only_model(&world), Some(casting_e));
    h.undo(&world);
    assert_eq!(stock_only_model(&world), None, "undo of the mark");
    h.redo(&world);
    assert_eq!(stock_only_model(&world), Some(casting_e), "redo of the mark");
    // A stock copied from another model: the mark was said of the casting.
    h.execute(&world, ImportModel::from_mesh("blank", casting()));
    let shape = StockShape::FromModel { source: "blank".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("restock"));
    assert_eq!(stock_only_model(&world), None, "the mark does not follow a new source");
    h.undo(&world);
    assert_eq!(stock_only_model(&world), Some(casting_e), "undo of SetStock restores the casting's mark");
    mark(&world, &mut h, false);
    assert_eq!(stock_only_model(&world), None, "unticked");
}

#[test]
fn the_mark_is_refused_on_a_stock_not_made_from_a_model() {
    let world = new_world();
    let mut h = History::default();
    let shape = StockShape::Box { dx: 60.0, dy: 60.0, dz: 30.0 };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("box"));
    let why = SetStockSourceOnly::new(&world, true).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(why.contains("not made from a model"), "{why}");
}

#[test]
fn the_mark_round_trips_the_project_file_and_an_older_file_judges_as_before() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    let dir = std::env::temp_dir().join(format!("camcraft-1094-proj-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("marked.camproj");
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("raw_stock_only"), "the key is written");

    let loaded = new_world();
    camcraft_app::project::load(&loaded, &path).expect("load");
    assert_eq!(stock_only_model(&loaded), Some(entity(&loaded, "casting")), "the mark survives the round trip");

    // The same file as an older build wrote it: no key - judged as before.
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    let rows = doc["entities"].as_array_mut().expect("entities");
    for row in rows.iter_mut() {
        if let Some(obj) = row.as_object_mut() {
            obj.remove("raw_stock_only");
        }
    }
    let old = dir.join("older.camproj");
    std::fs::write(&old, serde_json::to_string(&doc).expect("json")).expect("write");
    let older = new_world();
    camcraft_app::project::load(&older, &old).expect("load older");
    assert_eq!(stock_only_model(&older), None, "an older file judges the casting");
    assert!(gouge_line(&older).is_some(), "and refuses exactly as it did");

    // An unmarked document writes no key at all.
    mark(&world, &mut h, false);
    camcraft_app::project::save(&world, &path).expect("save unmarked");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(!text.contains("raw_stock_only"), "an unmarked document saves as before");
    let _ = std::fs::remove_dir_all(&dir);
}


/// The second review's reproduction: the marked casting deleted, the
/// finished part imported under its old name, an op on an unrelated
/// model and one on the stock boring 5 mm under the new model's floor.
fn delete_and_reimport_job() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("casting", casting()));
    let other = cube(Point3::new(80.0, -10.0, 0.0), Point3::new(100.0, 10.0, 20.0));
    h.execute(&world, ImportModel::from_mesh("other", other));
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    mark(&world, &mut h, true);
    h.execute(&world, DeleteEntity::new(&world, "casting").expect("delete"));
    h.execute(&world, ImportModel::from_mesh("casting", part()));
    add_tool(&world, &mut h);
    op(&world, &mut h, "Op2", SrcRef::Entity("other".into()), FLOOR);
    op(&world, &mut h, "Op1", SrcRef::Stock, FLOOR - 5.0);
    (world, h)
}

#[test]
fn a_model_imported_under_a_deleted_marked_sources_name_does_not_inherit_the_mark() {
    let (world, _) = delete_and_reimport_job();
    assert_eq!(raw_stock_only_of(&world, None), Some(false), "the mark died with the casting");
    assert_eq!(stock_only_model(&world), None);
    assert!(gouge_line(&world).is_some(), "the 5 mm gouge into the new model went unjudged");
    assert!(post(&world, "reimport").is_err(), "the review's delete + re-import reproduction posted");
}

#[test]
fn undo_of_deleting_the_marked_source_restores_the_model_and_the_mark() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    h.execute(&world, DeleteEntity::new(&world, "casting").expect("delete"));
    assert_eq!(raw_stock_only_of(&world, None), Some(false), "deleted with the model");
    h.undo(&world);
    assert_eq!(stock_only_model(&world), Some(entity(&world, "casting")), "undo: model and mark together");
    h.redo(&world);
    assert!(model::find_by_name(&world, "casting").is_none());
    assert_eq!(raw_stock_only_of(&world, None), Some(false), "redo drops it again");
}

#[test]
fn a_re_import_in_place_drops_the_mark_and_undo_restores_it() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    h.execute(&world, UpdateModel::new(&world, "casting", part()).expect("update"));
    assert_eq!(raw_stock_only_of(&world, None), Some(false), "new geometry, no mark");
    assert_eq!(stock_only_model(&world), None);
    h.undo(&world);
    assert_eq!(stock_only_model(&world), Some(entity(&world, "casting")), "undo restores geometry and mark");
}

#[test]
fn a_stock_changed_to_a_box_and_back_comes_back_unmarked() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    let shape = StockShape::Box { dx: 60.0, dy: 60.0, dz: 30.0 };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("box"));
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("back"));
    assert_eq!(raw_stock_only_of(&world, None), Some(false), "changing the stock dropped the mark");
    assert_eq!(stock_only_model(&world), None);
    h.undo(&world);
    h.undo(&world);
    assert_eq!(stock_only_model(&world), Some(entity(&world, "casting")), "only undo brings it back");
}

#[test]
fn an_empty_model_is_nothing_to_judge() {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("casting", casting()));
    h.execute(&world, ImportModel::from_mesh("empty", Mesh::from_facets(Vec::new())));
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    add_tool(&world, &mut h);
    op(&world, &mut h, "Op1", SrcRef::Stock, FLOOR - 5.0);
    mark(&world, &mut h, true);
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|l| l.starts_with("nothing is judged as the part")), "{gate:?}");
    assert!(post(&world, "empty").is_err());
}

#[test]
fn a_file_whose_mark_names_a_missing_source_loads_unmarked_with_a_note() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    let dir = std::env::temp_dir().join(format!("camcraft-1094-miss-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("marked.camproj");
    camcraft_app::project::save(&world, &path).expect("save");
    let mut doc: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("read")).expect("json");
    let rows = doc["entities"].as_array_mut().expect("entities");
    rows.retain(|r| r["name"] != serde_json::json!("casting"));
    std::fs::write(&path, serde_json::to_string(&doc).expect("json")).expect("write");
    let loaded = new_world();
    let notes = camcraft_app::project::load_noted(&loaded, &path).expect("load");
    assert_eq!(raw_stock_only_of(&loaded, None), Some(false), "read as unmarked");
    assert!(notes.iter().any(|n| n.contains("read as unmarked")), "{notes:?}");
    // A model imported later under the old name inherits nothing.
    let mut h2 = History::default();
    h2.execute(&loaded, ImportModel::from_mesh("casting", part()));
    assert_eq!(stock_only_model(&loaded), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The program gate's visible, non-blocking line (a void mark's note).
fn visible_line(world: &World) -> String {
    posting::verification_report(world).not_judged.unwrap_or_default()
}

/// The stock's mark as stored (test-side read).
fn stored_mark(world: &World) -> Option<StockSourceOnly> {
    let stock = model::find_by_name(world, &camcraft_app::setup::stock_entity_name(world, None))?;
    stock.entity_view(world).try_get::<&StockSourceOnly>(|f| f.clone())
}

/// The casting + "other" job with the casting marked; `swap` then puts
/// some geometry under the name "casting", and the program bores 5 mm
/// under that geometry's floor from the stock (plus an op on "other").
fn swapped(swap: impl Fn(&World, &mut History)) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("casting", casting()));
    let other = cube(Point3::new(80.0, -10.0, 0.0), Point3::new(100.0, 10.0, 20.0));
    h.execute(&world, ImportModel::from_mesh("other", other));
    let shape = StockShape::FromModel { source: "casting".into() };
    h.execute(&world, SetStock::new(&world, shape, Location::default()).expect("stock"));
    mark(&world, &mut h, true);
    swap(&world, &mut h);
    add_tool(&world, &mut h);
    op(&world, &mut h, "Op2", SrcRef::Entity("other".into()), FLOOR);
    op(&world, &mut h, "Op1", SrcRef::Stock, FLOOR - 5.0);
    world
}

#[test]
fn delete_fixture_then_a_part_under_the_old_name_refuses_the_gouge() {
    // The third review's reproduction.
    let world = swapped(|w, h| {
        h.execute(w, TagFixture::new(w, "casting", None).expect("tag"));
        h.execute(w, DeleteFixture::new(w, "casting").expect("delete fixture"));
        h.execute(w, ImportModel::from_mesh("casting", part()));
    });
    assert_eq!(stock_only_model(&world), None);
    assert!(gouge_line(&world).is_some(), "the 5 mm gouge went unjudged");
    assert!(post(&world, "fixture").is_err(), "the DeleteFixture reproduction posted");
}

#[test]
fn the_print_alone_voids_a_mark_left_on_other_geometry_by_a_path_with_no_drop() {
    // Bypass every command's drop on purpose: remove the casting straight
    // off the world, land the finished part under its name, and put the
    // old mark back by hand. Only the print check stands in the way.
    let world = swapped(|w, h| {
        let old = stored_mark(w).expect("marked");
        let e = model::find_by_name(w, "casting").expect("casting");
        model::remove(w, &e);
        h.execute(w, ImportModel::from_mesh("casting", part()));
        let stock = model::find_by_name(w, &camcraft_app::setup::stock_entity_name(w, None)).expect("stock");
        stock.entity_view(w).set(old);
    });
    assert!(stored_mark(&world).is_some(), "the fixture must leave the stale mark in place");
    assert_eq!(stock_only_model(&world), None, "a mark on other geometry is void");
    assert!(gouge_line(&world).is_some(), "the gouge is judged");
    assert!(visible_line(&world).contains("no longer matches the model"), "the void is named: {}", visible_line(&world));
    assert!(post(&world, "bypass").is_err());
}

#[test]
fn an_identical_re_import_under_the_same_name_keeps_the_mark() {
    // The same casting, removed and imported again (no drop): same print.
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    let old = stored_mark(&world).expect("marked");
    let e = model::find_by_name(&world, "casting").expect("casting");
    model::remove(&world, &e);
    h.execute(&world, ImportModel::from_mesh("casting", casting()));
    assert_eq!(stored_mark(&world), Some(old));
    assert_eq!(stock_only_model(&world), Some(entity(&world, "casting")), "the same casting stays raw stock");
    assert_eq!(visible_line(&world), "");
}

#[test]
fn a_moved_or_edited_marked_model_voids_the_mark_with_a_note() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    // Moved (MoveModel carries no drop): placement is part of the print.
    let to = Location { pos: Point3::new(5.0, 0.0, 0.0), ..Location::default() };
    h.execute(&world, MoveModel::new(&world, "casting", to).expect("move"));
    assert_eq!(stock_only_model(&world), None, "a moved casting is not what was marked");
    assert!(visible_line(&world).contains("'casting'"), "named: {}", visible_line(&world));
    h.undo(&world);
    assert_eq!(stock_only_model(&world), Some(entity(&world, "casting")), "undo: it prints the same again");
    // Edited in place with no command at all: new facets, same name.
    let e = entity(&world, "casting");
    let edited = cube(Point3::new(-30.0, -30.0, -10.0), Point3::new(30.0, 30.0, 25.0)).facets;
    model::set_facets(&world, &e, edited);
    assert_eq!(stock_only_model(&world), None);
    assert!(visible_line(&world).contains("no longer matches the model"));
}

#[test]
fn a_file_with_the_flag_but_no_print_loads_unmarked_with_a_note() {
    let (world, mut h) = casting_job(FLOOR);
    mark(&world, &mut h, true);
    let dir = std::env::temp_dir().join(format!("camcraft-1094-noprint-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dir");
    let path = dir.join("p.camproj");
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("raw_stock_print"), "the print is saved beside the flag");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    for row in doc["entities"].as_array_mut().expect("entities") {
        if let Some(obj) = row.as_object_mut() {
            obj.remove("raw_stock_print");
        }
    }
    std::fs::write(&path, serde_json::to_string(&doc).expect("json")).expect("write");
    let loaded = new_world();
    let notes = camcraft_app::project::load_noted(&loaded, &path).expect("loads, never refused");
    assert_eq!(stock_only_model(&loaded), None, "a flag without its print is not trusted");
    assert!(notes.iter().any(|n| n.contains("without the fingerprint")), "{notes:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
