//! The pocket family's region on a part under a closed floor: a block
//! with a blind square hole, its floor face picked, Bottom anywhere -
//! down to under the block's underside, at stepdowns that may skip the
//! floor slab entirely. The region law reads ALL the part material
//! standing above each ladder plane (section plus the shadow of what
//! hangs over it), so a plane under the underside sees the floor above
//! it and the pick is refused by name; above the floor it cuts silent.
//! Checked against the block's analytic solid, not the law that built it.
//!
//! Provenance: our own design (the shadowed-islands follow-up of the
//! blind-floor fix), specced from the Prime Directive; no vendor material.
//!
//! Gate map (shadowed islands): covers app::operation (area, selection,
//! generate), motion::machining2d (pocket), sim::verify.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, feature};
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// Half the hole's side (the hole is 20 x 20, the block 40 x 40 x 20).
const H: f64 = 10.0;
/// The 10 mm end mill's radius.
const R: f64 = 5.0;
/// Penetration granted the analytic check (the arc fitter's chord sag).
const TOL: f64 = 0.011;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// A 40 x 40 x [0, 20] block with a 20 x 20 blind hole from the top to
/// `floor`. Outward-wound (the fixture of `blind_floor.rs`).
fn block_with_hole(floor: f64) -> Mesh {
    let quad = |a: Point3, b: Point3, c: Point3, d: Point3, f: &mut Vec<Triangle>| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    let mut f = Vec::new();
    let outer = cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 20.0));
    f.extend(outer.facets[4..12].iter().cloned());
    let o = [(-20.0, -20.0), (20.0, -20.0), (20.0, 20.0), (-20.0, 20.0)];
    let i = [(-H, -H), (H, -H), (H, H), (-H, H)];
    for k in 0..4 {
        let n = (k + 1) % 4;
        let p = |(x, y): (f64, f64)| Point3::new(x, y, 20.0);
        quad(p(o[k]), p(o[n]), p(i[n]), p(i[k]), &mut f);
    }
    let inner = cube(Point3::new(-H, -H, floor), Point3::new(H, H, 20.0));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

/// Does the cutter at `q` (shank up from its tip) share volume with the
/// block? The frame column stands to Z 20, the hole column to the floor.
fn in_solid(q: Point3, floor: f64) -> bool {
    let rr = R - TOL;
    let (dx, dy) = ((q.x.abs() - 20.0).max(0.0), (q.y.abs() - 20.0).max(0.0));
    if dx.hypot(dy) >= rr {
        return false;
    }
    let roof = if q.x.abs() + rr > H || q.y.abs() + rr > H { 20.0 } else { floor };
    q.z < roof - TOL
}

fn first_in_solid(t: &Toolpath, floor: f64) -> Option<(usize, Point3)> {
    for (i, m) in t.moves.iter().enumerate() {
        for (a, b) in m.segments3(0.005) {
            let n = ((b.x - a.x).hypot(b.y - a.y).max((b.z - a.z).abs()) / 0.1).ceil().clamp(1.0, 10_000.0) as usize;
            for k in 0..=n {
                let s = k as f64 / n as f64;
                let q = Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s);
                if in_solid(q, floor) {
                    return Some((i, q));
                }
            }
        }
    }
    None
}

fn pocket(strategy: PocketStrategy, bottom: f64, stepdown: f64) -> OpParams {
    OpParams::Pocket {
        links: camcraft_app::links::Links::PLANAR,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(bottom),
        stepdown,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
    }
}

fn world_with(floor: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_hole(floor)));
    let loc = Location { pos: Point3::new(0.0, 0.0, -10.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 30.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", tool).expect("tool"));
    world
}

/// The hole's floor face: up-facing, its outer loop the hole square.
fn floor_face(world: &World, floor: f64) -> FeatureSel {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let face = f
        .faces
        .iter()
        .filter(|face| face.normal[2] > 0.99)
        .find(|face| {
            face.loops.iter().filter(|l| l.kind == feature::LoopKind::Outer).any(|l| {
                l.verts.iter().filter_map(|&v| f.verts.get(v as usize)).all(|p| {
                    (p[2] - floor).abs() < 1e-4 && p[0].abs() <= H + 1e-4 && p[1].abs() <= H + 1e-4
                })
            })
        })
        .expect("the floor face");
    FeatureSel::Face(face.id)
}

/// Generate `params` on the picked floor: the path and the warning count.
fn generate(world: &World, floor: f64, params: &OpParams) -> Result<(Toolpath, usize), String> {
    let op = Operation {
        tool: "EM-1000-4FL".into(),
        src: SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: floor_face(world, floor) }]),
        params: params.clone(),
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    operation::generate(world, &op, "").map(|(t, w)| (t, w.len())).map_err(|e| e.to_string())
}

#[test]
fn a_bottom_under_the_underside_sees_the_thin_floor_above_it() {
    // Floor 1 mm thick; Top 20, stepdown 4.6, Bottom -3: the planes are
    // 20, 15.4, 10.8, 6.2, 1.6, -3 - none sections the floor slab, and
    // the last one sections nothing at all. Pre-fix the region was the
    // hole square at every plane and the ladder plunged through the
    // floor to Z -3 (the analytic solid convicts it). Now the plane at
    // -3 sees the whole block standing over it: refused by name.
    let world = world_with(1.0);
    let err = generate(&world, 1.0, &pocket(PocketStrategy::Zigzag, -3.0, 4.6))
        .expect_err("a pocket under the closed floor must refuse");
    assert!(err.contains("Bottom Z sits below the selected pocket's floor"), "{err}");
    // To the floor itself it cuts, silent and out of the solid.
    let (t, warns) = generate(&world, 1.0, &pocket(PocketStrategy::Zigzag, 1.0, 4.6)).expect("to the floor");
    assert_eq!(warns, 0, "{:?}", t.notes);
    assert!(first_in_solid(&t, 1.0).is_none());
    assert!(t.moves.iter().any(|m| m.kind == MoveKind::Feed && m.to.z == 1.0), "the floor level is cut");
}

#[test]
fn random_bottoms_and_stepdowns_never_pocket_into_the_block() {
    let mut s: u64 = 0xf100_7ed5;
    let mut next = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (s >> 11) as f64 / (1u64 << 53) as f64
    };
    let strategies = [PocketStrategy::Zigzag, PocketStrategy::Waterline, PocketStrategy::Adaptive];
    let (mut cut, mut refused) = (0, 0);
    for case in 0..18 {
        let floor = 0.3 + 15.0 * next();
        let stepdown = 0.8 + 8.0 * next();
        // Half the cases at or above the floor, half below - down to
        // well under the block's underside.
        let bottom = if case % 2 == 0 { floor + (18.0 - floor) * 0.5 * next() } else { floor - (floor + 6.0) * next() };
        let strategy = strategies[case % strategies.len()];
        let tag = format!("case {case}: floor {floor:.4} stepdown {stepdown:.4} bottom {bottom:.4} {strategy:?}");
        let world = world_with(floor);
        match generate(&world, floor, &pocket(strategy, bottom, stepdown)) {
            Ok((t, warns)) => {
                assert!(bottom >= floor - 1e-9, "{tag}: under the floor must refuse, yet it cut");
                assert_eq!(warns, 0, "{tag}: {:?}", t.notes);
                if let Some((i, q)) = first_in_solid(&t, floor) {
                    panic!("{tag}: move {i} enters solid at {q:?} ({:?})", t.moves[i]);
                }
                cut += 1;
            }
            Err(e) => {
                assert!(bottom < floor, "{tag}: at or above the floor must cut: {e}");
                assert!(e.contains("Bottom Z sits below the selected pocket's floor"), "{tag}: {e}");
                refused += 1;
            }
        }
    }
    assert_eq!((cut, refused), (9, 9));
}
