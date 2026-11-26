//! The lathe spindle law (issue #62): a turning op's fitted axis of
//! revolution must BE the machine spindle the lathe post writes for -
//! world Z through the work offset's origin - or generation refuses it
//! by name. The post computes the diameter from the origin's XY and
//! the station from world Z; a part whose axis leans a few degrees in
//! Y with its X on the origin keeps every move in one meridian half-
//! plane, so the post's own off-meridian check cannot see it, and the
//! program would turn a taper the model does not have. The fixtures
//! are the lathe_post suite's facing bar, moved or leaned; a plumb bar
//! on the origin (and one on a work offset moved under it) still posts,
//! byte-identically to the bar the goldens lock.
//!
//! Gate map (M4.4): covers app::operation (collect_job's spindle_law
//! door, the stamp's spindle-origin term), app::posting (wcs_context,
//! program_ops' spindle_law door, stream_records on the lathe post).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, LoadMachine,
    SetSetupWorkOffset, SetStock, SetWcsOrigin,
};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, HeightReference, OpParams, OpStatus, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
/// The lathe_post suite's facing bar: wall r6 to z 2, neck r5 above.
const BAR: [(f64, f64); 6] = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];

/// The closed revolve about world Z, each vertex then mapped by `at`.
fn revolve(meridian: &[(f64, f64)], segs: usize, at: impl Fn(Point3) -> Point3) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        at(Point3::new(r * th.cos(), r * th.sin(), z))
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// A rotation about world X by `deg`: the axis leans in Y, its X stays.
fn lean_y(deg: f64) -> impl Fn(Point3) -> Point3 {
    let (s, c) = deg.to_radians().sin_cos();
    move |p| Point3::new(p.x, p.y * c - p.z * s, p.y * s + p.z * c)
}

fn lathe_world(bar: Mesh, tools: &[&str]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 40.0 },
        Location::default(),
    );
    history.execute(&world, cmd.expect("box stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    };
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(lathe));
    history.execute(&world, cmd.expect("lathe"));
    for t in tools {
        let tool = catalog().into_iter().find(|(n, _)| n == t).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, t, tool).expect("catalog tool"));
    }
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    (world, history)
}

fn facing() -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    }
}

fn finish() -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(2.0),
        feed: 120.0,
        plunge_feed: 60.0,
        picks: Default::default(),
    }
}

/// The document's stream through the lathe script, gate-free as the
/// lathe_post goldens run it: (program, the SCRIPT's own refusals - the
/// stream's WARN records, the verifier's notes on the fixture, are
/// echoed by the engine and taken out here).
fn post(world: &World) -> (String, Vec<String>) {
    let records = posting::stream_records(world).expect("every op generated");
    let out = engine::run(LATHE_CPS, &posting::machine_value(world), &cnc::serialize(&records))
        .expect("the lathe script runs");
    let notes: Vec<&String> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Warn { text } => Some(text),
            _ => None,
        })
        .collect();
    let own = out.warnings.into_iter().filter(|w| !notes.contains(&w)).collect();
    (out.program, own)
}

/// Generate `name`; Err is the refusal text. A generation that goes
/// through is posted, so a failing refusal test shows what the lathe
/// post would have written in its place.
fn generate(world: &World, history: &mut History, name: &str, tool: &str, p: OpParams) -> Result<(), String> {
    let cmd = CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), p).expect("create");
    history.execute(world, cmd);
    match GenerateOperation::new(world, name) {
        Ok(cmd) => {
            history.execute(world, cmd);
            Ok(())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn assert_refused(world: &World, history: &mut History, name: &str, tool: &str, p: OpParams, says: &[&str]) {
    match generate(world, history, name, tool, p) {
        Ok(()) => {
            let (program, warnings) = post(world);
            panic!("{name} generated; the lathe post wrote (refusals {warnings:?}):\n{program}");
        }
        Err(e) => {
            for s in says {
                assert!(e.contains(s), "{name}'s refusal names '{s}': {e}");
            }
        }
    }
}

#[test]
fn a_bar_leaning_in_y_on_the_origin_is_refused_by_name_at_generation() {
    // 3 degrees about world X: the axis is (0, -sin, cos) through the
    // origin, so every move has X = 0 and one meridian holds them all -
    // the post's off-meridian check passes and it would write the
    // stations and diameters of a vertical bar: a taper on the machine.
    let bar = revolve(&BAR, 16, lean_y(3.0));
    let (world, mut history) = lathe_world(bar, &["TN-1600-0R8"]);
    let says = ["spindle", "3.000 degrees", "align"];
    assert_refused(&world, &mut history, "F1", "TN-1600-0R8", facing(), &says);
    assert_refused(&world, &mut history, "P1", "TN-1600-0R8", finish(), &says);
}

#[test]
fn a_plumb_bar_off_the_work_origin_is_refused_by_name_at_generation() {
    // Vertical, but 10 mm off G54 in X: the post's diameters are
    // measured from the origin, so the part's axis is not the spindle.
    let bar = revolve(&BAR, 16, |p| Point3::new(p.x + 10.0, p.y, p.z));
    let (world, mut history) = lathe_world(bar, &["TN-1600-0R8"]);
    let says = ["spindle", "10.000 mm", "G54"];
    assert_refused(&world, &mut history, "F1", "TN-1600-0R8", facing(), &says);
}

#[test]
fn an_axial_drill_on_a_bar_off_the_work_origin_is_refused_by_name() {
    let bar = revolve(
        &[(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)],
        64,
        |p| Point3::new(p.x, p.y + 5.0, p.z),
    );
    let (world, mut history) = lathe_world(bar, &["DR-0400"]);
    let hole = Hole { xy: Point2::new(0.0, 5.0), radius: 2.05, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false };
    let params = OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
        picks: Default::default(),
    };
    assert_refused(&world, &mut history, "AX1", "DR-0400", params, &["spindle", "5.000 mm"]);
}

#[test]
fn a_plumb_bar_on_the_origin_or_under_a_moved_work_offset_posts_the_same_program() {
    // The baseline: the lathe_post golden's own bar and op.
    let (world, mut history) = lathe_world(revolve(&BAR, 16, |p| p), &["TN-1600-0R8"]);
    generate(&world, &mut history, "F1", "TN-1600-0R8", facing()).expect("a plumb bar on the origin generates");
    let (plain, warnings) = post(&world);
    assert!(warnings.is_empty(), "{warnings:?}");

    // The same bar 10 mm over in X, its stock with it (#122: facing reads
    // the stock, so a blank left behind is a different job), and G54
    // moved onto its axis: the spindle IS the part's axis, and the
    // program measures from it.
    let bar = revolve(&BAR, 16, |p| Point3::new(p.x + 10.0, p.y, p.z));
    let (world, mut history) = lathe_world(bar, &["TN-1600-0R8"]);
    let at = Location { pos: Point3::new(10.0, 0.0, 0.0), rot: IDENTITY_Q };
    let stock = SetStock::new(&world, camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 40.0 }, at);
    history.execute(&world, stock.expect("the box moves with the bar"));
    history.execute(&world, SetWcsOrigin::new(&world, at).expect("set G54"));
    generate(&world, &mut history, "F1", "TN-1600-0R8", facing()).expect("a bar on its work offset generates");
    let (moved, warnings) = post(&world);
    assert!(warnings.is_empty(), "{warnings:?}");
    assert_eq!(moved, plain, "the work offset on the axis posts the on-origin program");
}

fn moved(x: f64, y: f64) -> impl Fn(Point3) -> Point3 {
    move |p| Point3::new(p.x + x, p.y + y, p.z)
}

fn place(world: &World, history: &mut History, x: f64, y: f64) {
    let at = Location { pos: Point3::new(x, y, 0.0), rot: IDENTITY_Q };
    history.execute(world, SetWcsOrigin::new(world, at).expect("set the work offset"));
}

#[test]
fn a_plumb_bar_off_the_work_origin_in_y_is_refused_by_name_at_generation() {
    // The offset the post could not see: every move keeps X = 0, so
    // the meridian check passes and the post's radius, read as y - 1,
    // would make every diameter 2 mm small.
    let (world, mut history) = lathe_world(revolve(&BAR, 16, moved(0.0, 1.0)), &["TN-1600-0R8"]);
    let says = ["spindle", "1.000 mm", "G54"];
    assert_refused(&world, &mut history, "F1", "TN-1600-0R8", facing(), &says);
    assert_refused(&world, &mut history, "P1", "TN-1600-0R8", finish(), &says);
}

#[test]
fn a_work_offset_moved_after_generation_stales_the_op_and_is_refused_by_name_at_posting() {
    let (world, mut history) = lathe_world(revolve(&BAR, 16, |p| p), &["TN-1600-0R8"]);
    generate(&world, &mut history, "F1", "TN-1600-0R8", facing()).expect("a plumb bar on the origin generates");
    let op = camcraft_app::model::find_by_name(&world, "F1").expect("F1");
    assert_eq!(camcraft_app::operation::status_of(&world, &op), OpStatus::Ok);

    // G54 moved 1 mm in Y under the generated op: the post would now
    // measure every diameter from y = 1.
    place(&world, &mut history, 0.0, 1.0);
    assert_eq!(
        camcraft_app::operation::status_of(&world, &op),
        OpStatus::Stale,
        "the work offset is an input of a turning op"
    );
    let e = posting::stream_records(&world).expect_err("a moved spindle never posts");
    for s in ["F1", "spindle", "1.000 mm", "G54"] {
        assert!(e.contains(s), "the post's refusal names '{s}': {e}");
    }
    let e = posting::program_ops(&world).expect_err("the sim walks the same door");
    assert!(e.contains("spindle"), "{e}");

    // Moved back, the cached path is fresh again and posts.
    place(&world, &mut history, 0.0, 0.0);
    assert_eq!(camcraft_app::operation::status_of(&world, &op), OpStatus::Ok);
    let (_, warnings) = post(&world);
    assert!(warnings.is_empty(), "{warnings:?}");
}

#[test]
fn a_setup_on_g55_measures_the_spindle_from_g55() {
    // The WCS is the setup's (#196, #329, #342): labelled G55 and placed
    // at y = 1 under a bar on the world origin, it is 1 mm off.
    let on_g55 = || match facing() {
        OpParams::Facing { tolerance, top_z, bottom_z, stepdown, feed, plunge_feed, .. } => {
            let clearance_z = Height { reference: HeightReference::WorkOffset, offset: 25.0 };
            OpParams::Facing { tolerance, clearance_z, top_z, bottom_z, stepdown, feed, plunge_feed, picks: Default::default() }
        }
        other => other,
    };
    let on_g55_setup = |world: &World, history: &mut History| {
        history.execute(world, CreateSetup::new(world, None, None).expect("materialize Setup1"));
        history.execute(world, SetSetupWorkOffset::new(world, "Setup1", Some(1)).expect("pick G55"));
    };
    let (world, mut history) = lathe_world(revolve(&BAR, 16, |p| p), &["TN-1600-0R8"]);
    on_g55_setup(&world, &mut history);
    place(&world, &mut history, 0.0, 1.0);
    assert_refused(&world, &mut history, "F1", "TN-1600-0R8", on_g55(), &["spindle", "1.000 mm", "G55"]);

    // And the G55-labelled WCS on the bar's axis passes.
    let (world, mut history) = lathe_world(revolve(&BAR, 16, moved(10.0, 0.0)), &["TN-1600-0R8"]);
    on_g55_setup(&world, &mut history);
    place(&world, &mut history, 10.0, 0.0);
    generate(&world, &mut history, "F1", "TN-1600-0R8", on_g55()).expect("G55 on the axis generates");
}

#[test]
fn a_second_setups_own_work_offset_is_the_spindle_it_is_judged_against() {
    // Setup2's own WCS sits 1 mm off the bar; the legacy G54 (never
    // placed, the world origin) is on it - the op must read its own.
    let (world, mut history) = lathe_world(revolve(&BAR, 16, |p| p), &["TN-1600-0R8"]);
    history.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    history.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    let stock = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 40.0 },
        Location::default(),
    );
    history.execute(&world, stock.expect("Setup2 stock"));
    place(&world, &mut history, 0.0, 1.0);
    assert!(camcraft_app::model::find_by_name(&world, "Setup2/WCS").is_some(), "Setup2's own WCS");
    assert_refused(&world, &mut history, "F1", "TN-1600-0R8", facing(), &["spindle", "1.000 mm", "G54"]);
}
