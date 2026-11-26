//! #1559: a 2D ladder whose Top is set below the material standing over
//! it (here: Top 4 mm under a faced top, to skip the air pass at the
//! faced top) enters safely: it rapids no lower than the plunge height
//! over the remaining material and feeds the rest of the way down, so
//! the rapid law (#96, #1066, #1091) finds nothing to convict and the
//! program posts. Before the fix the entry rapided to Top + the plunge
//! height, inside the uncut stock, and verify refused the op.
//!
//! Gate map (#1559): covers camcraft_app::operation (generate,
//! top_stock), camcraft_motion::machining2d (levels, descent,
//! contour_bore, adaptive_troch), camcraft_motion::approach.
//!
//! Provenance: our own fixtures (common::solid_part); the shop rule
//! never to rapid into uncut stock. No vendor material.

mod common;

use common::solid_part::{circle, plate, rect};
use flecs_ecs::prelude::World;

use camcraft_app::cl::{MoveKind, Toolpath};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{Mesh, Triangle, feature};
use camcraft_app::machining2d::{AdaptiveMethod, AdaptiveTrochParams, Lead, Side, TrochoidReturn};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{CARRIED_STOCK_NOTE, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef, TOP_STOCK_WARNING};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

const TOOL: &str = "EM-0600-4FL";
/// The stock top (0.5 over the part) and the faced top.
const STOCK_TOP: f64 = 10.5;
const FACED: f64 = 10.0;
/// The op's Top: 4 mm under the faced top - the first real cut.
const TOP: f64 = 6.0;
/// The approach default plunge height (`DEFAULT_PLUNGE_HEIGHT`, M11.3).
const PLUNGE_HEIGHT: f64 = 2.0;

/// A 100 x 60 x [0, 10] plate with a centred Ø40 through bore.
fn plate_bore() -> Vec<Triangle> {
    let mut f = Vec::new();
    plate(&rect(-50.0, -30.0, 50.0, 30.0), Some((&circle((0.0, 0.0), 20.0, 64), 0.0)), 0.0, 10.0, &mut f);
    f
}

/// The plate in a 110 x 70 stock 0.5 mm taller, faced to the part top.
fn faced_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("plate", Mesh::from_facets(plate_bore())));
    h.execute(
        &world,
        SetStock::new(&world, StockShape::Box { dx: 110.0, dy: 70.0, dz: STOCK_TOP }, Default::default()).expect("stock"),
    );
    let aabb = model::stock_world_aabb_for(&world, None).expect("stock box");
    assert!((aabb.max.z - STOCK_TOP).abs() < 1e-9 && aabb.min.z.abs() < 1e-9, "stock {aabb:?}");
    let t = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, t).expect("tool"));
    let face = OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 4.0,
        clearance_z: Height::fixed(20.0),
        cut_z: Height::fixed(FACED),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    };
    h.execute(&world, CreateOperation::new(&world, "Face", TOOL, SrcRef::Stock, face).expect("face"));
    h.execute(&world, GenerateOperation::new(&world, "Face").expect("face generation"));
    (world, h)
}

/// The bore's closed rim chain at the plate top.
fn bore_chain(world: &World) -> Vec<GeoSel> {
    let e = model::find_by_name(world, "plate").expect("plate");
    let f = model::features_of(world, &e).expect("features");
    let id = feature::chains(&f)
        .into_iter()
        .find(|c| {
            let pts = feature::chain_points(&f, c);
            c.closed && pts.iter().all(|p| p[0].hypot(p[1]) < 20.5 && (p[2] - 10.0).abs() < 1e-9)
        })
        .map(|c| c.id)
        .expect("the bore's top rim chains closed");
    vec![GeoSel { entity: "plate".into(), feature: FeatureSel::Chain(id) }]
}

fn adaptive(method: AdaptiveMethod, top: f64) -> OpParams {
    adaptive_with(method, top, Default::default())
}

fn adaptive_with(method: AdaptiveMethod, top: f64, troch: AdaptiveTrochParams) -> OpParams {
    OpParams::Adaptive2D {
        rest: None,
        blend_radius: None,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
        tilt: None,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0),
        retract_z: Some(Height::fixed(15.0)),
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(0.0),
        stepdown: 4.0,
        feed: 1000.0,
        plunge_feed: 300.0,
        climb: true,
        entry: Entry3D::Helix,
        helix_radius: 2.0,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        method,
        troch,
        corner_relief: Default::default(),
        pass_start: Default::default(),
        picks: Default::default(),
    }
}

fn contour_inside(entry: Entry3D, top: f64) -> OpParams {
    OpParams::Contour {
        tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(20.0),
        retract_z: Height::fixed(15.0),
        top_z: Height::fixed(top),
        bottom_z: Height::fixed(0.0),
        stepdown: 4.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Inside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: camcraft_app::links::Links::PLANAR,
        chain_overrides: Vec::new(),
        picks: Default::default(),
    }
}

/// Create and generate on the bore picks; the cached path and verdict.
fn run(world: &World, h: &mut History, name: &str, p: OpParams) -> (Toolpath, Vec<Warning>) {
    let picks = bore_chain(world);
    h.execute(world, CreateOperation::new(world, name, TOOL, SrcRef::Selection(picks), p).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{name}");
    let tp = model::toolpath_of(world, &e).expect("cached path");
    (tp.path.clone(), tp.warnings.clone())
}

/// The law the issue asks for: until the tool first reaches the first
/// level, no rapid ends under the plunge height over the faced top, and
/// the first move under the faced top is a feed. No air level at the
/// faced top; the program verifies clean and posts.
fn assert_safe_entry(world: &World, name: &str, path: &Toolpath, warns: &[Warning]) {
    let safe = FACED + PLUNGE_HEIGHT - 1e-6;
    let first_level = path.moves.iter().position(|m| m.to.z <= TOP + 1e-6).expect("reaches the first level");
    for (i, m) in path.moves[..=first_level].iter().enumerate() {
        if m.kind == MoveKind::Rapid {
            assert!(m.to.z >= safe, "{name}: rapid {i} ends at z {:.3}, under {safe:.3} (inside the faced stock)", m.to.z);
        }
    }
    let first_under = path.moves.iter().find(|m| m.to.z < FACED - 1e-6).expect("cuts under the faced top");
    assert_ne!(first_under.kind, MoveKind::Rapid, "{name}: the descent into stock is a rapid");
    // A level is a planar feed; a trochoidal microlift return (0.3 mm
    // over its level) is not one.
    let air_level = path.moves.iter().any(|m| m.kind == MoveKind::Feed && m.from.z == m.to.z && m.to.z > TOP + 1.0);
    assert!(!air_level, "{name}: a level above Top was cut");
    assert!(warns.is_empty(), "{name}: verify convicts {warns:?}");
    assert!(
        path.notes.iter().any(|n| n.starts_with(CARRIED_STOCK_NOTE) && n.contains("Z6.000") && n.contains("Z10.000")),
        "{name}: the carried stock is said: {:?}",
        path.notes
    );
    assert!(path.notes.iter().all(|n| !n.starts_with(TOP_STOCK_WARNING)), "{name}: {:?}", path.notes);
    let _ = world;
}

fn posts(world: &World) {
    if let Err(e) = camcraft_app::posting::post_setup_unwritten(world) {
        panic!("the program must post: {e}");
    }
}

#[test]
fn adaptive_spiral_with_top_under_the_faced_top_feeds_through_the_carried_stock() {
    let (world, mut h) = faced_world();
    let (path, warns) = run(&world, &mut h, "Bore", adaptive(AdaptiveMethod::Spiral, TOP));
    assert_safe_entry(&world, "adaptive spiral", &path, &warns);
    posts(&world);
}

#[test]
fn adaptive_trochoidal_with_top_under_the_faced_top_feeds_through_the_carried_stock() {
    let (world, mut h) = faced_world();
    let (path, warns) = run(&world, &mut h, "Bore", adaptive(AdaptiveMethod::Trochoidal, TOP));
    assert_safe_entry(&world, "adaptive trochoidal", &path, &warns);
    posts(&world);
    // The retract return between loops too.
    let (world, mut h) = faced_world();
    let troch = AdaptiveTrochParams { ret: Some(TrochoidReturn::Retract), ..Default::default() };
    let (path, warns) = run(&world, &mut h, "Bore", adaptive_with(AdaptiveMethod::Trochoidal, TOP, troch));
    assert_safe_entry(&world, "adaptive trochoidal, retract returns", &path, &warns);
    posts(&world);
}

#[test]
fn contour_inside_with_top_under_the_faced_top_feeds_through_the_carried_stock() {
    for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp] {
        let (world, mut h) = faced_world();
        let (path, warns) = run(&world, &mut h, "Bore", contour_inside(entry, TOP));
        assert_safe_entry(&world, &format!("contour inside {entry:?}"), &path, &warns);
        posts(&world);
    }
}

#[test]
fn top_at_the_faced_top_carries_nothing_and_keeps_its_path() {
    // The ordinary case: Top on the material - no carried note, and the
    // entry rapids to Top + the plunge height exactly as before.
    let (world, mut h) = faced_world();
    let (path, warns) = run(&world, &mut h, "Bore", contour_inside(Entry3D::Plunge, FACED));
    assert!(warns.is_empty(), "{warns:?}");
    assert!(path.notes.iter().all(|n| !n.starts_with(CARRIED_STOCK_NOTE)), "{:?}", path.notes);
    let lowest_first_rapid = path
        .moves
        .iter()
        .take_while(|m| m.kind != MoveKind::Plunge && m.kind != MoveKind::Feed)
        .map(|m| m.to.z)
        .fold(f64::INFINITY, f64::min);
    assert!((lowest_first_rapid - (FACED + PLUNGE_HEIGHT)).abs() < 1e-6, "first rapid stop {lowest_first_rapid}");
}

#[test]
fn a_finish_after_the_rough_enters_through_the_cleared_bore_unchanged() {
    // The rough clears the bore; a one-level finish at the bottom then
    // rapids down through air the program ahead cut. Nothing is carried
    // there (no second generation, no note): its entry is the plain one.
    let (world, mut h) = faced_world();
    let (_, warns) = run(&world, &mut h, "Op2Rough", adaptive(AdaptiveMethod::Spiral, TOP));
    assert!(warns.is_empty(), "{warns:?}");
    let mut finish = contour_inside(Entry3D::Plunge, 1.0);
    if let OpParams::Contour { stepdown, .. } = &mut finish {
        *stepdown = 10.0;
    }
    let (path, warns) = run(&world, &mut h, "Op3Finish", finish);
    assert!(warns.is_empty(), "{warns:?}");
    assert!(path.notes.iter().all(|n| !n.starts_with(CARRIED_STOCK_NOTE)), "{:?}", path.notes);
    let entry = path
        .moves
        .iter()
        .find(|m| m.kind == MoveKind::Rapid && m.to.z < m.from.z && m.from.x == m.to.x && m.from.y == m.to.y && m.to.z < FACED)
        .map(|m| m.to.z);
    assert_eq!(entry.map(|z| (z - 1.0 - PLUNGE_HEIGHT).abs() < 1e-6), Some(true), "the plain entry to Top + the plunge height: {entry:?}");
    posts(&world);
}
