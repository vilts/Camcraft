//! Issue #2240: 2D Adaptive on `fig8.step` (two round areas with a narrow neck)
//! must verify with zero feed-crossing warnings and post cleanly without being blocked.
//!
//! Gate map (#2240): covers app::operation (generate, area), app::posting,
//! app::stock_sim, sim::verify, motion::machining2d (pocket_emit, pocket_hsm).
//! Provenance: regression for #2240; clean-room, no vendor material.

use std::path::Path;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::geometry::Point3;
use camcraft_app::links::Links;
use camcraft_app::load::DEFAULT_OPTIMAL_LOAD;
use camcraft_app::machining2d::{AdaptiveMethod, AdaptiveTrochParams};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::model;
use camcraft_app::operation::{
    AreaControls, BoundaryMode, BoundaryShape, ExtraBoundary, FeatureSel, GeoSel, Height, OpParams, SrcRef,
};
use camcraft_app::pick::feature_graph::feature_graph;
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::crossed_pairs;
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "EM-0800-4FL";

fn fig8_params() -> OpParams {
    OpParams::Adaptive2D {
        stepover: 3.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(27.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 5.0,
        feed: 2400.0,
        plunge_feed: 500.0,
        climb: true,
        entry: Entry3D::Helix,
        helix_radius: 1.0,
        optimal_load: DEFAULT_OPTIMAL_LOAD,
        method: AdaptiveMethod::Spiral,
        troch: AdaptiveTrochParams::default(),
        tilt: None,
        blend_turns: false,
        blend_radius: None,
        links: Links::PLANAR,
    }
}

fn setup_fig8(stock_dx: f64, stock_dy: f64) -> (World, History, String) {
    let world = new_world();
    let mut h = History::default();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fig8.step");
    let imp = ImportModel::new(&path).expect("import fig8.step");
    let part_name = imp.name.clone();
    h.execute(&world, imp);
    let part = model::find_by_name(&world, &part_name).expect("part entity");
    let b = model::entity_world_aabb(&world, &part).expect("part aabb");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("EM-0800-4FL");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    let at = Location {
        pos: Point3::new(0.5 * (b.min.x + b.max.x), 0.5 * (b.min.y + b.max.y), 0.0),
        ..Default::default()
    };
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: stock_dx, dy: stock_dy, dz: 22.0 }, at).expect("stock"));
    (world, h, part_name)
}

fn assert_clean_post(world: &World, h: &mut History, op_name: &str, crossed_before: &str) {
    let cmd = GenerateOperation::new(world, op_name).expect("2D Adaptive1 generates");
    h.execute(world, cmd);
    let entity = model::find_by_name(world, op_name).expect("op entity");
    let cached = model::toolpath_of(world, &entity).expect("cached toolpath");
    assert!(!cached.path.moves.is_empty(), "toolpath has moves");

    let pairs = crossed_pairs(&cached.path, 0.01);
    assert!(
        pairs.is_empty(),
        "expected 0 crossing feed pairs (before fix, {crossed_before} crossed); got {pairs:?}, warnings: {:?}",
        cached.warnings
    );
    let cross_warns: Vec<String> = cached
        .warnings
        .iter()
        .map(ToString::to_string)
        .filter(|s| s.contains("cross"))
        .collect();
    assert!(
        cross_warns.is_empty(),
        "expected 0 crossing warnings (before fix: {crossed_before}); got {cross_warns:?}"
    );
    assert!(
        !cached.warnings.iter().any(|w| w.blocks_post()),
        "no blocking verify warnings expected: {:?}",
        cached.warnings
    );
    // posting::post_setup_unwritten runs verification_report_by -> sim_clause (3D dexel gouge check)
    // and blocks if any verify blocker or gouge remains.
    let posted = posting::post_setup_unwritten(world).expect("post is not blocked");
    assert!(!posted.program.is_empty(), "posted G-code is non-empty");
}

#[test]
#[ignore = "release heavy lane: fig8 generation and verification exceed 60 seconds in debug; run with --release -- --ignored"]
fn adaptive2d_on_fig8_model_in_tight_stock_has_no_crossings_and_posts() {
    let (world, mut h, part_name) = setup_fig8(90.0, 200.0);
    let params = fig8_params();
    h.execute(&world, CreateOperation::new(&world, "2D Adaptive1", TOOL, SrcRef::Entity(part_name), params).expect("create op"));
    // Before the fix: 4 crossing warnings at shared depth on pairs [69, 71],
    // [197, 199], [1003, 1005], and [1131, 1133], blocking the post.
    assert_clean_post(&world, &mut h, "2D Adaptive1", "[69, 71], [197, 199], [1003, 1005], [1131, 1133]");
}

#[test]
#[ignore = "release heavy lane: fig8 generation and verification exceed 60 seconds in debug; run with --release -- --ignored"]
fn adaptive2d_on_fig8_stock_excluding_top_face_has_no_crossings_and_posts() {
    let (world, mut h, part_name) = setup_fig8(100.0, 220.0);
    let part = model::find_by_name(&world, &part_name).expect("part");
    let facets = model::world_facets(&world, &part).expect("facets");
    let fg = feature_graph(&facets);
    let top_face = fg.features.faces.iter().find(|f| f.normal[2] > 0.9).expect("top face");
    let top_sel = GeoSel { entity: part_name, feature: FeatureSel::Face(top_face.id) };

    let params = fig8_params();
    let area = AreaControls {
        extra: vec![ExtraBoundary { mode: BoundaryMode::Exclude, shape: BoundaryShape::Picks(vec![top_sel]) }],
        ..Default::default()
    };
    h.execute(&world, CreateOperation::new(&world, "2D Adaptive1", TOOL, SrcRef::Stock, params.clone()).expect("create op"));
    h.execute(&world, EditOperationParams::with_area(&world, "2D Adaptive1", params, area).expect("area"));
    // Before the fix: 4 crossing warnings at shared depth on pairs [69, 71],
    // [197, 199], [1338, 1340], and [1466, 1468], blocking the post.
    assert_clean_post(&world, &mut h, "2D Adaptive1", "[69, 71], [197, 199], [1338, 1340], [1466, 1468]");
}
