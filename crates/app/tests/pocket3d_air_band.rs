//! Issue #1950: the whole-model Adaptive 3D Pocket reaches 2r past the
//! stock side, so its stock ring is cut from the air. Steel that stands
//! in that air band (a clamp, a vise jaw) is in the cutter's reach now:
//! the post gate must name the strike and block the post, never pass it.
//!
//! Gate map (#1950): covers app::operation (generate's Pocket3D arm,
//! open_side), app::fixture_struck, app::posting.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock, TagFixture};
use camcraft_app::geometry::{cube, Point3};
use camcraft_app::links::Links;
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::cl::MoveKind;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-1000-4FL";
const R: f64 = 5.0;
/// The part's top and the stock's top: the 6 mm between is the cut.
const TOP: f64 = 20.0;
const STOCK_TOP: f64 = 26.0;
/// The stock's half side: the stock is the part's footprint.
const HALF: f64 = 15.0;

/// A 30 mm block under a 30 mm stock and a 10 mm end mill.
fn world_with_part() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", cube(Point3::new(-HALF, -HALF, 0.0), Point3::new(HALF, HALF, TOP))));
    let stock = StockShape::Box { dx: 2.0 * HALF, dy: 2.0 * HALF, dz: STOCK_TOP };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, h)
}

/// A steel block that stands from the floor to over the stock top, its
/// near face `gap` past the stock's +X side, tagged as a fixture.
fn add_steel(world: &World, h: &mut History, gap: f64) {
    let x0 = HALF + gap;
    let steel = cube(Point3::new(x0, -5.0, 0.0), Point3::new(x0 + 3.0, 5.0, STOCK_TOP + 4.0));
    h.execute(world, ImportModel::from_mesh("clamp", steel));
    h.execute(world, TagFixture::new(world, "clamp", None).expect("tag the steel"));
}

fn pocket3d(strategy: PocketStrategy, entry: Entry3D) -> OpParams {
    OpParams::Pocket3D {
        pass_start: Default::default(),
        links: Links::ROUGH3D,
        stepover: 4.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(STOCK_TOP + 20.0),
        retract_z: None,
        top_z: Height::fixed(STOCK_TOP),
        bottom_z: Height::fixed(TOP),
        stepdown: 3.0,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        strategy,
        entry,
        helix_radius: 2.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: false,
        method: camcraft_app::machining2d::AdaptiveMethod::Spiral,
        picks: Default::default(),
    }
}

fn generate(world: &World, h: &mut History, strategy: PocketStrategy, entry: Entry3D) -> Result<(), String> {
    let src = SrcRef::Entity("part".into());
    h.execute(world, CreateOperation::new(world, "Rough", TOOL, src, pocket3d(strategy, entry)).expect("op"));
    let g = GenerateOperation::new(world, "Rough").map_err(|e| e.to_string())?;
    h.execute(world, g);
    Ok(())
}

const GAP: f64 = R + 1.0;

/// The second line: the steel's near face is r + 1 mm past the stock
/// side, and it is put there AFTER generation (a clamp slid in later).
/// The Adaptive air band puts the centre up to r past the side, so its
/// disc enters the steel. The post gate must name the strike, never
/// pass it.
#[test]
fn a_clamp_in_the_air_band_blocks_the_post() {
    let (world, mut h) = world_with_part();
    generate(&world, &mut h, PocketStrategy::Adaptive, Entry3D::Plunge).expect("no steel yet: the op generates");
    add_steel(&world, &mut h, GAP);
    let problems = posting::verification_gate(&world);
    assert!(
        problems.iter().any(|p| p.contains("strikes a fixture")),
        "the Adaptive air band reaches the steel {GAP} mm past the stock, so the post is blocked: {problems:?}"
    );

    // The contrast: the old band never reaches that far, so the same
    // steel beside an Offset rings pocket is clear.
    let (world, mut h) = world_with_part();
    add_steel(&world, &mut h, GAP);
    generate(&world, &mut h, PocketStrategy::OffsetRings, Entry3D::Plunge).expect("rings generate");
    let problems = posting::verification_gate(&world);
    assert!(
        !problems.iter().any(|p| p.contains("strikes a fixture")),
        "the old band stays clear of the steel: {problems:?}"
    );
}

/// The first line (round D): steel that stands in the air band when the
/// op is generated refuses the op by name, with what the user can do.
#[test]
fn a_clamp_in_the_air_band_refuses_the_adaptive_op_by_name() {
    let (world, mut h) = world_with_part();
    add_steel(&world, &mut h, GAP);
    let why = generate(&world, &mut h, PocketStrategy::Adaptive, Entry3D::Plunge).expect_err("the steel in the band refuses the op");
    assert!(why.contains(camcraft_app::operation::AIR_BAND_FIXTURE), "{why}");
    assert!(why.contains("'clamp'") && why.contains("Offset rings"), "{why}");
    // Steel out past the band does not refuse it.
    let (world, mut h) = world_with_part();
    add_steel(&world, &mut h, 2.0 * R + 2.0);
    generate(&world, &mut h, PocketStrategy::Adaptive, Entry3D::Plunge).expect("steel past the band is clear");
    let problems = posting::verification_gate(&world);
    assert!(!problems.iter().any(|p| p.contains("strikes a fixture")), "{problems:?}");
}

/// Round D: the air plan enters each level by a plunge in the air at the
/// start of its first pass - the tool's disc clear of the stock - and
/// never bores a helix into the stock, though Helix is the asked entry.
#[test]
fn the_air_plan_enters_by_a_plunge_outside_the_stock() {
    let (world, mut h) = world_with_part();
    generate(&world, &mut h, PocketStrategy::Adaptive, Entry3D::Helix).expect("generate");
    let e = model::find_by_name(&world, "Rough").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let path = &tp.path;
    assert!(path.notes.iter().any(|n| n == camcraft_app::machining3d::AIR_ENTRY), "{:?}", path.notes);
    // Every move that goes down into the cut (below the stock top) at a
    // level's entry is an axial plunge whose disc is clear of the stock.
    let out = |x: f64, y: f64| {
        let (dx, dy) = ((x.abs() - HALF).max(0.0), (y.abs() - HALF).max(0.0));
        dx.hypot(dy) >= R
    };
    let first = path.moves.iter().position(|m| m.to.z < STOCK_TOP - 1e-6).expect("the path cuts");
    let m = &path.moves[first];
    assert_eq!(m.kind, MoveKind::Plunge, "the first move into the cut is a plunge: {m:?}");
    assert!(m.from.x == m.to.x && m.from.y == m.to.y && m.arc.is_none(), "axial: {m:?}");
    assert!(out(m.to.x, m.to.y), "the plunge at ({:.3}, {:.3}) is clear of the stock", m.to.x, m.to.y);
    let warns: Vec<String> = tp.warnings.iter().map(|w| format!("{w:?}")).collect();
    assert!(warns.is_empty(), "{warns:?}");
}

/// Round D: a non-Adaptive whole-model pocket keeps the old band - its
/// cutter centre stays on the stock side of the stock outline.
#[test]
fn a_non_adaptive_whole_model_keeps_the_old_band() {
    let (world, mut h) = world_with_part();
    generate(&world, &mut h, PocketStrategy::OffsetRings, Entry3D::Plunge).expect("generate");
    let e = model::find_by_name(&world, "Rough").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached path");
    let reach = tp.path.moves.iter().filter(|m| m.to.z < STOCK_TOP).map(|m| m.to.x.abs().max(m.to.y.abs())).fold(0.0_f64, f64::max);
    assert!(reach > 0.0 && reach <= HALF, "the centre reaches {reach:.3}, the stock side is at {HALF}");
}
