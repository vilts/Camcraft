//! Provenance: #1743, an analytic wedge and exact rigid half-turn; no vendor material consulted.
//! Gate map (#1743): covers app::setup_align, app::operation (job, material, heights), app::posting.

use camcraft_app::cl::Toolpath;
use camcraft_app::commands::{
    AddTool, AlignPartToSetup, CreateOperation, CreateSetup, EditOperationParams,
    GenerateOperation, History, ImportModel, SetStock,
};
use camcraft_app::finishing3d::FinishPattern;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::links::Links;
use camcraft_app::machining2d::PocketStrategy;
use camcraft_app::machining3d::Entry3D;
use camcraft_app::operation::{
    AreaControls, BoundaryMode, BoundaryShape, ExtraBoundary, FeatureSel, GeoSel, Height,
    HeightReference, OpParams, SrcRef,
};
use camcraft_app::schema::Location;
use camcraft_app::setup_align::AlignPreset;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, world::new_world};
use flecs_ecs::prelude::World;

/// A closed wedge: x [-8,8], y [-6,10], floor 0, sloping roof z=6+x/4.
/// Its back edge tapers from y=6 at the left to y=10 at the right.
/// Its X half-turn about (0,2,4) is (x,4-y,8-z), all exact arithmetic.
fn wedge(flipped: bool) -> Mesh {
    let p = |x: f64, y: f64, z: f64| {
        if flipped {
            Point3::new(x, 4.0 - y, 8.0 - z)
        } else {
            Point3::new(x, y, z)
        }
    };
    let v = [
        p(-8.0, -6.0, 0.0),
        p(8.0, -6.0, 0.0),
        p(8.0, 10.0, 0.0),
        p(-8.0, 6.0, 0.0),
        p(-8.0, -6.0, 4.0),
        p(8.0, -6.0, 8.0),
        p(8.0, 10.0, 8.0),
        p(-8.0, 6.0, 4.0),
    ];
    let quads = [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 1, 5, 4],
        [1, 2, 6, 5],
        [2, 3, 7, 6],
        [3, 0, 4, 7],
    ];
    Mesh::from_facets(
        quads
            .into_iter()
            .flat_map(|[a, b, c, d]| {
                [
                    Triangle::new(v[a], v[b], v[c]),
                    Triangle::new(v[a], v[c], v[d]),
                ]
            })
            .collect(),
    )
}

fn document(aligned: bool, tool_name: &str) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::from_mesh("part", wedge(aligned)));
    let stock = StockShape::Box {
        dx: 24.0,
        dy: 24.0,
        dz: 12.0,
    };
    let at = Location {
        pos: Point3::new(0.0, 2.0, -2.0),
        ..Default::default()
    };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == tool_name)
        .expect("catalog tool")
        .1;
    h.execute(&world, AddTool::new(&world, tool_name, tool).expect("tool"));
    h.execute(
        &world,
        CreateSetup::new(&world, None, None).expect("Setup1"),
    );
    h.execute(
        &world,
        CreateSetup::new(&world, None, Some("Setup1")).expect("Setup2"),
    );
    if aligned {
        h.execute(
            &world,
            AlignPartToSetup::preset(&world, "Setup2", AlignPreset::FlipX).expect("flip"),
        );
    }
    (world, h)
}

fn rough(strategy: PocketStrategy) -> OpParams {
    OpParams::Pocket3D {
        pass_start: Default::default(),
        links: Links::ROUGH3D,
        stepover: 1.5,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(16.0),
        retract_z: None,
        top_z: Height::fixed(10.0),
        bottom_z: Height::fixed(4.0),
        stepdown: 2.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        rest_rough: false,
        optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        feed_opt: false,
        shell: true,
        method: camcraft_app::machining2d::AdaptiveMethod::Spiral,
        picks: Default::default(),
    }
}

fn finish() -> OpParams {
    OpParams::Finish3D {
        pattern: FinishPattern::Waterline,
        direction_angle: 0.0,
        pass_start: camcraft_app::machining2d::PassStart::Auto,
        stepover: None,
        tolerance: 0.1,
        clearance_z: Height::fixed(16.0),
        retract_z: None,
        top_z: Height::of(HeightReference::TopOfModel, -0.5),
        bottom_z: Height::of(HeightReference::BottomOfModel, 4.5),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        fill_openings: None,
        gap_link: camcraft_app::finishing3d::GapLink::Jump,
        waterlines: true,
        picks: Default::default(),
    }
}

fn generate(aligned: bool, tool: &str, params: OpParams) -> (Toolpath, String) {
    generate_area(aligned, tool, params, AreaControls::default())
}

fn generate_area(
    aligned: bool,
    tool: &str,
    params: OpParams,
    area: AreaControls,
) -> (Toolpath, String) {
    generate_case(aligned, tool, params, area, false)
}

fn generate_case(
    aligned: bool,
    tool: &str,
    params: OpParams,
    mut area: AreaControls,
    picked: bool,
) -> (Toolpath, String) {
    let (world, mut h) = document(aligned, tool);
    let src = if picked {
        let part = model::find_by_name(&world, "part").expect("part");
        let features = model::features_of(&world, &part).expect("features");
        let roof = features
            .faces
            .iter()
            .find(|f| f.normal[0].abs() > 0.1 && f.normal[2].abs() > 0.8)
            .expect("sloped roof");
        let picks = vec![GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(roof.id),
        }];
        area.extra.push(ExtraBoundary {
            mode: BoundaryMode::Include,
            shape: BoundaryShape::Picks(picks.clone()),
        });
        SrcRef::Selection(picks)
    } else {
        SrcRef::Entity("part".into())
    };
    h.execute(
        &world,
        CreateOperation::new(&world, "Cut", tool, src, params.clone()).expect("create"),
    );
    if !area.is_default() {
        h.execute(
            &world,
            EditOperationParams::with_area(&world, "Cut", params, area).expect("area controls"),
        );
    }
    h.execute(
        &world,
        GenerateOperation::new(&world, "Cut").expect("3D operation generates in Setup2"),
    );
    let entity = model::find_by_name(&world, "Cut").expect("op");
    let cached = model::toolpath_of(&world, &entity).expect("path");
    assert!(!cached.path.moves.is_empty(), "the cut contains moves");
    assert!(
        !cached.warnings.iter().any(|w| w.blocks_post()),
        "unsafe path: {:?}",
        cached.warnings
    );
    let sim =
        camcraft_app::stock_sim::simulate_document(&world).expect("the flipped part is judged");
    assert_eq!(
        sim.dev.gouge_cells, 0,
        "gouge {} at {:?}",
        sim.dev.max_gouge, sim.dev.max_gouge_at
    );
    let nc = posting::post_setup_unwritten(&world)
        .expect("clean flipped 3D path posts")
        .program;
    assert!(nc.contains("G54"), "setup offset posts");
    (cached.path, nc)
}

fn equivalent(tool: &str, params: OpParams) {
    let (aligned, nc) = generate(true, tool, params.clone());
    let (baked, reference_nc) = generate(false, tool, params);
    assert_eq!(
        aligned, baked,
        "the flip is baked into the part, never into the tool axis"
    );
    assert_eq!(
        nc, reference_nc,
        "identical setup geometry posts identical NC"
    );
}

#[test]
fn flipped_pocket3d_generates_verifies_and_posts_like_baked_geometry() {
    equivalent("EM-0400-2FL", rough(PocketStrategy::OffsetRings));
}

#[test]
fn flipped_adaptive3d_generates_verifies_and_posts_like_baked_geometry() {
    equivalent("EM-0400-2FL", rough(PocketStrategy::Adaptive));
}

#[test]
fn flipped_finish3d_generates_verifies_and_posts_like_baked_geometry() {
    equivalent("BN-0600", finish());
}

#[test]
fn flipped_pocket3d_area_controls_match_baked_geometry() {
    let area = AreaControls {
        offset: -0.25,
        extra: vec![ExtraBoundary {
            mode: BoundaryMode::Include,
            shape: BoundaryShape::Silhouette,
        }],
        ..Default::default()
    };
    let (aligned, nc) = generate_area(
        true,
        "EM-0400-2FL",
        rough(PocketStrategy::OffsetRings),
        area.clone(),
    );
    let (baked, reference_nc) = generate_area(
        false,
        "EM-0400-2FL",
        rough(PocketStrategy::OffsetRings),
        area,
    );
    assert_eq!(
        aligned, baked,
        "the controlled region reads the aligned part"
    );
    assert_eq!(nc, reference_nc);
    let (plain, _) = generate(false, "EM-0400-2FL", rough(PocketStrategy::OffsetRings));
    assert_ne!(
        baked.moves, plain.moves,
        "the area restriction changes the path"
    );
}

#[test]
fn flipped_finish3d_picked_source_and_boundary_match_baked_geometry() {
    let (aligned, nc) = generate_case(true, "BN-0600", finish(), AreaControls::default(), true);
    let (baked, reference_nc) =
        generate_case(false, "BN-0600", finish(), AreaControls::default(), true);
    assert_eq!(
        aligned, baked,
        "picked geometry is read through the setup frame"
    );
    assert_eq!(nc, reference_nc);
}

#[test]
fn enabling_aligned_3d_keeps_tilt_refused_and_passes_2d_area() {
    let (world, _) = document(true, "EM-0400-2FL");
    let mut params = camcraft_app::operation::retype_params(
        &rough(PocketStrategy::OffsetRings),
        camcraft_app::operation::OpType::Face as usize,
    );
    let OpParams::Face { tilt, .. } = &mut params else {
        panic!("face params")
    };
    *tilt = Some(camcraft_app::operation::Tilt {
        basis_version: 1,
        a_deg: 45.0,
        b_deg: 0.0,
        origin: [0.0; 3],
    });
    let mut op = camcraft_app::schema::Operation {
        tool: "EM-0400-2FL".into(),
        src: SrcRef::Entity("part".into()),
        params,
        stamp: None,
        setup: Some("Setup2".into()),
        area: AreaControls::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    };
    let why =
        camcraft_app::setup_align::generation_problem(&world, &op).expect("tilt is still refused");
    assert!(why.contains("tilted plane is not supported yet"), "{why}");
    let OpParams::Face { tilt, .. } = &mut op.params else {
        panic!("face params")
    };
    *tilt = None;
    op.area.offset = 0.25;
    // #2210: the 2D area controls resolve in the setup frame now.
    assert_eq!(camcraft_app::setup_align::generation_problem(&world, &op), None);
}
