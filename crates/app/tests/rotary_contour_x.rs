//! Provenance: original cylindrical setup and full post checks using the repository's A-axis machine.
//! Gate map (M4.4): covers camcraft_app::operation, camcraft_app::posting, camcraft_app::commands.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetStock,
};
use camcraft_app::geometry::{quat_from_axis_angle, Mesh, Point3, Triangle};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

fn cylinder() -> Vec<Triangle> {
    let point = |x, j: usize, radius| {
        let angle = std::f64::consts::TAU * j as f64 / 16.0;
        Point3::new(x, radius * angle.cos(), radius * angle.sin())
    };
    let mut facets = Vec::new();
    for j in 0..16 {
        let (a, b, c, d) = (
            point(-16.0, j, 6.0),
            point(-16.0, j + 1, 6.0),
            point(16.0, j, 6.0),
            point(16.0, j + 1, 6.0),
        );
        facets.push(Triangle::new(a, b, d));
        facets.push(Triangle::new(a, d, c));
        facets.push(Triangle::new(point(-16.0, j, 0.0), b, a));
        facets.push(Triangle::new(point(16.0, j, 0.0), c, d));
    }
    facets
}

fn world(stock_diameter: f64) -> World {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    let machine = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/machines/tormach-770mx-microarc4.mch");
    history.execute(
        &world,
        LoadMachine::from_file(&world, &machine).expect("mount the A-axis machine"),
    );
    history.execute(
        &world,
        SelectPost::new(&world, PostRef::Named("Tormach PathPilot".into())).expect("select post"),
    );
    let mut tool = catalog()
        .into_iter()
        .find(|(name, _)| *name == "BN-0600")
        .map(|(_, tool)| tool)
        .expect("catalog ball");
    tool.rpm = 8000.0;
    history.execute(
        &world,
        AddTool::new(&world, "BN-0600", tool).expect("add tool"),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Cylinder {
                diameter: stock_diameter,
                height: 32.0,
            },
            Location {
                pos: Point3::new(-16.0, 0.0, 0.0),
                rot: quat_from_axis_angle(1, std::f64::consts::FRAC_PI_2),
            },
        )
        .expect("coaxial round stock"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("bar", Mesh::from_facets(cylinder())),
    );
    let params = OpParams::RotaryContour {
        tolerance: 0.1,
        clearance_z: Height::fixed(40.0),
        retract_z: Some(Height::fixed(30.0)),
        top_z: Height::fixed(14.0),
        bottom_z: Height::fixed(-14.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
        stated_axis: None,
        picks: Default::default(),
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "OP",
            "BN-0600",
            SrcRef::Entity("bar".into()),
            params,
        )
        .expect("create Rotary Contour"),
    );
    history.execute(
        &world,
        GenerateOperation::new(&world, "OP").expect("generate X-axis rings"),
    );
    world
}

fn cache(world: &World) -> camcraft_app::schema::Toolpath {
    let operation = model::find_by_name(world, "OP").expect("operation");
    model::toolpath_of(world, &operation).expect("the rings must generate a cached path")
}

#[test]
fn rotary_contour_x_verifies_and_posts_finite_a_moves() {
    let world = world(12.4);
    let cache = cache(&world);
    let blockers: Vec<_> = cache
        .warnings
        .iter()
        .filter(|warning| warning.blocks_post())
        .collect();
    assert!(
        blockers.is_empty(),
        "close round stock must verify: {blockers:?}"
    );
    let report = posting::verification_report(&world);
    assert!(
        report.problems.is_empty(),
        "verification problems: {:?}",
        report.problems
    );
    let posted = posting::post_setup_unwritten(&world).expect("safe X-axis contour posts on A");
    let angles: Vec<_> = posted
        .program
        .split_whitespace()
        .filter_map(|word| word.strip_prefix('A'))
        .filter(|word| word.starts_with(|ch: char| ch.is_ascii_digit() || ch == '-' || ch == '+'))
        .map(|word| word.parse::<f64>().expect("numeric A word"))
        .collect();
    assert!(
        angles.len() > 20,
        "expected continuous A moves: {}",
        posted.program
    );
    assert!(
        angles.iter().all(|angle| angle.is_finite()),
        "non-finite A moves: {angles:?}"
    );
    let lo = angles.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = angles.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(hi - lo > 180.0, "A does not traverse the rings: {lo}..{hi}");
}

#[test]
fn rotary_contour_x_in_oversized_stock_still_blocks_posting() {
    let world = world(50.0);
    let cache = cache(&world);
    assert!(
        cache
            .warnings
            .iter()
            .any(|warning| matches!(warning, Warning::ShankHit { part: false, .. })),
        "the shank must hit the uncut stock: {:?}",
        cache.warnings
    );
    let error =
        posting::post_setup_unwritten(&world).expect_err("unsafe rotary rings must not post");
    assert!(!error.is_empty(), "the refusal must name the unsafe input");
}
