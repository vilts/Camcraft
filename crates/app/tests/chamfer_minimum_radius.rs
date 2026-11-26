//! #3153 operation-layer pins for the 2D Chamfer cone band.
//!
//! Gate map (#3153): covers app::operation (generate, params, staleness),
//! app::project (ParamsDto and v80 migration), app::posting, and
//! motion::machining2d::chamfer through the public application seam.

use camcraft_app::cl::MoveKind;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::machining2d::Side;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::schema::Operation;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, ToolKind};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

const TOOL: &str = "CM-TEST-0600-90";

fn taper(diameter: f64, flute_len: f64) -> Tool {
    Tool {
        kind: ToolKind::Taper,
        diameter,
        corner_r: 0.0,
        tip_angle: 90.0,
        pitch: 0.0,
        nose_radius: 0.0,
        width: 0.0,
        flute_len,
        shank_d: diameter,
        shank_len: 40.0,
        holder_d: 44.0,
        holder_len: 35.0,
        material_group: "aluminum".into(),
        center_cutting: true,
        rpm: 8000.0,
        feed: 1500.0,
        tool_material: None,
        flutes: None,
        orientation: None,
        pitch_min: 0.0,
        thread_hand: Default::default(),
        catalog: camcraft_tools::catalog_fields::ToolCatalog::default(),
        cutting: Vec::new(),
        outline: Vec::new(),
        turning: None,
        tip_diameter: 0.0,
        form: None,
        tip_form: Default::default(),
    }
}

fn params(minimum_cone_radius: f64) -> OpParams {
    OpParams::Chamfer2D {
        width: 1.0,
        angle_deg: 45.0,
        minimum_cone_radius,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Outside,
        tolerance: 0.01,
        tilt: None,
        picks: Default::default(),
    }
}

fn world(tool: Tool) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 60.0,
                dy: 60.0,
                dz: 20.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    history.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    (world, history)
}

fn op(tool: &str, params: OpParams) -> Operation {
    Operation {
        tool: tool.into(),
        src: SrcRef::Stock,
        params,
        stamp: None,
        setup: None,
        area: Default::default(),
        coolant: Default::default(),
        after: Default::default(),
        suppressed: false,
    }
}

fn generate(world: &World, history: &mut History, name: &str, params: OpParams) {
    history.execute(
        world,
        CreateOperation::new(world, name, TOOL, SrcRef::Stock, params).expect("create"),
    );
    history.execute(
        world,
        GenerateOperation::new(world, name).expect("generate"),
    );
}

#[test]
fn six_mm_tool_posts_the_two_to_three_band_and_one_mm_break() {
    let (world, mut history) = world(taper(6.0, 3.0));
    generate(&world, &mut history, "CHM", params(2.0));
    let e = model::find_by_name(&world, "CHM").expect("operation");
    let cached = model::toolpath_of(&world, &e).expect("cached path");
    let feeds: Vec<_> = cached
        .path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed)
        .collect();
    assert!(!feeds.is_empty());
    assert!(
        feeds.iter().all(|m| (m.to.z - 17.0).abs() < 1e-12),
        "the exact 45-degree 6 mm case stands at tip Z 17: {feeds:?}"
    );
    let b = cached.path.breaks.first().expect("declared break");
    assert!((b.depth - 1.0).abs() < 1e-12);
    assert!((b.width() - 1.0).abs() < 1e-12);
    assert!(cached.warnings.is_empty(), "{:?}", cached.warnings);
    assert_eq!(camcraft_app::operation::status_of(&world, &e), OpStatus::Ok);
    assert!(
        posting::verification_gate(&world).is_empty(),
        "the stock simulation credits only the authored 1 mm break"
    );
}

#[test]
fn tool_radius_and_flute_reach_refuse_the_missing_dimension() {
    let (small, _) = world(taper(4.0, 3.0));
    let radius = camcraft_app::operation::generate(&small, &op(TOOL, params(2.0)), "")
        .expect_err("the tool radius must reach 3 mm");
    let radius = radius.to_string();
    assert!(
        radius.contains("3 mm") && radius.contains("tool radius"),
        "{radius}"
    );

    let (short, _) = world(taper(6.0, 2.9));
    let reach = camcraft_app::operation::generate(&short, &op(TOOL, params(2.0)), "")
        .expect_err("the cone/flutes must reach 3 mm");
    let reach = reach.to_string();
    assert!(
        reach.contains("3.0 mm cone reach") && reach.contains("flute"),
        "{reach}"
    );

    for bad in [-1.0, f64::NAN, f64::INFINITY] {
        let error = match CreateOperation::new(&small, "BAD", TOOL, SrcRef::Stock, params(bad)) {
            Ok(_) => panic!("invalid minimum radius was accepted"),
            Err(error) => error,
        };
        let error = error.to_string();
        assert!(error.contains("tip offset"), "{error}");
    }
}

#[test]
fn seed_stamp_json_and_project_keep_zero_compatible_and_positive_exact() {
    let (work, mut history) = world(taper(6.0, 3.0));
    generate(&work, &mut history, "ZERO", params(0.0));
    let zero = model::operation_of(
        &work,
        &model::find_by_name(&work, "ZERO").expect("zero operation"),
    )
    .expect("zero params");
    let zero_json = project::params_to_json(&zero.params);
    assert!(
        zero_json["chamfer2_d"].get("minimum_cone_radius").is_none(),
        "zero keeps the legacy key shape: {zero_json}"
    );
    let stamp0 = camcraft_app::operation::current_stamp(
        &work,
        TOOL,
        &SrcRef::Stock,
        &zero.params,
        "ZERO",
        None,
    )
    .expect("zero stamp");
    let mut minus_zero = params(-0.0);
    let stamp_minus = camcraft_app::operation::current_stamp(
        &work,
        TOOL,
        &SrcRef::Stock,
        &minus_zero,
        "ZERO",
        None,
    )
    .expect("negative-zero stamp");
    assert_eq!(stamp0, stamp_minus, "zero adds no digest bytes");

    let seed =
        camcraft_app::operation::seed_params(camcraft_app::operation::OpType::Chamfer2D.ordinal());
    let OpParams::Chamfer2D {
        minimum_cone_radius,
        ..
    } = seed
    else {
        panic!("Chamfer seed")
    };
    assert_eq!(minimum_cone_radius, 0.0);

    if let OpParams::Chamfer2D {
        minimum_cone_radius,
        ..
    } = &mut minus_zero
    {
        *minimum_cone_radius = 2.0;
    }
    let stamp_two = camcraft_app::operation::current_stamp(
        &work,
        TOOL,
        &SrcRef::Stock,
        &minus_zero,
        "ZERO",
        None,
    )
    .expect("positive-radius stamp");
    assert_ne!(stamp0, stamp_two, "the selected cone band moves the stamp");
    let retyped = camcraft_app::operation::retype_params(
        &minus_zero,
        camcraft_app::operation::OpType::Chamfer2D.ordinal(),
    );
    let OpParams::Chamfer2D {
        minimum_cone_radius,
        ..
    } = retyped
    else {
        panic!("Chamfer retype")
    };
    assert_eq!(
        minimum_cone_radius, 0.0,
        "a kind retype resets the Chamfer-specific radius to its seed"
    );
    let positive_json = project::params_to_json(&minus_zero);
    assert_eq!(
        positive_json["chamfer2_d"]["minimum_cone_radius"],
        serde_json::json!(2.0),
        "a positive radius is visible on the wire"
    );

    let file = std::env::temp_dir().join(format!("camcraft-3153-{}.camproj", std::process::id()));
    generate(&work, &mut history, "POS", minus_zero.clone());
    project::save(&work, &file).expect("save positive radius");
    let loaded = new_world();
    project::load(&loaded, &file).expect("load positive radius");
    let actual = model::operation_of(
        &loaded,
        &model::find_by_name(&loaded, "POS").expect("positive operation"),
    )
    .expect("positive params");
    assert_eq!(actual.params, minus_zero);
    let _ = std::fs::remove_file(&file);

    let (legacy_world, mut legacy_history) = world(taper(6.0, 3.0));
    generate(&legacy_world, &mut legacy_history, "ZERO", params(0.0));
    let legacy =
        std::env::temp_dir().join(format!("camcraft-3153-v78-{}.camproj", std::process::id()));
    project::save(&legacy_world, &legacy).expect("save zero project");
    let mut json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&legacy).expect("read")).expect("JSON");
    json["version"] = serde_json::json!(78);
    std::fs::write(
        &legacy,
        serde_json::to_vec_pretty(&json).expect("serialize"),
    )
    .expect("v78");
    let old = new_world();
    project::load(&old, &legacy).expect("v78 Chamfer without a radius migrates");
    let before = model::toolpath_of(
        &legacy_world,
        &model::find_by_name(&legacy_world, "ZERO").expect("zero operation"),
    )
    .expect("before")
    .path;
    let after = model::toolpath_of(
        &old,
        &model::find_by_name(&old, "ZERO").expect("zero operation"),
    )
    .expect("after")
    .path;
    assert_eq!(
        before, after,
        "the v78 zero-radius path is byte/path identical"
    );
    let _ = std::fs::remove_file(&legacy);
}
