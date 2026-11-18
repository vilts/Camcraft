//! Shared fixtures of the cross-drilling app suites (`drilling_cross`,
//! `drilling_cross_rapids`): the drilled bar mesh, its typed holes, the
//! box- and bar-stock worlds, the op builders. Not a suite itself.
//!
//! Provenance: our own hand-built bar (M5.2 F2), moved out of
//! drilling_cross.rs whole (#79 review: the 400-line law); no vendor
//! material.

#![allow(dead_code, unused_imports)]

pub use std::path::PathBuf;

pub use flecs_ecs::prelude::World;

pub use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, ImportModel,
    LoadMachine, SetStock,
};
pub use camcraft_app::drilling::CrossHole;
pub use camcraft_app::geo2d::Point2;
pub use camcraft_app::geometry::{Mesh, Point3, Triangle};
pub use camcraft_app::machine::Machine;
pub use camcraft_app::model;
pub use camcraft_app::operation::{self, Height, OpParams, OpStatus, SrcRef};
pub use camcraft_app::project;
pub use camcraft_app::schema::{Location, Operation};
pub use camcraft_app::stock::StockShape;
pub use camcraft_app::tools::{catalog, Tool};
pub use camcraft_app::world::new_world;
pub use camcraft_app::cl::MoveKind;
pub use camcraft_app::drilling::CycleChoice;

pub const TAU: f64 = std::f64::consts::TAU;

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

pub fn machine(word: &str) -> Machine {
    Machine {
        model: format!("TestBench {word}"),
        kind: Some(word.to_string()),
        ..Default::default()
    }
}

// ---- the fixture: the drilled bar, closed and punched ----

/// The point at along-coordinate `s` and transverse (`pu`, `pv`) on
/// axes `u`, `v` - the same backward-`coord` builder the motion
/// cross suite welds its walls with.
pub fn sp(along: usize, u: usize, v: usize, s: f64, pu: f64, pv: f64) -> Point3 {
    let mut c = [0.0; 3];
    c[along] = s;
    c[u] = pu;
    c[v] = pv;
    Point3::new(c[0], c[1], c[2])
}

pub fn quad(a: Point3, b: Point3, c: Point3, d: Point3) -> Vec<Triangle> {
    vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

pub fn circle(cu: f64, cv: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    (0..n)
        .map(|k| {
            let a = TAU * (k as f64) / (n as f64);
            (cu + r * a.cos(), cv + r * a.sin())
        })
        .collect()
}

pub fn circle_cw(cu: f64, cv: f64, r: f64, n: usize) -> Vec<(f64, f64)> {
    let mut c = circle(cu, cv, r, n);
    c.reverse();
    c
}

pub fn wall_ring(along: usize, u: usize, v: usize, poly: &[(f64, f64)], s0: f64, s1: f64) -> Vec<Triangle> {
    let (s0, s1) = if matches!((u, v, along), (0, 1, 2) | (1, 2, 0) | (2, 0, 1)) {
        (s0, s1)
    } else {
        (s1, s0)
    };
    let mut f = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let a = sp(along, u, v, s0, poly[i].0, poly[i].1);
        let b = sp(along, u, v, s0, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
        let c = sp(along, u, v, s1, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
        let d = sp(along, u, v, s1, poly[i].0, poly[i].1);
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    f
}

#[allow(clippy::too_many_arguments)] // the frame-permutation builders: one slot per axis, every call site fills all eight
pub fn disc(along: usize, u: usize, v: usize, cu: f64, cv: f64, r: f64, n: usize, s: f64) -> Vec<Triangle> {
    let p = circle(cu, cv, r, n);
    let ctr = sp(along, u, v, s, cu, cv);
    (0..n)
        .map(|i| {
            Triangle::new(
                ctr,
                sp(along, u, v, s, p[i].0, p[i].1),
                sp(along, u, v, s, p[(i + 1) % n].0, p[(i + 1) % n].1),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)] // same law as `disc`, plus the rounded-rect hole's own radii
pub fn annulus(
    along: usize,
    u: usize,
    v: usize,
    cu: f64,
    cv: f64,
    hu: f64,
    hv: f64,
    r: f64,
    n: usize,
    s: f64,
) -> Vec<Triangle> {
    let q = |k: usize| -> (f64, f64) {
        let a = TAU * (k as f64) / (n as f64);
        let (co, si) = (a.cos(), a.sin());
        let t = (co.abs() / hu).max(si.abs() / hv);
        (cu + co / t, cv + si / t)
    };
    let c = |k: usize| -> (f64, f64) {
        let a = TAU * (k as f64) / (n as f64);
        (cu + r * a.cos(), cv + r * a.sin())
    };
    let mut f = Vec::new();
    for k in 0..n {
        let qa = sp(along, u, v, s, q(k).0, q(k).1);
        let qb = sp(along, u, v, s, q(k + 1).0, q(k + 1).1);
        let ca = sp(along, u, v, s, c(k).0, c(k).1);
        let cb = sp(along, u, v, s, c(k + 1).0, c(k + 1).1);
        f.push(Triangle::new(qa, ca, cb));
        f.push(Triangle::new(qa, cb, qb));
    }
    f
}

/// The F2 bar: x in [-15, 15], y in [-5, 5], z in [-15, 15], with a
/// BLIND X hole at z = 10 (floor x = 5) and a THROUGH X hole at
/// z = -10. The motion cross suite's port, closed: every face is
/// one surface - the two z-bands the holes occupy are punched
/// annuli, everywhere else solid quads - because the document
/// silhouettes it by slicing, and a coplanar double surface is a
/// non-manifold contact to the slicer even when the ray laws ride
/// it. Same walls, floors, and holes; honest caps.
pub fn radial_bar() -> Vec<Triangle> {
    let p = Point3::new;
    let mut t = Vec::new();
    // The four prismatic faces, band by band on the x-caps.
    t.extend(quad(p(-15.0, 5.0, -15.0), p(-15.0, 5.0, 15.0), p(15.0, 5.0, 15.0), p(15.0, 5.0, -15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(15.0, -5.0, -15.0), p(15.0, -5.0, 15.0), p(-15.0, -5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, 15.0), p(15.0, -5.0, 15.0), p(15.0, 5.0, 15.0), p(-15.0, 5.0, 15.0)));
    t.extend(quad(p(-15.0, -5.0, -15.0), p(-15.0, 5.0, -15.0), p(15.0, 5.0, -15.0), p(15.0, -5.0, -15.0)));
    // -X face: punched once in the through hole's band, solid
    // across everything else (the blind hole never reaches it).
    t.extend(quad(p(-15.0, -5.0, -5.0), p(-15.0, -5.0, 15.0), p(-15.0, 5.0, 15.0), p(-15.0, 5.0, -5.0)));
    t.extend(annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, -15.0));
    // +X face: punched where each hole meets it, solid between.
    t.extend(annulus(0, 1, 2, 0.0, 10.0, 5.0, 5.0, 2.0, 24, 15.0));
    t.extend(annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, 15.0));
    t.extend(quad(p(15.0, -5.0, -5.0), p(15.0, 5.0, -5.0), p(15.0, 5.0, 5.0), p(15.0, -5.0, 5.0)));
    // The bores: blind wall + floor, through wall.
    t.extend(wall_ring(0, 1, 2, &circle_cw(0.0, 10.0, 2.0, 24), 5.0, 15.0));
    t.extend(disc(0, 1, 2, 0.0, 10.0, 2.0, 24, 5.0));
    t.extend(wall_ring(0, 1, 2, &circle_cw(0.0, -10.0, 2.0, 24), -15.0, 15.0));
    t
}

pub fn cross(origin: Point3, dir: Point3, radius: f64, floor_d: f64, through: bool) -> CrossHole {
    CrossHole { origin, dir, radius, entry_d: 0.0, floor_d, through }
}

pub fn nx() -> Point3 {
    Point3::new(-1.0, 0.0, 0.0)
}

pub fn ny() -> Point3 {
    Point3::new(0.0, -1.0, 0.0)
}

/// The two typed holes the bar actually carries.
pub fn typed() -> Vec<CrossHole> {
    vec![
        cross(Point3::new(15.0, 0.0, 10.0), nx(), 2.0, -10.0, false),
        cross(Point3::new(15.0, 0.0, -10.0), nx(), 2.0, -30.0, true),
    ]
}

pub fn cross_params(cross: Vec<CrossHole>, bottom_z: Option<Height>) -> OpParams {
    OpParams::Drill {
        break_through: None,
        clearance_z: Height::fixed(20.0),
        retract_z: None,
        top_z: Height::fixed(15.0),
        bottom_z,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![],
        cross,
    }
}

pub fn cross_world(word: &str) -> (World, History) {
    cross_world_on(word, StockShape::Box { dx: 44.0, dy: 44.0, dz: 44.0 }, Location::default())
}

/// The bar's own envelope as raw stock: X [-15, 15], Y [-5, 5],
/// Z [-15, 15] - a turned blank finished on its faces, so the
/// kernel's part-measured standoffs are the stock's too (#79).
pub fn bar_stock_world(word: &str) -> (World, History) {
    let at = Location { pos: Point3::new(0.0, 0.0, -15.0), ..Location::default() };
    cross_world_on(word, StockShape::Box { dx: 30.0, dy: 10.0, dz: 30.0 }, at)
}

pub fn cross_world_on(word: &str, stock: StockShape, at: Location) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, SetStock::new(&world, stock, at).expect("box stock"));
    let cmd = LoadMachine::new(&world, "bench.mch".into(), Some(machine(word)));
    history.execute(&world, cmd.expect("machine"));
    // A long-series Ø4 (issue #84's reach law): the through hole is 31
    // mm deep from the bar's face and 38 from the Ø44 raw box's face
    // (the law reads 47 today, off the box's corner radius - #85) - far
    // past the catalog jobber's 12 mm flute, a real shank collision.
    let mut drill = catalog_tool("DR-0400");
    drill.flute_len = 50.0;
    history.execute(&world, AddTool::new(&world, "DR-0400", drill).expect("long-series drill"));
    history.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(radial_bar())));
    (world, history)
}

pub fn create(world: &World, history: &mut History, name: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, "DR-0400", SrcRef::Entity("bar".into()), params)
            .expect("create"),
    );
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// Max hypot over the bar's vertices - the silhouette radius the
/// kernel's traverse law stands off from.
pub fn stock_r() -> f64 {
    radial_bar()
        .iter()
        .flat_map(|f| [f.a, f.b, f.c])
        .map(|p| (p.x * p.x + p.y * p.y).sqrt())
        .fold(0.0_f64, f64::max)
}
