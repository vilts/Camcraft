//! #2210: the area controls of the 2D kinds (Face, 2D Pocket, 2D Adaptive,
//! 2D Contour) in a setup that holds the part turned 90 degrees about Z.
//! The part is an L with unequal arms, so the turned L, the L as
//! modelled and the L turned the wrong way (-90) are three different
//! regions: a wrong sign, order or pivot cuts a different place.
//!
//! Provenance: an analytic L prism and an exact quarter turn about the
//! part's bounds centre (plain rigid-body math); no vendor material
//! consulted.
//!
//! Gate map (#2210): covers app::setup_align, app::operation::boundary_ctx,
//! app::operation::boundary_picks, app::operation::selection, app::posting,
//! app::stock_sim. Lane shape (M4.4): every test here runs in seconds.

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, EditOperationParams,
    GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machining2d::{Lead, Side};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{
    AreaControls, BoundaryMode, BoundaryShape, Carried, ExtraBoundary, FeatureSel, GeoSel, Height,
    KindInfo, OpParams, OpType, SrcRef,
};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, world::new_world};
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0400-2FL";
/// The part's top; the clearing kinds cut the stock layer above it.
const TOP: f64 = 10.0;

/// The L outline as modelled, counter-clockwise: arms of 30 and 20 mm.
const L: [(f64, f64); 6] = [(0.0, 0.0), (30.0, 0.0), (30.0, 8.0), (10.0, 8.0), (10.0, 20.0), (0.0, 20.0)];

/// The +90 degree turn about the part's bounds centre (15, 10), written
/// out by hand: (x, y) -> (25 - y, x - 5). Exact integers, independent
/// of the quaternion code under test.
fn turn(p: (f64, f64)) -> (f64, f64) {
    (25.0 - p.1, p.0 - 5.0)
}

/// The turned L as two rectangles (min, max): the long arm along Y and
/// the short arm along -X.
const TURNED_L: [((f64, f64), (f64, f64)); 2] = [((17.0, -5.0), (25.0, 25.0)), ((5.0, -5.0), (17.0, 5.0))];

/// An L prism, z in [0, TOP], as modelled or with the turn baked in.
fn l_prism(baked: bool) -> Mesh {
    let pts: Vec<(f64, f64)> = L.iter().map(|&p| if baked { turn(p) } else { p }).collect();
    let b = |i: usize| Point3::new(pts[i].0, pts[i].1, 0.0);
    let t = |i: usize| Point3::new(pts[i].0, pts[i].1, TOP);
    let n = pts.len();
    let mut f = Vec::new();
    for i in 1..n - 1 {
        // The fan from vertex 0 lies inside the L (checked by hand).
        f.push(Triangle::new(t(0), t(i), t(i + 1)));
        f.push(Triangle::new(b(0), b(i + 1), b(i)));
    }
    for i in 0..n {
        let j = (i + 1) % n;
        f.push(Triangle::new(b(i), b(j), t(j)));
        f.push(Triangle::new(b(i), t(j), t(i)));
    }
    Mesh::from_facets(f)
}

/// The part, Setup1 (turned or not), the tool, and one stock box that
/// holds the L in every orientation. The stock is set AFTER the turn, so
/// the two documents hold the very same stock.
fn document(aligned: bool) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", l_prism(!aligned)));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    if aligned {
        h.execute(&world, AlignPartToSetup::preset(&world, "Setup1", AlignPreset::TurnZ90).expect("turn 90 Z"));
    }
    let at = Location { pos: Point3::new(15.0, 10.0, 0.0), ..Default::default() };
    let stock = StockShape::Box { dx: 50.0, dy: 50.0, dz: 12.0 };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    (world, h)
}

/// The part's top face, read from the part's own features (document
/// frame as modelled, or the baked part).
fn top_face(world: &World) -> GeoSel {
    let part = model::find_by_name(world, "part").expect("part");
    let features = model::features_of(world, &part).expect("features");
    let face = features.faces.iter().find(|f| f.normal[2] > 0.9).expect("top face");
    GeoSel { entity: "part".into(), feature: FeatureSel::Face(face.id) }
}

fn contour_params() -> OpParams {
    OpParams::Contour {
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: Height::fixed(20.0),
        top_z: Height::fixed(TOP),
        bottom_z: Height::fixed(6.0),
        stepdown: 4.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 5.0,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        tilt: None,
        links: camcraft_app::links::Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    }
}

/// A clearing kind seeded from the contour, cutting the stock layer from
/// the stock top down to the part's top.
fn clearing(kind: OpType) -> OpParams {
    let mut p = (KindInfo::ALL[kind.ordinal()].seed)(&Carried::of(&contour_params()));
    match &mut p {
        OpParams::Pocket { top_z, bottom_z, stepdown, .. } | OpParams::Adaptive2D { top_z, bottom_z, stepdown, .. } => {
            *top_z = Height::fixed(12.0);
            *bottom_z = Height::fixed(TOP);
            *stepdown = 2.0;
        }
        OpParams::Face { cut_z, clearance_z, .. } => {
            *cut_z = Height::fixed(TOP);
            *clearance_z = Height::fixed(25.0);
        }
        other => panic!("not a clearing kind: {other:?}"),
    }
    p
}

/// Generate one op in a fresh document; judge it with the verify
/// warnings, the program stock simulation and the post.
fn generate(aligned: bool, params: &OpParams, area_of: impl Fn(GeoSel) -> (SrcRef, AreaControls)) -> (Toolpath, String) {
    let (world, mut h) = document(aligned);
    let (src, area) = area_of(top_face(&world));
    h.execute(&world, CreateOperation::new(&world, "Cut", TOOL, src, params.clone()).expect("create"));
    h.execute(&world, EditOperationParams::with_area(&world, "Cut", params.clone(), area).expect("area controls"));
    let cmd = GenerateOperation::new(&world, "Cut").unwrap_or_else(|e| panic!("generates (aligned {aligned}): {e}"));
    h.execute(&world, cmd);
    let entity = model::find_by_name(&world, "Cut").expect("op");
    let cached = model::toolpath_of(&world, &entity).expect("a path in the cache");
    assert!(!cached.path.moves.is_empty(), "the cut contains moves");
    assert!(!cached.warnings.iter().any(|w| w.blocks_post()), "unsafe path: {:?}", cached.warnings);
    let sim = camcraft_app::stock_sim::simulate_document(&world).expect("the turned part is judged");
    assert_eq!(sim.dev.gouge_cells, 0, "gouge {} at {:?}", sim.dev.max_gouge, sim.dev.max_gouge_at);
    let nc = posting::post_setup_unwritten(&world).expect("a clean turned path posts").program;
    (cached.path, nc)
}

/// The aligned document and the baked one cut the same path (to float
/// noise: the alignment reads through a quaternion, the baked part is
/// exact) and post the same program.
fn same_cut(aligned: &Toolpath, baked: &Toolpath, a_nc: &str, b_nc: &str) {
    assert_eq!(aligned.moves.len(), baked.moves.len(), "same move count");
    for (i, (a, b)) in aligned.moves.iter().zip(&baked.moves).enumerate() {
        assert_eq!(a.kind, b.kind, "move {i} kind");
        let d = (a.to.x - b.to.x).abs().max((a.to.y - b.to.y).abs()).max((a.to.z - b.to.z).abs());
        assert!(d < 1e-6, "move {i}: aligned {:?} baked {:?}", a.to, b.to);
    }
    assert_eq!(a_nc, b_nc, "the same setup geometry posts the same program");
}

fn dist_to_rect(p: (f64, f64), (lo, hi): ((f64, f64), (f64, f64))) -> f64 {
    let dx = (lo.0 - p.0).max(0.0).max(p.0 - hi.0);
    let dy = (lo.1 - p.1).max(0.0).max(p.1 - hi.1);
    dx.hypot(dy)
}

fn dist_to_turned_l(p: (f64, f64)) -> f64 {
    TURNED_L.iter().map(|&r| dist_to_rect(p, r)).fold(f64::INFINITY, f64::min)
}

/// Every XY the tool feeds through at or under `z_max`: each feed chord
/// (arcs flattened to 0.01 mm) sampled at least every 0.25 mm, so a pass
/// that crosses a region between its endpoints is seen.
fn cut_points(path: &Toolpath, z_max: f64) -> Vec<(f64, f64)> {
    let mut out = Vec::new();
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        if m.from.z.max(m.to.z) > z_max + 1e-6 {
            continue;
        }
        for (a, b) in m.chords(0.01) {
            let n = ((b.x - a.x).hypot(b.y - a.y) / 0.25).ceil().max(1.0) as usize;
            for i in 0..=n {
                let t = i as f64 / n as f64;
                out.push((a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
            }
        }
    }
    out
}

fn tool_r() -> f64 {
    catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t.diameter * 0.5).expect("tool")
}

/// The picked top face of the L as an EXCLUDE boundary on a stock
/// clearing: the cut must stay a tool radius off the TURNED L, and must
/// reach (8, 15) - a point that lies inside the L as modelled and inside
/// the L turned the wrong way, so either error leaves it uncut.
fn exclude_case(kind: OpType) {
    let params = clearing(kind);
    let area_of = |top: GeoSel| {
        let area = AreaControls {
            extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Picks(vec![top]) }],
            ..Default::default()
        };
        (SrcRef::Stock, area)
    };
    let (aligned, a_nc) = generate(true, &params, area_of);
    let (baked, b_nc) = generate(false, &params, area_of);
    same_cut(&aligned, &baked, &a_nc, &b_nc);
    let r = tool_r();
    let pts = cut_points(&aligned, TOP + 0.5);
    assert!(!pts.is_empty(), "{kind:?} cuts the layer");
    for &p in &pts {
        assert!(dist_to_turned_l(p) >= r - 0.05, "{kind:?}: the tool enters the excluded turned L at {p:?}");
    }
    let near = pts.iter().any(|&(x, y)| (x - 8.0).abs() < 4.0 && (y - 15.0).abs() < 4.0);
    assert!(near, "{kind:?} clears the area that the turned L leaves free near (8, 15)");
}

#[test]
fn turned_adaptive2d_excludes_the_picked_face_where_the_part_lies() {
    exclude_case(OpType::Adaptive2D);
}

#[test]
fn turned_pocket2d_excludes_the_picked_face_where_the_part_lies() {
    exclude_case(OpType::Pocket);
}

/// Face refuses an exclude boundary by its own law, so it faces the
/// picked top grown 1 mm: its band is the TURNED L's box, which reaches
/// Y 25 where the L as modelled stops at Y 20, and stays in X [4, 26].
#[test]
fn turned_face_clears_the_band_of_the_turned_pick() {
    let params = clearing(OpType::Face);
    let area_of = |top: GeoSel| (SrcRef::Selection(vec![top]), AreaControls { offset: 1.0, ..Default::default() });
    let (aligned, a_nc) = generate(true, &params, area_of);
    let (baked, b_nc) = generate(false, &params, area_of);
    same_cut(&aligned, &baked, &a_nc, &b_nc);
    let pts = cut_points(&aligned, TOP + 0.5);
    assert!(pts.iter().any(|&(_, y)| y > 23.0), "the face pass covers the turned long arm's end");
    // The turned box grown 1 is X [4, 26]; the box as modelled is X [-1, 31].
    let r = tool_r();
    for &(x, _) in &pts {
        assert!((4.0 - r - 0.05..=26.0 + r + 0.05).contains(&x), "the face pass leaves the turned band at X {x}");
    }
}

/// A contour of the picked top face, grown 1 mm by the area offset: the
/// ride stays r + 1 off the TURNED L, and passes beside its long arm at
/// (25 + r + 1, 0) - inside the L as modelled, far from the -90 L.
#[test]
fn turned_contour2d_offset_rides_the_turned_outline() {
    let params = contour_params();
    let area_of = |top: GeoSel| (SrcRef::Selection(vec![top]), AreaControls { offset: 1.0, ..Default::default() });
    let (aligned, a_nc) = generate(true, &params, area_of);
    let (baked, b_nc) = generate(false, &params, area_of);
    same_cut(&aligned, &baked, &a_nc, &b_nc);
    let r = tool_r();
    let pts = cut_points(&aligned, TOP - 0.5);
    assert!(!pts.is_empty(), "the contour cuts");
    for &p in &pts {
        assert!(dist_to_turned_l(p) >= r + 1.0 - 0.05, "the ride comes closer than r + 1 to the turned L at {p:?}");
    }
    let side = 25.0 + r + 1.0;
    let beside = pts.iter().any(|&(x, y)| (x - side).abs() < 0.05 && y.abs() < 4.0);
    assert!(beside, "the ride passes beside the turned long arm at X {side}");
}

/// An unaligned setup reads nothing new: the same area on the L as
/// modelled generates as it always did (the door's early return).
#[test]
fn as_modelled_area_controls_never_meet_the_aligned_door() {
    let (world, mut h) = document(false);
    let top = top_face(&world);
    let area = AreaControls { offset: 1.0, ..Default::default() };
    let params = contour_params();
    h.execute(&world, CreateOperation::new(&world, "Cut", TOOL, SrcRef::Selection(vec![top]), params.clone()).expect("create"));
    h.execute(&world, EditOperationParams::with_area(&world, "Cut", params, area).expect("area"));
    let entity = model::find_by_name(&world, "Cut").expect("op");
    let op = model::operation_of(&world, &entity).expect("operation");
    assert_eq!(camcraft_app::setup_align::generation_problem(&world, &op), None);
}
