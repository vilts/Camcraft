//! #303 through the post gate: a 2D Chamfer or a Deburr on a finished
//! part is judged against the break it was asked to cut, not the
//! model's sharp edge. The defaults post; a ride deeper than its stated
//! break, or a break on an edge the op never picked, still refuses as a
//! gouge by name; the declared break survives save and load.
//!
//! Gate map (#303): covers app::stock_sim, sim::stock_sim (simulate_document), app::posting
//! (verification_gate), app::project (the breaks key), motion::machining2d
//! (chamfer), motion::finishing3d (deburr).

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// The 90-degree chamfer mill both kinds ride.
const CONE: &str = "CM-0600-90";

/// The kind's own defaults, as a new operation seeds them.
fn seeded(ty: OpType) -> OpParams {
    let base = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    operation::retype_params(&base, ty.ordinal())
}

fn brick(x0: f64) -> camcraft_app::geometry::Mesh {
    cube(Point3::new(x0, -10.0, 0.0), Point3::new(x0 + 40.0, 10.0, 10.0))
}

fn add_cone(world: &World, h: &mut History) {
    let tool = catalog().into_iter().find(|(n, _)| *n == CONE).map(|(_, t)| t).expect("catalog cone");
    h.execute(world, AddTool::new(world, CONE, tool).expect("tool"));
}

fn generate(world: &World, h: &mut History, name: &str, model_name: &str, p: OpParams) {
    h.execute(world, CreateOperation::new(world, name, CONE, SrcRef::Entity(model_name.into()), p).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// The issue's job: the 40 x 20 x 10 brick, the stock IS the brick (a
/// finished part - nothing but the break is cut), one op on it.
fn finished_brick(p: OpParams) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("brick", brick(-20.0)));
    let stock = SetStock::new(&world, StockShape::FromModel { source: "brick".into() }, Default::default());
    h.execute(&world, stock.expect("stock"));
    add_cone(&world, &mut h);
    generate(&world, &mut h, "OP", "brick", p);
    world
}

/// Rewrite the op's cached moves (what a generator bug would post) and
/// keep everything else it declared.
fn tamper(world: &World, name: &str, f: impl Fn(Point3) -> Point3) {
    let e = model::find_by_name(world, name).expect("op");
    let mut t = model::toolpath_of(world, &e).expect("cached");
    for m in &mut t.path.moves {
        m.from = f(m.from);
        m.to = f(m.to);
    }
    let rebuilt = camcraft_app::schema::Toolpath::new(t.path, t.warnings);
    e.entity_view(world).set(rebuilt);
}

fn gouge_line(world: &World) -> Option<String> {
    posting::verification_gate(world).into_iter().find(|l| l.starts_with("gouge:"))
}

fn assert_posts(world: &World, tag: &str) {
    let gate = posting::verification_gate(world);
    assert!(gate.is_empty(), "{tag}: the gate refused its own break: {gate:?}");
    let path = std::env::temp_dir().join(format!("camcraft-303-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_setup(world, &path);
    let _ = std::fs::remove_file(&path);
    assert!(posted.is_ok(), "{tag}: {:?}", posted.err());
}

#[test]
fn a_default_2d_chamfer_on_a_finished_brick_posts() {
    let world = finished_brick(seeded(OpType::Chamfer2D));
    let e = model::find_by_name(&world, "OP").expect("op");
    let t = model::toolpath_of(&world, &e).expect("cached");
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    assert_eq!(t.path.breaks.len(), 1, "the chamfer declares its loop");
    assert_posts(&world, "chamfer");
}

#[test]
fn a_default_deburr_on_a_finished_brick_posts() {
    let world = finished_brick(seeded(OpType::Deburr));
    assert_posts(&world, "deburr");
    // The issue's own ride (width 2, tips z 6..8): its 2 mm band was
    // 1455 of 1544 convicted columns, the vertical corners the rest.
    let world = finished_brick(OpParams::Deburr {
        width: 2.0,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(6.0),
        feed: 500.0,
        plunge_feed: 100.0,
    });
    assert_posts(&world, "deburr-w2");
}

#[test]
fn a_chamfer_ridden_deeper_than_its_stated_break_still_refuses() {
    let world = finished_brick(seeded(OpType::Chamfer2D));
    tamper(&world, "OP", |p| Point3::new(p.x, p.y, p.z - 0.3));
    let line = gouge_line(&world).expect("an over-deep chamfer posted");
    assert!(line.contains("worst 0.30 mm"), "charged at its excess under the bevel: {line}");
}

#[test]
fn a_chamfer_on_an_edge_it_was_not_asked_to_break_still_refuses() {
    // Two bricks on a box stock as tall as they are; the chamfer picks
    // brick A, and its moves are carried onto brick B's edge.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("a", brick(-45.0)));
    h.execute(&world, ImportModel::from_mesh("b", brick(5.0)));
    let stock = SetStock::new(&world, StockShape::Box { dx: 110.0, dy: 40.0, dz: 10.0 }, Location::default());
    h.execute(&world, stock.expect("stock"));
    add_cone(&world, &mut h);
    generate(&world, &mut h, "OP", "a", seeded(OpType::Chamfer2D));
    assert!(gouge_line(&world).is_none(), "brick A's own chamfer is its break");
    tamper(&world, "OP", |p| Point3::new(p.x + 50.0, p.y, p.z));
    let line = gouge_line(&world).expect("a break on brick B nobody asked for posted");
    assert!(line.contains("worst 0.7"), "B's bevel is charged whole: {line}");
}

#[test]
fn the_declared_break_survives_save_and_load() {
    let world = finished_brick(seeded(OpType::Chamfer2D));
    let path = std::env::temp_dir().join(format!("camcraft-303-roundtrip-{}.ccp", std::process::id()));
    project::save(&world, &path).expect("save");
    let loaded = new_world();
    project::load(&loaded, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let before = model::toolpath_of(&world, &model::find_by_name(&world, "OP").expect("op")).expect("cached");
    let after = model::toolpath_of(&loaded, &model::find_by_name(&loaded, "OP").expect("op")).expect("cached");
    assert_eq!(after.path.breaks, before.path.breaks, "the break rode the file");
    assert!(gouge_line(&loaded).is_none(), "the loaded chamfer posts as it did");
}
