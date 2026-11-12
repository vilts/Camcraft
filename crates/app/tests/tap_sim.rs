//! A tap in the program sim (#266). CAD models a tapped hole at the
//! thread's minor (tap-drill) size, and a tap only cuts the thread band
//! outside that wall, so the program sim sweeps a Tap-kind tool at its
//! thread's core, not at its full major diameter. On a hole modelled at
//! tap-drill size the sim must find no gouge; a tap whose core is wider
//! than the hole under it must still be a gouge.
//!
//! Gate map (M4.4): covers app::stock_sim, sim::stock_sim (plan_program's tap atom), app::operation (tool_law::cutting_radius).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

const TOP: f64 = 20.0;
const BOTTOM: f64 = -10.0;
/// 12 mm deep: inside the catalog tap's 20 mm flute, so the reach law
/// (a shank wider than the hole) never enters.
const HOLE_FLOOR: f64 = 8.0;
/// The catalog's M6 x 1 tap: Ø6 major, so its thread core is
/// 6 - 2 x 0.5413 = Ø4.917 and its tap drill Ø5.
const TAP: &str = "TAP-M6";

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 block from z -10 to 20 with a blind square hole of side
/// `2 x half_width` from the top down to `HOLE_FLOOR`. Outward-wound.
fn block_with_hole(half_width: f64) -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, BOTTOM), Point3::new(20.0, 20.0, TOP));
    // The outer box without its top face; the top ring is built below.
    f.extend(outer.facets[4..12].iter().cloned());
    f.extend(outer.facets[0..2].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let h = half_width;
    let i = [(-h, -h), (h, -h), (h, h), (-h, h)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, TOP);
    for k in 0..4 {
        let n = (k + 1) % 4;
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-h, -h, HOLE_FLOOR), Point3::new(h, h, TOP));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// The one hole every test machines, at the block's center.
fn hole(radius: f64) -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: TOP, floor_z: HOLE_FLOOR, through: false, stepped: false }
}

/// A world holding the block (hole of `half_width`), a box stock that
/// is exactly the block, and one operation `params` with `tool`,
/// generated through the command path as the post would find it.
fn world_with_op(half_width: f64, tool_name: &str, tool: Tool, params: OpParams) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock_at = Location { pos: Point3::new(0.0, 0.0, BOTTOM), ..Location::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: TOP - BOTTOM }, stock_at);
    history.execute(&world, stock.expect("box stock"));
    history.execute(&world, AddTool::new(&world, tool_name, tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("block", block_with_hole(half_width)));
    let op = CreateOperation::new(&world, "Op1", tool_name, SrcRef::Entity("block".into()), params);
    history.execute(&world, op.expect("create op"));
    history.execute(&world, GenerateOperation::new(&world, "Op1").expect("generate op"));
    let e = model::find_by_name(&world, "Op1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cached path");
    assert!(!cache.path.moves.is_empty(), "the operation generated moves");
    (world, history)
}

/// The generated op's warnings that block the post: the generator's
/// own verdict, which the post gate reads beside the program sim.
fn blocking_warnings(world: &World) -> Vec<String> {
    let e = model::find_by_name(world, "Op1").expect("op");
    let cache = model::toolpath_of(world, &e).expect("cached path");
    cache.warnings.iter().filter(|w| w.blocks_post()).map(ToString::to_string).collect()
}

/// The catalog M6 tap's Tap cycle in a hole of `half_width`.
fn tapped_world(half_width: f64) -> (World, History) {
    let tool = catalog().into_iter().find(|(n, _)| *n == TAP).map(|(_, t)| t).expect("catalog tap");
    let params = OpParams::Drill {
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle: CycleChoice::Tap,
        peck: 0.0,
        dwell: 0.0,
        feed: 600.0,
        plunge_feed: 600.0,
        holes: vec![hole(half_width)],
        cross: vec![],
    };
    world_with_op(half_width, TAP, tool, params)
}

#[test]
fn a_tap_in_a_hole_modelled_at_tap_drill_size_is_not_a_gouge() {
    // #266: a Ø5 hole (M6 x 1's tap drill) under an M6 tap. The sim's
    // full Ø6 disc read the thread band as a 12 mm gouge down the hole
    // wall, which blocks the post; the Ø4.917 core fits the hole.
    let (world, _history) = tapped_world(2.5);
    let sim = simulate_document(&world).expect("a fresh tap program simulates");
    assert_eq!(
        sim.dev.gouge_cells, 0,
        "a tap riding a tap-drill-size hole cuts only its thread: worst {:.3} mm at {:?}",
        sim.dev.max_gouge, sim.dev.max_gouge_at
    );
}

#[test]
fn a_tap_wider_than_its_hole_is_still_a_gouge() {
    // Guards the other side of #266: sweeping the core must not let a
    // tap into metal it cannot enter. A Ø3 hole under the M6 tap: the
    // Ø4.917 core cuts the wall, so the post stays blocked.
    let (world, _history) = tapped_world(1.5);
    assert!(!blocking_warnings(&world).is_empty(), "the generator still convicts the tap");
    let sim = simulate_document(&world).expect("a fresh tap program simulates");
    assert!(sim.dev.gouge_cells > 0, "the tap's core cuts below the model around a Ø3 hole");
    assert!(sim.dev.max_gouge > 1.0, "the gouge runs down the hole wall: {:.3} mm", sim.dev.max_gouge);
}
