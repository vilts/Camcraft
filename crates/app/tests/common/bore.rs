//! The boring suites' shared document (#109 split it out of
//! turning_bore.rs, which stood at the 400-line law): a bored bar and a
//! plain wall on a mounted lathe, the lathe steel on the rack, and the
//! axial pilot drill a boring bar's approach runs down (#103).
//!
//! Provenance: our own fixtures, specced from the turning kernel's
//! design notes; no vendor material consulted.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::world::new_world;

pub const TAU: f64 = std::f64::consts::TAU;
pub const BLANK: f64 = 10.0;
pub const WALL: f64 = 6.0;
pub const FLOOR: f64 = -20.0;
pub const D: f64 = 0.4;

pub fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

pub fn add_tool(world: &World, history: &mut History, name: &str) {
    let cmd = AddTool::new(world, name, catalog_tool(name));
    history.execute(world, cmd.expect("catalog tool"));
}

pub fn status(world: &World, name: &str) -> OpStatus {
    let e = model::find_by_name(world, name).expect("entity exists");
    camcraft_app::operation::status_of(world, &e)
}

pub fn cache(world: &World, name: &str) -> Option<camcraft_app::schema::Toolpath> {
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e))
}

pub fn rho(q: Point3) -> f64 {
    (q.x * q.x + q.y * q.y).sqrt()
}

/// The closed-solid revolve the turning suites share.
pub fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
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

/// The bored bar: OD 10, an open hole of radius 6 from the top down
/// to a floor at -20, on a bar long past the hole so the axis fit
/// reads a revolve, not a dome.
pub fn bored() -> Mesh {
    revolve(&[
        (0.0, -50.0),
        (BLANK, -50.0),
        (BLANK, 50.0),
        (WALL, 50.0),
        (WALL, FLOOR),
        (0.0, FLOOR),
    ])
}

/// The plain bar the hole-existence door must refuse.
pub fn wall() -> Mesh {
    revolve(&[(0.0, -50.0), (BLANK, -50.0), (BLANK, 50.0), (0.0, 50.0)])
}

pub fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

/// A document the boring kind lives in: the bored bar AND a smooth
/// wall in bar-sized stock, the lathe mounted, the catalog boring
/// bar (and the other lathe steel) on the rack.
pub fn bore_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            // Bar-sized stock (#103): the blank's own diameter over
            // its Z span - the turning kernels never cut stock beyond
            // the part (#130), so a wider blank is a real crash.
            camcraft_app::stock::StockShape::Cylinder { diameter: 2.0 * BLANK, height: 100.0 },
            Location { pos: Point3::new(0.0, 0.0, -50.0), ..Location::default() },
        )
        .expect("bar stock"),
    );
    let cmd = LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe")));
    history.execute(&world, cmd.expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    add_tool(&world, &mut history, "BR-1000");
    add_tool(&world, &mut history, "TN-1600-0R8");
    add_tool(&world, &mut history, "EM-0400-2FL");
    // The pilots' long-series drills: 70 mm of flute for the 62 mm
    // hole - a Ø11.2 that clears the bar's Ø10 body (#103 review B1), and
    // a Ø4 it cannot pass.
    let mut long = catalog_tool("DR-0400");
    long.flute_len = 70.0;
    history.execute(&world, AddTool::new(&world, "DR-0400-L", long.clone()).expect("a sane drill"));
    long.diameter = 11.2;
    long.shank_d = 11.2;
    history.execute(&world, AddTool::new(&world, "DR-1100-L", long).expect("a sane drill"));
    history.execute(&world, ImportModel::from_mesh("bar", bored()));
    history.execute(&world, ImportModel::from_mesh("wall", wall()));
    (world, history)
}

pub fn bore_params() -> OpParams {
    OpParams::BoringTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(60.0),
        top_z: Height::fixed(40.0),
        bottom_z: Height::fixed(-10.0),
        feed: 120.0,
        plunge_feed: 40.0,
    }
}

pub fn create_on(world: &World, history: &mut History, name: &str, tool: &str, src: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, tool, SrcRef::Entity(src.into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

pub fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    create_on(world, history, name, "BR-1000", "bar", params);
}

/// The pilot a boring bar needs (#103): an axial drill ahead (AX0 -
/// program order reads names, so it sorts ahead of BR1 and ST), Ø11.2
/// down the bar's axis from its top to Z -16 - its point clear under the ride's -10
/// window, over the model's -20 floor, wide enough for the bar's Ø10
/// body. The bar stock is solid; the bar's approach runs down this
/// hole, and the rapid law credits it.
pub fn pilot(world: &World, history: &mut History) {
    pilot_of(world, history, "DR-1100-L", 5.65);
}

pub fn pilot_of(world: &World, history: &mut History, drill: &str, radius: f64) {
    let hole = Hole { xy: Point2::new(0.0, 0.0), radius, entry_z: 50.0, floor_z: -16.0, through: false, stepped: false };
    let params = OpParams::Drill {
        clearance_z: Height::fixed(60.0),
        retract_z: None,
        top_z: Height::fixed(50.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![hole],
        cross: vec![],
    };
    create_on(world, history, "AX0", drill, "bar", params);
    assert_eq!(status(world, "AX0"), OpStatus::Ok, "the pilot drills");
}
