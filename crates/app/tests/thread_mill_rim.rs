//! #1071: a thread mill on a model-sourced hole. The hole the model shows
//! (and the hole the op declares) is the thread's MINOR circle; the helix
//! runs the thread depth H = 0.5413 x pitch past it by design, out to the
//! major circle (`motion::machining2d::thread_mill`). The containment
//! verify read the declared rim at the minor circle, and the solid law the
//! model's minor wall, so every helix move of a matched thread mill was
//! convicted. These pin that a matched thread mill verifies clean against
//! a model, and that one cutting past its major circle - into a nearby
//! wall, or into a model hole smaller than the declared one - still
//! convicts.
//!
//! Gate map (#1071): covers app::operation::generate (the Thread verify),
//! app::operation::material_source (rim loops), sim::verify::section,
//! sim::verify::solid.
//! Lane shape (M4.4): runs in seconds.
//!
//! Provenance: our own fixtures over our own laws; the 0.5413 x pitch
//! radial depth is ISO 724's, as the generator cites it.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::drilling::Hole;
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{cube, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{catalog, Tool};
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// The block's top, and the floor of its hole or cavity.
const TOP: f64 = 20.0;
const FLOOR: f64 = 10.0;
/// Sides of the round hole's polygon: 2.7 um of sagitta at r 5.
const SIDES: usize = 96;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [0, 20] block with a 20 x 20 square cavity from the top,
/// blind at Z 10 (#1073's `coarse_rims` part): a thread there runs in air.
fn cavity_block() -> Mesh {
    const H: f64 = 10.0;
    let mut f: Vec<Triangle> = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-H, -H), (H, -H), (H, H), (-H, H)];
    let p = |(x, y): (f64, f64)| Point3::new(x, y, TOP);
    for k in 0..4 {
        let n = (k + 1) % 4;
        f.push(Triangle::new(p(o[k]), p(o[n]), p(i[n])));
        f.push(Triangle::new(p(o[k]), p(i[n]), p(i[k])));
    }
    let inner = cube(Point3::new(-H, -H, FLOOR), Point3::new(H, H, TOP));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// The same block with a round blind hole of radius `r` at the origin,
/// from the top down to Z 10: what CAD draws of a tapped hole (the minor
/// cylinder). Closed and outward-facing.
fn drilled_block(r: f64) -> Mesh {
    holed_block(r, false)
}

/// [`drilled_block`], or with `through` the hole running out of the
/// block's bottom (the model's own rim loop then stands in its footprint).
fn holed_block(r: f64, through: bool) -> Mesh {
    polygon_block(&ngon(r, SIDES), through)
}

/// `n` points on a circle of radius `r`, from 45 degrees on, so the
/// block's corners (45, 135, 225, 315 degrees) fall on rays of the rim
/// whenever `n` is a multiple of 4.
fn ngon(r: f64, n: usize) -> Vec<Point2> {
    (0..n)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / n as f64 + std::f64::consts::FRAC_PI_4;
            Point2::new(r * a.cos(), r * a.sin())
        })
        .collect()
}

/// The 40 x 40 x [0, TOP] block with a hole whose rim is `rim` (counter-
/// clockwise, star-shaped about the origin, with points on the 45-degree
/// rays so the top face reaches the block's corners), blind to FLOOR or
/// through. One closed, outward shell.
fn polygon_block(rim: &[Point2], through: bool) -> Mesh {
    let mut f: Vec<Triangle> = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, TOP));
    f.extend(outer.facets[4..12].iter().cloned());
    if !through {
        f.extend(outer.facets[0..2].iter().cloned());
    }
    let n = rim.len();
    let at = |k: usize, z: f64| {
        let p = rim[k % n];
        Point3::new(p.x, p.y, z)
    };
    // The square boundary point on the same ray.
    let edge = |k: usize, z: f64| {
        let p = rim[k % n];
        let t = 20.0 / p.x.abs().max(p.y.abs());
        Point3::new(t * p.x, t * p.y, z)
    };
    let low = if through { 0.0 } else { FLOOR };
    let centre = Point3::new(0.0, 0.0, FLOOR);
    for k in 0..n {
        let (p0, p1) = (at(k, TOP), at(k + 1, TOP));
        let (q0, q1) = (edge(k, TOP), edge(k + 1, TOP));
        f.push(Triangle::new(p0, q0, q1)); // top face, +z
        f.push(Triangle::new(p0, q1, p1));
        let (b0, b1) = (at(k, low), at(k + 1, low));
        f.push(Triangle::new(b0, p0, p1)); // hole wall, facing the axis
        f.push(Triangle::new(b0, p1, b1));
        if through {
            let (e0, e1) = (edge(k, 0.0), edge(k + 1, 0.0));
            f.push(Triangle::new(b0, e1, e0)); // bottom face, -z
            f.push(Triangle::new(b0, b1, e1));
        } else {
            f.push(Triangle::new(centre, b0, b1)); // floor, +z
        }
    }
    Mesh::from_facets(f)
}

fn hole(x: f64, radius: f64) -> Hole {
    Hole { xy: Point2::new(x, 0.0), radius, entry_z: TOP, floor_z: 10.2, through: false, stepped: false }
}

fn thread(x: f64, radius: f64) -> OpParams {
    OpParams::Thread {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(TOP),
        bottom_z: None,
        infeed: 0.1,
        feed: 400.0,
        plunge_feed: 150.0,
        climb: true,
        holes: vec![hole(x, radius)],
        pitch: None,
        hand: camcraft_app::cl::ThreadHand::Right,
        external: false,
        passes: camcraft_app::machining2d::ThreadPasses::default(),
    }
}

/// A 6 mm multi-form thread mill, pitch 1 (`mill2d_c`'s, #894's kind),
/// long enough for the hole.
fn thread_mill() -> Tool {
    Tool { material_group: "steel".into(), ..camcraft_app::thread_mill::thread_mill(6.0, 1.0, 0.0, 25.0) }
}

/// #992: a thread mill is not centre-cutting, so each declared hole is
/// opened ahead of the thread: a Bore of the same holes (centre, minor
/// radius, floor) with a flat end mill of 1.2 x the radius, whose ring
/// (0.4 r out) leaves no core standing. The Bore's own verdict is not read.
fn open_ahead(world: &World, h: &mut History, thread: &OpParams) {
    let OpParams::Thread { holes, .. } = thread else { unreachable!("a thread op") };
    for (k, hole) in holes.iter().enumerate() {
        let d = 1.2 * hole.radius;
        let em = catalog().into_iter().find(|(n, _)| *n == "EM-0800-4FL").map(|(_, t)| t).expect("catalog end mill");
        let tool = Tool { diameter: d, shank_d: d, flute_len: 30.0, ..em };
        let (t, op) = (format!("OPEN-T{k}"), format!("OPEN{k}"));
        h.execute(world, AddTool::new(world, &t, tool).expect("open tool"));
        let bore = OpParams::Bore {
            links: camcraft_app::links::Links::PLANAR,
            clearance_z: Height::fixed(30.0),
            retract_z: None,
            top_z: Height::fixed(TOP),
            bottom_z: None,
            stock_to_leave: 0.0,
            stepdown: 5.0,
            feed: 500.0,
            plunge_feed: 150.0,
            climb: true,
            tolerance: 0.01,
            holes: vec![*hole],
        };
        h.execute(world, CreateOperation::new(world, &op, &t, SrcRef::Entity("part".into()), bore).expect("create bore"));
        h.execute(world, GenerateOperation::new(world, &op).expect("generate bore"));
    }
}

/// The blocking warnings of `params` run over `part`, model-sourced,
/// with its holes opened ahead.
fn verdict(part: Mesh, params: OpParams) -> Vec<Warning> {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", part));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 }, loc).expect("stock"));
    open_ahead(&world, &mut h, &params);
    h.execute(&world, AddTool::new(&world, "T", thread_mill()).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", "T", SrcRef::Entity("part".into()), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    let e = model::find_by_name(&world, "OP").expect("op");
    model::toolpath_of(&world, &e).expect("cached").warnings.into_iter().filter(Warning::blocks_post).collect()
}

fn gouged(w: &[Warning]) -> bool {
    w.iter().any(|w| matches!(w, Warning::Gouge { .. }))
}

#[test]
fn a_matched_thread_mill_in_air_verifies_clean() {
    // M10-style: a 5 mm minor radius in the cavity, nothing but the
    // declared rim to read against.
    assert_eq!(verdict(cavity_block(), thread(0.0, 5.0)), Vec::new());
}

#[test]
fn a_matched_thread_mill_in_a_drilled_hole_verifies_clean() {
    // The model's hole IS the minor cylinder the op declares.
    assert_eq!(verdict(drilled_block(5.0), thread(0.0, 5.0)), Vec::new());
}

#[test]
fn a_thread_whose_major_circle_reaches_a_wall_convicts() {
    // At X 4.7 the major circle (5.5413) reaches X 10.24: 0.24 mm into
    // the cavity's wall at X 10, while the minor circle (9.7) stays
    // short of it.
    let w = verdict(cavity_block(), thread(4.7, 5.0));
    assert!(gouged(&w), "{w:?}");
}

#[test]
fn a_thread_declared_wider_than_the_models_hole_convicts() {
    // The model drilled at r 4.5, the op declaring r 5: the helix cuts
    // 1.04 mm into the model's wall, not the thread depth past a wall it
    // was declared against.
    let w = verdict(drilled_block(4.5), thread(0.0, 5.0));
    assert!(gouged(&w), "{w:?}");
}

#[test]
fn a_thread_taken_below_the_models_hole_floor_convicts() {
    // The model's hole is blind at Z 10; the op declares its floor at
    // Z 9, so the helix's last turns cut a millimetre into the floor:
    // the grant covers the wall and the mouth, never a floor.
    let mut h = hole(0.0, 5.0);
    h.floor_z = 9.0;
    let OpParams::Thread { clearance_z, retract_z, top_z, bottom_z, infeed, feed, plunge_feed, climb, pitch, hand, .. } = thread(0.0, 5.0) else {
        unreachable!("a thread op")
    };
    let p = OpParams::Thread { clearance_z, retract_z, top_z, bottom_z, infeed, feed, plunge_feed, climb, holes: vec![h], pitch, hand, external: false, passes: camcraft_app::machining2d::ThreadPasses::default() };
    let w = verdict(drilled_block(5.0), p);
    assert!(gouged(&w), "{w:?}");
}

#[test]
fn a_stock_sourced_thread_mill_verifies_clean_too() {
    // The same matched thread cut from the stock source: before #1071
    // it was blocked as well (52 gouges and 21 crossing pairs), which
    // `mill2d_c` never asserted.
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cavity_block()));
    let loc = Location { pos: Point3::new(0.0, 0.0, -2.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 22.0 }, loc).expect("stock"));
    open_ahead(&world, &mut h, &thread(0.0, 5.0));
    h.execute(&world, AddTool::new(&world, "T", thread_mill()).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", "T", SrcRef::Stock, thread(0.0, 5.0)).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    let e = model::find_by_name(&world, "OP").expect("op");
    let w: Vec<Warning> =
        model::toolpath_of(&world, &e).expect("cached").warnings.into_iter().filter(Warning::blocks_post).collect();
    assert_eq!(w, Vec::new());
}

/// PR #1099 review: the grant must hold the cutter to the MODEL's wall,
/// not the declared circle. A declared hole wider than the model's, or off
/// its centre, by more than the gouge tolerance cuts the thread that much
/// past the model's real major circle: convicted, however small.
#[test]
fn a_declared_hole_wider_than_the_models_by_a_hair_convicts() {
    for (model, declared) in [(5.0, 5.05), (5.0, 5.1), (15.0, 15.25)] {
        let w = verdict(drilled_block(model), thread(0.0, declared));
        assert!(gouged(&w), "r {declared} declared on an r {model} hole: {w:?}");
    }
    // The same on a through hole, where the model's own rim loop stands
    // in the region (the region law's reading of it).
    let w = verdict(holed_block(5.0, true), thread(0.0, 5.05));
    assert!(gouged(&w), "through hole: {w:?}");
}

#[test]
fn a_declared_hole_off_the_models_centre_by_a_hair_convicts() {
    for (model, x) in [(5.0, 0.05), (5.0, 0.1), (15.0, 0.28)] {
        let w = verdict(drilled_block(model), thread(x, model));
        assert!(gouged(&w), "centre X {x} on an r {model} hole: {w:?}");
    }
    let w = verdict(holed_block(5.0, true), thread(0.05, 5.0));
    assert!(gouged(&w), "through hole: {w:?}");
}

#[test]
fn a_matched_thread_stays_clean_at_other_sizes_and_through() {
    // The cap reads the model's own radius, so a match is still a match:
    // a bigger bore, a through hole, and an off-centre declaration inside
    // the gouge tolerance (5 um).
    assert_eq!(verdict(drilled_block(15.0), thread(0.0, 15.0)), Vec::new());
    assert_eq!(verdict(holed_block(5.0, true), thread(0.0, 5.0)), Vec::new());
    assert_eq!(verdict(drilled_block(5.0), thread(0.005, 5.0)), Vec::new());
}

/// PR #1099 third review: a flat whose corners stand on the declared
/// circle is not the thread's wall. A chord dips inside the circle by its
/// sagitta, and the helix, cut to the declared major circle, goes that
/// much further into the flat than the thread depth: convicted, from
/// both the region law and the solid law. Before the fix each case below
/// verified clean.
#[test]
fn a_flat_whose_corners_stand_on_the_circle_convicts() {
    // A D-bore: an r 5 96-gon with a flat from (3, 4) to (-3, 4), 1 mm
    // inside the circle at its middle.
    let (lo, hi) = (4.0f64.atan2(3.0), 4.0f64.atan2(-3.0));
    let mut d: Vec<Point2> = ngon(5.0, SIDES).into_iter().filter(|p| !(p.y.atan2(p.x) > lo && p.y.atan2(p.x) < hi)).collect();
    d.extend([Point2::new(3.0, 4.0), Point2::new(-3.0, 4.0)]);
    d.sort_by(|a, b| {
        let ang = |p: &Point2| (p.y.atan2(p.x) - std::f64::consts::FRAC_PI_4).rem_euclid(std::f64::consts::TAU);
        ang(a).total_cmp(&ang(b))
    });
    for (name, rim) in [("D-bore", d), ("octagon on r 5", ngon(5.0, 8)), ("16-gon on r 5", ngon(5.0, 16))] {
        let w = verdict(polygon_block(&rim, false), thread(0.0, 5.0));
        assert!(gouged(&w), "{name}: {w:?}");
    }
    // Through, where the model's own rim loop stands in the region.
    let w = verdict(polygon_block(&ngon(5.0, 8), true), thread(0.0, 5.0));
    assert!(gouged(&w), "octagon, through: {w:?}");
}

/// The flats' twin: a fine polygon's sagitta fits the band, so a matched
/// thread on it stays clean (96 sides: 2.7 um; 48 sides: 11 um).
#[test]
fn a_fine_polygon_bore_still_verifies_clean() {
    assert_eq!(verdict(polygon_block(&ngon(5.0, 48), false), thread(0.0, 5.0)), Vec::new());
    assert_eq!(verdict(polygon_block(&ngon(5.0, 96), true), thread(0.0, 5.0)), Vec::new());
}
