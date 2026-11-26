//! #171 acceptance: a Probe op seeded from one pick - a planar face
//! seeds a Face touch against its outward normal, a straight top edge
//! an Edge touch on the wall under it, a vertex an outside Corner, a
//! round wall or rim a Bore or Boss centre from the circle fit. The
//! seed rides the pick command (one undo), and a pick the probe cannot
//! touch is refused by name with the op untouched.
//!
//! Provenance: touch-probe practice from machine-tool handbooks; our
//! own fixtures (a box, a sloped block, a bored plate, a round boss).
//!
//! Gate map (M4.4): covers app::operation (probe_pick, probe_pick_round,
//! picks::selection_account), app::commands (SetOperationGeometry's
//! probe seed, CreateOperation from a pick, undo / redo).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetOperationGeometry, SetStock,
};
use camcraft_app::geometry::feature;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpType, ProbeSeed, SrcRef,
};
use camcraft_app::probing::{ProbeDir, ProbeMode};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::probe_tool;
use camcraft_app::world::new_world;

const N: usize = 96;

/// Eight corners, bottom ring then top ring (CCW from above), as an
/// outward-wound hexahedron.
fn hexa(c: [Point3; 8]) -> Vec<Triangle> {
    let quads = [[0, 3, 2, 1], [4, 5, 6, 7], [0, 1, 5, 4], [1, 2, 6, 5], [2, 3, 7, 6], [3, 0, 4, 7]];
    let mut f = Vec::new();
    for q in quads {
        f.push(Triangle::new(c[q[0]], c[q[1]], c[q[2]]));
        f.push(Triangle::new(c[q[0]], c[q[2]], c[q[3]]));
    }
    f
}

/// The 40 x 30 x 10 box at the origin; `drop` lowers its +X top edge.
fn box_mesh(drop: f64) -> Mesh {
    let p = Point3::new;
    Mesh::from_facets(hexa([
        p(0.0, 0.0, 0.0),
        p(40.0, 0.0, 0.0),
        p(40.0, 30.0, 0.0),
        p(0.0, 30.0, 0.0),
        p(0.0, 0.0, 10.0),
        p(40.0, 0.0, 10.0 - drop),
        p(40.0, 30.0, 10.0 - drop),
        p(0.0, 30.0, 10.0),
    ]))
}

/// A 40 x 40 x 8 plate centred on (0, 0) with a through bore of radius 10.
fn bored_plate() -> Mesh {
    let tau = std::f64::consts::TAU;
    let ang = |i: usize| tau * (i % N) as f64 / N as f64;
    let bp = |i: usize, z: f64| Point3::new(10.0 * ang(i).cos(), 10.0 * ang(i).sin(), z);
    let op = |i: usize, z: f64| {
        let (x, y) = (ang(i).cos(), ang(i).sin());
        let s = x.abs().max(y.abs());
        Point3::new(x * 20.0 / s, y * 20.0 / s, z)
    };
    let mut f = Vec::new();
    let mut quad = |a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    for i in 0..N {
        let j = i + 1;
        quad(bp(j, 8.0), bp(i, 8.0), op(i, 8.0), op(j, 8.0));
        quad(bp(i, 8.0), bp(j, 8.0), bp(j, 0.0), bp(i, 0.0));
        quad(op(j, 8.0), op(i, 8.0), op(i, 0.0), op(j, 0.0));
        quad(bp(i, 0.0), bp(j, 0.0), op(j, 0.0), op(i, 0.0));
    }
    Mesh::from_facets(f)
}

/// A round boss of radius 8 about (100, 0), Z 0..12, outward-wound.
fn boss() -> Mesh {
    let tau = std::f64::consts::TAU;
    let p = |i: usize, z: f64| {
        let a = tau * (i % N) as f64 / N as f64;
        Point3::new(100.0 + 8.0 * a.cos(), 8.0 * a.sin(), z)
    };
    let (top, bot) = (Point3::new(100.0, 0.0, 12.0), Point3::new(100.0, 0.0, 0.0));
    let mut f = Vec::new();
    for i in 0..N {
        let j = i + 1;
        f.push(Triangle::new(top, p(i, 12.0), p(j, 12.0)));
        f.push(Triangle::new(bot, p(j, 0.0), p(i, 0.0)));
        f.push(Triangle::new(p(i, 0.0), p(j, 0.0), p(j, 12.0)));
        f.push(Triangle::new(p(i, 0.0), p(j, 12.0), p(i, 12.0)));
    }
    Mesh::from_facets(f)
}

fn world_with(name: &str, mesh: Mesh) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh(name, mesh));
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 30.0, dz: 10.0 }, Location::default()).expect("stock"),
    );
    h.execute(&world, AddTool::new(&world, "PROBE-4", probe_tool(4.0, 30.0)).expect("tool"));
    (world, h)
}

fn probe_params() -> OpParams {
    OpParams::Probe {
        clearance_z: Height::of(HeightReference::TopOfStock, 15.0),
        mode: ProbeMode::Face,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: [0.0; 3],
        radius: 10.0,
        approach: 5.0,
        overtravel: 1.0, // #570: the new-op default
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    }
}

fn verts_of(world: &World, entity: &str, facets: &[u32]) -> Vec<[f64; 3]> {
    let e = model::find_by_name(world, entity).expect("entity");
    let t = model::facets_of(world, &e).expect("facets");
    facets
        .iter()
        .flat_map(|&i| {
            let t = t[i as usize];
            [[t.a.x, t.a.y, t.a.z], [t.b.x, t.b.y, t.b.z], [t.c.x, t.c.y, t.c.z]]
        })
        .collect()
}

/// The face every vertex of which passes `on`.
fn face(world: &World, entity: &str, on: impl Fn([f64; 3]) -> bool) -> GeoSel {
    let e = model::find_by_name(world, entity).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let id = f
        .faces
        .iter()
        .find(|fa| verts_of(world, entity, &fa.facets).into_iter().all(&on))
        .map(|fa| fa.id)
        .expect("the face");
    GeoSel { entity: entity.into(), feature: FeatureSel::Face(id) }
}

/// The chain every point of which passes `on`.
fn chain(world: &World, entity: &str, on: impl Fn([f64; 3]) -> bool) -> GeoSel {
    let e = model::find_by_name(world, entity).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| feature::chain_points(&f, c).into_iter().all(&on))
        .map(|c| c.id)
        .expect("the chain");
    GeoSel { entity: entity.into(), feature: FeatureSel::Chain(id) }
}

fn vertex(world: &World, entity: &str, at: [f64; 3]) -> GeoSel {
    let e = model::find_by_name(world, entity).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let vi = feature::corner_verts(&f).into_iter().find(|&v| f.verts[v as usize] == at).expect("the vertex");
    GeoSel { entity: entity.into(), feature: FeatureSel::Vertex(feature::vertex_id(&f, vi).expect("id")) }
}

fn patch(world: &World, entity: &str) -> GeoSel {
    let e = model::find_by_name(world, entity).expect("entity");
    let f = model::features_of(world, &e).expect("features");
    let ps = feature::patches(&f);
    assert_eq!(ps.len(), 1, "one round wall");
    GeoSel { entity: entity.into(), feature: FeatureSel::Patch(ps[0].id) }
}

fn seed(world: &World, sel: &GeoSel) -> ProbeSeed {
    operation::probe_seed(world, None, sel).expect("a seed")
}

fn near(a: [f64; 3], b: [f64; 3], tol: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < tol)
}

#[test]
fn a_planar_face_seeds_a_face_touch_against_its_normal() {
    let (world, _) = world_with("part", box_mesh(0.0));
    let top = seed(&world, &face(&world, "part", |v| v[2] == 10.0));
    assert_eq!((top.mode, top.dir), (ProbeMode::Face, ProbeDir::ZNeg));
    assert!(near(top.target, [20.0, 15.0, 10.0], 1e-9), "{top:?}");
    let side = seed(&world, &face(&world, "part", |v| v[0] == 40.0));
    assert_eq!((side.mode, side.dir), (ProbeMode::Face, ProbeDir::XNeg), "a +X face is touched travelling -X");
    assert!(near(side.target, [40.0, 15.0, 5.0], 1e-9), "{side:?}");
    let front = seed(&world, &face(&world, "part", |v| v[1] == 0.0));
    assert_eq!(front.dir, ProbeDir::YPos, "a -Y face is touched travelling +Y");
}

#[test]
fn a_straight_top_edge_seeds_an_edge_touch_on_the_wall_under_it() {
    let (world, _) = world_with("part", box_mesh(0.0));
    let s = seed(&world, &chain(&world, "part", |v| v[1] == 0.0 && v[2] == 10.0));
    assert_eq!((s.mode, s.dir), (ProbeMode::Edge, ProbeDir::YPos));
    assert!(near(s.target, [20.0, 0.0, 10.0], 1e-9), "the edge's midpoint: {s:?}");
    let s = seed(&world, &chain(&world, "part", |v| v[0] == 40.0 && v[2] == 10.0));
    assert_eq!((s.mode, s.dir), (ProbeMode::Edge, ProbeDir::XNeg));
}

#[test]
fn a_vertex_seeds_an_outside_corner_from_its_two_walls() {
    let (world, _) = world_with("part", box_mesh(0.0));
    let s = seed(&world, &vertex(&world, "part", [0.0, 0.0, 10.0]));
    assert_eq!((s.mode, s.dir, s.dir_y), (ProbeMode::Corner, ProbeDir::XPos, ProbeDir::YPos));
    assert_eq!(s.target, [0.0, 0.0, 10.0]);
    let s = seed(&world, &vertex(&world, "part", [40.0, 30.0, 10.0]));
    assert_eq!((s.dir, s.dir_y), (ProbeDir::XNeg, ProbeDir::YNeg));
}

#[test]
fn a_round_wall_or_rim_seeds_a_bore_or_boss_centre() {
    let (world, mut h) = world_with("plate", bored_plate());
    h.execute(&world, ImportModel::from_mesh("boss", boss()));
    let rim = |v: [f64; 3]| (v[0].hypot(v[1]) - 10.0).abs() < 1e-6 && v[2] == 8.0;
    for sel in [patch(&world, "plate"), chain(&world, "plate", rim)] {
        let s = seed(&world, &sel);
        assert_eq!(s.mode, ProbeMode::Bore, "{sel:?}: the wall faces its axis");
        assert!(near(s.target, [0.0, 0.0, 8.0], 1e-6), "{s:?}");
        assert!((s.radius.expect("a radius") - 10.0).abs() < 0.01, "{s:?}");
    }
    let top = |v: [f64; 3]| ((v[0] - 100.0).hypot(v[1]) - 8.0).abs() < 1e-6 && v[2] == 12.0;
    for sel in [patch(&world, "boss"), chain(&world, "boss", top)] {
        let s = seed(&world, &sel);
        assert_eq!(s.mode, ProbeMode::Boss, "{sel:?}: the wall faces away from its axis");
        assert!(near(s.target, [100.0, 0.0, 12.0], 1e-6), "{s:?}");
        assert!((s.radius.expect("a radius") - 8.0).abs() < 0.01, "{s:?}");
    }
}

#[test]
fn picks_the_probe_cannot_touch_are_refused_by_name() {
    let (world, _) = world_with("part", box_mesh(0.0));
    let why = |sel: GeoSel| operation::probe_seed(&world, None, &sel).expect_err("refused");
    let w = why(face(&world, "part", |v| v[2] == 0.0));
    assert!(w.contains("looks down"), "{w}");
    let w = why(chain(&world, "part", |v| v[0] == 0.0 && v[1] == 0.0));
    assert!(w.contains("not level"), "a vertical edge: {w}");
    let w = why(vertex(&world, "part", [0.0, 0.0, 0.0]));
    assert!(w.contains("0 wall(s) facing X"), "a bottom vertex tops no wall: {w}");

    let (world, _) = world_with("ramp", box_mesh(5.0));
    let slope = face(&world, "ramp", |v| (v[2] - (10.0 - v[0] / 8.0)).abs() < 1e-9);
    let w = operation::probe_seed(&world, None, &slope).expect_err("a sloped face");
    assert!(w.contains("off X, Y and Z"), "{w}");
    let w = operation::probe_seed(&world, None, &vertex(&world, "ramp", [0.0, 0.0, 10.0])).expect_err("a sloped corner");
    assert!(w.contains("off the axes"), "{w}");
    let w = operation::probe_seed(&world, None, &chain(&world, "ramp", |v| v[1] == 0.0 && v[2] > 0.0)).expect_err("a sloped edge");
    assert!(w.contains("not level"), "{w}");
}

fn probe_op(world: &World) -> camcraft_app::schema::Operation {
    let e = model::find_by_name(world, "Probe1").expect("op");
    model::operation_of(world, &e).expect("operation")
}

#[test]
fn the_pick_command_seeds_the_probe_and_one_undo_takes_both_back() {
    let (world, mut h) = world_with("part", box_mesh(0.0));
    h.execute(&world, CreateOperation::new(&world, "Probe1", "PROBE-4", SrcRef::Stock, probe_params()).expect("create"));
    let before = probe_op(&world);
    let edge = chain(&world, "part", |v| v[1] == 0.0 && v[2] == 10.0);
    h.execute(&world, SetOperationGeometry::new(&world, "Probe1", vec![edge.clone()]).expect("pick"));
    let after = probe_op(&world);
    assert_eq!(after.src, SrcRef::Selection(vec![edge.clone()]));
    let OpParams::Probe { mode, dir, target, approach, .. } = after.params else { panic!("a probe") };
    assert_eq!((mode, dir, target), (ProbeMode::Edge, ProbeDir::YPos, [20.0, 0.0, 10.0]));
    assert!(matches!(before.params, OpParams::Probe { approach: a, .. } if a == approach), "the pick seeds, the numbers stay");
    // The page's account names the seed.
    let (notes, refusal) = operation::selection_account(&world, None, &[edge], &after.params);
    assert_eq!(refusal, None);
    assert!(notes.iter().any(|n| n.contains("Edge touch toward +Y")), "{notes:?}");

    assert!(h.undo(&world));
    assert_eq!(probe_op(&world).src, before.src);
    assert_eq!(probe_op(&world).params, before.params, "one undo restores the typed params too");
    assert!(h.redo(&world));
    assert_eq!(probe_op(&world).params, after.params);

    // A pick the probe cannot touch, or two picks, change nothing.
    let down = face(&world, "part", |v| v[2] == 0.0);
    let err = SetOperationGeometry::new(&world, "Probe1", vec![down.clone()]).err().expect("refused");
    assert!(err.to_string().contains("looks down"), "{err}");
    let top = face(&world, "part", |v| v[2] == 10.0);
    let err = SetOperationGeometry::new(&world, "Probe1", vec![top.clone(), down.clone()]).err().expect("refused");
    assert!(err.to_string().contains("one pick"), "{err}");
    let (_, refusal) = operation::selection_account(&world, None, &[down], &after.params);
    assert!(refusal.is_some_and(|r| r.contains("looks down")));
}

#[test]
fn a_probe_created_from_a_pick_generates_clean() {
    for (name, mesh, sel_of) in [
        ("part", box_mesh(0.0), Box::new(|w: &World| face(w, "part", |v| v[2] == 10.0)) as Box<dyn Fn(&World) -> GeoSel>),
        ("part", box_mesh(0.0), Box::new(|w: &World| vertex(w, "part", [0.0, 0.0, 10.0]))),
        ("plate", bored_plate(), Box::new(|w: &World| patch(w, "plate"))),
    ] {
        let (world, mut h) = world_with(name, mesh);
        let sel = sel_of(&world);
        let params = operation::probe_from_picks(&world, None, &probe_params(), std::slice::from_ref(&sel))
            .expect("a probe")
            .expect("seeded");
        let src = SrcRef::Selection(vec![sel]);
        h.execute(&world, CreateOperation::new(&world, "Probe1", "PROBE-4", src, params).expect("create"));
        h.execute(&world, GenerateOperation::new(&world, "Probe1").expect("generate"));
        let e = model::find_by_name(&world, "Probe1").expect("op");
        let cache = model::toolpath_of(&world, &e).expect("cached");
        assert!(!cache.path.moves.is_empty());
        assert!(cache.warnings.is_empty(), "{name}: a seeded touch is clean: {:?}", cache.warnings);
        assert!(h.undo(&world) && h.undo(&world), "generate and create undo");
        assert!(model::find_by_name(&world, "Probe1").is_none());
    }
}

#[test]
fn other_kinds_are_not_seeded() {
    let (world, _) = world_with("part", box_mesh(0.0));
    let face_params = operation::retype_params(&probe_params(), OpType::Face as usize);
    let top = face(&world, "part", |v| v[2] == 10.0);
    assert!(operation::probe_from_picks(&world, None, &face_params, std::slice::from_ref(&top)).is_none());
    assert!(operation::probe_account(&world, None, &[top], &face_params).is_none());
}

/// An L-shaped block, Z 0..10: the notch's corner at (10, 10) is inside.
fn l_block() -> Mesh {
    let ring = [(0.0, 0.0), (10.0, 0.0), (40.0, 0.0), (40.0, 10.0), (10.0, 10.0), (10.0, 30.0), (0.0, 30.0), (0.0, 10.0)];
    let p = |(x, y): (f64, f64), z: f64| Point3::new(x, y, z);
    let mut f = Vec::new();
    // Caps: three squares sharing every corner (no T-junctions).
    for [a, b, c, d] in [[0, 1, 4, 7], [1, 2, 3, 4], [7, 4, 5, 6]] {
        let (a, b, c, d) = (ring[a], ring[b], ring[c], ring[d]);
        f.push(Triangle::new(p(a, 10.0), p(b, 10.0), p(c, 10.0)));
        f.push(Triangle::new(p(a, 10.0), p(c, 10.0), p(d, 10.0)));
        f.push(Triangle::new(p(a, 0.0), p(c, 0.0), p(b, 0.0)));
        f.push(Triangle::new(p(a, 0.0), p(d, 0.0), p(c, 0.0)));
    }
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        f.push(Triangle::new(p(a, 0.0), p(b, 0.0), p(b, 10.0)));
        f.push(Triangle::new(p(a, 0.0), p(b, 10.0), p(a, 10.0)));
    }
    Mesh::from_facets(f)
}

#[test]
fn an_inside_corner_is_refused_and_an_outside_one_on_the_l_seeds() {
    let (world, _) = world_with("l", l_block());
    let w = operation::probe_seed(&world, None, &vertex(&world, "l", [10.0, 10.0, 10.0])).expect_err("inside");
    assert!(w.contains("inside corner"), "{w}");
    let s = seed(&world, &vertex(&world, "l", [40.0, 0.0, 10.0]));
    assert_eq!((s.mode, s.dir, s.dir_y), (ProbeMode::Corner, ProbeDir::XNeg, ProbeDir::YPos));
}
