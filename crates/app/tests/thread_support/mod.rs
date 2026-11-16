//! Shared fixtures of the thread-milling suites (#894, #992): a disc with
//! a blind tap-drill hole modelled deeper than its thread, a box stock,
//! the catalog's thread mills, and a Drill operation ahead of the thread
//! that opens the hole the way a shop does - a tap drill at the minor
//! diameter, deeper than the thread so the mill clears its point.
//!
//! Provenance: ISO 724 thread sizes (M8 x 1.25: minor Ø6.647; M8 x 1:
//! minor Ø6.917) and public handbook thread milling; our own fixtures.
#![allow(dead_code)] // each suite uses its own share

// #985: the boss fixtures of the external-thread suite.
pub mod boss;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{ThreadHand, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::simulate_document;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

pub const TOP: f64 = 20.0;
pub const BOTTOM: f64 = 0.0;
/// The thread's blind floor: 12 mm of thread.
pub const FLOOR: f64 = 8.0;
/// The modelled tap-drill hole's floor (#992): deeper than the thread,
/// so the drill's point stays under the thread mill.
pub const DRILL_FLOOR: f64 = 4.0;
/// M8 x 1.25's minor radius (ISO 724: D1 = 8 - 1.0825 x 1.25).
pub const R_M8: f64 = 3.3235;
/// M8 x 1 (fine): D1 = 8 - 1.0825.
pub const R_M8_FINE: f64 = 3.45875;
pub const MULTI: &str = "TM-0600-P125";
pub const SINGLE: &str = "TMS-0600-P050-150";
pub const DRILL: &str = "TAP-DRILL";

/// A Ø36 disc from z 0 to 20 with a blind round hole of `radius` from
/// the top down to `DRILL_FLOOR`, walls in 96 flats. Outward-wound.
pub fn disc_with_hole(radius: f64) -> Mesh {
    let n = 96;
    let at = |r: f64, k: usize, z: f64| {
        let a = std::f64::consts::TAU * (k % n) as f64 / n as f64;
        Point3::new(r * a.cos(), r * a.sin(), z)
    };
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    for k in 0..n {
        let (o, i) = (18.0, radius);
        quad(at(o, k, BOTTOM), at(o, k + 1, BOTTOM), at(o, k + 1, TOP), at(o, k, TOP), &mut f);
        quad(at(o, k, TOP), at(o, k + 1, TOP), at(i, k + 1, TOP), at(i, k, TOP), &mut f);
        quad(at(i, k + 1, DRILL_FLOOR), at(i, k, DRILL_FLOOR), at(i, k, TOP), at(i, k + 1, TOP), &mut f);
        f.push(Triangle::new(Point3::new(0.0, 0.0, DRILL_FLOOR), at(i, k, DRILL_FLOOR), at(i, k + 1, DRILL_FLOOR)));
        f.push(Triangle::new(Point3::new(0.0, 0.0, BOTTOM), at(o, k + 1, BOTTOM), at(o, k, BOTTOM)));
    }
    Mesh::from_facets(f)
}

/// The thread's hole: the minor wall of `radius`, threaded down to `FLOOR`.
pub fn hole(radius: f64) -> Hole {
    Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: TOP, floor_z: FLOOR, through: false, stepped: false }
}

pub fn catalog_tool(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

/// A 118-degree drill of `diameter`, long enough for the disc.
pub fn tap_drill(diameter: f64) -> Tool {
    Tool { diameter, shank_d: diameter, flute_len: 25.0, ..catalog_tool("DR-0600") }
}

/// The tap drill a modelled hole of `radius` takes: its diameter to the
/// tenth below, so the drill never touches the drawn wall.
pub fn tap_drill_for(radius: f64) -> f64 {
    ((radius * 2.0 - 0.02) * 10.0).floor() / 10.0
}

pub fn thread(radius: f64, pitch: Option<f64>, hand: ThreadHand, climb: bool) -> OpParams {
    OpParams::Thread {
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        infeed: 0.25,
        feed: 300.0,
        plunge_feed: 150.0,
        climb,
        holes: vec![hole(radius)],
        pitch,
        hand,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
    }
}

/// A Drill operation's params: the hole at the origin, `radius` wide,
/// drilled with its point to `floor`.
pub fn drill(radius: f64, floor: f64) -> OpParams {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: TOP, floor_z: floor, through: false, stepped: false };
    OpParams::Drill {
        clearance_z: Height::fixed(TOP + 10.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        cycle: CycleChoice::Drill,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    }
}

/// A world holding the disc (hole of `model_radius`), a box stock
/// around it and the named tools; nothing machined yet.
pub fn bare_world(model_radius: f64, tools: &[(&str, Tool)]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock_at = Location { pos: Point3::new(0.0, 0.0, BOTTOM), ..Location::default() };
    let stock = SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: TOP - BOTTOM }, stock_at);
    history.execute(&world, stock.expect("box stock"));
    for (name, tool) in tools {
        history.execute(&world, AddTool::new(&world, name, tool.clone()).expect("tool"));
    }
    history.execute(&world, ImportModel::from_mesh("disc", disc_with_hole(model_radius)));
    (world, history)
}

/// Add a tap drill of `diameter` and a generated Drill operation
/// "Drill1" whose point reaches `floor`, at the end of the program.
pub fn drill_ahead(world: &World, history: &mut History, diameter: f64, floor: f64) {
    history.execute(world, AddTool::new(world, DRILL, tap_drill(diameter)).expect("drill"));
    let op = CreateOperation::new(world, "Drill1", DRILL, SrcRef::Entity("disc".into()), drill(diameter * 0.5, floor));
    history.execute(world, op.expect("create drill"));
    history.execute(world, GenerateOperation::new(world, "Drill1").expect("generate drill"));
}

/// [`bare_world`] with the hole already tap-drilled to `DRILL_FLOOR` by
/// a Drill operation ahead (#992: a thread mill needs the hole open).
pub fn world_with(model_radius: f64, tools: &[(&str, Tool)]) -> (World, History) {
    let (world, mut history) = bare_world(model_radius, tools);
    drill_ahead(&world, &mut history, tap_drill_for(model_radius), DRILL_FLOOR);
    (world, history)
}

pub fn make(world: &World, history: &mut History, tool: &str, params: OpParams) -> Toolpath {
    let op = CreateOperation::new(world, "Thread1", tool, SrcRef::Entity("disc".into()), params);
    history.execute(world, op.expect("create op"));
    history.execute(world, GenerateOperation::new(world, "Thread1").expect("generate op"));
    let e = model::find_by_name(world, "Thread1").expect("op");
    model::toolpath_of(world, &e).expect("cached path").path
}

pub fn blocking(world: &World) -> Vec<String> {
    let e = model::find_by_name(world, "Thread1").expect("op");
    let cache = model::toolpath_of(world, &e).expect("cached path");
    cache.warnings.iter().filter(|w| w.blocks_post()).map(ToString::to_string).collect()
}

pub fn assert_clean(world: &World) {
    assert_eq!(blocking(world), Vec::<String>::new(), "the generator's own verdict");
    assert_eq!(posting::verification_gate(world), Vec::<String>::new(), "the post gate");
    let sim = simulate_document(world).expect("the thread program simulates");
    assert_eq!(sim.dev.gouge_cells, 0, "worst {:.3} mm at {:?}", sim.dev.max_gouge, sim.dev.max_gouge_at);
}
