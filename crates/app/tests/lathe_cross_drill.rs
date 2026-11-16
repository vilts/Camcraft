//! A lathe cross drill is judged as the radial work it is (issue #80):
//! the axial-hole verdict (M13.5 A) reads a Z drill running down the
//! spindle, so it judges only a lathe Drill with no cross holes. Before
//! the fix every lathe cross drill - clear, with or without raw stock -
//! was convicted by that verdict ("CD1: move 1 cuts past the finish
//! surface") and never posted. The cross path keeps its own laws: the
//! spindle law (#62/#67), the raw-stock standoffs (#77), the reach and
//! holder law (#84); the axial hole keeps its verdict.
//!
//! Gate map (M4.4): covers app::operation (generate's lathe_axial
//! seam, the cross arm on a lathe, the axial verdict), app::posting.
//! Lane shape (M4.4): all tests here run in seconds.

mod punched_bar;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const PART_R: f64 = 10.0;

/// The finished bar of radius `r`, z in [-80, 20] (`punched_bar::FOOT`), revolved about
/// world Z, with the two cross holes of `cross(6.0)` modelled in it
/// (issue #117: the part law refuses a hole the model lacks).
fn bar(r: f64) -> Mesh {
    punched_bar::punched_bar(r, punched_bar::FOOT, punched_bar::TOP, &cross(6.0))
}

/// Two stations on the r10 face, plunging toward the axis, `depth` deep.
fn cross(depth: f64) -> Vec<CrossHole> {
    let h = |x: f64, y: f64, z: f64, dir: Point3| CrossHole {
        origin: Point3::new(x, y, z),
        dir,
        radius: 2.0,
        entry_d: 0.0,
        floor_d: -depth,
        through: false,
    };
    vec![
        h(PART_R, 0.0, 10.0, Point3::new(-1.0, 0.0, 0.0)),
        h(0.0, PART_R, 0.0, Point3::new(0.0, -1.0, 0.0)),
    ]
}

fn params(holes: Vec<Hole>, cross: Vec<CrossHole>) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes,
        cross,
    }
}

/// A live-tooling lathe (it states the A and B tilts the radial poses
/// ask for, so the post's machine-limits clause passes) with the
/// finished r10 bar on the world origin (G54 unmoved), `raw` as the
/// setup's stock diameter.
fn lathe(raw: Option<f64>) -> (World, History) {
    lathe_at(raw, 0.0, "lathe")
}

/// `lathe` on a mill-turn: what the posting test needs - a 2-axis
/// lathe has no live tool for a cross hole, so only a mill-turn's
/// milling post may post it (#1232).
fn mill_turn(raw: Option<f64>) -> (World, History) {
    lathe_at(raw, 0.0, "mill-turn")
}

fn rotaries() -> Vec<AxisLimits> {
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])]
}

/// `lathe` with the bar shifted `dx` along world X (G54 left at the
/// world origin, so the bar no longer turns on the programmed spindle).
fn lathe_at(raw: Option<f64>, dx: f64, kind: &str) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    if let Some(d) = raw {
        let at = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
        let shape = StockShape::Cylinder { diameter: d, height: 40.0 };
        history.execute(&world, SetStock::new(&world, shape, at).expect("stock"));
    }
    let m = Machine { model: "TestBench lathe".into(), kind: Some(kind.into()), axes: rotaries(), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    tool.flute_len = 30.0;
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("tool"));
    let s = |p: Point3| Point3::new(p.x + dx, p.y, p.z);
    let moved = bar(PART_R).facets.iter().map(|t| Triangle::new(s(t.a), s(t.b), s(t.c))).collect();
    history.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(moved)));
    (world, history)
}

fn op(p: OpParams) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity("bar".into()),
        params: p,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

fn blocking(w: &[Warning]) -> Vec<&Warning> {
    w.iter().filter(|w| w.blocks_post()).collect()
}

#[test]
fn a_clear_lathe_cross_drill_generates_with_no_blocking_warning_and_posts() {
    for raw in [None, Some(40.0)] {
        let (world, mut history) = mill_turn(raw);
        let (_, warns) = operation::generate(&world, &op(params(vec![], cross(6.0))), "")
            .unwrap_or_else(|e| panic!("raw {raw:?}: the clear lathe cross drill generates: {e}"));
        assert!(blocking(&warns).is_empty(), "raw {raw:?}: a clear cross drill is convicted: {warns:?}");
        let create = CreateOperation::new(&world, "CD1", "DR-0400", SrcRef::Entity("bar".into()), params(vec![], cross(6.0)));
        history.execute(&world, create.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
        let e = model::find_by_name(&world, "CD1").expect("op");
        assert_eq!(operation::status_of(&world, &e), OpStatus::Ok, "raw {raw:?}: the cross drill is fresh");
        let path = std::env::temp_dir().join(format!("camcraft-80-{}-{}.nc", raw.is_some(), std::process::id()));
        let posted = posting::post_setup(&world, &path);
        let _ = std::fs::remove_file(&path);
        assert!(posted.is_ok(), "raw {raw:?}: the clear lathe cross drill posts: {:?}", posted.err());
    }
}

#[test]
fn a_lathe_cross_drill_deeper_than_its_flute_is_still_refused_by_name() {
    let (world, _h) = lathe(None);
    let e = operation::generate(&world, &op(params(vec![], cross(32.0))), "").err().map(|e| e.to_string());
    let e = e.unwrap_or_else(|| panic!("a 32 mm cross hole on a 30 mm flute drilled"));
    assert!(e.contains("cross hole") && e.contains("flute"), "the refusal names the reach: {e}");
}

#[test]
fn a_lathe_cross_drill_whose_bar_is_off_the_spindle_is_still_refused_by_name() {
    let (world, _h) = lathe_at(None, 30.0, "lathe");
    let moved: Vec<CrossHole> = cross(6.0)
        .into_iter()
        .map(|h| CrossHole { origin: Point3::new(h.origin.x + 30.0, h.origin.y, h.origin.z), ..h })
        .collect();
    let r = operation::generate(&world, &op(params(vec![], moved)), "");
    let e = match r {
        Ok((_, w)) => panic!("a cross drill on a bar off the spindle drilled: {w:?}"),
        Err(e) => e.to_string(),
    };
    // The spindle law (#62) names the spindle; a refusal for any other
    // reason would not prove the off-spindle bar is caught.
    assert!(e.contains("spindle"), "the refusal names the spindle: {e}");
}

#[test]
fn an_axial_lathe_drill_where_the_bar_has_no_bore_is_still_convicted() {
    let (world, _h) = lathe(None);
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.05, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false };
    let (_, warns) = operation::generate(&world, &op(params(vec![hole], vec![])), "")
        .unwrap_or_else(|e| panic!("the axial drill generates (its verdict is a warning): {e}"));
    assert!(
        warns.iter().any(|w| matches!(w, Warning::Gouge { .. }) && w.blocks_post()),
        "an axial hole the bar does not have is still convicted: {warns:?}"
    );
}

/// Issue #116: a round bar with ONE modelled cross hole (no mirror
/// hole on the far face) reads its true axis within the spindle
/// tolerance - the hole's wall, floor and the mouth ring's off-meridian
/// facets are set aside, and the band tightens on the clean surface of
/// revolution - so the cross drill generates clean and posts. Before
/// the fix the fit leaned ~0.001 mm and the spindle law refused it.
#[test]
fn a_bar_with_one_modelled_cross_hole_fits_on_the_spindle_and_drills_clean() {
    let one = vec![cross(6.0)[0]];
    let part = punched_bar::punched_bar(PART_R, punched_bar::FOOT, punched_bar::TOP, &one);
    let r = operation::spindle_reading(&part.facets, [0.0; 3]).expect("the one-hole bar reads as revolved");
    println!("one hole: {r:?}");
    assert!(r.trusted && r.worst() <= operation::SPINDLE_TOL, "the one-hole bar sits on the spindle: {r:?}");
    let world = new_world();
    let mut history = History::default();
    let m = Machine { model: "TestBench lathe".into(), kind: Some("mill-turn".into()), axes: rotaries(), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    tool.flute_len = 30.0;
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", part));
    let (_, warns) = operation::generate(&world, &op(params(vec![], one.clone())), "")
        .unwrap_or_else(|e| panic!("the one-hole cross drill generates: {e}"));
    assert!(blocking(&warns).is_empty(), "the one-hole cross drill is convicted: {warns:?}");
    let create = CreateOperation::new(&world, "CD1", "DR-0400", SrcRef::Entity("bar".into()), params(vec![], one));
    history.execute(&world, create.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "CD1").expect("generate"));
    let path = std::env::temp_dir().join(format!("camcraft-116-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_ok(), "the one-hole cross drill posts: {:?}", posted.err());
}
