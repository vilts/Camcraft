//! #1557: the through-hole break-through is a stated parameter of Drill
//! and Bore (and Thread), not a hidden 1.0 mm: absent it keeps today's
//! 1.0 mm (so every older program posts byte-identical), stated it moves
//! the deepest feed, the op says what it did, a tip under an unmodelled
//! stock bottom is named, and one that reaches modelled steel under the
//! stock is refused by the fixture law. The parameter round-trips the
//! project file and the MCP params JSON.
//!
//! Gate map (#1557): covers camcraft_app::operation (generate,
//! break_through, types::sane, params::holes), camcraft_app::project
//! (params, params_impl, json), camcraft_motion::drilling (cycle),
//! camcraft_motion::machining2d (bore).
//!
//! Provenance: our own fixtures (common::solid_part); the through-drilling
//! exit allowance of machining handbooks. No vendor material.

mod common;

use common::solid_part::{circle, plate, rect};
use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, TagFixture};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{BELOW_STOCK_NOTE, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::project;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::{FixturePart, Warning};

/// A 40 x 40 plate, z 0..10, with a Ø8 through hole at the origin.
fn holed_plate() -> Vec<Triangle> {
    let mut f = Vec::new();
    plate(&rect(-20.0, -20.0, 20.0, 20.0), Some((&circle((0.0, 0.0), 4.0, 48), 0.0)), 0.0, 10.0, &mut f);
    f
}

fn hole() -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius: 4.0, entry_z: 10.0, floor_z: 0.0, through: true, stepped: false }
}

/// The plate in a stock of the same footprint, 0.5 taller, with the
/// tools: the part bottom is the stock bottom (a plate sawn to size).
fn world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", Mesh::from_facets(holed_plate())));
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.5 }, Default::default()).expect("stock"),
    );
    let aabb = model::stock_world_aabb_for(&world, None).expect("stock box");
    assert!(aabb.min.z.abs() < 1e-9, "the stock bottom is the part bottom: {aabb:?}");
    for name in ["DR-0600", "EM-0400-2FL"] {
        let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog");
        h.execute(&world, AddTool::new(&world, name, t).expect("tool"));
    }
    (world, h)
}

fn drill(break_through: Option<f64>) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 200.0,
        plunge_feed: 100.0,
        holes: vec![hole()],
        cross: Vec::new(),
        break_through,
    }
}

fn bore(break_through: Option<f64>) -> OpParams {
    OpParams::Bore {
        links: camcraft_app::links::Links::PLANAR,
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(12.0),
        bottom_z: None,
        stock_to_leave: 0.0,
        stepdown: 2.0,
        feed: 600.0,
        plunge_feed: 200.0,
        climb: true,
        tolerance: 0.01,
        holes: vec![hole()],
        break_through,
    }
}

fn run(world: &World, h: &mut History, name: &str, tool: &str, p: OpParams) -> (Toolpath, Vec<Warning>) {
    h.execute(world, CreateOperation::new(world, name, tool, SrcRef::Entity("plate".into()), p).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{name}");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.clone())
}

/// The lowest Z any cutting move or drill cycle reaches.
fn deepest(t: &Toolpath) -> f64 {
    let feeds = t.moves.iter().filter(|m| m.kind != MoveKind::Rapid).map(|m| m.to.z);
    feeds.chain(t.cycles.iter().map(|c| c.bottom_z)).fold(f64::INFINITY, f64::min)
}

fn said(t: &Toolpath, text: &str) -> bool {
    t.notes.iter().any(|n| n.contains(text))
}

#[test]
fn the_default_is_today_s_millimetre_and_is_said() {
    for (name, tool, p) in [("D", "DR-0600", drill(None)), ("B", "EM-0400-2FL", bore(None))] {
        let (world, mut h) = world();
        let (t, _) = run(&world, &mut h, name, tool, p);
        assert!((deepest(&t) + 1.0).abs() < 1e-6, "{name}: deepest {}", deepest(&t));
        assert!(said(&t, "through holes run 1.000 mm past the part bottom") && said(&t, "Z-1.000"), "{name}: {:?}", t.notes);
        // Nothing is modelled under the stock: the exit is named, the
        // program still posts as it always has.
        assert!(t.notes.iter().any(|n| n.starts_with(BELOW_STOCK_NOTE) && n.contains("1.000 mm under the stock bottom Z0.000")), "{name}: {:?}", t.notes);
    }
}

#[test]
fn a_stated_break_through_moves_the_deepest_feed() {
    for (name, tool, p, want) in [
        ("D", "DR-0600", drill(Some(0.3)), -0.3),
        ("B", "EM-0400-2FL", bore(Some(0.3)), -0.3),
        ("D0", "DR-0600", drill(Some(0.0)), 0.0),
    ] {
        let (world, mut h) = world();
        let (t, _) = run(&world, &mut h, name, tool, p);
        assert!((deepest(&t) - want).abs() < 1e-6, "{name}: deepest {} want {want}", deepest(&t));
        assert!(said(&t, &format!("Z{want:.3}")), "{name}: {:?}", t.notes);
        if want >= 0.0 {
            assert!(t.notes.iter().all(|n| !n.starts_with(BELOW_STOCK_NOTE)), "{name}: {:?}", t.notes);
        }
    }
}

#[test]
fn a_spot_dimple_does_not_claim_to_break_through_the_stock() {
    let (world, mut h) = world();
    let mut p = drill(Some(0.3));
    if let OpParams::Drill { cycle, .. } = &mut p {
        *cycle = CycleChoice::Spot;
    }
    let (t, _) = run(&world, &mut h, "Spot", "DR-0600", p);
    assert!((deepest(&t) - 8.0).abs() < 1e-6, "the dimple stops two millimetres below entry Z10: {}", deepest(&t));
    assert!(
        t.notes.iter().all(|n| !n.contains("through holes run") && !n.starts_with(BELOW_STOCK_NOTE)),
        "a spot never reaches the through floor: {:?}",
        t.notes
    );
}

#[test]
fn a_negative_break_through_is_refused_at_creation() {
    let (world, _) = world();
    assert!(CreateOperation::new(&world, "D", "DR-0600", SrcRef::Entity("plate".into()), drill(Some(-1.0))).is_err());
    assert!(CreateOperation::new(&world, "D", "DR-0600", SrcRef::Entity("plate".into()), drill(Some(f64::NAN))).is_err());
}

#[test]
fn a_break_through_into_modelled_steel_under_the_stock_is_refused() {
    let (world, mut h) = world();
    // A spoil board modelled under the stock, tagged as workholding.
    let board = camcraft_app::geometry::cube(Point3::new(-30.0, -30.0, -6.0), Point3::new(30.0, 30.0, 0.0));
    h.execute(&world, ImportModel::from_mesh("board", board));
    h.execute(&world, TagFixture::new(&world, "board", None).expect("tag"));
    let (t, warns) = run(&world, &mut h, "D", "DR-0600", drill(None));
    assert!(t.notes.iter().all(|n| !n.starts_with(BELOW_STOCK_NOTE)), "the fixture law judges it: {:?}", t.notes);
    assert!(
        warns.iter().any(|w| matches!(w, Warning::FixtureHit { fixture_part: FixturePart::Flute, .. })),
        "the drill's tip in the board is a flute hit: {warns:?}"
    );
    assert!(camcraft_app::posting::post_setup_unwritten(&world).is_err(), "and it holds the post");
    // Stated to stop at the part bottom, the same drill posts.
    let (world, mut h) = self::world();
    let board = camcraft_app::geometry::cube(Point3::new(-30.0, -30.0, -6.0), Point3::new(30.0, 30.0, 0.0));
    h.execute(&world, ImportModel::from_mesh("board", board));
    h.execute(&world, TagFixture::new(&world, "board", None).expect("tag"));
    let (_, warns) = run(&world, &mut h, "D", "DR-0600", drill(Some(0.0)));
    assert!(warns.iter().all(|w| !w.blocks_post()), "{warns:?}");
}

/// The kind-tagged params object's body (`{"drill": {...}}`).
fn body(v: &serde_json::Value) -> &serde_json::Value {
    v.as_object().and_then(|o| o.values().next()).unwrap_or(v)
}

#[test]
fn the_parameter_round_trips_the_file_and_the_mcp_params() {
    for p in [drill(Some(0.4)), bore(Some(0.4))] {
        let v = project::params_to_json(&p);
        assert_eq!(body(&v).get("break_through").and_then(serde_json::Value::as_f64), Some(0.4), "{v}");
        assert_eq!(project::params_from_json(&v).expect("back"), p);
    }
    // Absent is the default, and the default writes no key: older files
    // read unchanged and default ops write what they always wrote.
    for p in [drill(None), bore(None)] {
        let v = project::params_to_json(&p);
        assert!(body(&v).get("break_through").is_none(), "{v}");
        assert_eq!(project::params_from_json(&v).expect("back"), p);
    }
}


#[test]
fn v65_preserves_cutting_steps_and_loads_v63_break_through_and_v62_defaults() {
    use camcraft_app::commands::ChangeTool;
    use camcraft_app::tool_cutting_rows::CuttingRow;
    use serde_json::json;

    let (world, mut history) = world();
    let entity = model::find_by_name(&world, "DR-0600").unwrap();
    let mut tool = model::tool_of(&world, &entity).unwrap();
    tool.cutting = vec![CuttingRow {
        stepdown: Some(0.5), ..CuttingRow::plain(None, tool.rpm, tool.feed)
    }];
    history.execute(&world, ChangeTool::new(&world, "DR-0600", tool.clone()).unwrap());
    history.execute(&world, CreateOperation::new(&world, "D", "DR-0600", SrcRef::Entity("plate".into()), drill(Some(0.4))).unwrap());
    let saved = project::save_text(&world).unwrap();
    let current: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(current["version"], json!(project::PROJECT_VERSION), "the trunnion-schema format follows main's v65 tilt-basis format");
    for version in [62, 63, 64, 65] {
        let mut file = current.clone();
        file["version"] = json!(version);
        for row in file["entities"].as_array_mut().unwrap() {
            if version < 64 && let Some(tool) = row.get_mut("tool").and_then(serde_json::Value::as_object_mut) {
                tool.remove("cutting_data");
            }
            if version < 63 && let Some(params) = row.get_mut("operation").and_then(|op| op.get_mut("params")).and_then(serde_json::Value::as_object_mut) {
                for body in params.values_mut() {
                    if let Some(body) = body.as_object_mut() { body.remove("break_through"); }
                }
            }
        }
        let loaded = camcraft_app::world::new_world();
        project::open(&loaded, &file.to_string()).unwrap();
        let op = model::find_by_name(&loaded, "D").and_then(|e| model::operation_of(&loaded, &e)).unwrap();
        let expected = if version >= 63 { Some(0.4) } else { None };
        assert!(matches!(op.params, OpParams::Drill { break_through, .. } if break_through == expected), "v{version}: {:?}", op.params);
        let actual = model::find_by_name(&loaded, "DR-0600").and_then(|e| model::tool_of(&loaded, &e)).unwrap();
        assert_eq!((actual.rpm, actual.feed), (tool.rpm, tool.feed));
        if version >= 64 { assert_eq!(actual.cutting, tool.cutting); }
        else { assert!(actual.cutting.is_empty(), "older tools keep their derived Unknown row"); }
        let rewritten: serde_json::Value = serde_json::from_str(&project::save_text(&loaded).unwrap()).unwrap();
        assert_eq!(rewritten["version"], json!(project::PROJECT_VERSION), "old documents migrate forward");
    }
}

#[test]
fn yawed_break_through_note_reports_the_hole_in_setup_coordinates() {
    use camcraft_app::commands::SetWcsOrigin;
    use camcraft_app::geometry::quat_from_euler;
    use camcraft_app::schema::Location;

    for (name, tool, params) in [("D", "DR-0600", drill(Some(0.3))), ("B", "EM-0400-2FL", bore(Some(0.3)))] {
        let (world, mut h) = world();
        let placement = Location {
            pos: Point3::new(5.0, 7.0, 0.0),
            rot: quat_from_euler(0.0, 0.0, 30f64.to_radians()),
        };
        h.execute(&world, SetWcsOrigin::new(&world, placement).expect("yaw WCS"));
        let (path, _) = run(&world, &mut h, name, tool, params);
        let note = path.notes.iter().find(|n| n.starts_with(BELOW_STOCK_NOTE)).expect("below stock note");
        let xy = note.split_once('(').unwrap().1.split_once(')').unwrap().0;
        for coord in xy.split(',') {
            let value: f64 = coord.split_whitespace().next().unwrap().parse().unwrap();
            assert!(value.abs() < 1e-6, "the source hole is (0, 0) in Setup S: {note}");
        }
        assert!(note.contains("Z-0.300"), "{note}");
    }
}
