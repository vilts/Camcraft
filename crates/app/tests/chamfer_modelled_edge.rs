//! #2435: a 2D Chamfer with no picks on a model whose edge already
//! carries a modelled chamfer. The cone's tip rides the WALL at the
//! corner depth (the section there), never the bevel's top edge inside
//! it; the width moves the tip along the bevel in X/Y as well as Z; and
//! a cone whose tip stands inside the part - the old ride - is a
//! blocking gouge at the post gate, not edge-break credit.
//!
//! Gate map (#2435): covers app::operation (material_source, the
//! Chamfer2D region), motion::machining2d (chamfer_ride), app::stock_sim
//! (breaks::program_breaks), sim::stock_sim (wall_anchor::on_walls),
//! app::posting (verification_gate). #2596: also
//! sim::stock_sim (wall_anchor::models_a_bevel), app::stock_sim_breaks
//! (auto_edges) - a no-pick chamfer leaves a sharp edge beside a
//! modelled chamfer alone, and a cut there is a gouge.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{self, Height, HeightReference, OpParams, OpType, SrcRef};
use camcraft_app::posting;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// The 90-degree chamfer mill (D6) the #303 gate rides.
const CONE: &str = "CM-0600-90";

/// A 40 x 20 x 10 brick centred on the origin in plan, z 0..10, whose
/// four top edges carry a modelled `bevel` x 45-degree chamfer: the
/// walls stand at x = +-20, y = +-10 up to z = 10 - bevel, the top face
/// is inset by `bevel`. Outward-facing CCW triangles.
fn bevelled_brick(bevel: f64) -> Mesh {
    let ring = |inset: f64, z: f64| {
        let (x, y) = (20.0 - inset, 10.0 - inset);
        [Point3::new(-x, -y, z), Point3::new(x, -y, z), Point3::new(x, y, z), Point3::new(-x, y, z)]
    };
    let (b, m, t) = (ring(0.0, 0.0), ring(0.0, 10.0 - bevel), ring(bevel, 10.0));
    let mut facets = Vec::new();
    let mut quad = |a: Point3, b: Point3, c: Point3, d: Point3| {
        facets.push(Triangle::new(a, b, c));
        facets.push(Triangle::new(a, c, d));
    };
    quad(b[3], b[2], b[1], b[0]);
    quad(t[0], t[1], t[2], t[3]);
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad(b[i], b[j], m[j], m[i]);
        quad(m[i], m[j], t[j], t[i]);
    }
    Mesh::from_facets(facets)
}

fn chamfer(width: f64) -> OpParams {
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
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    let mut p = operation::retype_params(&base, OpType::Chamfer2D.ordinal());
    if let OpParams::Chamfer2D { width: w, .. } = &mut p {
        *w = width;
    }
    p
}

/// The finished part (the stock IS the bevelled brick) and one no-pick
/// 2D Chamfer of `width` on it.
fn job(width: f64) -> World {
    job_on_mesh(width, bevelled_brick(1.0))
}

fn job_on_mesh(width: f64, mesh: Mesh) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("brick", mesh));
    let stock = SetStock::new(&world, StockShape::FromModel { source: "brick".into() }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == CONE).map(|(_, t)| t).expect("catalog cone");
    h.execute(&world, AddTool::new(&world, CONE, tool).expect("tool"));
    let op = CreateOperation::new(&world, "OP", CONE, SrcRef::Entity("brick".into()), chamfer(width));
    h.execute(&world, op.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    world
}

/// Every feed move's end point: the cone's tip along its ride.
fn tips(world: &World) -> Vec<Point3> {
    let e = model::find_by_name(world, "OP").expect("op");
    let t = model::toolpath_of(world, &e).expect("generated");
    assert!(t.warnings.is_empty(), "{:?}", t.warnings);
    t.path.moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| m.to).collect()
}

/// Whether `p` lies on the axis-aligned rectangle +-hx by +-hy in plan.
fn on_rect(p: Point3, hx: f64, hy: f64) -> bool {
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-6;
    let (x, y) = (p.x.abs(), p.y.abs());
    (near(x, hx) && y <= hy + 1e-6) || (near(y, hy) && x <= hx + 1e-6)
}

fn gouge_line(world: &World) -> Option<String> {
    posting::verification_gate(world).into_iter().find(|l| l.starts_with("gouge:"))
}

#[test]
fn a_no_pick_chamfer_on_a_modelled_chamfer_rides_the_wall() {
    // The issue's numbers: a 1 mm chamfer on a 1 x 45 bevel. The tip
    // stands on the wall (x = +-20, y = +-10) at the bevel's foot, z 9
    // - not on the bevel's top edge (+-19, +-9), 1 mm inside the wall.
    let world = job(1.0);
    let tips = tips(&world);
    assert!(!tips.is_empty(), "the chamfer rides");
    for p in &tips {
        assert!((p.z - 9.0).abs() <= 1e-6, "tip at the corner depth: {p:?}");
        assert!(on_rect(*p, 20.0, 10.0), "tip on the wall, not inside it: {p:?}");
    }
    assert_eq!(gouge_line(&world), None, "the modelled chamfer's own ride posts");
}

#[test]
fn the_width_moves_the_tip_along_the_bevel_not_only_in_z() {
    // Half the bevel: the cone's face IS the modelled bevel, its tip on
    // the bevel at z 9.5 - half a millimetre out from the top edge.
    let world = job(0.5);
    let tips = tips(&world);
    assert!(!tips.is_empty(), "the chamfer rides");
    for p in &tips {
        assert!((p.z - 9.5).abs() <= 1e-6, "tip at the corner depth: {p:?}");
        assert!(on_rect(*p, 19.5, 9.5), "tip on the bevel at that depth: {p:?}");
    }
    assert_eq!(gouge_line(&world), None, "a cone on the modelled bevel cuts nothing");
}

#[test]
fn a_cone_tip_inside_the_part_is_a_gouge_not_edge_break_credit() {
    // The pre-#2435 ride, put back by hand: the tip and the declared
    // edge on the bevel's top edge (+-19, +-9), 1 mm inside the wall at
    // z 9. The cone cuts a V 0.7 mm into the bevel face; its record
    // declares an edge that is no wall, and must forgive none of it.
    let world = job(1.0);
    let inward = |p: Point3| Point3::new(p.x.clamp(-19.0, 19.0), p.y.clamp(-9.0, 9.0), p.z);
    let e = model::find_by_name(&world, "OP").expect("op");
    let mut t = model::toolpath_of(&world, &e).expect("cached");
    for m in &mut t.path.moves {
        m.from = inward(m.from);
        m.to = inward(m.to);
    }
    assert!(!t.path.breaks.is_empty(), "the chamfer declares its loop");
    for b in &mut t.path.breaks {
        for q in &mut b.edge {
            *q = inward(*q);
        }
    }
    e.entity_view(&world).set(camcraft_app::schema::Toolpath::new(t.path, t.warnings));
    let line = gouge_line(&world).expect("a cone tip inside the part posted");
    assert!(line.contains("worst 0.") || line.contains("worst 1."), "charged at the V's depth: {line}");
}

fn quad(facets: &mut Vec<Triangle>, a: Point3, b: Point3, c: Point3, d: Point3) {
    facets.extend([Triangle::new(a, b, c), Triangle::new(a, c, d)]);
}

/// A sharp L prism with its re-entrant corner at (0, 0).
fn l_shaped_boss() -> Mesh {
    let ring = [(-20.0, -20.0), (20.0, -20.0), (20.0, 0.0), (0.0, 0.0), (0.0, 20.0), (-20.0, 20.0)];
    let p = |i: usize, z| Point3::new(ring[i].0, ring[i].1, z);
    let mut facets = Vec::new();
    for i in 1..ring.len() - 1 {
        facets.push(Triangle::new(p(0, 10.0), p(i, 10.0), p(i + 1, 10.0)));
        facets.push(Triangle::new(p(0, 0.0), p(i + 1, 0.0), p(i, 0.0)));
    }
    for i in 0..ring.len() {
        let j = (i + 1) % ring.len();
        quad(&mut facets, p(i, 0.0), p(j, 0.0), p(j, 10.0), p(i, 10.0));
    }
    Mesh::from_facets(facets)
}

/// A 40 x 40 x 10 block with a 20 x 20 pocket down to z 5.
fn rectangular_pocket() -> Mesh {
    let ring = |h: f64, z| [Point3::new(-h, -h, z), Point3::new(h, -h, z), Point3::new(h, h, z), Point3::new(-h, h, z)];
    let (bottom, outer, inner, floor) = (ring(20.0, 0.0), ring(20.0, 10.0), ring(10.0, 10.0), ring(10.0, 5.0));
    let mut facets = Vec::new();
    quad(&mut facets, bottom[3], bottom[2], bottom[1], bottom[0]);
    quad(&mut facets, floor[0], floor[1], floor[2], floor[3]);
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad(&mut facets, bottom[i], bottom[j], outer[j], outer[i]);
        quad(&mut facets, outer[i], outer[j], inner[j], inner[i]);
        quad(&mut facets, inner[i], inner[j], floor[j], floor[i]);
    }
    Mesh::from_facets(facets)
}

#[test]
fn a_no_pick_chamfer_on_an_l_shaped_boss_stays_clean() {
    let world = job_on_mesh(1.0, l_shaped_boss());
    let tips = tips(&world);
    assert!(!tips.is_empty(), "the chamfer rides the L-shaped boss");
    assert!(tips.iter().any(|p| p.x.abs() < 1e-6 && p.y.abs() < 1e-6), "the ride includes the inside corner");
    assert_eq!(posting::verification_gate(&world), Vec::<String>::new(), "the legal inside corner keeps its credit");
}

#[test]
fn a_no_pick_chamfer_on_a_rectangular_pocket_stays_clean() {
    let world = job_on_mesh(1.0, rectangular_pocket());
    let tips = tips(&world);
    assert!(!tips.is_empty(), "the chamfer rides the pocket");
    assert!(tips.iter().any(|p| on_rect(*p, 10.0, 10.0)), "the ride includes the pocket rim");
    // #2596: a model with no modelled bevel at all still breaks every
    // top edge (#303) - the pocket rim here is the op's own break.
    assert_eq!(posting::verification_gate(&world), Vec::<String>::new(), "the legal pocket keeps its credit");
}

/// #2596's plate: 60 x 40 x 10 centred in plan, a modelled 1 x 45
/// chamfer on its four outer top edges (walls x = +-30, y = +-20 up to
/// z 9, top face inset 1 mm), and a SHARP 20 x 10 pocket down to z 5.
fn bevelled_plate_with_sharp_pocket() -> Mesh {
    let ring = |hx: f64, hy: f64, z| [Point3::new(-hx, -hy, z), Point3::new(hx, -hy, z), Point3::new(hx, hy, z), Point3::new(-hx, hy, z)];
    let (bottom, wall, top) = (ring(30.0, 20.0, 0.0), ring(30.0, 20.0, 9.0), ring(29.0, 19.0, 10.0));
    let (rim, floor) = (ring(10.0, 5.0, 10.0), ring(10.0, 5.0, 5.0));
    let mut facets = Vec::new();
    quad(&mut facets, bottom[3], bottom[2], bottom[1], bottom[0]);
    quad(&mut facets, floor[0], floor[1], floor[2], floor[3]);
    for i in 0..4 {
        let j = (i + 1) % 4;
        quad(&mut facets, bottom[i], bottom[j], wall[j], wall[i]);
        quad(&mut facets, wall[i], wall[j], top[j], top[i]);
        quad(&mut facets, top[i], top[j], rim[j], rim[i]);
        quad(&mut facets, rim[i], rim[j], floor[j], floor[i]);
    }
    Mesh::from_facets(facets)
}

#[test]
fn a_no_pick_chamfer_leaves_a_sharp_pocket_beside_a_modelled_chamfer_alone() {
    // #2596: the outer edge carries the model's chamfer, the pocket rim
    // is sharp in the model. The cone rides the outer wall only.
    let world = job_on_mesh(1.0, bevelled_plate_with_sharp_pocket());
    let tips = tips(&world);
    assert!(!tips.is_empty(), "the chamfer rides the modelled chamfer");
    for p in &tips {
        assert!((p.z - 9.0).abs() <= 1e-6, "tip at the corner depth: {p:?}");
        assert!(on_rect(*p, 30.0, 20.0), "tip on the outer wall, never on the pocket rim: {p:?}");
    }
    assert_eq!(posting::verification_gate(&world), Vec::<String>::new(), "the modelled chamfer's own ride posts");
}

#[test]
fn a_no_pick_chamfer_cut_into_a_sharp_pocket_rim_is_a_gouge() {
    // #2596: the old ride, put back by hand - the outer ride plus a copy
    // carried onto the sharp pocket rim (+-10, +-5), with its declared
    // break. The model leaves that rim sharp: no credit, a gouge.
    let world = job_on_mesh(1.0, bevelled_plate_with_sharp_pocket());
    let e = model::find_by_name(&world, "OP").expect("op");
    let mut t = model::toolpath_of(&world, &e).expect("cached");
    let onto_rim = |p: Point3| Point3::new(p.x / 3.0, p.y / 4.0, p.z);
    let rim_moves: Vec<_> = t
        .path
        .moves
        .iter()
        .map(|m| {
            let mut m = *m;
            (m.from, m.to) = (onto_rim(m.from), onto_rim(m.to));
            m
        })
        .collect();
    let rim_breaks: Vec<_> = t
        .path
        .breaks
        .iter()
        .map(|b| {
            let mut b = b.clone();
            b.edge.iter_mut().for_each(|q| *q = onto_rim(*q));
            b
        })
        .collect();
    assert!(!rim_breaks.is_empty(), "the chamfer declares its loop");
    t.path.moves.extend(rim_moves);
    t.path.breaks.extend(rim_breaks);
    e.entity_view(&world).set(camcraft_app::schema::Toolpath::new(t.path, t.warnings));
    let line = gouge_line(&world).expect("a chamfer into the sharp pocket rim posted");
    assert!(line.contains("worst 0.") || line.contains("worst 1."), "charged at the V's depth: {line}");
}
