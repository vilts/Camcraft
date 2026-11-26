//! A lathe cross drill is judged as the radial work it is (issue #80):
//! the axial-hole verdict (M13.5 A) reads a Z drill running down the
//! spindle, so it judges only a lathe Drill with no cross holes. Before
//! the fix every lathe cross drill - clear, with or without raw stock -
//! was convicted by that verdict ("CD1: move 1 cuts past the finish
//! surface") and never posted. The cross path keeps its own laws: the
//! spindle law (#62/#67), the raw-stock standoffs (#77), the reach and
//! holder law (#84); the axial hole keeps its verdict.
//!
//! #3077: the first rapid from the turret's index position is judged
//! against the work and the steel - a tailstock over the holes refuses
//! the drill by name, a chuck under them does not; a mill-turn's cross
//! drill also answers to the upright law of its mill post.
//!
//! Gate map (M4.4): covers app::operation (generate's lathe_axial
//! seam, the cross arm on a lathe, the axial verdict, the cross_traverse
//! seam), app::posting.
//! Lane shape (M4.4): all tests here run in seconds.

mod punched_bar;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetFixtureFixed, SetStock, TagFixture};
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
        break_through: None,
        break_through_from: Default::default(),
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
        picks: Default::default(),
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

/// A box of steel from `lo` to `hi`, as twelve facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
    let c = |i: usize| Point3::new(if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] });
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))]).collect()
}

/// Tag a box of steel `name` on the setup: `fixed` (a tailstock) or
/// turning with the part (a chuck).
fn steel(world: &World, history: &mut History, name: &str, lo: [f64; 3], hi: [f64; 3], fixed: bool) {
    history.execute(world, ImportModel::from_mesh(name, Mesh::from_facets(cuboid(lo, hi))));
    history.execute(world, TagFixture::new(world, name, None).expect("tag"));
    history.execute(world, SetFixtureFixed::new(world, name, fixed).expect("fixed"));
}

/// A chuck under the bar's foot (z -80): out of the band the first
/// rapid can pass (radius 15 and out, Z 8 and up, for the hole at Z 10).
fn chuck(world: &World, history: &mut History) {
    steel(world, history, "chuck", [-40.0, -40.0, -100.0], [40.0, 40.0, -82.0], false);
}

/// #3077: with a chuck under the holes, the first rapid from the
/// turret's index position is clear on a lathe and on a mill-turn.
#[test]
fn a_cross_drill_over_its_chuck_has_a_clear_first_traverse() {
    for kind in ["lathe", "mill-turn"] {
        let (world, mut history) = lathe_at(None, 0.0, kind);
        chuck(&world, &mut history);
        let (_, warns) = operation::generate(&world, &op(params(vec![], cross(6.0))), "")
            .unwrap_or_else(|e| panic!("{kind}: the cross drill over its chuck generates: {e}"));
        assert!(blocking(&warns).is_empty(), "{kind}: a clear first traverse is convicted: {warns:?}");
    }
}

/// #3077: a tailstock wider than the first station's radius stands over
/// the holes. The post sends X and Z in one rapid from the index
/// position, so the tool can pass through it: refused by name on a
/// lathe and on a mill-turn, before any move.
#[test]
fn a_cross_drill_under_a_wide_tailstock_is_refused_by_name() {
    for kind in ["lathe", "mill-turn"] {
        let (world, mut history) = lathe_at(None, 0.0, kind);
        chuck(&world, &mut history);
        steel(&world, &mut history, "tailstock", [-30.0, -30.0, 25.0], [30.0, 30.0, 60.0], true);
        let e = match operation::generate(&world, &op(params(vec![], cross(6.0))), "") {
            Ok((_, w)) => panic!("{kind}: a cross drill under a wide tailstock generated: {w:?}"),
            Err(e) => e.to_string(),
        };
        assert!(e.contains("first rapid") && e.contains("index position"), "{kind}: the refusal names the first traverse: {e}");
    }
}

/// #3077: a thin tailstock centre (radius 5.7, under the first station's
/// radius 15) over the bar's end. The lathe post never traverses over
/// the end face, so a lathe drill is clear. A mill-turn's mill post
/// sends Z to the clearance height (Z 25), then X/Y from an unknown X/Y:
/// that traverse can cross the centre (top Z 60), so the upright law
/// convicts it - the blocking first-traverse verdict.
#[test]
fn a_thin_tailstock_centre_is_clear_on_a_lathe_and_convicts_a_mill_turn() {
    for kind in ["lathe", "mill-turn"] {
        let (world, mut history) = lathe_at(None, 0.0, kind);
        chuck(&world, &mut history);
        steel(&world, &mut history, "centre", [-4.0, -4.0, 22.0], [4.0, 4.0, 60.0], true);
        let (_, warns) = operation::generate(&world, &op(params(vec![], cross(6.0))), "")
            .unwrap_or_else(|e| panic!("{kind}: the cross drill under a thin centre generates: {e}"));
        let low = warns.iter().any(|w| matches!(w, Warning::FirstTraverseLow { fixture: true, .. }) && w.blocks_post());
        if kind == "lathe" {
            assert!(blocking(&warns).is_empty(), "lathe: the lathe post never crosses the centre: {warns:?}");
        } else {
            assert!(low, "mill-turn: the mill post's X/Y traverse at Z25 crosses the centre: {warns:?}");
        }
    }
}

/// #3077: the lathe post's first G00 X Z goes to the first move's END,
/// the R-plane point (radius 15 at Z 10 here), not to the kernel's
/// standoff. A collar that turns with the part over the bar's end,
/// flush with that radius, only touches the band the rapid can pass: it
/// is clear. One 1 mm proud of it stands in the band: refused by name.
#[test]
fn a_turning_collar_flush_with_the_first_point_is_clear_and_one_proud_of_it_is_refused() {
    let (world, mut history) = lathe(None);
    steel(&world, &mut history, "collar", [-15.0, -15.0, 22.0], [15.0, 15.0, 30.0], false);
    let (_, warns) = operation::generate(&world, &op(params(vec![], cross(6.0))), "")
        .unwrap_or_else(|e| panic!("a flush collar is only touched by the first rapid: {e}"));
    assert!(blocking(&warns).is_empty(), "a flush collar convicts: {warns:?}");
    let (world, mut history) = lathe(None);
    steel(&world, &mut history, "collar", [-16.0, -16.0, 22.0], [16.0, 16.0, 30.0], false);
    let e = match operation::generate(&world, &op(params(vec![], cross(6.0))), "") {
        Ok((_, w)) => panic!("a collar proud of the first point generated: {w:?}"),
        Err(e) => e.to_string(),
    };
    assert!(e.contains("first rapid") && e.contains("turns with the part") && e.contains("16.000"), "{e}");
}
