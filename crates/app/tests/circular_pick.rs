//! The Circular operation takes its circle from the picks (0.8.0 hand
//! test: "the Circular operation don't contain to selection"): a bore's
//! rim edge, a round pocket floor, refused by name on a square pocket,
//! and the typed centre and radius untouched without a selection.
//!
//! Provenance: our own tests over the circle-fit law (area centroid,
//! mean vertex radius, stated deviation); no vendor material consulted.
//!
//! Gate map: covers app::operation::circle_fit, app::operation::generate
//! (Circular arm).

use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::feature::{self, FaceKind};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0600-4FL";
const N: usize = 128;

/// A 40x40x8 block centred on `c` with one hole whose rim is `rim(a)`
/// (offset from `c` at angle `a`): through when `floor` is None, else a
/// blind pocket with a flat floor at that height.
fn block(c: Point2, rim: impl Fn(f64) -> (f64, f64), floor: Option<f64>) -> Mesh {
    let tau = std::f64::consts::TAU;
    let ang = |i: usize| tau * (i % N) as f64 / N as f64;
    let bp = |i: usize, z: f64| {
        let (x, y) = rim(ang(i));
        Point3::new(c.x + x, c.y + y, z)
    };
    let op = |i: usize, z: f64| {
        let (x, y) = (ang(i).cos(), ang(i).sin());
        let s = x.abs().max(y.abs());
        Point3::new(c.x + x * 20.0 / s, c.y + y * 20.0 / s, z)
    };
    let quad = |a: Point3, b: Point3, cc: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, cc));
        f.push(Triangle::new(a, cc, d));
    };
    let mid = |z: f64| Point3::new(c.x, c.y, z);
    let low = floor.unwrap_or(0.0);
    let mut f = Vec::new();
    for i in 0..N {
        let j = i + 1;
        quad(bp(j, 8.0), bp(i, 8.0), op(i, 8.0), op(j, 8.0), &mut f);
        quad(bp(i, 8.0), bp(j, 8.0), bp(j, low), bp(i, low), &mut f);
        quad(op(j, 8.0), op(i, 8.0), op(i, 0.0), op(j, 0.0), &mut f);
        match floor {
            None => quad(bp(i, 0.0), bp(j, 0.0), op(j, 0.0), op(i, 0.0), &mut f),
            Some(zf) => {
                f.push(Triangle::new(mid(zf), bp(i, zf), bp(j, zf)));
                f.push(Triangle::new(mid(0.0), op(j, 0.0), op(i, 0.0)));
            }
        }
    }
    Mesh::from_facets(f)
}

fn round(r: f64) -> impl Fn(f64) -> (f64, f64) {
    move |a| (r * a.cos(), r * a.sin())
}

fn square(h: f64) -> impl Fn(f64) -> (f64, f64) {
    move |a| {
        let (x, y) = (a.cos(), a.sin());
        let s = x.abs().max(y.abs());
        (x * h / s, y * h / s)
    }
}

fn world_with(mesh: Mesh, c: Point2) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", mesh));
    let at = Location { pos: Point3::new(c.x - 20.0, c.y - 20.0, 0.0), ..Location::default() };
    history.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 8.0 }, at).expect("stock"),
    );
    (world, history)
}

fn circular(bottom: f64) -> OpParams {
    OpParams::Circular {
        pass_start: Default::default(),
        links: camcraft_app::links::Links::PLANAR,
        center: Point2::new(-3.0, 7.0),
        radius: 12.0,
        vault: false,
        stepover: 2.0,
        stock_to_leave: 0.5,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(8.0),
        bottom_z: Height::fixed(bottom),
        stepdown: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        tilt: None,
        picks: Default::default(),
    }
}

fn pick(f: FeatureSel) -> SrcRef {
    SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: f }])
}

/// The closed top rim chain of the hole about `c`.
fn rim_chain(world: &World, c: Point2, r: f64) -> FeatureSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = feature::chains(&f)
        .into_iter()
        .find(|ch| {
            let p = feature::chain_points(&f, ch);
            p.len() > 8
                && p.iter().all(|q| ((q[0] - c.x).hypot(q[1] - c.y) - r).abs() < 1e-6 && (q[2] - 8.0).abs() < 1e-6)
        })
        .map(|ch| ch.id)
        .expect("the bore's rim chain");
    FeatureSel::Chain(id)
}

/// The upward planar face standing at `z`.
fn floor_face(world: &World, z: f64) -> FeatureSel {
    let e = model::find_by_name(world, "part").expect("part");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, FaceKind::Planar) && fa.normal[2] > 0.5)
        .find(|fa| (fa.offset - z).abs() < 1e-6)
        .map(|fa| fa.id)
        .expect("the floor face");
    FeatureSel::Face(id)
}

fn op(src: SrcRef, params: OpParams) -> Operation {
    Operation { tool: TOOL.into(), src, params, stamp: None, setup: None, area: Default::default(), coolant: Default::default(), after: Default::default(), suppressed: false }
}

fn generated(world: &World, history: &mut History, name: &str, src: SrcRef, params: OpParams) -> Toolpath {
    history.execute(world, CreateOperation::new(world, name, TOOL, src, params).expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    model::find_by_name(world, name).and_then(|e| model::toolpath_of(world, &e)).expect("generated").path
}

/// Every feed move stands inside the ring at `rho` about `c`, and the
/// outermost ring reaches it on both sides of the centre.
fn rings_about(path: &Toolpath, c: Point2, rho: f64) {
    let feeds: Vec<_> = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).collect();
    assert!(!feeds.is_empty(), "rings were cut");
    let d = |m: &&camcraft_app::cl::Move| (m.to.x - c.x).hypot(m.to.y - c.y);
    let far = feeds.iter().map(d).fold(0.0f64, f64::max);
    assert!((far - rho).abs() < 1e-3, "outermost ring {far} vs {rho}");
    assert!(feeds.iter().any(|m| m.to.x > c.x + rho - 1e-3), "ring reaches +x");
    assert!(feeds.iter().any(|m| m.to.x < c.x - rho + 1e-3), "ring reaches -x");
}

#[test]
fn a_picked_bore_rim_centres_the_rings_on_the_bore() {
    let c = Point2::new(10.0, 5.0);
    let (world, mut history) = world_with(block(c, round(10.0), None), c);
    let src = pick(rim_chain(&world, c, 10.0));
    let fit = operation::circular_fit_of(&world, &op(src.clone(), circular(1.0)));
    let (fc, fr) = fit.expect("a selection").expect("a circle");
    assert!((fc.x - 10.0).abs() < 1e-6 && (fc.y - 5.0).abs() < 1e-6 && (fr - 10.0).abs() < 1e-6, "{fc:?} {fr}");
    let path = generated(&world, &mut history, "C1", src, circular(1.0));
    rings_about(&path, c, 10.0 - 3.0 - 0.5);
}

#[test]
fn a_picked_round_pocket_floor_centres_the_rings_there() {
    let c = Point2::new(-4.0, 6.0);
    let (world, mut history) = world_with(block(c, round(15.0), Some(3.0)), c);
    let src = pick(floor_face(&world, 3.0));
    let path = generated(&world, &mut history, "C2", src, circular(3.0));
    rings_about(&path, c, 15.0 - 3.0 - 0.5);
}

#[test]
fn a_picked_square_pocket_is_refused_by_name() {
    let c = Point2::new(0.0, 0.0);
    let (world, _history) = world_with(block(c, square(10.0), Some(3.0)), c);
    let o = op(pick(floor_face(&world, 3.0)), circular(3.0));
    let msg = match operation::generate(&world, &o, "") {
        Ok((p, _)) => panic!("a square must be refused, got {} moves", p.moves.len()),
        Err(e) => e.to_string(),
    };
    assert!(msg.contains("the selection is not a circle (deviation"), "{msg}");
    assert!(msg.contains("clear the selection to use the typed centre and radius"), "{msg}");
    let fit = operation::circular_fit_of(&world, &o).expect("a selection");
    assert!(fit.is_err(), "the page echoes the refusal");
}

#[test]
fn without_a_selection_the_typed_circle_rules_unchanged() {
    let c = Point2::new(10.0, 5.0);
    let (world, mut history) = world_with(block(c, round(10.0), None), c);
    // An empty pick list is no selection either: the typed numbers rule.
    let o = op(SrcRef::Selection(Vec::new()), circular(1.0));
    assert!(operation::circular_fit_of(&world, &o).is_none());
    let stock = generated(&world, &mut history, "C3", SrcRef::Stock, circular(1.0));
    rings_about(&stock, Point2::new(-3.0, 7.0), 12.0 - 3.0 - 0.5);
    let direct = operation::generate(&world, &op(SrcRef::Stock, circular(1.0)), "").expect("typed").0;
    assert_eq!(direct, stock, "the typed circle is the same path every time");
}
