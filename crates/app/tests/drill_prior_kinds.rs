//! Issue #89 review fixes: which predecessors' removal a drill may
//! credit. On a lathe only the Facing kind (a nose that never undercuts)
//! is credited - a boring bar cut as a disc would clear the core it never
//! touched; a drill predecessor is cut with its point cone, not a flat
//! disc at the tip; and an axial drill's stamp carries the stock it read.
//! Provenance: our own fixtures, specced from the Prime Directive
//! (crediting removal that did not happen is a collision); no vendor
//! material.
//!
//! Gate map (#89): covers app::rest (Ahead, PriorPlan), app::operation
//! (drill_prior, the Drill stamp).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

fn tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog")
}

fn add(world: &World, h: &mut History, name: &str, t: Tool) {
    h.execute(world, AddTool::new(world, name, t).expect("tool"));
}

/// A twist drill of diameter `d` (DR-0600's 118 deg point, long flute).
fn drill_tool(d: f64) -> Tool {
    Tool { diameter: d, shank_d: d, flute_len: 30.0, ..tool("DR-0600") }
}

fn drill(radius: f64, entry: f64, floor: f64, r: f64) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: entry, floor_z: floor, through: false, stepped: false };
    OpParams::Drill {
        break_through: None,
        break_through_from: Default::default(),
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(r),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
        picks: Default::default(),
    }
}

fn create(world: &World, h: &mut History, name: &str, tool: &str, src: &str, p: OpParams) {
    let c = CreateOperation::new(world, name, tool, SrcRef::Entity(src.into()), p);
    h.execute(world, c.expect("create"));
}

fn generate(world: &World, h: &mut History, name: &str) -> Result<(), String> {
    h.execute(world, GenerateOperation::new(world, name).map_err(|e| format!("{e:?}"))?);
    let e = model::find_by_name(world, name).ok_or("no op")?;
    match operation::status_of(world, &e) {
        OpStatus::Ok => Ok(()),
        _ => {
            let op = model::operation_of(world, &e).ok_or("no op")?;
            Err(operation::generate(world, &op, name).err().map_or_else(|| "not Ok".into(), |e| e.to_string()))
        }
    }
}

fn status(world: &World, name: &str) -> OpStatus {
    model::find_by_name(world, name).map_or(OpStatus::Never, |e| operation::status_of(world, &e))
}

fn box_stock(world: &World, h: &mut History, top: f64) {
    let at = Location { pos: Point3::new(0.0, 0.0, -10.0), ..Default::default() };
    let s = SetStock::new(world, StockShape::Box { dx: 40.0, dy: 40.0, dz: top + 10.0 }, at);
    h.execute(world, s.expect("stock"));
}

/// A 40 x 40 plate, z -10..0, under a box stock z -10..`stock_top`.
fn plate_world(stock_top: f64) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let plate = cube(Point3::new(-20.0, -20.0, -10.0), Point3::new(20.0, 20.0, 0.0));
    h.execute(&world, ImportModel::from_mesh("plate", plate));
    box_stock(&world, &mut h, stock_top);
    (world, h)
}

/// Review fix 2: a 10 mm pilot to tip Z 5 leaves its 118 deg point cone
/// standing - at r 4 it reaches Z 5 + 4 / tan 59 deg (about 7.4), so an
/// 8 mm drill with R 6 rapids into it.
#[test]
fn a_pilot_drills_point_cone_still_stands_over_a_smaller_drills_r_plane() {
    let (world, mut h) = plate_world(20.0);
    add(&world, &mut h, "D10", drill_tool(10.0));
    add(&world, &mut h, "D8", drill_tool(8.0));
    create(&world, &mut h, "Op1", "D10", "plate", drill(5.0, 20.0, 5.0, 21.0));
    generate(&world, &mut h, "Op1").expect("the pilot generates");
    create(&world, &mut h, "Op2", "D8", "plate", drill(4.0, 20.0, 1.0, 6.0));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("R 6 under the pilot's cone generated"));
    assert!(e.contains("R plane Z 6.000"), "{e}");
}

/// Review fix 5: an axial drill reads the stock (#84, #91), so a stock
/// change makes it Stale.
#[test]
fn an_axial_drill_goes_stale_when_the_stock_changes() {
    let (world, mut h) = plate_world(20.0);
    add(&world, &mut h, "D4", tool("DR-0400"));
    create(&world, &mut h, "Op1", "D4", "plate", drill(2.0, 20.0, 12.0, 21.0));
    generate(&world, &mut h, "Op1").expect("the drill generates");
    assert_eq!(status(&world, "Op1"), OpStatus::Ok);
    box_stock(&world, &mut h, 20.5);
    assert_eq!(status(&world, "Op1"), OpStatus::Stale, "the drill was judged against the old stock");
}

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / segs as f64;
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

/// Review fix B1/B2: a bar r10 (z -20..22) bored r7 to Z 10 in a solid
/// box stock, on a lathe. The boring bar only rides the bore's wall - cut
/// as a full-diameter disc on the y = 0 strip and revolved, it would
/// "clear" the core the bar never touched, and the drill's R plane in the
/// bore would pass under the uncut stock.
#[test]
fn a_turned_boring_op_ahead_of_a_lathe_axial_drill_is_not_credited() {
    let world = new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), ..Location::default() };
    let s = SetStock::new(&world, StockShape::Box { dx: 24.0, dy: 24.0, dz: 42.0 }, at);
    h.execute(&world, s.expect("stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        ..Default::default()
    };
    h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    add(&world, &mut h, "D4", tool("DR-0400"));
    add(&world, &mut h, "BAR", Tool { diameter: 13.6, shank_d: 13.6, ..tool("BR-1000") });
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 22.0), (7.0, 22.0), (7.0, 10.0), (0.0, 10.0)], 64);
    h.execute(&world, ImportModel::from_mesh("bar", bar));
    let bore = OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(22.0),
        bottom_z: Height::fixed(10.5),
        feed: 120.0,
        plunge_feed: 40.0,
        picks: Default::default(),
    };
    create(&world, &mut h, "Op1", "BAR", "bar", bore);
    generate(&world, &mut h, "Op1").expect("the boring generates");
    create(&world, &mut h, "Op2", "D4", "bar", drill(2.0, 10.0, 2.0, 12.0));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("the boring bar credited the core"));
    assert!(e.contains("stock"), "{e}");
}

// #89: a turning kind ahead on a mill-turn is read no more generously than on
// a lathe - the column grid cannot read its undercuts either.
#[test]
fn a_turned_boring_op_ahead_of_a_mill_turn_axial_drill_is_not_credited() {
    let world = new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -20.0), ..Location::default() };
    let s = SetStock::new(&world, StockShape::Box { dx: 24.0, dy: 24.0, dz: 42.0 }, at);
    h.execute(&world, s.expect("stock"));
    let lathe = Machine {
        model: "TestBench mill-turn".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("mill-turn".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        ..Default::default()
    };
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(lathe)).expect("mill-turn"));
    add(&world, &mut h, "D4", tool("DR-0400"));
    add(&world, &mut h, "BAR", Tool { diameter: 13.6, shank_d: 13.6, ..tool("BR-1000") });
    let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 22.0), (7.0, 22.0), (7.0, 10.0), (0.0, 10.0)], 64);
    h.execute(&world, ImportModel::from_mesh("bar", bar));
    let bore = OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(22.0),
        bottom_z: Height::fixed(10.5),
        feed: 120.0,
        plunge_feed: 40.0,
        picks: Default::default(),
    };
    create(&world, &mut h, "Op1", "BAR", "bar", bore);
    generate(&world, &mut h, "Op1").expect("the boring generates");
    create(&world, &mut h, "Op2", "D4", "bar", drill(2.0, 10.0, 2.0, 12.0));
    let e = generate(&world, &mut h, "Op2").err().unwrap_or_else(|| panic!("on a mill-turn the boring bar credited the core"));
    assert!(e.contains("stock"), "{e}");
}
