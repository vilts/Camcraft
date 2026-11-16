//! Shared fixtures of the M11.5 area-controls suites (`area_controls`,
//! `area_open`): the block / stock worlds, the op builders, the path
//! readers. Not a suite itself.

#![allow(dead_code, unused_imports)]

pub use camcraft_app::cl::{MoveKind, Toolpath};
pub use camcraft_app::commands::{AddTool, CreateOperation, EditOperationParams, History, ImportModel, SetStock};
pub use camcraft_app::geometry::{Point3, cube, feature};
pub use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
pub use camcraft_app::machining3d::Entry3D;
pub use camcraft_app::model;
pub use camcraft_app::operation::{
    self, AreaControls, BoundaryMode, BoundaryShape, ChainSide, Containment, CornerStyle, ExtraBoundary,
    FeatureSel, GeoSel, Height, OpParams, OpType, OpenBoundary, SrcRef,
};
pub use camcraft_app::project;
pub use camcraft_app::schema::{Location, Operation};
pub use camcraft_app::stock::StockShape;
pub use camcraft_app::tools::catalog;
pub use camcraft_app::world::new_world;
pub use flecs_ecs::prelude::World;

pub const TOOL: &str = "EM-1000-4FL";
pub const R: f64 = 5.0;
pub const EPS: f64 = 1e-3;

pub fn pocket(top: f64, bottom: f64) -> OpParams {
    OpParams::Pocket {
        blend_radius: None,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(40.0),
        retract_z: None,
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(bottom),
        stepdown: 5.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy: PocketStrategy::Zigzag,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        blend_turns: false,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
    }
}

/// A contour through the retype law (its field list is not this
/// suite's business), then the numbers this suite reads.
pub fn contour(top: f64, bottom: f64, want: Side) -> OpParams {
    let mut p = operation::retype_params(&pocket(top, bottom), OpType::Contour as usize);
    if let OpParams::Contour { side, lead, top_z, bottom_z, retract_z, clearance_z, stepdown, tab_count, .. } = &mut p {
        *side = want;
        *lead = Lead::None;
        *top_z = Height::fixed(top);
        *bottom_z = Height::fixed(bottom);
        *retract_z = Height::fixed(30.0);
        *clearance_z = Height::fixed(40.0);
        *stepdown = top - bottom;
        *tab_count = 0;
    }
    assert_eq!(p.kind(), OpType::Contour);
    p
}

pub fn face(cut: f64) -> OpParams {
    OpParams::Face { holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 6.0,
        clearance_z: Height::fixed(40.0),
        cut_z: Height::fixed(cut),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

pub fn world_with(import: Option<ImportModel>, z0: f64, dz: f64) -> World {
    let world = new_world();
    let mut h = History::default();
    if let Some(i) = import {
        h.execute(&world, i);
    }
    let loc = Location { pos: Point3::new(0.0, 0.0, z0), ..Default::default() };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 70.0, dy: 70.0, dz }, loc).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    world
}

/// A 40 x 40 x 10 block centred on the origin inside a 70 x 70 x 10 stock.
pub fn block_world() -> World {
    world_with(
        Some(ImportModel::from_mesh("part", cube(Point3::new(-20.0, -20.0, 0.0), Point3::new(20.0, 20.0, 10.0)))),
        0.0,
        10.0,
    )
}

pub fn op(src: SrcRef, params: OpParams, area: AreaControls) -> Operation {
    Operation { tool: TOOL.into(), src, params, stamp: None, setup: None, area, coolant: Default::default(), after: Default::default(), suppressed: false }
}

pub fn run(world: &World, o: &Operation) -> Result<(Toolpath, usize), String> {
    operation::generate(world, o, "").map(|(t, w)| (t, w.len())).map_err(|e| e.to_string())
}

pub fn feeds(t: &Toolpath) -> Vec<Point3> {
    t.moves.iter().filter(|m| m.kind == MoveKind::Feed).flat_map(|m| [m.from, m.to]).collect()
}

pub fn max_by(pts: &[Point3], f: impl Fn(&Point3) -> f64) -> f64 {
    pts.iter().map(f).fold(f64::NEG_INFINITY, f64::max)
}

pub fn stock_box(world: &World) -> (Point3, Point3) {
    let b = model::stock_world_aabb_for(world, None).expect("stock box");
    (b.min, b.max)
}

/// The up-facing face of `name` whose outer loop sits at `z` with the
/// widest extent.
pub fn top_face(world: &World, name: &str, z: f64) -> FeatureSel {
    let e = model::find_by_name(world, name).expect("model");
    let f = model::features_of(world, &e).expect("features");
    let mut best: Option<(f64, FeatureSel)> = None;
    for face in f.faces.iter().filter(|face| face.normal[2] > 0.99) {
        let Some(l) = face.loops.iter().find(|l| l.kind == feature::LoopKind::Outer) else { continue };
        let pts: Vec<[f64; 3]> = l.verts.iter().filter_map(|&v| f.verts.get(v as usize).copied()).collect();
        if pts.is_empty() || pts.iter().any(|p| (p[2] - z).abs() > 1e-6) {
            continue;
        }
        let ext = pts.iter().map(|p| p[0].abs().max(p[1].abs())).fold(0.0, f64::max);
        if best.is_none_or(|(b, _)| ext > b) {
            best = Some((ext, FeatureSel::Face(face.id)));
        }
    }
    best.expect("a top face").1
}

/// The open chain of the block's top rim along `pick` (all its points
/// satisfy it).
pub fn rim_edge(world: &World, pick: impl Fn(&[f64; 3]) -> bool) -> FeatureSel {
    let e = model::find_by_name(world, "part").expect("model");
    let f = model::features_of(world, &e).expect("features");
    feature::chains(&f)
        .into_iter()
        .find(|c| !c.closed && {
            let p = feature::chain_points(&f, c);
            p.len() >= 2 && p.iter().all(|q| (q[2] - 10.0).abs() < 1e-9 && pick(q))
        })
        .map(|c| FeatureSel::Chain(c.id))
        .expect("the rim edge is its own open chain")
}

pub fn sel(name: &str, f: FeatureSel) -> SrcRef {
    SrcRef::Selection(vec![GeoSel { entity: name.into(), feature: f }])
}

pub fn pick(name: &str, f: FeatureSel) -> Vec<GeoSel> {
    vec![GeoSel { entity: name.into(), feature: f }]
}
