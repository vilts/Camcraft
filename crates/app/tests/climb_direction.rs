//! Climb means climb through the app (#1555): a 40 x 40 x 20 block with
//! a 20 x 20 blind hole, generated the way the GUI and MCP generate it.
//! With M3 (every 2D contour and pocket posts M3), climb keeps the
//! material on the RIGHT of travel: the outside contour runs clockwise,
//! the hole's contour and the 2D Pocket's wall pass counter-clockwise;
//! conventional is the reverse. Every path is checked against the
//! block's analytic solid, so the reversed leads cannot gouge.
//!
//! Provenance: the down-milling convention of the public machining
//! handbooks (Machinery's Handbook, "climb and conventional milling");
//! RS-274 / ISO 6983 G41 / G42. No vendor material consulted.
//!
//! Gate map (#1555): covers app::operation (generate), motion::machining2d
//! (contour, pocket, chamfer, leads), sim::verify.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle, cube, feature};
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, Operation};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

/// Half the hole's side, the floor, and the 10 mm end mill's radius.
const H: f64 = 10.0;
const FLOOR: f64 = 10.0;
const R: f64 = 5.0;
const TOL: f64 = 0.011;

fn flip(t: &Triangle) -> Triangle {
    Triangle::new(t.a, t.c, t.b)
}

/// 40 x 40 x [0, 20] with a 20 x 20 blind hole down to `FLOOR`.
fn block_with_hole() -> Mesh {
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
    let inner = cube(Point3::new(-H, -H, FLOOR), Point3::new(H, H, 20.0));
    f.extend(inner.facets[4..12].iter().map(flip));
    f.extend(outer.facets[0..2].iter().cloned());
    f.extend(inner.facets[0..2].iter().map(flip));
    Mesh::from_facets(f)
}

fn world() -> World {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", block_with_hole()));
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-1000-4FL").map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, "EM-1000-4FL", tool).expect("tool"));
    world
}

/// The hole's floor face.
fn floor_face(world: &World) -> FeatureSel {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    let face = f
        .faces
        .iter()
        .filter(|face| face.normal[2] > 0.99)
        .find(|face| {
            face.loops.iter().filter(|l| l.kind == feature::LoopKind::Outer).any(|l| {
                l.verts.iter().filter_map(|&v| f.verts.get(v as usize)).all(|p| (p[2] - FLOOR).abs() < 1e-4)
            })
        })
        .expect("the floor face");
    FeatureSel::Face(face.id)
}

fn generate(world: &World, src: SrcRef, params: OpParams) -> Toolpath {
    let op = Operation {
        tool: "EM-1000-4FL".into(),
        src,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    match operation::generate(world, &op, "") {
        Ok((t, w)) => {
            assert!(w.is_empty(), "verify flagged the path: {w:?}");
            t
        }
        Err(e) => panic!("generation refused: {e}"),
    }
}

fn hole(world: &World) -> SrcRef {
    SrcRef::Selection(vec![GeoSel { entity: "part".into(), feature: floor_face(world) }])
}

fn pocket(strategy: PocketStrategy, climb: bool) -> OpParams {
    OpParams::Pocket { rest: None,
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 10.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        // Issue #1716: 40% of D keeps the Adaptive plan fast in debug.
        // The cut direction and the analytic-solid check do not read the
        // ring step; only the Adaptive strategy reads the load.
        optimal_load: 0.4,
        blend_turns: false,
        tilt: None,
    }
}

fn contour(side: Side, climb: bool, lead: Lead, bottom: f64) -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: Height::fixed(25.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(bottom),
        stepdown: 20.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb,
        side,
        lead,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// The swept turn about the origin of the planar feeds at `z` whose ends
/// both stand `rmin` or more from it: positive counter-clockwise.
fn turn_at(path: &Toolpath, z: f64, rmin: f64) -> f64 {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && m.from.z == z && m.to.z == z)
        .filter(|m| m.from.x.hypot(m.from.y) >= rmin && m.to.x.hypot(m.to.y) >= rmin)
        .map(|m| m.from.x * m.to.y - m.from.y * m.to.x)
        .sum()
}

/// Does the cutter at `q` share volume with the block?
fn in_solid(q: Point3) -> bool {
    let rr = R - TOL;
    let (dx, dy) = ((q.x.abs() - 20.0).max(0.0), (q.y.abs() - 20.0).max(0.0));
    if dx.hypot(dy) >= rr {
        return false;
    }
    // The tip at `q.z`, the shank up: inside the hole's column the roof
    // is the floor, anywhere else the block's top.
    let roof = if q.x.abs() + rr > H || q.y.abs() + rr > H { 20.0 } else { FLOOR };
    q.z < roof - TOL
}

fn first_in_solid(t: &Toolpath) -> Option<Point3> {
    for m in &t.moves {
        for (a, b) in m.segments3(0.005) {
            let n = ((b.x - a.x).hypot(b.y - a.y).max((b.z - a.z).abs()) / 0.1).ceil().clamp(1.0, 10_000.0) as usize;
            for k in 0..=n {
                let s = k as f64 / n as f64;
                let q = Point3::new(a.x + (b.x - a.x) * s, a.y + (b.y - a.y) * s, a.z + (b.z - a.z) * s);
                if in_solid(q) {
                    return Some(q);
                }
            }
        }
    }
    None
}

fn adaptive2d(climb: bool) -> OpParams {
    OpParams::Adaptive2D {
        blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(FLOOR),
        stepdown: 10.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: 0.4,
        method: camcraft_app::machining2d::AdaptiveMethod::Spiral,
        troch: camcraft_app::machining2d::AdaptiveTrochParams::default(),
        blend_turns: false,
        tilt: None,
    }
}

#[test]
fn the_2d_pocket_wall_pass_runs_counter_clockwise_under_climb() {
    let world = world();
    for strategy in [PocketStrategy::Waterline, PocketStrategy::OffsetRings] {
        for climb in [true, false] {
            let t = generate(&world, hole(&world), pocket(strategy, climb));
            assert!(first_in_solid(&t).is_none(), "{strategy:?} climb={climb}: the pocket cuts the block");
            // The wall ring is the [-5, 5] centre square.
            let turn = turn_at(&t, FLOOR, 4.99);
            assert_ne!(turn, 0.0, "{strategy:?}: no wall pass at the floor");
            assert_eq!(turn > 0.0, climb, "{strategy:?} climb={climb}: wall pass swept {turn}");
        }
    }
    for climb in [true, false] {
        let t = generate(&world, hole(&world), adaptive2d(climb));
        assert!(first_in_solid(&t).is_none(), "Adaptive2D climb={climb}: cuts the block");
        let turn = turn_at(&t, FLOOR, 4.99);
        assert_ne!(turn, 0.0, "Adaptive2D: no wall pass at the floor");
        assert_eq!(turn > 0.0, climb, "Adaptive2D climb={climb}: wall pass swept {turn}");
    }
}

#[test]
fn the_2d_contour_runs_clockwise_outside_and_counter_clockwise_in_the_hole_under_climb() {
    let world = world();
    for lead in [Lead::None, Lead::Arc { radius: 3.0 }, Lead::Tangential { radius: 3.0 }, Lead::Perpendicular { length: 3.0 }] {
        for climb in [true, false] {
            let out = generate(&world, SrcRef::Entity("part".into()), contour(Side::Outside, climb, lead, 0.0));
            assert!(first_in_solid(&out).is_none(), "outside {lead:?} climb={climb}: gouges the block");
            let turn = turn_at(&out, 0.0, 24.9);
            assert_ne!(turn, 0.0, "outside: no ring at Z 0");
            assert_eq!(turn < 0.0, climb, "outside {lead:?} climb={climb}: swept {turn} (climb = CW outside)");

            let bore = generate(&world, hole(&world), contour(Side::Inside, climb, lead, FLOOR));
            assert!(first_in_solid(&bore).is_none(), "hole {lead:?} climb={climb}: gouges the block");
            let turn = turn_at(&bore, FLOOR, 4.99);
            assert_ne!(turn, 0.0, "hole: no ring at the floor");
            assert_eq!(turn > 0.0, climb, "hole {lead:?} climb={climb}: swept {turn} (climb = CCW inside)");
        }
    }
}
