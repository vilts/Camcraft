//! Drill reach (issue #84): no drilling op - cross or axial - feeds a
//! drill deeper into material than its usable flute, measured from the
//! STOCK surface at the hole (past the flutes the shank feeds into
//! uncut stock: a collision), and the cross drill's holder is proven
//! clear of the stock on every move, the end-face swing included. Each
//! is refused by name, never posted.
//!
//! Gate map (M4.4): covers app::operation (drill_reach, the cross arm's
//! cross_spindle call, the Z arm of generate's Drill).
//! Lane shape (M4.4): all tests here run in seconds.

mod punched_bar;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CrossHole, CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{IDENTITY_Q, Mesh, Point3, Triangle, cube};
use camcraft_app::machine::{AxisLimits, Machine};
use camcraft_app::operation::{self, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const PART_R: f64 = 10.0;

/// The finished bar of radius `r`, z in [-80, 20] (`punched_bar::FOOT`), revolved about world
/// Z, with this suite's cross holes modelled (issue #117: the part law
/// refuses a hole the model lacks): +X 6 deep, -X 3 deep, both at z0.
fn bar(r: f64) -> Mesh {
    punched_bar::punched_bar(r, punched_bar::FOOT, punched_bar::TOP, &[cross_hole(r, -1.0, -6.0), cross_hole(-r, 1.0, -3.0)])
}

/// A closed solid of revolution about world Z from its (r, z) meridian.
fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let segs = 64;
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
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

fn dr0400(grind: impl Fn(&mut Tool)) -> Tool {
    let mut t = catalog().into_iter().find(|(n, _)| *n == "DR-0400").map(|(_, t)| t).expect("catalog entry");
    grind(&mut t);
    t
}

/// A `word` machine (live-tooling rotaries stated) with the r10 bar,
/// the raw r20 bar as stock when `raw`, and DR-0400 re-ground by `grind`.
fn turret_world(word: &str, raw: bool, grind: impl Fn(&mut Tool)) -> (World, History) {
    turret_world_on(word, raw, bar(PART_R), dr0400(grind))
}

/// `turret_world` on any part `mesh`, with `tool` filed as DR-0400.
fn turret_world_on(word: &str, raw: bool, mesh: Mesh, tool: Tool) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    if raw {
        let at = Location { pos: Point3::new(0.0, 0.0, -20.0), rot: IDENTITY_Q };
        let s = SetStock::new(&world, StockShape::Cylinder { diameter: 40.0, height: 40.0 }, at);
        history.execute(&world, s.expect("stock"));
    }
    let r = |name: &str, axis: [f64; 3]| AxisLimits { name: name.into(), rotary: true, axis: Some(axis), ..Default::default() };
    let axes = vec![r("A", [1.0, 0.0, 0.0]), r("B", [0.0, 1.0, 0.0]), r("C", [0.0, 0.0, 1.0])];
    let m = Machine { model: format!("TestBench {word}"), kind: Some(word.into()), axes, ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "bench.mch".into(), Some(m)).expect("machine"));
    history.execute(&world, AddTool::new(&world, "DR-0400", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", mesh));
    (world, history)
}

fn cross_hole(x: f64, dir: f64, floor_d: f64) -> CrossHole {
    cross_hole_at(Point3::new(x, 0.0, 0.0), dir, floor_d)
}

/// A hole along `dir` X at `origin`, Ø4.
fn cross_hole_at(origin: Point3, dir: f64, floor_d: f64) -> CrossHole {
    CrossHole {
        origin,
        dir: Point3::new(dir, 0.0, 0.0),
        radius: 2.0,
        entry_d: 0.0,
        floor_d,
        through: false,
    }
}

fn drill_op(src: &str, top: f64, clear: f64, holes: Vec<Hole>, cross: Vec<CrossHole>) -> Operation {
    Operation {
        tool: "DR-0400".into(),
        src: SrcRef::Entity(src.into()),
        params: OpParams::Drill {
            break_through: None,
            clearance_z: Height::fixed(top + clear),
            retract_z: None,
            top_z: Height::fixed(top),
            bottom_z: None,
            cycle: CycleChoice::Auto,
            peck: 0.0,
            dwell: 0.0,
            feed: 120.0,
            plunge_feed: 80.0,
            holes,
            cross,
        },
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

/// One +X hole 6 mm into the r10 part: on the raw r20 bar its mouth
/// stands at r20, 16 mm above the floor - past DR-0400's 12 mm flute.
fn raw_cross_op() -> Operation {
    drill_op("bar", 20.0, 5.0, vec![], vec![cross_hole(PART_R, -1.0, -6.0)])
}

#[test]
fn a_short_flute_cross_drill_on_raw_stock_wider_than_the_part_is_refused_by_name() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = turret_world(word, true, |_| {});
        let err = operation::generate(&world, &raw_cross_op(), "").err();
        // #677: the refusal marks the modelled mouth, in world.
        assert_eq!(err.as_ref().and_then(|e| e.location()), Some(Point3::new(10.0, 0.0, 0.0)), "{word}: {err:?}");
        let e = err.map(|e| e.to_string());
        let e = e.unwrap_or_else(|| panic!("{word}: a 16 mm hole drilled on a 12 mm flute"));
        for s in ["cross hole 0 at (10.000, 0.000, 0.000)", "16.000 mm deep from the stock surface", "DR-0400", "12.000 mm"] {
            assert!(e.contains(s), "{word}: the refusal names '{s}': {e}");
        }
    }
}

#[test]
fn the_same_cross_hole_drills_with_a_flute_long_enough() {
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = turret_world(word, true, |t| t.flute_len = 16.0);
        let r = operation::generate(&world, &raw_cross_op(), "");
        assert!(r.is_ok(), "{word}: a 16 mm flute reaches the 16 mm hole: {:?}", r.err());
        // On the finished part alone the hole is 6 mm deep: the catalog
        // drill reaches it.
        let (world, _h) = turret_world(word, false, |_| {});
        let r = operation::generate(&world, &raw_cross_op(), "");
        assert!(r.is_ok(), "{word}: the part-only 6 mm hole drills: {:?}", r.err());
    }
}

/// Opposite stations: the swing between them runs over the end face on
/// the new station's pose, the holder trailing `flute + shank` behind
/// the tip. With 2 mm of shank the Ø44 holder's face stands 14 mm
/// behind a tip that starts the swing at r15: right over the bar, its
/// lower edge 22 mm under the swing plane (z25) - deep in the z20 end
/// face. Refused by name (the swing is not raised); the catalog's 40 mm
/// shank, or a swing plane high enough for the holder, drills.
#[test]
fn a_holder_that_would_sweep_the_end_face_on_the_swing_is_refused_by_name() {
    let pair = vec![cross_hole(PART_R, -1.0, -3.0), cross_hole(-PART_R, 1.0, -3.0)];
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = turret_world(word, false, |t| t.shank_len = 2.0);
        let e = operation::generate(&world, &drill_op("bar", 20.0, 5.0, vec![], pair.clone()), "").err();
        let e = e.map(|e| e.to_string()).unwrap_or_else(|| panic!("{word}: the holder swept the end face"));
        for s in ["DR-0400's holder", "dia. 44.000", "14.000 mm behind the tip", "inside material"] {
            assert!(e.contains(s), "{word}: the refusal names '{s}': {e}");
        }
        // The swing raised by the user clears it: z45, the holder's
        // lower edge at z23 above the z20 end face.
        let r = operation::generate(&world, &drill_op("bar", 20.0, 25.0, vec![], pair.clone()), "");
        assert!(r.is_ok(), "{word}: a swing above the holder drills: {:?}", r.err());
        let (world, _h) = turret_world(word, false, |_| {});
        let r = operation::generate(&world, &drill_op("bar", 20.0, 5.0, vec![], pair.clone()), "");
        assert!(r.is_ok(), "{word}: the catalog shank keeps the holder off the bar: {:?}", r.err());
    }
}

/// A 40 x 40 x 20 block (top at z20) on a mill, with `stock` boxed over
/// it when given (its top Z).
fn mill_world(stock_top: Option<f64>) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    if let Some(top) = stock_top {
        let at = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
        let s = SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 50.0, dz: top }, at);
        history.execute(&world, s.expect("stock"));
    }
    history.execute(&world, AddTool::new(&world, "DR-0400", dr0400(|_| {})).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("block", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0))));
    (world, history)
}

fn z_hole(floor_z: f64) -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius: 2.0, entry_z: 20.0, floor_z, through: false, stepped: false }
}

#[test]
fn an_axial_hole_deeper_than_the_flute_from_the_stock_top_is_refused_by_name() {
    // 15 mm from the part top: past the 12 mm flute.
    let (world, _h) = mill_world(None);
    let e = operation::generate(&world, &drill_op("block", 20.0, 10.0, vec![z_hole(5.0)], vec![]), "").err();
    let e = e.map(|e| e.to_string()).unwrap_or_else(|| panic!("a 15 mm hole drilled on a 12 mm flute"));
    for s in ["hole at (0.000, 0.000)", "15.000 mm deep from the stock surface", "DR-0400", "12.000 mm"] {
        assert!(e.contains(s), "the refusal names '{s}': {e}");
    }
    // 10 mm from the part top, but a 6 mm stock skin stands above it
    // and the cycle feeds from the stock top: 16 mm of material.
    let (world, _h) = mill_world(Some(26.0));
    let e = operation::generate(&world, &drill_op("block", 26.0, 10.0, vec![z_hole(10.0)], vec![]), "").err();
    let e = e.map(|e| e.to_string()).unwrap_or_else(|| panic!("a 16 mm hole through the skin drilled"));
    assert!(e.contains("16.000 mm deep from the stock surface"), "measured from the stock: {e}");
    // The same 10 mm hole with the stock faced to the part: it drills.
    let (world, _h) = mill_world(Some(20.0));
    let r = operation::generate(&world, &drill_op("block", 20.0, 10.0, vec![z_hole(10.0)], vec![]), "");
    assert!(r.is_ok(), "a 10 mm hole on a 12 mm flute drills: {:?}", r.err());
}

/// Review fix (#84, finding 2): the holder is measured against the
/// material in its own Z band. A stepped shaft - a Ø60 flange at z -20
/// to -10, then Ø20 to z40 - with one +X hole 3 mm into the Ø20
/// section: at the floor the Ø44 holder's face (14 mm behind the tip)
/// stands at r21, outside the Ø20 section but inside the flange's r30.
/// At z20 the holder (z -2 to 42) is nowhere near the flange: it
/// drills. At z0 the holder (z -22 to 22) hangs over the flange's face:
/// refused by name.
#[test]
fn a_holder_is_judged_against_the_stock_in_its_own_z_band() {
    // The r10 shaft carries the z20 hole modelled (issue #117); the
    // flange is its own shell, 0.5 mm below it.
    let shaft = || {
        let mut f = punched_bar::punched_bar(PART_R, -10.0, 40.0, &[cross_hole_at(Point3::new(PART_R, 0.0, 20.0), -1.0, -3.0)]).facets.to_vec();
        f.extend(revolve(&[(0.0, -20.0), (30.0, -20.0), (30.0, -10.5), (0.0, -10.5)]).facets.iter().copied());
        Mesh::from_facets(f)
    };
    let op = |z: f64| drill_op("bar", 40.0, 5.0, vec![], vec![cross_hole_at(Point3::new(PART_R, 0.0, z), -1.0, -3.0)]);
    for word in ["lathe", "mill-turn"] {
        let (world, _h) = turret_world_on(word, false, shaft(), dr0400(|t| t.shank_len = 2.0));
        let r = operation::generate(&world, &op(20.0), "");
        assert!(r.is_ok(), "{word}: the flange is far below the holder: {:?}", r.err());
        let e = operation::generate(&world, &op(0.0), "").err().map(|e| e.to_string());
        let e = e.unwrap_or_else(|| panic!("{word}: the holder swept the flange"));
        for s in ["DR-0400's holder", "inside material 30.000 mm out"] {
            assert!(e.contains(s), "{word}: the refusal names '{s}': {e}");
        }
    }
}

/// Review fix (#84, finding 3): an off-centre hole's reach is the wider
/// body's. Raw bar r20 over a stubby r18 part, a Ø3 drill (9 mm flute)
/// along -X at y15: its edge (y13.5) meets the raw surface at x14.756,
/// its Ø6 shank's edge (y12) at x16 - 1.244 mm further out. A floor
/// 8.5 mm below the drill's mouth is 9.744 mm below the shank's: past
/// the flute, refused by name. On a Ø3 shank the same hole drills.
/// Mill-turn only: the plain lathe's axis reader cannot read a part
/// this stubby as turned (its own refusal, not this law's).
#[test]
fn an_off_centre_hole_measures_reach_from_where_a_wider_shank_meets_the_raw_bar() {
    let grind = |shank: f64| dr0400(move |t| (t.diameter, t.shank_d, t.flute_len) = (3.0, shank, 9.0));
    let floor = -(8.5 - ((400.0_f64 - 13.5 * 13.5).sqrt() - 10.0));
    let mut hole = cross_hole_at(Point3::new(10.0, 15.0, 0.0), -1.0, floor);
    hole.radius = 1.5;
    let op = drill_op("bar", 20.0, 5.0, vec![], vec![hole]);
    let part = || punched_bar::punched_bar(18.0, punched_bar::FOOT, punched_bar::TOP, &[hole]);
    for word in ["mill-turn"] {
        let (world, _h) = turret_world_on(word, true, part(), grind(6.0));
        let e = operation::generate(&world, &op, "").err().map(|e| e.to_string());
        let e = e.unwrap_or_else(|| panic!("{word}: the Ø6 shank sat 1.244 mm inside the raw bar"));
        for s in ["cross hole 0", "9.744 mm deep from the stock surface", "9.000 mm"] {
            assert!(e.contains(s), "{word}: the refusal names '{s}': {e}");
        }
        let (world, _h) = turret_world_on(word, true, part(), grind(3.0));
        let r = operation::generate(&world, &op, "");
        assert!(r.is_ok(), "{word}: on a Ø3 shank the hole is 8.5 mm deep: {:?}", r.err());
    }
}
