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
use camcraft_app::drilling::{BreakThroughFrom, CycleChoice, Hole};
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
    drill_from(break_through, BreakThroughFrom::FullDiameter)
}

fn drill_from(break_through: Option<f64>, break_through_from: BreakThroughFrom) -> OpParams {
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
        break_through_from,
        picks: Default::default(),
    }
}

fn bore(break_through: Option<f64>) -> OpParams {
    OpParams::Bore {
        pass_start: camcraft_app::machining2d::PassStart::Auto,
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
        picks: Default::default(),
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

fn dr6_point() -> f64 {
    3.0 / 59.0_f64.to_radians().tan()
}

#[test]
fn the_default_is_today_s_millimetre_and_is_said() {
    let want_drill = 1.0 + dr6_point();
    for (name, tool, p, exit_mm) in [
        ("D", "DR-0600", drill(None), want_drill),
        ("DTip", "DR-0600", drill_from(None, BreakThroughFrom::Tip), 1.0),
        ("B", "EM-0400-2FL", bore(None), 1.0),
    ] {
        let (world, mut h) = world();
        let (t, _) = run(&world, &mut h, name, tool, p);
        assert!((deepest(&t) + exit_mm).abs() < 1e-6, "{name}: deepest {}", deepest(&t));
        let txt = format!("{exit_mm:.3}");
        assert!(said(&t, &format!("through holes run {txt} mm past the part bottom")) && said(&t, &format!("Z-{txt}")), "{name}: {:?}", t.notes);
        // Nothing is modelled under the stock: the exit is named, the
        // program still posts as it always has.
        assert!(t.notes.iter().any(|n| n.starts_with(BELOW_STOCK_NOTE) && n.contains(&format!("{txt} mm under the stock bottom Z0.000"))), "{name}: {:?}", t.notes);
    }
}

#[test]
fn a_stated_break_through_moves_the_deepest_feed() {
    let point = dr6_point();
    for (name, tool, p, want) in [
        ("D", "DR-0600", drill(Some(0.3)), -(0.3 + point)),
        ("DTip", "DR-0600", drill_from(Some(0.3), BreakThroughFrom::Tip), -0.3),
        ("B", "EM-0400-2FL", bore(Some(0.3)), -0.3),
        ("D0Tip", "DR-0600", drill_from(Some(0.0), BreakThroughFrom::Tip), 0.0),
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
    // Full-diameter zero still extends the drill point into the spoil board and holds post.
    let (world, mut h) = self::world();
    let board = camcraft_app::geometry::cube(Point3::new(-30.0, -30.0, -6.0), Point3::new(30.0, 30.0, 0.0));
    h.execute(&world, ImportModel::from_mesh("board", board));
    h.execute(&world, TagFixture::new(&world, "board", None).expect("tag"));
    let (_, warns) = run(&world, &mut h, "D", "DR-0600", drill(Some(0.0)));
    assert!(warns.iter().any(|w| w.blocks_post()), "full-diameter zero leaves point cone in spoil board: {warns:?}");
    // Stated in Tip mode to stop at the part bottom, the same drill posts.
    let (world, mut h) = self::world();
    let board = camcraft_app::geometry::cube(Point3::new(-30.0, -30.0, -6.0), Point3::new(30.0, 30.0, 0.0));
    h.execute(&world, ImportModel::from_mesh("board", board));
    h.execute(&world, TagFixture::new(&world, "board", None).expect("tag"));
    let (_, warns) = run(&world, &mut h, "D", "DR-0600", drill_from(Some(0.0), BreakThroughFrom::Tip));
    assert!(warns.iter().all(|w| !w.blocks_post()), "{warns:?}");
}

/// The kind-tagged params object's body (`{"drill": {...}}`).
fn body(v: &serde_json::Value) -> &serde_json::Value {
    v.as_object().and_then(|o| o.values().next()).unwrap_or(v)
}

#[test]
fn the_parameter_round_trips_the_file_and_the_mcp_params() {
    for p in [drill(Some(0.4)), drill_from(Some(0.4), BreakThroughFrom::Tip), bore(Some(0.4))] {
        let v = project::params_to_json(&p);
        assert_eq!(body(&v).get("break_through").and_then(serde_json::Value::as_f64), Some(0.4), "{v}");
        assert_eq!(project::params_from_json(&v).expect("back"), p);
    }
    // Absent break_through is the default, and the default writes no break_through key;
    // break_through_from writes its variant explicitly and defaults to full_diameter.
    for p in [drill(None), drill_from(None, BreakThroughFrom::Tip), bore(None)] {
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
    assert!(
        current["version"].as_u64().expect("saved format version") >= 68,
        "break_through_from needs v68 so v67 readers refuse its changed drill-depth semantics"
    );
    assert_eq!(current["version"], json!(project::PROJECT_VERSION), "current build saves latest format");
    for version in [62, 63, 64, 65, 67] {
        let mut file = current.clone();
        file["version"] = json!(version);
        for row in file["entities"].as_array_mut().unwrap() {
            if version < 64 && let Some(tool) = row.get_mut("tool").and_then(serde_json::Value::as_object_mut) {
                tool.remove("cutting_data");
            }
            if let Some(params) = row.get_mut("operation").and_then(|op| op.get_mut("params")).and_then(serde_json::Value::as_object_mut) {
                for body in params.values_mut() {
                    if let Some(body) = body.as_object_mut() {
                        if version < 63 { body.remove("break_through"); }
                        if version < 68 { body.remove("break_through_from"); }
                    }
                }
            }
        }
        let loaded = camcraft_app::world::new_world();
        project::open(&loaded, &file.to_string()).unwrap();
        let op = model::find_by_name(&loaded, "D").and_then(|e| model::operation_of(&loaded, &e)).unwrap();
        let expected = if version >= 63 { Some(0.4) } else { None };
        let expected_from = BreakThroughFrom::Tip;
        assert!(
            matches!(op.params, OpParams::Drill { break_through, break_through_from, .. } if break_through == expected && break_through_from == expected_from),
            "v{version}: {:?}", op.params
        );
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

    let want_drill = format!("Z-{:.3}", 0.3 + dr6_point());
    for (name, tool, params, want_z) in [
        ("D", "DR-0600", drill(Some(0.3)), want_drill.as_str()),
        ("DTip", "DR-0600", drill_from(Some(0.3), BreakThroughFrom::Tip), "Z-0.300"),
        ("B", "EM-0400-2FL", bore(Some(0.3)), "Z-0.300"),
    ] {
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
        assert!(note.contains(want_z), "{name}: {note}");
    }
}

/// A 100 x 60 x 10 plate (z 0..10) with one Ø6 through hole at the
/// origin, in 100 x 60 x 17 stock (z 0..17) with G54 at (-50, -30, 17)
/// - the #2441 reproduction geometry.
fn plate_10_with_d6_hole() -> (World, History) {
    use camcraft_app::commands::SetWcsOrigin;
    use camcraft_app::geometry::IDENTITY_Q;
    use camcraft_app::schema::Location;

    let mut f = Vec::new();
    plate(
        &rect(-50.0, -30.0, 50.0, 30.0),
        Some((&circle((0.0, 0.0), 3.0, 48), 0.0)),
        0.0,
        10.0,
        &mut f,
    );
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", Mesh::from_facets(f)));
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 100.0, dy: 60.0, dz: 17.0 }, Default::default()).expect("stock"),
    );
    let g54 = Location { pos: Point3::new(-50.0, -30.0, 17.0), rot: IDENTITY_Q };
    h.execute(&world, SetWcsOrigin::new(&world, g54).expect("G54"));
    let t = catalog().into_iter().find(|(n, _)| *n == "DR-0600").map(|(_, t)| t).expect("DR-0600");
    h.execute(&world, AddTool::new(&world, "DR-0600", t).expect("tool"));
    (world, h)
}

fn d6_through_hole() -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius: 3.0, entry_z: 10.0, floor_z: 0.0, through: true, stepped: false }
}

/// Column-sim top vertices inside the drilled hole at radial distance `r_min..=r_max`.
fn column_max_z_in_ring(world: &World, r_min: f64, r_max: f64) -> Option<f64> {
    let sim = camcraft_app::stock_sim::simulate_document(world).expect("simulate_document");
    sim.surface
        .verts
        .iter()
        .filter(|v| {
            let r = (v.x * v.x + v.y * v.y).sqrt();
            r >= r_min && r <= r_max
        })
        .map(|v| v.z)
        .max_by(f64::total_cmp)
}

#[test]
fn issue_2441_through_hole_drill_adds_point_length_and_posts_full_breakthrough() {
    use camcraft_app::operation::HeightReference;

    let (world, mut h) = plate_10_with_d6_hole();
    let mill = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("EM-1000-4FL");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", mill).expect("mill"));
    let face = OpParams::Face {
        holes: camcraft_app::machining2d::FaceHoles::CutAir,
        tilt: None,
        blend_turns: false, blend_radius: None,
        stepover: 4.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
        cut_z: Height::of(HeightReference::TopOfModel, 0.0),
        top_z: Some(Height::of(HeightReference::TopOfStock, 0.0)),
        stepdown: Some(4.0),
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "F1", "EM-1000-4FL", SrcRef::Stock, face).expect("create face"));
    h.execute(&world, GenerateOperation::new(&world, "F1").expect("gen face"));

    let point_len = 3.0 / 59.0_f64.to_radians().tan();
    let p = OpParams::Drill {
        clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
        retract_z: Some(Height::of(HeightReference::TopOfStock, 2.0)),
        top_z: Height::of(HeightReference::TopOfModel, 0.0),
        bottom_z: None,
        cycle: CycleChoice::Peck,
        peck: 2.0,
        dwell: 0.0,
        feed: 300.0,
        plunge_feed: 100.0,
        holes: vec![d6_through_hole()],
        cross: Vec::new(),
        break_through: None,
        break_through_from: BreakThroughFrom::FullDiameter,
        picks: Default::default(),
    };
    let (tp, _) = run(&world, &mut h, "D1", "DR-0600", p);
    let want_world_z = -(1.0 + point_len);
    assert!(
        (deepest(&tp) - want_world_z).abs() < 1e-4,
        "deepest tip must include the 118 deg drill point ({want_world_z:.3}), got {:.3}",
        deepest(&tp)
    );
    let posted = camcraft_app::posting::post_setup_unwritten(&world).expect("post");
    assert!(
        posted.program.contains("Z-19.803"),
        "Posted Z must break the full Ø6 diameter 1 mm past z0 (-17 - 1 - 1.803 = Z-19.803):\n{}",
        posted.program
    );
    // Both column and field stock sims must show zero remaining lip inside the hole.
    if let Some(col_z) = column_max_z_in_ring(&world, 1.5, 2.5) {
        assert!(col_z <= 0.0, "column sim leaves a lip inside the hole at z={col_z:.3}");
    }
    let view = camcraft_app::stock_view::field_view(&world).expect("field_view");
    let field_z = view.stock.top(2.5, 0.0);
    assert!(
        field_z.is_nan() || field_z <= 0.0,
        "field sim must show a clean exit at r=2.5 (got z={field_z:?})"
    );
}

#[test]
fn issue_2441_stock_sim_models_drill_point_cone_lip_when_stopped_at_floor() {
    use camcraft_app::operation::HeightReference;

    let (world, mut h) = plate_10_with_d6_hole();
    // Stop the drill tip right at the bottom plane z=0: a 118 deg
    // twist drill only cuts full Ø6 up to z=+1.803, so at r=2.5 it
    // must leave a ~1.502 mm conical lip in BOTH sim engines.
    let p = OpParams::Drill {
        clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
        retract_z: Some(Height::of(HeightReference::TopOfStock, 2.0)),
        top_z: Height::of(HeightReference::TopOfStock, 0.0),
        bottom_z: Some(Height::fixed(0.0)),
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 300.0,
        plunge_feed: 100.0,
        holes: vec![d6_through_hole()],
        cross: Vec::new(),
        break_through: None,
        break_through_from: BreakThroughFrom::FullDiameter,
        picks: Default::default(),
    };
    let _ = run(&world, &mut h, "D1", "DR-0600", p);
    let col_z = column_max_z_in_ring(&world, 2.2, 2.7).expect("columns inside r=2.5 ring");
    assert!(
        col_z > 0.8 && col_z < 1.8,
        "column sim must leave the conical drill-point lip (~1.5 mm) at r=2.5 when tip stops at z=0 (got z={col_z:.3})"
    );
    let view = camcraft_app::stock_view::field_view(&world).expect("field_view");
    let field_z = view.stock.top(2.5, 0.0);
    assert!(
        field_z.is_finite() && field_z > 0.8 && field_z < 1.8,
        "field sim must leave the conical drill-point lip (~1.5 mm) at r=2.5 when tip stops at z=0 (got z={field_z:?})"
    );
}

#[test]
fn issue_2441_mcp_list_operation_types_lists_break_through() {
    use camcraft_app::mcp::Server;
    use serde_json::json;

    let mut srv = Server::new();
    let reply = srv
        .handle(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": "list_operation_types", "arguments": {} }
        }))
        .expect("reply");
    let kinds = reply["result"]["structuredContent"]["kinds"]
        .as_array()
        .expect("kinds array");
    for key in ["drill", "bore", "thread"] {
        let kind = kinds
            .iter()
            .find(|k| k["key"] == json!(key))
            .unwrap_or_else(|| panic!("missing kind {key}"));
        let fields = kind["fields"].as_array().expect("fields array");
        let bt = fields
            .iter()
            .find(|f| f["name"] == json!("break_through"))
            .unwrap_or_else(|| panic!("{key} is missing break_through in list_operation_types: {fields:?}"));
        assert_eq!(bt["type"], json!("number"), "{bt}");
    }
    let drill = kinds.iter().find(|k| k["key"] == json!("drill")).expect("drill");
    let btf = drill["fields"]
        .as_array()
        .expect("fields")
        .iter()
        .find(|f| f["name"] == json!("break_through_from"))
        .expect("drill lists break_through_from");
    assert_eq!(btf["type"], json!("string"));
    assert_eq!(btf["default"], json!("full_diameter"));
}

#[test]
fn issue_2441_mcp_can_patch_break_through_and_break_through_from() {
    use camcraft_app::mcp::Server;
    use serde_json::json;

    let mut srv = Server::new();
    let call = |srv: &mut Server, name: &str, args: serde_json::Value| {
        let r = srv
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } }))
            .expect("reply");
        r["result"].clone()
    };
    assert_ne!(call(&mut srv, "set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 10.0 } }))["isError"], json!(true));
    assert_ne!(call(&mut srv, "add_tool", json!({ "catalog": "DR-0600" }))["isError"], json!(true));
    let add = call(&mut srv, "add_operation", json!({ "kind": "drill", "tool": "DR-0600" }));
    assert_ne!(add["isError"], json!(true), "{add}");
    let edit = call(&mut srv, "edit_operation", json!({
        "name": "Drill1",
        "params": { "break_through": 0.5, "break_through_from": "tip" }
    }));
    assert_ne!(edit["isError"], json!(true), "{edit}");
    assert_eq!(edit["structuredContent"]["params"]["drill"]["break_through"], json!(0.5));
    assert_eq!(edit["structuredContent"]["params"]["drill"]["break_through_from"], json!("tip"));
    let bad = call(&mut srv, "edit_operation", json!({
        "name": "Drill1",
        "params": { "break_through_from": "shank" }
    }));
    assert_eq!(bad["isError"], json!(true), "unknown mode must refuse: {bad}");
}

