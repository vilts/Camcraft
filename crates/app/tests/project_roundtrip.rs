//! M1.1 acceptance: save/load round-trips are byte-identical; a wrong
//! version or corrupt file is a clean error.
//!
//! Gate map (M4.4): covers app::commands, motion::drilling, motion::finishing3d, app::geometry, app::machine, post::machine, motion::machining2d, motion::machining3d, app::model, app::operation, app::posting, app::project, app::schema, app::setup, geometry::geometry::stock, tools::tools, app::units, app::world.

use std::path::PathBuf;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, ImportModel, SetStock,
    SetWcsOrigin,
};
use camcraft_app::geometry::feature;
use camcraft_app::machine;
use camcraft_app::setup;
use camcraft_app::machining2d::{Lead, PocketStrategy, Side};
use camcraft_app::operation::{self, FeatureSel, GeoSel, Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::geometry::{IDENTITY_Q, cube, cylinder, Point3};
use camcraft_app::model;
use camcraft_app::project::{self, ProjectError};
use camcraft_app::world::new_world;

fn tmp_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("camcraft_proj_{tag}_{}.camproj", std::process::id()))
}

fn populated_world() -> flecs_ecs::prelude::World {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("block_a", cube(Point3::ZERO, Point3::new(2.0, 2.0, 2.0))),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("block_b", cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0))),
    );
    // give block_b an offset so the transform round-trip is observable
    if let Some(e) = model::find_by_name(&world, "block_b") {
        model::set_location(&world, &e, Point3::new(10.0, 0.5, -3.25));
    }
    world
}

#[test]
fn save_load_save_is_byte_identical() {
    let path = tmp_path("rt");
    let world = populated_world();
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");

    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "round-trip must be byte-identical");

    // loaded world has the same content
    let file = project::collect(&world2);
    assert_eq!(file.entities.len(), 2);
    let names: Vec<&str> = file.entities.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["block_a", "block_b"], "serializer order is by name");
}

#[test]
fn transforms_survive() {
    let path = tmp_path("tf");
    let world = populated_world();
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let e = model::find_by_name(&world2, "block_b").expect("loaded");
    let p = model::location_of(&world2, &e).expect("location");
    assert_eq!((p.x, p.y, p.z), (10.0, 0.5, -3.25));
    assert_eq!(model::facets_of(&world2, &e).map(|t| t.len()), Some(12));
}

#[test]
fn wrong_version_is_refused() {
    // Anything newer than this build's format stays refused; v1..v22
    // migrate (see v1_file_migrates / v2_file_migrates_to_v3 /
    // v3_file_migrates_to_v4 / v4_file_migrates_to_v5; heights.rs covers
    // the v5 numeric heights -> v6 Fixed migration,
    // side_roundtrip_v6_defaults_to_auto the v6 contour -> v7 side step,
    // units_round_trip_and_a_v7_file_means_millimeters v7 -> v8,
    // v8_file_steps_to_v9 v8 -> v9, v9_file_steps_to_v10 v9 -> v10,
    // v10_file_steps_to_v11_and_the_pocket3d_grammar_round_trips v10 ->
    // v11, v11_file_steps_to_v12 v11 -> v12, v12_file_steps_to_v13
    // v12 -> v13, v13_file_steps_to_v14 v13 -> v14,
    // v14_file_steps_to_v15_and_the_finish3d_grammar_round_trips
    // v14 -> v15, and
    // v15_file_steps_to_v16_and_the_parallel_grammar_round_trips
    // v15 -> v16, and
    // v16_file_steps_to_v17_and_the_ramp_grammar_round_trips v16 -> v17,
    // v17_file_steps_to_v18_and_the_surface_finish_grammar_round_trips
    // v17 -> v18, and v18_file_steps_to_v19_and_the_machine_grammar_round_trips
    // v18 -> v19, and v19_file_steps_to_v20_and_the_rev_counter_round_trips
    // v19 -> v20, and v20_file_steps_to_v21_and_the_setups_grammar_round_trips
    // v20 -> v21, and v21_file_steps_to_v22_and_the_fixture_tag_round_trips
    // v21 -> v22, and the_post_choice_round_trips_and_a_v22_file_means_
    // the_built_in v22 -> v23 (the additive top-level `post` key), and
    // v23_file_steps_to_v24_and_the_5axis_grammar_round_trips v23 -> v24
    // (the `contour5d` / `parallel5d` params tags), and
    // v24_file_steps_to_v25_and_the_swarf_grammar_round_trips v24 -> v25
    // (the `swarf` / `advanced_swarf` tags and the ladder's `passes`),
    // and v25_file_steps_to_v26_and_the_rotary_grammar_round_trips
    // v25 -> v26 (the `rotary_contour` / `rotary_parallel` tags),
    // and v26_file_steps_to_v27_and_the_wrap_ladder_grammar_round_trips
    // v26 -> v27 (the `rotary_offset` tag), and
    // v27_file_steps_to_v28_and_the_stamped_ladder_grammar_round_trips
    // v27 -> v28 (the `multi_axis_rough` tag), and
    // v28_file_steps_to_v29_and_shank_avoid_round_trips v28 -> v29
    // (the `shank_avoid` key on the axis-law kinds), and
    // v29_file_steps_to_v30_and_a_sweep_warning_round_trips v29 -> v30
    // (the `sweep_hit` word in the cached warnings), and
    // v30_file_steps_to_v31_and_the_flow_grammar_round_trips v30 -> v31
    // (the `flow5d` tag), and
    // v31_file_steps_to_v32_and_the_drive_curve_grammar_round_trips
    // v31 -> v32 (the `multi_axis_contour` tag).
    // Spell the probe from the constant: a rung named by hand goes stale
    // (grey, then red) the day the stamp moves, as it did at v25.
    let future = project::PROJECT_VERSION + 1;
    let text = format!(r#"{{"version": {future}, "entities": []}}"#);
    let world = new_world();
    match project::open(&world, &text) {
        Err(ProjectError::UnsupportedVersion(v)) if v == future => {}
        other => panic!("expected UnsupportedVersion({future}), got {other:?}"),
    }
}

#[test]
fn v1_file_migrates_to_v2() {
    // A literal v1 project: no rotation field anywhere (v2 default + step-up).
    let text = r#"{
        "version": 1,
        "entities": [{
            "name": "legacy",
            "translation": [1.0, 2.0, 3.0],
            "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]
        }]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v1 must migrate");
    let e = model::find_by_name(&world, "legacy").expect("entity");
    let loc = model::transform_of(&world, &e).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (1.0, 2.0, 3.0));
    assert_eq!(loc.rot, IDENTITY_Q, "v1 translations mean identity");
    assert_eq!(model::facets_of(&world, &e).map(|t| t.len()), Some(1));
}

#[test]
fn rotation_survives_round_trip() {
    use camcraft_app::geometry::quat_from_yaw;
    use std::f64::consts::FRAC_PI_2;

    let path = tmp_path("rot");
    let world = new_world();
    let e = model::spawn(&world, "tilt");
    model::set_facets(&world, &e, cube(Point3::ZERO, Point3::new(1.0, 1.0, 1.0)).facets);
    model::set_transform(
        &world,
        &e,
        Location {
            pos: Point3::new(5.0, 0.0, 0.0),
            rot: quat_from_yaw(FRAC_PI_2),
        },
    );
    project::save(&world, &path).expect("save");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);

    let e2 = model::find_by_name(&world2, "tilt").expect("loaded");
    let loc = model::transform_of(&world2, &e2).expect("location");
    assert_eq!((loc.pos.x, loc.pos.y, loc.pos.z), (5.0, 0.0, 0.0));
    // Local (1,0,0) must land at world (5,1,0) after the yaw-90 rotation.
    let w = camcraft_app::geometry::transform_point(Point3::new(1.0, 0.0, 0.0), loc.pos, loc.rot);
    assert!((w.x - 5.0).abs() < 1e-12 && (w.y - 1.0).abs() < 1e-12 && w.z.abs() < 1e-12);
}

#[test]
fn garbage_rotation_loads_as_identity() {
    // Prime Directive: a hand-corrupted quaternion is a logged fallback,
    // not a NaN geometry bomb.
    let text = r#"{"version": 2, "entities": [{"name": "g", "translation": [0.0, 0.0, 0.0], "rotation": [0.0, 0.0, 0.0, 0.0]}]}"#;
    let world = new_world();
    project::open(&world, text).expect("opens");
    let e = model::find_by_name(&world, "g").expect("entity");
    assert_eq!(
        model::transform_of(&world, &e).expect("location").rot,
        IDENTITY_Q
    );
}

#[test]
fn corrupt_project_is_a_clean_error() {
    let world = new_world();
    assert!(matches!(
        project::open(&world, "this is not json"),
        Err(ProjectError::Corrupt(_))
    ));
    assert!(matches!(
        project::open(&world, r#"{"version": 1, "entities": [{"name": 5}]}"#),
        Err(ProjectError::Corrupt(_))
    ));
}

fn at(x: f64, y: f64, z: f64) -> Location {
    Location {
        pos: Point3::new(x, y, z),
        rot: IDENTITY_Q,
    }
}

#[test]
fn stock_and_wcs_round_trip() {
    let path = tmp_path("setup");
    let world = populated_world();
    let mut history = History::default();
    let boxy = StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 30.0,
    };
    history.execute(&world, SetStock::new(&world, boxy.clone(), at(1.0, 2.0, -1.0)).unwrap());
    history
        .execute(&world, SetWcsOrigin::new(&world, at(12.5, -3.0, 0.5)).unwrap());
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "setup round-trip must be byte-identical");
    // Parametric stock persists parameters, never regenerated floats.
    let stock_dto = first
        .split("\"name\": \"Stock\"")
        .nth(1)
        .expect("stock entity in file");
    let stock_dto = &stock_dto[..stock_dto.find("\"name\"").unwrap_or(stock_dto.len())];
    assert!(stock_dto.contains("\"stock\""), "parameters persisted");
    assert!(!stock_dto.contains("facets"), "box stock stores no facets");

    let names: Vec<String> = project::collect(&world2)
        .entities
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["G54", "Stock", "block_a", "block_b"]);

    let stock = model::find_by_name(&world2, "Stock").expect("stock loaded");
    assert_eq!(model::stock_def_of(&world2, &stock), Some(boxy));
    assert_eq!(model::facets_of(&world2, &stock).map(|t| t.len()), Some(12));
    assert_eq!(
        model::transform_of(&world2, &stock).map(|l| [l.pos.x, l.pos.y, l.pos.z]),
        Some([1.0, 2.0, -1.0])
    );
    let g54 = model::find_by_name(&world2, "G54").expect("wcs loaded");
    assert!(model::is_wcs(&world2, &g54));
    assert_eq!(model::facets_of(&world2, &g54), None);
    assert_eq!(
        model::transform_of(&world2, &g54).map(|l| [l.pos.x, l.pos.y, l.pos.z]),
        Some([12.5, -3.0, 0.5])
    );
}

#[test]
fn from_model_stock_round_trips_through_facets() {
    let path = tmp_path("frommodel");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        ImportModel::from_mesh("src", cube(Point3::ZERO, Point3::new(2.0, 3.0, 1.0))),
    );
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::FromModel { source: "src".into() },
            Location::default(),
        )
        .unwrap(),
    );
    project::save(&world, &path).expect("save");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let stock = model::find_by_name(&world2, "Stock").expect("stock loaded");
    assert_eq!(
        model::stock_def_of(&world2, &stock),
        Some(StockShape::FromModel {
            source: "src".into()
        })
    );
    // The source is absent here, yet the stock still shows its geometry:
    // FromModel facets came through the file, not through a live copy.
    assert_eq!(model::facets_of(&world2, &stock).map(|t| t.len()), Some(12));
    assert!(
        project::collect(&world2).entities.len() >= 2,
        "Stock and src persist"
    );
}

#[test]
fn corrupt_stock_and_wcs_fields_are_refused() {
    let cases = [
        // negative dimension
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"box": {"dx": -5.0, "dy": 1.0, "dz": 1.0}}}]}"#,
        // table-defying size
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"box": {"dx": 5000000.0, "dy": 1.0, "dz": 1.0}}}]}"#,
        // stock must be named Stock
        r#"{"version": 2, "entities": [{"name": "Billet", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"cylinder": {"diameter": 10.0, "height": 1.0}}}]}"#,
        // parametric stock carrying a facet array
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"cylinder": {"diameter": 10.0, "height": 1.0}}, "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]}]}"#,
        // offsets are tags, not solids
        r#"{"version": 2, "entities": [{"name": "G54", "translation": [0.0, 0.0, 0.0], "kind": "wcs", "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]}]}"#,
        // not a machine slot
        r#"{"version": 2, "entities": [{"name": "G60", "translation": [0.0, 0.0, 0.0], "kind": "wcs"}]}"#,
        // from-model stock with nothing to show
        r#"{"version": 2, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock", "stock": {"from_model": {"source": "gone"}}}]}"#,
    ];
    for text in cases {
        let world = new_world();
        assert!(
            matches!(project::open(&world, text), Err(ProjectError::Corrupt(_))),
            "must refuse: {text}"
        );
        assert!(
            model::named_entities(&world).is_empty(),
            "refused file must leave the world untouched"
        );
    }
}

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

#[test]
fn tools_and_generated_operations_round_trip() {
    let path = tmp_path("cam");
    let world = new_world();
    let mut history = History::default();
    let boxy = StockShape::Box {
        dx: 60.0,
        dy: 60.0,
        dz: 20.0,
    };
    history.execute(
        &world,
        SetStock::new(&world, boxy, Location::default()).expect("box stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(19.5),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("create op"),
    );
    history
        .execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "CAM round-trip must be byte-identical");
    assert!(first.contains("\"tool\"") && first.contains("\"operation\""), "the v3 payloads are in the file: {first}");

    let names: Vec<String> = project::collect(&world2)
        .entities
        .iter()
        .map(|e| e.name.clone())
        .collect();
    assert_eq!(names, ["EM-0400-2FL", "Face1", "Stock"]);

    // The tool is the catalog definition again, and it is not geometry.
    let tool = model::find_by_name(&world2, "EM-0400-2FL").expect("tool loaded");
    assert!(model::is_tool(&world2, &tool));
    assert_eq!(model::tool_of(&world2, &tool), Some(catalog_tool("EM-0400-2FL")));
    assert_eq!(model::facets_of(&world2, &tool), None);

    // The operation component - including the stamp the cache was taken
    // for - survives untouched, so it reads Ok, not Stale, after a load.
    let src1 = model::find_by_name(&world, "Face1").expect("source op");
    let src2 = model::find_by_name(&world2, "Face1").expect("loaded op");
    assert!(model::is_operation(&world2, &src2));
    assert_eq!(
        model::operation_of(&world2, &src2),
        model::operation_of(&world, &src1)
    );
    assert_eq!(
        model::operation_of(&world2, &src2).map(|o| o.stamp.is_some()),
        Some(true)
    );
    // The cached path replays exactly: same moves, same byte values.
    assert_eq!(
        model::toolpath_of(&world2, &src2).map(|t| t.path),
        model::toolpath_of(&world, &src1).map(|t| t.path)
    );
    assert!(model::toolpath_of(&world2, &src2).is_some_and(|t| !t.path.moves.is_empty()));

    // No operation leaked geometry or a stock definition either.
    let file = project::collect(&world2);
    let op_dto = file.entities.iter().find(|e| e.name == "Face1").expect("op dto");
    assert!(op_dto.operation.is_some() && op_dto.tool.is_none() && op_dto.facets.is_none());
}

#[test]
fn arc_move_round_trips() {
    // Cylinder stock, contour at a fitter-friendly tolerance: the wall
    // rim is tessellation, the fitter says circle, and the cache must
    // persist arc moves - not silently fall back to chords on reload.
    let path = tmp_path("arcs");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Cylinder {
                diameter: 30.0,
                height: 10.0,
            },
            Location::default(),
        )
        .expect("cylinder stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Skin1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Contour { tilt: None,
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(20.0),
                retract_z: Height::fixed(15.0),
                top_z: Height::fixed(10.0),
                bottom_z: Height::fixed(8.0),
                stepdown: 2.0,
                feed: 800.0,
                plunge_feed: 200.0,
                climb: true,
                side: Side::Auto,
                lead: Lead::None,
                tolerance: 0.1,
                tab_count: 0,
                tab_width: 0.0,
                tab_height: 0.0,
                entry: camcraft_app::machining3d::Entry3D::Plunge,
                helix_radius: 1.0,
                blend_turns: false,
                links: camcraft_app::links::Links::PLANAR,
            },
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Skin1").expect("generate"));
    let src = model::find_by_name(&world, "Skin1").expect("op");
    let before = model::toolpath_of(&world, &src).expect("cache").path;
    assert!(before.has_arcs(), "a 32-gon wall at tol 0.1 contours to arcs");

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"arc\""), "the arc payload is in the file: {first}");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "arc moves round-trip byte-identically");

    let dst = model::find_by_name(&world2, "Skin1").expect("op loaded");
    let after = model::toolpath_of(&world2, &dst).expect("cache").path;
    assert_eq!(after, before, "arcs survive exactly - centers, sense, endpoints");
    assert_eq!(after.arc_count(), before.arc_count());
}

#[test]
fn a_stated_pose_round_trips_verbatim() {
    // Pose (M5.1 A) in the file: `axis` is an optional key of a move.
    // A prismatic document never writes it (its saves stay byte-shape
    // identical to v21's - the arc-v5 precedent again), and a tilted
    // one snaps the components ONCE at the write (lattice law) and
    // attaches them VERBATIM at the read - no re-normalization, or the
    // file would drift by bits under repeated reopening.
    use camcraft_app::cl::lattice;
    let path = tmp_path("pose");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.0 },
            Location::default(),
        )
        .expect("stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Face1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 5.0,
                clearance_z: Height::fixed(15.0),
                cut_z: Height::fixed(9.0),
                top_z: None,
                stepdown: None,
                feed: 1000.0,
                plunge_feed: 200.0,
                climb: true,
            },
        )
        .expect("face"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let clean = {
        project::save(&world, &path).expect("save clean");
        std::fs::read_to_string(&path).expect("read")
    };
    assert!(!clean.contains("\"axis\""), "a prismatic cache writes no pose key");

    // Lean one move 45 degrees about - the raw 45° pair has off-lattice
    // tails on purpose, so the write-time snap is the observable.
    let r45 = 45f64.to_radians();
    let raw = [r45.sin(), 0.0, r45.cos()];
    let e = model::find_by_name(&world, "Face1").expect("op");
    let mut t = model::toolpath_of(&world, &e).expect("cache");
    t.path.moves[1].axis = Some(raw);
    e.entity_view(&world).set(t);
    project::save(&world, &path).expect("save tilted");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"axis\""), "the pose key rides the move: {first}");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "a stated pose round-trips byte-identically");

    let dst = model::find_by_name(&world2, "Face1").expect("op loaded");
    let after = model::toolpath_of(&world2, &dst).expect("cache").path;
    assert_eq!(
        after.moves[1].axis,
        Some([lattice(raw[0]), 0.0, lattice(raw[2])]),
        "the file's numbers are the law, snapped once at the write"
    );
    assert!(after.moves[0].axis.is_none(), "the unleaned moves stay unleaned");
}

#[test]
fn v2_file_migrates_to_v3() {
    // A literal v2 project: no tool or operation fields anywhere. The
    // chained migration only carries the version forward.
    let text = r#"{
        "version": 2,
        "entities": [
            {"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {"box": {"dx": 40.0, "dy": 40.0, "dz": 10.0}}},
            {"name": "part", "translation": [1.0, 2.0, 3.0],
             "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v2 must migrate");
    let stock = model::find_by_name(&world, "Stock").expect("stock");
    assert_eq!(
        model::stock_def_of(&world, &stock),
        Some(StockShape::Box {
            dx: 40.0,
            dy: 40.0,
            dz: 10.0
        })
    );
    let part = model::find_by_name(&world, "part").expect("part");
    assert_eq!(model::facets_of(&world, &part).map(|t| t.len()), Some(1));

    // Saving stamps the file as current: the migration is a one-way door.
    let path = tmp_path("mig");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "migrated file must carry the current stamp: {saved}"
    );
}

#[test]
fn corrupt_tool_and_operation_fields_are_refused() {
    // Reusable payloads; `$tool` / `$face` are substituted below so the
    // JSON bodies stay single-brace and readable.
    let face = r#""params": {"face": {"stepover": 3.0, "clearance_z": 25.0, "cut_z": 19.5, "feed": 1200.0, "plunge_feed": 300.0, "climb": true}}"#;
    let tool = r#"{"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0, "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0, "holder_d": 44.0, "holder_len": 35.0, "material_group": "general", "rpm": 10000.0, "feed": 1200.0}"#;
    let bad_tool = tool.replace("\"diameter\": 4.0", "\"diameter\": -4.0");
    let bad_face = face.replace("\"stepover\": 3.0", "\"stepover\": -3.0");
    let cases = [
        // a tool entity without its definition
        r#"{"version": 3, "entities": [{"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool"}]}"#
            .to_string(),
        // negative diameter
        r#"{"version": 3, "entities": [{"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool", "tool": $tool}]}"#
            .replace("$tool", &bad_tool),
        // setup names are not tool names
        r#"{"version": 3, "entities": [{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "tool", "tool": $tool}]}"#
            .replace("$tool", tool),
        // a plain model wearing tool parameters
        r#"{"version": 3, "entities": [{"name": "m", "translation": [0.0, 0.0, 0.0], "tool": $tool}]}"#
            .replace("$tool", tool),
        // an operation without its definition
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation"}]}"#
            .to_string(),
        // an operation that cuts with nothing
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "", "src": "stock", $face}}]}"#
            .replace("$face", face),
        // an operation with an empty geometry-source name
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": {"entity": {"name": ""}}, $face}}]}"#
            .replace("$face", face),
        // operations have no geometry of their own
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]], "operation": {"tool": "T", "src": "stock", $face}}]}"#
            .replace("$face", face),
        // insane parameters
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": "stock", $face}}]}"#
            .replace("$face", &bad_face),
        // a cache that carries no path
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": "stock", $face, "cache": {"moves": [], "warnings": []}}}]}"#
            .replace("$face", face),
        // a cache whose moves do not chain
        r#"{"version": 3, "entities": [{"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation", "operation": {"tool": "T", "src": "stock", $face, "cache": {"moves": [{"kind": "rapid", "from": [-100.0, -100.0, 25.0], "to": [1.0, 0.0, 25.0]}, {"kind": "feed", "from": [0.0, 0.0, 25.0], "to": [0.0, 0.0, 19.5], "feed": 300.0}], "warnings": []}}}]}"#
            .replace("$face", face),
    ];
    for text in cases {
        let world = new_world();
        assert!(
            matches!(
                project::open(&world, &text),
                Err(ProjectError::Corrupt(_))
            ),
            "must refuse: {text}"
        );
        assert!(
            model::named_entities(&world).is_empty(),
            "refused file must leave the world untouched"
        );
    }
}

#[test]
fn v3_file_migrates_to_v4() {
    // A literal v3 file that already carries a feature pick: v3's src enum
    // could hold one, and v4 reads it verbatim - the migration step is the
    // stamp, not a reshape. The id 42 is unresolvable on purpose: a
    // tampered or stale pick is a generation error, never a load error.
    let text = r#"{
        "version": 3,
        "entities": [
            {"name": "part", "translation": [0.0, 0.0, 0.0],
             "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]},
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": {"selection": {"items": [
                 {"entity": "part", "feature": {"face": {"id": 42}}}
            ]}}, "params": {"face": {"stepover": 3.0, "clearance_z": 25.0,
                 "cut_z": 0.5, "feed": 1200.0, "plunge_feed": 300.0,
                 "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v3 must migrate");
    let op = model::find_by_name(&world, "Op1").expect("op");
    let o = model::operation_of(&world, &op).expect("operation component");
    assert_eq!(
        o.src,
        SrcRef::Selection(vec![GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(42),
        }]),
        "the pick survives the step-up intact"
    );
    // Unresolvable id and no cache in the file: shape-legal, so it loads -
    // and reads Never (nothing was ever generated in this world), never a
    // load-time refusal, with nothing to preview.
    assert_eq!(operation::status_of(&world, &op), OpStatus::Never);
    assert!(operation::region_of(&world, &o).is_none());

    let path = tmp_path("mig4");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "resaved file must be stamped current: {saved}"
    );
}

#[test]
fn v4_file_migrates_to_v5() {
    // A literal v4 file: the v5 additions (patch picks, arc moves, face
    // staging, pocket hops) all default to "absent", so a v4 file carries
    // none of them and v5 reads it verbatim - the step-up is the stamp,
    // exactly as v3 -> v4 was. Both params kinds are here because the face
    // arm grew top_z/stepdown and the pocket arm grew retract_z.
    let text = r#"{
        "version": 4,
        "entities": [
            {"name": "part", "translation": [0.0, 0.0, 0.0],
             "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]},
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": {"selection": {"items": [
                 {"entity": "part", "feature": {"face": {"id": 42}}}
            ]}}, "params": {"face": {"stepover": 3.0, "clearance_z": 25.0,
                 "cut_z": 0.5, "feed": 1200.0, "plunge_feed": 300.0,
                 "climb": true}}}},
            {"name": "Op2", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket": {
                 "stepover": 3.0, "stock_to_leave": 0.0, "clearance_z": 20.0,
                 "top_z": 8.0, "bottom_z": 2.0, "stepdown": 2.0,
                 "feed": 900.0, "plunge_feed": 250.0, "climb": true,
                 "strategy": "zigzag"}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v4 must migrate");
    let op = model::find_by_name(&world, "Op1").expect("op1");
    let o = model::operation_of(&world, &op).expect("operation component");
    assert_eq!(
        o.src,
        SrcRef::Selection(vec![GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(42),
        }]),
        "the pick survives the step-up intact"
    );
    let op2 = model::find_by_name(&world, "Op2").expect("op2");
    let p = match model::operation_of(&world, &op2).expect("op2 params").params {
        OpParams::Pocket { strategy, .. } => strategy,
        ref other => panic!("pocket params expected, got {other:?}"),
    };
    assert_eq!(p, PocketStrategy::Zigzag, "a v4 pocket means zigzag");

    let path = tmp_path("mig5");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "resaved file must be stamped current: {saved}"
    );
    // The declared shapes stay unwritten while unused: a migrated file
    // must not grow keys a v4 build could not read past the version.
    for absent in ["\"retract_z\"", "\"patch\"", "\"arc\""] {
        assert!(!saved.contains(absent), "v5 default must not be serialized: {absent}");
    }
}

#[test]
fn pocket_entry_family_is_a_step_up_on_the_wire() {
    // M5.1 H grew the 2-D pocket arm with serde defaults, so the format
    // version did not move: an old file means exactly the dive every
    // pocket posted before the family existed (plunge, the seed's 1.0 mm
    // helix, the law's 45 degrees), and the new keys round-trip.
    let plain = r#"{
        "version": 30,
        "entities": [
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket": {
                 "stepover": 3.0, "stock_to_leave": 0.0, "clearance_z": 20.0,
                 "top_z": 8.0, "bottom_z": 2.0, "stepdown": 2.0,
                 "feed": 900.0, "plunge_feed": 250.0, "climb": true,
                 "strategy": "zigzag"}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, plain).expect("a pre-H pocket file must read verbatim");
    let e = model::find_by_name(&world, "Op1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket { entry, helix_radius, optimal_load, .. } = &o.params else {
        panic!("pocket params expected, got {:?}", o.params);
    };
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Plunge, "absent means the dive");
    assert_eq!(*helix_radius, 1.0, "absent means the seed's lead radius");
    assert_eq!(*optimal_load, camcraft_app::load::DEFAULT_OPTIMAL_LOAD, "absent means the law's 45 degrees, as a load");

    // The explicit shape, hand-written so the tags are pinned.
    let text = plain
        .replace("\"strategy\": \"zigzag\"", "\"strategy\": \"adaptive\", \"max_engagement\": 30.0, \"entry\": \"helix\", \"helix_radius\": 2.5");
    let world2 = new_world();
    project::open(&world2, &text).expect("the grown pocket grammar must read");
    let e2 = model::find_by_name(&world2, "Op1").expect("op");
    let o2 = model::operation_of(&world2, &e2).expect("operation");
    let OpParams::Pocket { strategy, entry, helix_radius, optimal_load, .. } = &o2.params else {
        panic!("pocket params expected, got {:?}", o2.params);
    };
    assert_eq!(*strategy, PocketStrategy::Adaptive);
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Helix);
    assert_eq!(*helix_radius, 2.5);
    // M11.4 B: the legacy degree word converts exactly on load.
    assert_eq!(optimal_load.to_bits(), camcraft_app::load::load_of_degrees(30.0).to_bits());

    // Write-parse-write is a fixed point, and the entry tag is snake_case.
    let path = tmp_path("hentry");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world3 = new_world();
    project::load(&world3, &path).expect("reload");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the grown pocket shape round-trips byte-identically");
    assert!(first.contains("\"entry\": \"helix\""), "{first}");
    assert!(first.contains("\"helix_radius\": 2.5"), "{first}");

    // An unknown entry tag is corrupt text, refused, not defaulted -
    // the pocket3d arm's law, now the plane's too.
    let bad = text.replace("\"helix\"", "\"spiral\"");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad), Err(ProjectError::Corrupt(_))),
        "must refuse an entry tag outside the vocabulary"
    );
}

#[test]
fn slot_entry_family_is_a_step_up_on_the_wire() {
    // M8.2 C grew the slot arm with serde defaults, so the format
    // version did not move: an old file means exactly the wall dive at
    // the starter end every slot posted before the family existed, and
    // the new keys round-trip like the pocket's.
    let plain = r#"{
        "version": 30,
        "entities": [
            {"name": "Op1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"slot": {
                 "a": [0.0, 10.0], "b": [40.0, 10.0], "width": 12.0,
                 "stock_to_leave": 0.0, "clearance_z": 5.0,
                 "top_z": 0.0, "bottom_z": -6.0, "stepdown": 3.0,
                 "stepover": 4.0, "feed": 800.0, "plunge_feed": 200.0,
                 "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, plain).expect("a pre-C slot file must read verbatim");
    let e = model::find_by_name(&world, "Op1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Slot { entry, helix_radius, .. } = &o.params else {
        panic!("slot params expected, got {:?}", o.params);
    };
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Plunge, "absent means the dive");
    assert_eq!(*helix_radius, 1.0, "absent means the seed's lead radius");

    // The explicit shape, hand-written so the tags are pinned.
    let text = plain.replace(
        "\"climb\": true}}",
        "\"climb\": true, \"entry\": \"helix\", \"helix_radius\": 2.5}}",
    );
    let world2 = new_world();
    project::open(&world2, &text).expect("the grown slot grammar must read");
    let e2 = model::find_by_name(&world2, "Op1").expect("op");
    let o2 = model::operation_of(&world2, &e2).expect("operation");
    let OpParams::Slot { entry, helix_radius, .. } = &o2.params else {
        panic!("slot params expected, got {:?}", o2.params);
    };
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Helix);
    assert_eq!(*helix_radius, 2.5);

    // Write-parse-write is a fixed point, and the entry tag is snake_case.
    let path = tmp_path("csentry");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world3 = new_world();
    project::load(&world3, &path).expect("reload");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the grown slot shape round-trips byte-identically");
    assert!(first.contains("\"entry\": \"helix\""), "{first}");
    assert!(first.contains("\"helix_radius\": 2.5"), "{first}");

    // An unknown entry tag is corrupt text, refused, not defaulted.
    let bad = text.replace("\"helix\"", "\"spiral\"");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad), Err(ProjectError::Corrupt(_))),
        "must refuse a slot entry tag outside the vocabulary"
    );
}

#[test]
fn v5_roundtrips_retract_and_auto() {
    // The step-5 fields, once actually set, round-trip exactly: the
    // pocket's hop plane and Auto choice, and the trace's hop plane.
    let path = tmp_path("hop5");
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
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Pocket1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Pocket { tilt: None,
                links: camcraft_app::links::Links::PLANAR,
                stepover: 1.0,
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(25.0),
                retract_z: Some(Height::fixed(22.0)),
                top_z: Height::fixed(20.0),
                bottom_z: Height::fixed(18.0),
                stepdown: 2.0,
                feed: 900.0,
                plunge_feed: 250.0,
                climb: true,
                strategy: PocketStrategy::Auto,
                entry: camcraft_app::machining3d::Entry3D::Plunge,
                helix_radius: 1.0,
                optimal_load: camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
                blend_turns: false,
            },
        )
        .expect("pocket"),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Trace1",
            "EM-0400-2FL",
            SrcRef::Stock,
            OpParams::Trace { tilt: None,
                clearance_z: Height::fixed(25.0),
                retract_z: Some(Height::fixed(23.0)),
                top_z: Height::fixed(20.0),
                bottom_z: Height::fixed(18.0),
                stepdown: 2.0,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("trace"),
    );

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"retract_z\""), "the set hop plane is in the file");
    assert!(first.contains("\"auto\""), "the Auto choice is stored (resolution is not)");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "retract/Auto round-trip byte-identically");

    let op = model::find_by_name(&world2, "Pocket1").expect("pocket loaded");
    match model::operation_of(&world2, &op).expect("params").params {
        OpParams::Pocket {
            retract_z,
            strategy,
            ..
        } => {
            assert_eq!(retract_z, Some(Height::fixed(22.0)), "the hop plane survives exactly");
            assert_eq!(strategy, PocketStrategy::Auto, "the choice survives exactly");
        }
        ref other => panic!("pocket params expected, got {other:?}"),
    }
    let tr = model::find_by_name(&world2, "Trace1").expect("trace loaded");
    match model::operation_of(&world2, &tr).expect("params").params {
        OpParams::Trace { retract_z, .. } => assert_eq!(retract_z, Some(Height::fixed(23.0))),
        ref other => panic!("trace params expected, got {other:?}"),
    }
}

#[test]
fn patch_selection_round_trips() {
    // The welded wall of a faceted boss, picked once and saved by its
    // geometry hash: the file speaks `patch`, and a second world
    // recomputing features from the same facets resolves the same weld,
    // so the loaded op stays Ok rather than Stale.
    let path = tmp_path("patch");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("boss", cylinder(20.0, 8.0, 16)),
    );
    let e = model::find_by_name(&world, "boss").expect("boss");
    let f = model::features_of(&world, &e).expect("features");
    let pid = feature::patches(&f)
        .into_iter()
        .find(|p| p.faces.len() >= 8)
        .map(|p| p.id)
        .expect("the sixteen wall strips weld into one patch");
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Boss1",
            "EM-0400-2FL",
            SrcRef::Selection(vec![GeoSel {
                entity: "boss".into(),
                feature: FeatureSel::Patch(pid),
            }]),
            OpParams::Face { holes: Default::default(), tilt: None, blend_turns: false,
                stepover: 3.0,
                clearance_z: Height::fixed(25.0),
                cut_z: Height::fixed(4.5),
                top_z: None,
                stepdown: None,
                feed: 1200.0,
                plunge_feed: 300.0,
                climb: true,
            },
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Boss1").expect("generate"));
    let src_op = model::find_by_name(&world, "Boss1").expect("op");
    let o = model::operation_of(&world, &src_op).expect("operation");
    assert_eq!(operation::status_of(&world, &src_op), OpStatus::Ok, "generated first");

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"patch\""), "the wall pick is in the file: {first}");

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "a welded pick round-trips byte-identically");

    let dst_op = model::find_by_name(&world2, "Boss1").expect("op loaded");
    let p = model::operation_of(&world2, &dst_op).expect("operation");
    assert_eq!(p.src, o.src, "the same weld, named by the same id");
    assert_eq!(p.stamp, o.stamp);
    assert_eq!(
        operation::status_of(&world2, &dst_op),
        OpStatus::Ok,
        "patch ids are content hashes, not file positions"
    );
}

#[test]
fn a_selection_round_trips_and_stays_ok() {
    let path = tmp_path("pick");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("add tool"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    // Top face plus one rim edge: the boundary loop and a centerline run
    // in one pick list (M1.5 step 8's mixed shape), contour parameters.
    let e = model::find_by_name(&world, "part").expect("part");
    let f = model::features_of(&world, &e).expect("features");
    let top = f
        .faces
        .iter()
        .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
        .max_by(|a, b| a.offset.total_cmp(&b.offset))
        .map(|fa| fa.id)
        .expect("box top");
    let run = feature::chains(&f)
        .into_iter()
        .find(|c| {
            !c.closed
                && {
                    let p = feature::chain_points(&f, c);
                    p.len() >= 2 && (p[0][0] != p[1][0] || p[0][1] != p[1][1])
                }
        })
        .map(|c| c.id)
        .expect("a horizontal rim edge");
    let params = OpParams::Contour { tilt: None,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(10.0),
        retract_z: Height::fixed(8.0),
        top_z: Height::fixed(4.5),
        bottom_z: Height::fixed(0.0),
        stepdown: 2.5,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        side: Side::Auto,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: camcraft_app::machining3d::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: camcraft_app::links::Links::PLANAR,
    };
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "Pick1",
            "EM-0400-2FL",
            SrcRef::Selection(vec![
                GeoSel {
                    entity: "part".into(),
                    feature: FeatureSel::Face(top),
                },
                GeoSel {
                    entity: "part".into(),
                    feature: FeatureSel::Chain(run),
                },
            ]),
            params,
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "Pick1").expect("generate"));
    let src_op = model::find_by_name(&world, "Pick1").expect("op");
    let o = model::operation_of(&world, &src_op).expect("operation");
    assert_eq!(operation::status_of(&world, &src_op), OpStatus::Ok, "generated first");

    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(
        first.contains("\"selection\"") && first.contains("\"face\"") && first.contains("\"chain\""),
        "the pick list is in the file verbatim: {first}"
    );

    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "a picked project round-trips byte-identically");

    let dst_op = model::find_by_name(&world2, "Pick1").expect("op loaded");
    let p = model::operation_of(&world2, &dst_op).expect("operation");
    assert_eq!(p.src, o.src, "the pick list is the same list");
    // The cache replays against features recomputed from the same facets -
    // stable ids earn their name here: Ok, not Stale.
    assert_eq!(p.stamp, o.stamp);
    assert_eq!(operation::status_of(&world2, &dst_op), OpStatus::Ok, "a loaded pick is still honest");
    assert_eq!(
        operation::chains_of(&world2, &p).len(),
        operation::chains_of(&world, &o).len(),
        "preview sees the same runs before and after the file"
    );
}

#[test]
fn side_roundtrip_v6_defaults_to_auto() {
    // A literal v6 project: contour params predate the side key. The
    // serde default IS the migration - absent reads as `auto`, the ask
    // every pre-v7 file silently had - and saving stamps the v7 shape.
    let text = r#"{
        "version": 6,
        "entities": [
            {"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {"box": {"dx": 40.0, "dy": 40.0, "dz": 10.0}}},
            {"name": "T", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 6.0, "corner_r": 0.0,
                      "flute_len": 20.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 800.0}},
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": "none", "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v6 contour must migrate");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour { side, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*side, Side::Auto, "an absent side is Auto");

    let path = tmp_path("v7");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains("\"side\": \"auto\"")
            && saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v7 shape: {saved}"
    );
}

#[test]
fn contour_entry_family_is_a_step_up_on_the_wire() {
    // M8.2 A, the pocket doors' word: additive keys, so the format
    // version does not move - and an absent `entry` reads exactly as
    // the pre-entry file was, the dive, with the seed's radius unread.
    let text = r#"{
        "version": 42,
        "entities": [
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": "none", "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("a contour without the entry keys must read");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour {
        entry,
        helix_radius,
        ..
    } = &o.params
    else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(
        *entry,
        camcraft_app::machining3d::Entry3D::Plunge,
        "absent means the dive"
    );
    assert_eq!(*helix_radius, 1.0, "the absent radius is the seed's number");

    // The stated grammar, hand-written so the tags are pinned: the same
    // op with the family named on the wire.
    let stated = text.replace(
        "\"tab_height\": 0.0}",
        "\"tab_height\": 0.0, \"entry\": \"helix\", \"helix_radius\": 2.5}",
    );
    assert_ne!(stated, text, "the test must reach the contour grammar");
    let world2 = new_world();
    project::open(&world2, &stated).expect("the entry grammar must read");
    let e2 = model::find_by_name(&world2, "C1").expect("op");
    let o2 = model::operation_of(&world2, &e2).expect("operation");
    let OpParams::Contour {
        entry,
        helix_radius,
        ..
    } = &o2.params
    else {
        panic!("contour params expected, got {:?}", o2.params);
    };
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Helix);
    assert_eq!(*helix_radius, 2.5);

    // Save writes the keys, reload keeps them, and write-parse-write
    // is the fixed point.
    let path = tmp_path("centry");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(
        first.contains("\"entry\": \"helix\"") && first.contains("\"helix_radius\": 2.5"),
        "the choice rides the wire: {first}"
    );
    let world3 = new_world();
    project::load(&world3, &path).expect("reload");
    let e3 = model::find_by_name(&world3, "C1").expect("reloaded op");
    let o3 = model::operation_of(&world3, &e3).expect("operation");
    let OpParams::Contour { entry, .. } = &o3.params else {
        panic!("contour params expected");
    };
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Helix, "reload keeps the arm");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the entry shape round-trips byte-identically");
}

#[test]
fn v8_file_steps_to_v9() {
    // A literal v8 project: `lead` has been a required key since v1, so
    // the v8 -> v9 step is the stamp itself - the file reads unchanged
    // and saving restamps it at this build's version.
    let text = r#"{
        "version": 8,
        "entities": [
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": {"arc": {"radius": 3.0}}, "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v8 file must migrate");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour { lead, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*lead, Lead::Arc { radius: 3.0 }, "an arc lead reads verbatim");

    let path = tmp_path("v9");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    // save() is to_string_pretty, so match format-agnostic fragments:
    // the tag key and the radius value, which nowhere else in this
    // document can both occur.
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION))
            && saved.contains("\"arc\"")
            && saved.contains("\"radius\": 3.0"),
        "the migration writes the v9 shape: {saved}"
    );
}

#[test]
fn v9_file_steps_to_v10() {
    // A literal v9 project: the drilling landing adds no required keys.
    // ToolDto's tip_angle/pitch serde-default to the inert 0.0 they meant
    // before, PathDto's cycles defaults to empty (no pre-v10 build ever
    // generated a cycle), and ParamsDto just gains a variant - so the
    // v9 -> v10 step is the stamp itself, exactly like v8 -> v9.
    let text = r#"{
        "version": 9,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": "none", "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]},
                     {"kind": "plunge", "from": [0.0, 0.0, 10.0], "to": [0.0, 0.0, 3.0]}
                 ], "warnings": []}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v9 file must migrate");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    assert!(
        matches!(o.params, OpParams::Contour { .. }),
        "the contour params read unchanged: {:?}",
        o.params
    );

    let path = tmp_path("v10");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v10 shape: {saved}"
    );
    // skip_serializing_if keeps a cycle-free cache byte-identical to its
    // v9 form; the key must not appear just because the build learned it.
    assert!(
        !saved.contains("\"cycles\""),
        "an empty cycle list is not written: {saved}"
    );
}

#[test]
fn drill_params_and_cycle_cache_round_trip_and_a_stray_cycle_is_refused() {
    // Hand-written v10 text, so the drill wire grammar itself is pinned:
    // the "drill" params tag, holes with through stored (not re-derived),
    // and cycles beside the moves of a drilling cache.
    let text = r#"{
        "version": 10,
        "entities": [
            {"name": "D1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"drill": {
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_stock", "offset": 0.0},
                 "cycle": "peck", "peck": 6.0, "dwell": 0.0,
                 "feed": 240.0, "plunge_feed": 120.0,
                 "holes": [
                     {"xy": [0.0, 0.0], "radius": 3.0, "entry_z": 10.0,
                      "floor_z": -5.0, "through": false},
                     {"xy": [20.0, 15.5], "radius": 3.0, "entry_z": 10.0,
                      "floor_z": 0.0, "through": true}
                 ]}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]},
                     {"kind": "feed", "from": [0.0, 0.0, 10.0], "to": [0.0, 0.0, 4.0], "feed": 240.0},
                     {"kind": "plunge", "from": [0.0, 0.0, 4.0], "to": [0.0, 0.0, -5.0]},
                     {"kind": "rapid", "from": [0.0, 0.0, -5.0], "to": [20.0, 15.5, 10.0]}
                 ], "warnings": [], "cycles": [
                     {"kind": "peck", "xy": [0.0, 0.0], "r_z": 4.0,
                      "bottom_z": -5.0, "entry_z": 10.0, "peck": 6.0,
                      "dwell": 0.0, "first_move": 1}
                 ]}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v10 drill grammar must read");
    let e = model::find_by_name(&world, "D1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Drill {
        cycle,
        peck,
        holes,
        ..
    } = &o.params
    else {
        panic!("drill params expected, got {:?}", o.params);
    };
    assert_eq!(*cycle, camcraft_app::drilling::CycleChoice::Peck);
    assert_eq!(*peck, 6.0);
    assert_eq!(holes.len(), 2);
    assert_eq!((holes[1].xy.x, holes[1].xy.y), (20.0, 15.5));
    assert!(holes[1].through && !holes[0].through, "through reads verbatim");

    // Save, reload, save again: every float the file carries is a fixed
    // point of write-parse, so the drill shape must be byte-stable too.
    let path = tmp_path("drill");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the drill shape round-trips byte-identically");
    assert!(first.contains("\"drill\"") && first.contains("\"cycles\""));

    // Prime Directive: a cycle pointing past the end of its chain is a
    // corrupt file, refused before any world write - the warning index a
    // shank law would read out of it has nothing to point at.
    let stray = text.replace("\"first_move\": 1", "\"first_move\": 99");
    let world3 = new_world();
    match project::open(&world3, &stray) {
        Err(ProjectError::Corrupt(msg)) => assert!(msg.contains("past"), "{msg}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    assert!(model::named_entities(&world3).is_empty());
    // An unusable hole (a floor above the entry is not a hole) fails the
    // parameter sanity gate at open, not the generator.
    let bad_hole = text.replace("\"floor_z\": -5.0", "\"floor_z\": 12.0");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad_hole), Err(ProjectError::Corrupt(_))),
        "must refuse a hole whose floor is above its entry"
    );
}

#[test]
fn v10_file_steps_to_v11_and_the_pocket3d_grammar_round_trips() {
    // A literal v10 project (the newest shape before the 3-D pocket):
    // the v10 -> v11 step is the stamp itself - no required key is added,
    // since Pocket3D is a new variant and entry defaults to Plunge.
    let text = r#"{
        "version": 10,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket": {
                 "stepover": 2.0, "stock_to_leave": 0.0, "clearance_z": 10.0,
                 "top_z": 5.0, "bottom_z": -2.0, "stepdown": 2.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "strategy": "zigzag"}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]},
                     {"kind": "plunge", "from": [0.0, 0.0, 10.0], "to": [0.0, 0.0, 5.0]}
                 ], "warnings": []}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v10 file must migrate");
    let e = model::find_by_name(&world, "P1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    assert!(
        matches!(o.params, OpParams::Pocket { .. }),
        "the pocket params read unchanged: {:?}",
        o.params
    );
    let path = tmp_path("v11");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the current stamp: {saved}"
    );

    // The pocket3d wire grammar, hand-written so the tags are pinned:
    // "pocket3d", snake_case entry, and no footprint key - the silhouette
    // is derived at generate, never carried.
    let text = r#"{
        "version": 11,
        "entities": [
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.4, "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "entry": "helix", "helix_radius": 2.0}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]}
                 ], "warnings": []}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v11 pocket3d grammar must read");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D {
        stepover,
        stock_to_leave,
        strategy,
        entry,
        helix_radius,
        retract_z,
        rest_rough,
        ..
    } = &o.params
    else {
        panic!("pocket3d params expected, got {:?}", o.params);
    };
    assert_eq!(*stepover, 2.5);
    assert_eq!(*stock_to_leave, 0.4);
    assert_eq!(*strategy, PocketStrategy::Waterline);
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Helix);
    assert_eq!(*helix_radius, 2.0);
    assert!(retract_z.is_none(), "absent retract stays absent");
    assert!(
        !*rest_rough,
        "absent rest_rough is off - a v11 file generated the plain ladder"
    );

    // Write-parse-write is a fixed point, and the entry tag is snake_case.
    let path = tmp_path("p3");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the pocket3d shape round-trips byte-identically");
    assert!(first.contains("\"pocket3d\""), "{first}");
    assert!(first.contains("\"entry\": \"helix\""), "{first}");
    assert!(!first.contains("footprint"), "the silhouette is never on the wire");

    // An unknown entry tag is corrupt text, refused, not defaulted.
    let bad = text.replace("\"helix\"", "\"spiral\"");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "must refuse an entry tag outside the vocabulary"
    );
}

#[test]
fn v11_file_steps_to_v12() {
    // A literal v11 project: the rest pass arrives as one optional key
    // (`rest_rough`), and a v11 file can contain it nowhere - the pocket
    // could not rest-rough. The serde default (off) is exactly what v11
    // generated, so the step-up is the stamp itself, like every bump
    // since v6.
    let text = r#"{
        "version": 11,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "zigzag", "entry": "ramp", "helix_radius": 1.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v11 file must migrate");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D { rest_rough, entry, .. } = &o.params else {
        panic!("the pocket3d params must read unchanged, got {:?}", o.params);
    };
    assert!(!*rest_rough, "a v11 file never rest-roughed; off is its meaning");
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Ramp);

    let path = tmp_path("v12");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v12 stamp: {saved}"
    );
    // The learned key is written in its false state: a v12 file names
    // the choice it made, so the shape stays a fixed point.
    assert!(saved.contains("\"rest_rough\": false"), "{saved}");
}

#[test]
fn v12_file_steps_to_v13() {
    // A literal v12 project: the engagement angle arrives as one
    // optional key (`max_engagement`), and a v12 file can contain it
    // nowhere - no v12 strategy read it. The serde default (45°, the
    // law's own) is unobservable there for exactly that reason, so the
    // step-up is the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 12,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "adaptive", "entry": "plunge", "helix_radius": 1.0}}}}
        ]
    }"#;
    let world = new_world();
    // The tag itself cannot appear in a real v12 file (the combo had no
    // Adaptive entry), but the wire accepts it as any other enum name -
    // what must land is the angle default.
    project::open(&world, text).expect("v12 file must migrate");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D { strategy, optimal_load, .. } = &o.params else {
        panic!("the pocket3d params must read unchanged, got {:?}", o.params);
    };
    assert_eq!(*strategy, PocketStrategy::Adaptive);
    assert!(
        *optimal_load == camcraft_app::load::DEFAULT_OPTIMAL_LOAD,
        "absent means the law's 45°, got {optimal_load}"
    );

    let path = tmp_path("v13");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v13 stamp: {saved}"
    );
    // The learned key is written in its stated state: a fixed point, so
    // a second open cannot drift the number.
    assert!(saved.contains("\"strategy\": \"adaptive\""), "{saved}");
    // M11.4 B: the new key is written, the legacy word never again.
    assert!(saved.contains("\"optimal_load\": 0.1464466094067262"), "{saved}");
    assert!(!saved.contains("max_engagement"), "{saved}");
}

#[test]
fn v13_file_steps_to_v14() {
    // A literal v13 project: the feed modulation arrives as one new
    // params key (`feed_opt`), absent from every v13 file - no v13
    // generator re-read a path against the sim. The serde default (off)
    // is exactly what v13 posted, every F word at the programmed feed,
    // so the step-up is the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 13,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "entry": "plunge", "helix_radius": 1.0,
                 "rest_rough": true, "max_engagement": 40.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v13 file must migrate");
    let e = model::find_by_name(&world, "P3").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Pocket3D { feed_opt, rest_rough, optimal_load, .. } = &o.params else {
        panic!("the pocket3d params must read unchanged, got {:?}", o.params);
    };
    assert!(!*feed_opt, "a v13 file never modulated feeds; off is its meaning");
    assert!(*rest_rough, "the stated keys ride through the step-up untouched");
    assert!(
        optimal_load.to_bits() == camcraft_app::load::load_of_degrees(40.0).to_bits(),
        "a stated angle must survive the migration, got {optimal_load}"
    );

    let path = tmp_path("v14");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v14 stamp: {saved}"
    );
    // The learned key is written in its false state: a fixed point, so
    // a second open cannot drift it.
    assert!(saved.contains("\"feed_opt\": false"), "{saved}");
}

#[test]
fn v14_file_steps_to_v15_and_the_finish3d_grammar_round_trips() {
    // A literal v14 project: the waterline arrives as one new params
    // variant (`finish3d`), which no v14 file can contain - the tag did
    // not exist. Nothing existing changed shape; step-up is the stamp
    // itself, like every bump since v6.
    let text = r#"{
        "version": 14,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "entry": "plunge", "helix_radius": 1.0,
                 "rest_rough": false, "max_engagement": 45.0, "feed_opt": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v14 file must migrate");
    let path = tmp_path("v15a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v15 stamp: {saved}"
    );
    assert!(saved.contains("\"feed_opt\": true"), "the stated key rides through: {saved}");

    // The v15 wire grammar, hand-written so the tags are pinned:
    // "finish3d", no stepdown key (the scallop law derives it from
    // tolerance and tool - storing one could only disagree), and an
    // absent retract_z means hop-to-clearance.
    let text = r#"{
        "version": 15,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v15 finish3d grammar must read");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D {
        tolerance,
        retract_z,
        climb,
        feed,
        ..
    } = &o.params
    else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(*tolerance, 0.1);
    assert!(*climb);
    assert_eq!(*feed, 500.0);
    assert!(retract_z.is_none(), "absent retract means hop-to-clearance");

    // Write-parse-write is a fixed point, and the variant tag is clean.
    let path = tmp_path("v15b");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the finish3d shape round-trips byte-identically");
    assert!(first.contains("\"finish3d\""), "{first}");
    assert!(!first.contains("retract_z"), "None is absent, not null: {first}");

    // A tolerance of zero is nonsense the loader refuses, not a default.
    let bad = text.replace("\"tolerance\": 0.1", "\"tolerance\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v15 file with tolerance 0.0 must not load"
    );
}

#[test]
fn v15_file_steps_to_v16_and_the_parallel_grammar_round_trips() {
    // A literal v15 project: the parallel finish arrives as two new
    // keys inside `finish3d` (`pattern`, `stepover`), absent from every
    // v15 file - the serde defaults (waterline, derived stepover) are
    // exactly what v15 ran. Nothing existing changed shape; step-up is
    // the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 15,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v15 file must migrate");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { pattern, stepover, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(*pattern, camcraft_app::finishing3d::FinishPattern::Waterline);
    assert!(stepover.is_none(), "absent stepover means the cos-law derives it");
    let path = tmp_path("v16a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v16 stamp: {saved}"
    );
    // The default writes out explicitly now that it is a choice among
    // three, not the only possibility.
    assert!(saved.contains("\"pattern\": \"waterline\""), "{saved}");

    // The v16 wire grammar, hand-written so the spellings are pinned:
    // snake_case pattern words, an explicit stepover override.
    let text = r#"{
        "version": 16,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "pattern": "parallel_multi", "stepover": 0.3, "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v16 parallel grammar must read");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { pattern, stepover, ramp, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(
        *pattern,
        camcraft_app::finishing3d::FinishPattern::ParallelMulti
    );
    assert_eq!(*stepover, Some(0.3));
    assert!(!*ramp, "a v16 file has no `ramp` word: the hop, exactly what v16 ran");

    // Write-parse-write is a fixed point, and the new keys ride.
    let path = tmp_path("v16b");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world2 = new_world();
    project::load(&world2, &path).expect("reload");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the parallel shape round-trips byte-identically");
    assert!(first.contains("\"pattern\": \"parallel_multi\""), "{first}");
    assert!(first.contains("\"stepover\": 0.3"), "{first}");

    // A zero stepover is nonsense the loader refuses, not a default.
    let bad = text.replace("\"stepover\": 0.3", "\"stepover\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v16 file with stepover 0.0 must not load"
    );

    // M2.2c: "scallop" is a new word in the same v16 grammar - no
    // version bump, because no v16 file ever carried it and every
    // v16 file still reads exactly as it did.
    let sc = text.replace("\"parallel_multi\"", "\"scallop\"");
    let world4 = new_world();
    project::open(&world4, &sc).expect("the scallop spelling must read");
    let e4 = model::find_by_name(&world4, "F1").expect("op");
    let o4 = model::operation_of(&world4, &e4).expect("operation");
    let OpParams::Finish3D { pattern, .. } = &o4.params else {
        panic!("finish3d params expected");
    };
    assert_eq!(*pattern, camcraft_app::finishing3d::FinishPattern::Scallop);
    let path_sc = tmp_path("v16s");
    project::save(&world4, &path_sc).expect("save");
    let saved_sc = std::fs::read_to_string(&path_sc).expect("read");
    let _ = std::fs::remove_file(&path_sc);
    assert!(saved_sc.contains("\"pattern\": \"scallop\""), "{saved_sc}");

    // M2.2d and M2.2e, same law four times over: "radial", "spiral",
    // "contour" and "project" join the v16 grammar as words, not as
    // a version.
    for (word, want) in [
        ("radial", camcraft_app::finishing3d::FinishPattern::Radial),
        ("spiral", camcraft_app::finishing3d::FinishPattern::Spiral),
        ("contour", camcraft_app::finishing3d::FinishPattern::Contour),
        ("project", camcraft_app::finishing3d::FinishPattern::Project),
    ] {
        let t = text.replace("\"parallel_multi\"", &format!("\"{word}\""));
        let w = new_world();
        project::open(&w, &t).expect("the polar spellings must read");
        let e = model::find_by_name(&w, "F1").expect("op");
        let o = model::operation_of(&w, &e).expect("operation");
        let OpParams::Finish3D { pattern, .. } = &o.params else {
            panic!("finish3d params expected");
        };
        assert_eq!(*pattern, want);
        let path_p = tmp_path("v16p");
        project::save(&w, &path_p).expect("save");
        let saved_p = std::fs::read_to_string(&path_p).expect("read");
        let _ = std::fs::remove_file(&path_p);
        assert!(saved_p.contains(&format!("\"pattern\": \"{word}\"")), "{saved_p}");
    }
    // An unknown pattern word is refused, never defaulted.
    let bogus = text.replace("\"parallel_multi\"", "\"zigzag\"");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &bogus), Err(ProjectError::Corrupt(_))),
        "a v16 file with an unknown pattern must not load"
    );
}

#[test]
fn v16_file_steps_to_v17_and_the_ramp_grammar_round_trips() {
    // v16 -> v17: one more new key inside `finish3d` - `ramp` - absent
    // from every v16 file, whose serde default (false: the hop) is
    // exactly what v16 ran. Nothing existing changed shape; step-up is
    // the stamp itself, like every bump since v6.
    let text = r#"{
        "version": 16,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "pattern": "parallel", "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v16 file must migrate");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { ramp, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert!(!*ramp, "absent ramp means the hop - what every pre-v17 file ran");
    let path = tmp_path("v17a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v17 stamp: {saved}"
    );
    // Written always, like `climb`: the choice is a fact about the file,
    // not a default to lean on.
    assert!(saved.contains("\"ramp\": false"), "{saved}");

    // The v17 wire grammar, hand-written so the spelling is pinned:
    // `"ramp": true` rides, and the write-parse-write fixed point keeps it.
    let rt = text
        .replace("\"version\": 16", "\"version\": 17")
        .replace("\"climb\": true", "\"climb\": true, \"ramp\": true");
    let world2 = new_world();
    project::open(&world2, &rt).expect("the v17 ramp spelling must read");
    let e2 = model::find_by_name(&world2, "F1").expect("op");
    let o2 = model::operation_of(&world2, &e2).expect("operation");
    let OpParams::Finish3D { ramp, .. } = &o2.params else {
        panic!("finish3d params expected");
    };
    assert!(*ramp);
    let path = tmp_path("v17b");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world3 = new_world();
    project::load(&world3, &path).expect("reload");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the ramp shape round-trips byte-identically");
    assert!(first.contains("\"ramp\": true"), "{first}");

    // A ramp that is not a bool is corruption, not a default.
    let bad = rt.replace("\"ramp\": true", "\"ramp\": \"yes\"");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad), Err(ProjectError::Corrupt(_))),
        "a non-bool ramp must not load"
    );
}

#[test]
fn v17_file_steps_to_v18_and_the_surface_finish_grammar_round_trips() {
    // v17 -> v18: two more keys inside `finish3d` - `min_step` (the
    // march's refinement floor) and `ramp_angle` (the chain's slope
    // cap) - absent from every v17 file, whose serde defaults are
    // exactly what v17 ran: no floor (the march's own 1/4096 right)
    // and the 45-degree diagonal v17's chain law already certified.
    // Nothing existing changed shape; step-up is the stamp itself,
    // like every bump since v6.
    let text = r#"{
        "version": 17,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "pattern": "parallel", "tolerance": 0.1, "ramp": true,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v17 file must migrate");
    let e = model::find_by_name(&world, "F1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Finish3D { min_step, ramp_angle, .. } = &o.params else {
        panic!("finish3d params expected, got {:?}", o.params);
    };
    assert_eq!(*min_step, None, "absent floor means the march's own right - what every pre-v18 file ran");
    assert_eq!(*ramp_angle, 45.0, "absent angle means the v17 diagonal");
    let path = tmp_path("v18a");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v18 stamp: {saved}"
    );
    // The floor is Optional and stays absent while it is None (the
    // skip-if-none law, like stepover); the angle is always - like
    // the hop plane, a default is a fact about the machine, not a
    // key to economize on.
    assert!(!saved.contains("\"min_step\""), "None must not mint a key: {saved}");
    assert!(saved.contains("\"ramp_angle\": 45.0"), "{saved}");

    // The v18 wire grammar, hand-written so the spelling is pinned:
    // floor and angle ride, and the write-parse-write fixed point
    // keeps them.
    let rt = text
        .replace("\"version\": 17", "\"version\": 18")
        .replace("\"climb\": true", "\"climb\": true, \"min_step\": 0.05, \"ramp_angle\": 30.0");
    let world2 = new_world();
    project::open(&world2, &rt).expect("the v18 floor-and-angle spelling must read");
    let e2 = model::find_by_name(&world2, "F1").expect("op");
    let o2 = model::operation_of(&world2, &e2).expect("operation");
    let OpParams::Finish3D { min_step, ramp_angle, .. } = &o2.params else {
        panic!("finish3d params expected");
    };
    assert_eq!(*min_step, Some(0.05));
    assert_eq!(*ramp_angle, 30.0);
    let path = tmp_path("v18b");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    let world3 = new_world();
    project::load(&world3, &path).expect("reload");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the surface-finish shape round-trips byte-identically");
    assert!(first.contains("\"min_step\": 0.05"), "{first}");

    // Corruptions are corruption, not defaults: a floor that is not
    // a number, and an angle that is not one either.
    let bad = rt.replace("\"min_step\": 0.05", "\"min_step\": \"fine\"");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &bad), Err(ProjectError::Corrupt(_))),
        "a non-number min_step must not load"
    );
    let bad = rt.replace("\"ramp_angle\": 30.0", "\"ramp_angle\": \"steep\"");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &bad), Err(ProjectError::Corrupt(_))),
        "a non-number ramp_angle must not load"
    );
}

#[test]
fn v18_file_steps_to_v19_and_the_machine_grammar_round_trips() {
    // v18 -> v19 (M2.3 C): one new top-level key, `machine` - {path,
    // machine} - read into a world singleton exactly like Units. The
    // step-up is the serde default itself: a v18 file carries no key,
    // loads no machine, and behaves exactly as it did at v18 - an
    // unstated limit checks nothing, in document form.
    let text = r#"{"version": 18, "entities": []}"#;
    let world = new_world();
    project::open(&world, text).expect("a v18 file must still open");
    let (path, m) = machine::of(&world);
    assert_eq!(path, "", "a v18 file names no machine file");
    assert!(m.is_none(), "a v18 file mounts no machine");
    let p = tmp_path("v19a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    let _ = std::fs::remove_file(&p);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v19 stamp: {saved}"
    );
    assert!(
        !saved.contains("\"machine\""),
        "no machine must not mint a key (skip-if-none): {saved}"
    );

    // The v19 grammar, hand-written so the spelling is pinned. Numbers
    // are our own testbench, never a vendor's.
    let rt = r#"{
        "version": 19,
        "entities": [],
        "machine": {"path": "testbench.mch", "machine": {
            "model": "Testbench 1", "vendor": "CamCraft Testbench",
            "spindle_min": 50.0, "spindle_max": 12000.0,
            "axes": [
                {"name": "X", "feed": 5000.0, "rapid": 15000.0, "travel": [-100.0, 100.0]},
                {"name": "Z", "feed": 3000.0, "travel": [-50.0, 100.0]}
            ],
            "max_tool_length": 150.0, "max_tool_diameter": 20.0
        }}
    }"#;
    let world = new_world();
    project::open(&world, rt).expect("the v19 machine grammar must read");
    let (path, Some(m)) = machine::of(&world) else {
        panic!("the machine must be mounted by name");
    };
    assert_eq!(path, "testbench.mch");
    assert_eq!(m.model, "Testbench 1");
    assert_eq!(m.spindle_max, Some(12000.0));
    assert_eq!(m.axes.len(), 2);
    assert_eq!(m.axes[0].name, "X");
    assert_eq!(m.axes[0].feed, Some(5000.0));
    assert_eq!(m.axes[0].travel, Some((-100.0, 100.0)));
    assert_eq!(m.axes[1].rapid, None, "the file did not say, so nothing checks");

    // Write-parse-write fixed point, file style (the template's law).
    let p = tmp_path("v19b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    let (_, m2) = machine::of(&world2);
    assert_eq!(m2, Some(m), "the machine must round-trip intact");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert!(first.contains("\"machine\""), "a mounted machine mints its key: {first}");
    assert_eq!(first, second, "the machine shape round-trips byte-identically");

    // Corruption, not permissive defaults: a stroke that runs backwards
    // states a limit that cannot be meant, and the file refuses it
    // BEFORE the world moves (validate-early at the file boundary).
    let bad = rt.replace("\"travel\": [-100.0, 100.0]", "\"travel\": [100.0, -100.0]");
    assert_ne!(bad, rt, "the test must reach the travel spelling");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "an insane machine must refuse the file"
    );
    let (_, m3) = machine::of(&world3);
    assert!(m3.is_none(), "a refused load mounts nothing");
}

#[test]
fn v19_file_steps_to_v20_and_the_rev_counter_round_trips() {
    // v19 -> v20 (M3.1 A): one new per-entity key, `rev` - the geometry
    // revision counter. The step-up is the serde default itself: a v19
    // file carries no key, loads no counter, and `rev_of` reads it as 0,
    // exactly what a pre-M3.1 document was. Staleness recomputes from the
    // facets, so nothing else moves.
    let quad = r#"[[[0.0,0.0,0.0],[10.0,0.0,0.0],[10.0,10.0,0.0]],[[0.0,0.0,0.0],[10.0,10.0,0.0],[0.0,10.0,0.0]]]"#;
    let text = format!(
        r#"{{"version": 19, "entities": [{{"name": "legacy", "translation": [0.0, 0.0, 0.0], "facets": {quad}}}]}}"#
    );
    let world = new_world();
    project::open(&world, &text).expect("a v19 file must still open");
    let e = model::find_by_name(&world, "legacy").expect("loaded");
    assert_eq!(model::rev_of(&world, &e), 0, "no key means no remembered landings");
    let p = tmp_path("v20a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    let _ = std::fs::remove_file(&p);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v20 stamp: {saved}"
    );
    assert!(
        !saved.contains("\"rev\""),
        "a 0-revision model must not mint a key (skip-if-zero): {saved}"
    );

    // The v20 grammar, hand-written so the spelling is pinned: a model
    // that has been updated lands its counter.
    let rt = format!(
        r#"{{"version": 20, "entities": [{{"name": "grown", "translation": [0.0, 0.0, 0.0], "facets": {quad}, "rev": 3}}]}}"#
    );
    let world = new_world();
    project::open(&world, &rt).expect("the v20 rev grammar must read");
    let e = model::find_by_name(&world, "grown").expect("loaded");
    assert_eq!(model::rev_of(&world, &e), 3, "the counter reads back");

    // Write-parse-write fixed point (the machine test's law): a saved
    // counter is stable text, and the fresh 0-model stays key-free.
    let p = tmp_path("v20b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    let e2 = model::find_by_name(&world2, "grown").expect("reloaded");
    assert_eq!(model::rev_of(&world2, &e2), 3, "reload keeps the count");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert!(first.contains("\"rev\": 3"), "a counted model mints its key: {first}");
    assert_eq!(first, second, "the rev shape round-trips byte-identically");
}

#[test]
fn stock_set_twice_keeps_its_rev_across_save_load_save() {
    // #340: SetStock lands facets and moves the geometry counter like any
    // landing; the loader re-lands the stock mesh, so without the Model
    // arm's pin a twice-set stock saved "rev": 2, loaded as 1 and the
    // re-save differed from the file it was opened from.
    let world = new_world();
    let mut history = History::default();
    for dz in [10.0, 12.0] {
        history.execute(
            &world,
            SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 30.0, dz }, Location::default())
                .expect("box stock"),
        );
    }
    let p = tmp_path("stockrev");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    assert!(first.contains("\"rev\": 2"), "a twice-set stock saves its count: {first}");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "save -> load -> save of a stock is byte-identical");
}

#[test]
fn v20_file_steps_to_v21_and_the_setups_grammar_round_trips() {
    // v20 -> v21 (M3.2 A): setups. No v20 file can carry a setup row (the
    // kind tag did not exist), an operation membership, or an active-setup
    // cursor - and every serde default (none, none, none) is exactly the
    // legacy global the implicit-setup law already reproduces. Step-up is
    // the stamp itself; check_and_apply rewrites nothing.
    let quad = r#"[[[0.0,0.0,0.0],[40.0,0.0,0.0],[40.0,40.0,0.0]],[[0.0,0.0,0.0],[40.0,40.0,0.0],[0.0,40.0,0.0]]]"#;
    let text = format!(
        r#"{{"version": 20, "entities": [
            {{"name": "plate", "translation": [0.0, 0.0, 0.0], "facets": {quad}}},
            {{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
              "stock": {{"box": {{"dx": 40.0, "dy": 40.0, "dz": 20.0}}}}}},
            {{"name": "G54", "translation": [1.0, 2.0, 3.0], "kind": "wcs"}}
        ]}}"#
    );
    let world = new_world();
    project::open(&world, &text).expect("a v20 file must still open");
    assert!(setup::names(&world).is_empty(), "the step-up mints no rows");
    assert_eq!(setup::active(&world), None, "no setups: no cursor");
    assert_eq!(
        setup::stock_entity_name(&world, None),
        "Stock",
        "the globals read through the implicit-setup law"
    );
    let p = tmp_path("v21a");
    project::save(&world, &p).expect("save");
    let v21 = std::fs::read_to_string(&p).expect("read");
    let _ = std::fs::remove_file(&p);
    assert!(
        v21.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v21 stamp: {v21}"
    );
    assert!(
        !v21.contains("\"setup\"") && !v21.contains("active_setup"),
        "a save of the migrated legacy document keeps the legacy byte shape: {v21}"
    );

    // Materializing the legacy document adds rows, not data: the setup
    // points at the entities that were always there, and the grammar that
    // appears round-trips from here on.
    let mut history = History::default();
    let cmd = CreateSetup::new(&world, None, None).expect("materialize");
    history.execute(&world, cmd);
    assert_eq!(setup::names(&world), ["Setup1"]);
    assert_eq!(setup::active(&world).as_deref(), Some("Setup1"));
    let def = setup::def_of(&world, "Setup1").expect("definition");
    assert_eq!(def.stock, "Stock", "materialization points, never copies");
    assert_eq!(def.wcs, "G54", "#342: the setup's one WCS is the legacy G54");
    assert_eq!(def.datum_from, "");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    assert_eq!(setup::names(&world2), ["Setup1"]);
    assert_eq!(setup::active(&world2).as_deref(), Some("Setup1"));
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert!(
        first.contains("\"kind\": \"setup\"") && first.contains("\"active_setup\": \"Setup1\""),
        "a materialized document mints the setup grammar: {first}"
    );
    assert_eq!(first, second, "the setups grammar round-trips byte-identically");

    // The hand-written v21 grammar itself: a setup naming private furniture
    // (an entry naming nothing is an unplaced slot, not a missing entity),
    // and the raw cursor saved as stored, dangling included.
    let text = r#"{
        "version": 21,
        "active_setup": "Setup2",
        "entities": [
            {"name": "Setup1", "translation": [0.0, 0.0, 0.0], "kind": "setup",
             "setup": {"stock": "Stock", "wcs": ["G54"]}},
            {"name": "Setup2", "translation": [0.0, 0.0, 0.0], "kind": "setup",
             "setup": {"stock": "Setup2/Stock", "wcs": ["Setup2/G54", "Setup2/G55"],
                       "datum_from": "Setup1"}},
            {"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {"box": {"dx": 40.0, "dy": 40.0, "dz": 20.0}}},
            {"name": "Setup2/Stock", "translation": [10.0, 0.0, 0.0], "kind": "stock",
             "stock": {"box": {"dx": 20.0, "dy": 20.0, "dz": 5.0}}},
            {"name": "Setup2/G54", "translation": [5.0, 0.0, 0.0], "kind": "wcs"}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v21 setups grammar must read");
    assert_eq!(setup::names(&world), ["Setup1", "Setup2"]);
    assert_eq!(setup::active(&world).as_deref(), Some("Setup2"));
    assert_eq!(setup::stock_entity_name(&world, Some("Setup2")), "Setup2/Stock");
    assert!(
        setup::wcs_entity(&world, Some("Setup2")).is_some(),
        "the posted slot (G54) is the setup's one WCS (#342)"
    );
    assert_eq!(setup::wcs_entity_name(&world, Some("Setup2")), "Setup2/G54");
    assert_eq!(
        setup::def_of(&world, "Setup2").expect("def").datum_from,
        "Setup1",
        "clone provenance rides along for the reader"
    );
    let p = tmp_path("v21b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    assert_eq!(first, second, "hand-written and saved agree on the spelling");

    // A cursor naming a deleted setup is legal - the same existence
    // fallback that protects an undone DeleteSetup - and saves raw.
    let dangling = text.replace("\"active_setup\": \"Setup2\"", "\"active_setup\": \"Nope\"");
    let world = new_world();
    project::open(&world, &dangling).expect("a dangling cursor must read");
    assert_eq!(
        setup::active(&world).as_deref(),
        Some("Setup1"),
        "stored dangling, effective falls back to the first"
    );
    project::save(&world, &p).expect("save");
    let raw = std::fs::read_to_string(&p).expect("read");
    assert!(
        raw.contains("\"active_setup\": \"Nope\""),
        "the singleton saves raw, not resolved: {raw}"
    );
    let _ = std::fs::remove_file(&p);

    // Some("") is the one cursor state with no meaning: corrupt.
    let empty = text.replace("\"active_setup\": \"Setup2\"", "\"active_setup\": \"\"");
    let world = new_world();
    assert!(
        matches!(project::open(&world, &empty), Err(ProjectError::Corrupt(_))),
        "an empty cursor name must refuse, not guess"
    );
}

#[test]
fn v21_file_steps_to_v22_and_the_fixture_tag_round_trips() {
    // v21 -> v22 (M3.2 B): fixtures. No v21 file can carry a `fixture`
    // marker - the component did not exist - and its default (no tag)
    // is exactly the v21 collision world: checks against stock and
    // model alone. Step-up is the stamp itself; check_and_apply
    // rewrites nothing. A fixture is a tagged MODEL (stage B's law),
    // so no new kind appears in the grammar either.
    let quad = r#"[[[0.0,0.0,0.0],[40.0,0.0,0.0],[40.0,40.0,0.0]],[[0.0,0.0,0.0],[40.0,40.0,0.0],[0.0,40.0,0.0]]]"#;
    let text = format!(
        r#"{{"version": 21, "entities": [
            {{"name": "plate", "translation": [0.0, 0.0, 0.0], "facets": {quad}}},
            {{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
              "stock": {{"box": {{"dx": 40.0, "dy": 40.0, "dz": 20.0}}}}}}
        ]}}"#
    );
    let world = new_world();
    project::open(&world, &text).expect("a v21 file must still open");
    let plate = model::find_by_name(&world, "plate").expect("loaded");
    assert!(
        !model::is_fixture(&world, &plate),
        "the step-up mints no tags"
    );
    let p = tmp_path("v22a");
    project::save(&world, &p).expect("save");
    let v22 = std::fs::read_to_string(&p).expect("read");
    assert!(
        v22.contains(&format!("\"version\": {}", project::PROJECT_VERSION))
            && !v22.contains("\"fixture\""),
        "a migrated legacy save keeps the legacy byte shape: {v22}"
    );

    // The v22 grammar itself, hand-written so the WIRE spelling is
    // pinned: a bare tag (the implicit-setup binding) and a tag naming
    // a declared setup. Round-trips byte-identically from a world, and
    // the two spellings agree with what a load-then-save writes.
    let text = format!(
        r#"{{"version": 22, "entities": [
            {{"name": "Setup2", "translation": [0.0, 0.0, 0.0], "kind": "setup",
              "setup": {{"stock": "Setup2/Stock", "wcs": []}}}},
            {{"name": "vise", "translation": [7.0, 0.0, 0.0], "facets": {quad},
              "fixture": {{}}}},
            {{"name": "clamp", "translation": [0.0, 12.0, 0.0], "facets": {quad},
              "fixture": {{"setup": "Setup2"}}}},
            {{"name": "Setup2/Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
              "stock": {{"box": {{"dx": 40.0, "dy": 40.0, "dz": 20.0}}}}}}
        ]}}"#
    );
    let world = new_world();
    project::open(&world, &text).expect("the v22 fixture grammar must read");
    for name in ["vise", "clamp"] {
        let e = model::find_by_name(&world, name).expect("tagged model");
        assert!(model::is_fixture(&world, &e), "'{name}' loaded tagged");
        assert!(model::facets_of(&world, &e).is_some(), "and still a solid");
    }
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    assert_eq!(first, second, "the fixture grammar round-trips byte-identically");
    // Compare on whitespace-stripped text: the writer pretty-prints, so
    // a NON-empty map spans lines (serde_json's one quirk: empty maps
    // stay `{}` inline). The grammar is what's asserted, not its layout.
    let flat: String = first.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        flat.contains("\"fixture\":{}") && flat.contains("\"fixture\":{\"setup\":\"Setup2\"}"),
        "the writer spells the bare tag as an empty object: {first}"
    );
    let _ = std::fs::remove_file(&p);

    // A fixture tag on a non-model row describes a CAM concept with no
    // world representation: corrupt, refused before any world write.
    let text = r#"{
        "version": 22,
        "entities": [
            {"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "fixture": {}, "stock": {"box": {"dx": 40.0, "dy": 40.0, "dz": 20.0}}}
        ]
    }"#;
    let world = new_world();
    assert!(
        matches!(project::open(&world, text), Err(ProjectError::Corrupt(_))),
        "stock wearing a fixture tag must refuse"
    );
    assert!(
        model::named_entities(&world).is_empty(),
        "refusal wrote nothing"
    );
}

#[test]
fn tangential_lead_round_trips() {
    // The new tag in both directions, and by hand-written text so the
    // wire spelling itself is pinned, not just Rust's serde mirror.
    let text = r#"{
        "version": 9,
        "entities": [
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": {"tangential": {"radius": 4.5}}, "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("a v9 file with the tangential tag must read");
    let e = model::find_by_name(&world, "C1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour { lead, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*lead, Lead::Tangential { radius: 4.5 });

    let path = tmp_path("lead-t");
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains("\"tangential\"") && saved.contains("\"radius\": 4.5"),
        "the tag writes back: {saved}"
    );
}

#[test]
fn level_link_choice_round_trips_and_the_default_writes_no_key() {
    use camcraft_app::links::{Link, Links};
    let stay = |angle: f64| Links { level: Link::KeepDown, transition: Some(camcraft_app::machining3d::Entry3D::Ramp), ramp_angle: angle, ..Links::PLANAR };
    let job = |links: Links| {
        let world = new_world();
        let mut history = History::default();
        history.execute(
            &world,
            SetStock::new(&world, StockShape::Box { dx: 40.0, dy: 40.0, dz: 10.0 }, Location::default())
                .expect("stock"),
        );
        history.execute(
            &world,
            AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("tool"),
        );
        history.execute(
            &world,
            ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
        );
        let params = OpParams::Contour {
            tilt: None,
            stock_to_leave: 0.0,
            clearance_z: Height::fixed(10.0),
            retract_z: Height::fixed(8.0),
            top_z: Height::fixed(5.0),
            bottom_z: Height::fixed(0.0),
            stepdown: 2.0,
            feed: 800.0,
            plunge_feed: 200.0,
            climb: true,
            side: Side::Auto,
            lead: Lead::None,
            tolerance: 0.01,
            tab_count: 0,
            tab_width: 0.0,
            tab_height: 0.0,
            entry: camcraft_app::machining3d::Entry3D::Plunge,
            helix_radius: 1.0,
            blend_turns: false,
            links,
        };
        history.execute(
            &world,
            CreateOperation::new(&world, "C1", "EM-0600-4FL", SrcRef::Entity("part".into()), params)
                .expect("create"),
        );
        world
    };
    // The default is absent from the file - every earlier save holds.
    let path = tmp_path("link-default");
    project::save(&job(Links::PLANAR), &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(!text.contains("level_link") && !text.contains("ramp_angle") && !text.contains("\"links\""), "{text}");
    // The user's pick and angle write back, and round-trip byte-for-byte.
    let path = tmp_path("link-stay");
    project::save(&job(stay(5.0)), &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"level_link\": \"stay_down_ramp\""), "{first}");
    assert!(first.contains("\"ramp_angle\": 5.0"), "{first}");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second);
    let e = model::find_by_name(&world2, "C1").expect("op");
    let o = model::operation_of(&world2, &e).expect("operation");
    let OpParams::Contour { links, .. } = o.params else {
        panic!("contour expected");
    };
    assert_eq!(links, stay(5.0), "the old keys migrate in memory to Keep down + Ramp");
}

#[test]
fn side_choice_round_trips_and_a_hand_edited_inside_refuses_at_generate() {
    // The stored value is the user's ask, never a resolution: Inside
    // survives save/load byte-for-byte...
    let path = tmp_path("side");
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            StockShape::Box {
                dx: 40.0,
                dy: 40.0,
                dz: 10.0,
            },
            Location::default(),
        )
        .expect("stock"),
    );
    history.execute(
        &world,
        AddTool::new(&world, "EM-0600-4FL", catalog_tool("EM-0600-4FL")).expect("tool"),
    );
    history.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::ZERO, Point3::new(10.0, 10.0, 5.0))),
    );
    history.execute(
        &world,
        CreateOperation::new(
            &world,
            "C1",
            "EM-0600-4FL",
            SrcRef::Entity("part".into()),
            OpParams::Contour { tilt: None,
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(10.0),
                retract_z: Height::fixed(8.0),
                top_z: Height::fixed(4.0),
                bottom_z: Height::fixed(2.0),
                stepdown: 8.0,
                feed: 800.0,
                plunge_feed: 200.0,
                climb: true,
                side: Side::Auto,
                lead: Lead::None,
                tolerance: 0.01,
                tab_count: 0,
                tab_width: 0.0,
                tab_height: 0.0,
                entry: camcraft_app::machining3d::Entry3D::Plunge,
                helix_radius: 1.0,
                blend_turns: false,
                links: camcraft_app::links::Links::PLANAR,
            },
        )
        .expect("create"),
    );
    history.execute(&world, GenerateOperation::new(&world, "C1").expect("generate"));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");

    // ...which a hand-edited file proves is loadable verbatim: commands
    // refuse Inside on a whole-section source (operations.rs's
    // `inside_on_entity_source_is_refused`), so the only way an Inside
    // + Entity op reaches a world is by editing the JSON. The load
    // accepts it; generate refuses it by name - the backstop.
    let edited = text.replace("\"side\": \"auto\"", "\"side\": \"inside\"");
    assert_ne!(edited, text, "the saved file carries the side key once");
    let world2 = new_world();
    project::open(&world2, &edited).expect("a hand-edited file loads as any other");
    let e = model::find_by_name(&world2, "C1").expect("op");
    let o = model::operation_of(&world2, &e).expect("operation");
    let OpParams::Contour { side, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*side, Side::Inside, "the ask survives the file");
    let err = GenerateOperation::new(&world2, "C1")
        .err()
        .expect("generate must backstop the hand-edited Inside");
    assert!(err.to_string().contains("needs picked rings"), "{err}");

    // And the honest path: an Inside on a Selection source round-trips
    // the choice and generates. The pick list is built against the
    // loaded world (feature ids are per-world), on the same path the
    // hand-edit took - minus the edit.
    let items = {
        let e = model::find_by_name(&world2, "part").expect("part");
        let f = model::features_of(&world2, &e).expect("features");
        let top = f
            .faces
            .iter()
            .filter(|fa| matches!(fa.kind, feature::FaceKind::Planar) && fa.normal[2].abs() > 0.9)
            .max_by(|a, b| a.offset.total_cmp(&b.offset))
            .map(|fa| fa.id)
            .expect("box top");
        vec![GeoSel {
            entity: "part".into(),
            feature: FeatureSel::Face(top),
        }]
    };
    history.execute(
        &world2,
        CreateOperation::new(
            &world2,
            "C2",
            "EM-0600-4FL",
            SrcRef::Selection(items),
            OpParams::Contour { tilt: None,
                stock_to_leave: 0.0,
                clearance_z: Height::fixed(10.0),
                retract_z: Height::fixed(8.0),
                top_z: Height::fixed(4.0),
                bottom_z: Height::fixed(2.0),
                stepdown: 8.0,
                feed: 800.0,
                plunge_feed: 200.0,
                climb: true,
                side: Side::Inside,
                lead: Lead::None,
                tolerance: 0.01,
                tab_count: 0,
                tab_width: 0.0,
                tab_height: 0.0,
                entry: camcraft_app::machining3d::Entry3D::Plunge,
                helix_radius: 1.0,
                blend_turns: false,
                links: camcraft_app::links::Links::PLANAR,
            },
        )
        .expect("create C2"),
    );
    // The picked ring is the part's own top-face edge, and Inside puts
    // the cutter 3 mm into the 5 mm-tall part from Z 4 down: a groove
    // through the finished part. The depth law refuses it by name -
    // the choice still round-trips below, the cut never generates.
    let err = GenerateOperation::new(&world2, "C2")
        .err()
        .expect("an Inside ring inside the part solid must refuse");
    assert!(err.to_string().contains("cuts into the part"), "{err}");
    project::save(&world2, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(
        first.contains("\"side\": \"inside\""),
        "the choice is in the file"
    );
    let world3 = new_world();
    project::load(&world3, &path).expect("load");
    project::save(&world3, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "the side choice round-trips byte-identically");
    let e = model::find_by_name(&world3, "C2").expect("op loaded");
    let o = model::operation_of(&world3, &e).expect("operation");
    let OpParams::Contour { side, .. } = &o.params else {
        panic!("contour params expected, got {:?}", o.params);
    };
    assert_eq!(*side, Side::Inside, "Inside survives the file unchanged");
}

#[test]
fn units_round_trip_and_a_v7_file_means_millimeters() {
    use camcraft_app::units::{self, Unit};

    // An inch document: the file says so, a save-load-save is stable,
    // and the millimeter numbers inside are untouched by the lens.
    let world = populated_world();
    units::set(&world, Unit::Inches);
    let path = tmp_path("units_in");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"units\": \"in\""), "inch must be written: {first}");
    let world2 = new_world();
    project::open(&world2, &first).expect("reopen");
    assert_eq!(units::of(&world2), Unit::Inches);
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "save-load-save must be stable");

    // A millimeter document writes no units key at all - the whole v7
    // corpus is mm, so absence must keep meaning mm.
    let world = populated_world();
    let path = tmp_path("units_mm");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(!text.contains("\"units\""), "mm is the default, not a key: {text}");

    // A literal v7 file: no units key anywhere, the step-up is the stamp.
    let text = r#"{
        "version": 7,
        "entities": [{
            "name": "legacy",
            "translation": [0.0, 0.0, 0.0],
            "rotation": [0.0, 0.0, 0.0, 1.0],
            "facets": [[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]]
        }]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("v7 must migrate");
    assert_eq!(units::of(&world), Unit::Millimeters, "a v7 file is millimeters");

    // Opening always lands the value: an inch file into an inch-leaked
    // world must come back mm.
    let world = new_world();
    units::set(&world, Unit::Inches);
    project::open(&world, text).expect("reopen into a dirty world");
    assert_eq!(units::of(&world), Unit::Millimeters, "no previous-document leak");

    // A nonsense unit is a corrupt file, refused before any world write.
    let text = r#"{"version": 8, "units": "furlongs", "entities": []}"#;
    let world = new_world();
    match project::open(&world, text) {
        Err(ProjectError::Corrupt(msg)) => assert!(msg.contains("furlongs"), "{msg}"),
        other => panic!("expected Corrupt, got {other:?}"),
    }
    assert_eq!(units::of(&world), Unit::Millimeters, "refusal wrote nothing");
}

#[test]
fn the_post_choice_round_trips_and_a_v22_file_means_the_built_in() {
    // M3.3 A's persistence law: a chosen file post rides the save as
    // the top-level `post` key, opening lands exactly what was chosen,
    // a v22-shaped file (no key) means the built-in - the pure
    // step-up, same as every bump since v6 - and opening a file
    // without the key CLEARS a stale choice, the units-law leak guard.
    use camcraft_app::commands::SelectPost;
    use camcraft_app::posting;
    use camcraft_app::schema::PostRef;

    let cps = std::env::temp_dir().join(format!("camcraft_rtpost_{}.cps", std::process::id()));
    std::fs::write(&cps, "function onOpen() { writeln(\"(RT)\"); }").expect("write temp cps");
    let cps_name = cps.display().to_string();

    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SelectPost::new(&world, PostRef::File(cps_name.clone())).expect("the script runs"),
    );
    let p = tmp_path("postrt");
    project::save(&world, &p).expect("save");
    let text = std::fs::read_to_string(&p).expect("written");
    assert!(text.contains("\"post\""), "the chosen file rides the file: no key found");

    let world2 = new_world();
    project::load(&world2, &p).expect("load");
    assert_eq!(posting::post_of(&world2), PostRef::File(cps_name.clone()));

    // A bare v22 document migrates to "the built-in", verbatim.
    let world3 = new_world();
    project::open(&world3, r#"{"version": 22, "entities": []}"#).expect("v22 steps up");
    assert_eq!(posting::post_of(&world3), PostRef::Builtin);

    // The leak guard: opening a no-post file onto a world with a
    // choice lands BUILT-IN, not the stale singleton.
    let plain = tmp_path("postplain");
    let clean = new_world();
    project::save(&clean, &plain).expect("save clean");
    project::load(&world2, &plain).expect("load onto the chosen world");
    assert_eq!(posting::post_of(&world2), PostRef::Builtin, "opening never leaks a stale post");

    let _ = std::fs::remove_file(&cps);
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(&plain);
}

#[test]
fn the_project_file_never_carries_the_theme_and_an_old_theme_key_is_ignored() {
    // #877: the theme is the user's preference, not the document's. A
    // save writes no `theme` key whatever theme is worn, and opening an
    // older file that still carries one - any spelling, even a stranger
    // name or a gone file - loads cleanly and leaves the user's theme
    // exactly as it was. Without this, a project could override the
    // preference again.
    use camcraft_app::commands::{theme_of, theme_set};
    use camcraft_app::schema::ThemeRef;

    let world = new_world();
    theme_set(&world, &ThemeRef::Named("light".into()));
    let p = tmp_path("themenone");
    project::save(&world, &p).expect("save");
    let text = std::fs::read_to_string(&p).expect("written");
    let json: serde_json::Value = serde_json::from_str(&text).expect("parses");
    assert!(json.get("theme").is_none(), "no theme key: {text}");

    for old in [
        r#"{"version": 41, "theme": {"named": "dark"}, "entities": []}"#,
        r#"{"version": 41, "theme": {"named": "solarpunk"}, "entities": []}"#,
        r#"{"version": 41, "theme": {"file": "C:/gone/theme.json"}, "entities": []}"#,
        r#"{"version": 40, "entities": []}"#,
    ] {
        project::open(&world, old).expect("an older file opens");
        assert_eq!(theme_of(&world), ThemeRef::Named("light".into()), "opening kept the user's theme: {old}");
    }
    project::load(&world, &p).expect("load our own save");
    assert_eq!(theme_of(&world), ThemeRef::Named("light".into()));
    let _ = std::fs::remove_file(&p);
}

#[test]
fn v23_file_steps_to_v24_and_the_5axis_grammar_round_trips() {
    // M5.1 stage C: the 5-axis twins arrive as two new params tags
    // (`contour5d`, `parallel5d`), which no v23 file can contain - the
    // kind tags did not exist. Nothing existing changed shape;
    // step-up is the stamp itself, like every bump since v6.
    let world = new_world();
    project::open(&world, r#"{"version": 23, "entities": []}"#).expect("v23 steps up");
    let p = tmp_path("v24a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v24 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v24 wire grammar, hand-written so the tags are pinned:
    // the finish's scalar shape plus the axis trio; the contour mints
    // no stepover key at all (the kind has no such field), and the
    // parallel's absent retract means hop-to-clearance exactly like
    // the pocket family's.
    let text = r#"{
        "version": 24,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "C5", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"contour5d": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 600.0, "plunge_feed": 120.0, "climb": true,
                 "lead_deg": 15.0, "lag_deg": -5.0, "max_tilt_deg": 40.0}}}},
            {"name": "P5", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"parallel5d": {
                 "tolerance": 0.1, "stepover": 1.2,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": false,
                 "retract_z": {"reference": "fixed", "offset": 30.0},
                 "lead_deg": 0.0, "lag_deg": 0.0, "max_tilt_deg": 90.0}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v24 5-axis grammar must read");
    let e = model::find_by_name(&world, "C5").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour5D {
        tolerance,
        retract_z,
        lead_deg,
        lag_deg,
        max_tilt_deg,
        ..
    } = &o.params
    else {
        panic!("contour5d params expected, got {:?}", o.params);
    };
    assert_eq!(*tolerance, 0.05);
    assert!(retract_z.is_none(), "absent retract means hop-to-clearance");
    assert_eq!((*lead_deg, *lag_deg, *max_tilt_deg), (15.0, -5.0, 40.0));
    let e = model::find_by_name(&world, "P5").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Parallel5D { stepover, retract_z, .. } = &o.params else {
        panic!("parallel5d params expected, got {:?}", o.params);
    };
    assert_eq!(*stepover, Some(1.2));
    assert!(retract_z.is_some(), "the stated hop rides through");

    // Write-parse-write is a fixed point, and both tags are clean.
    let p = tmp_path("v24b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the 5-axis shapes round-trip byte-identically");
    assert!(first.contains("\"contour5d\"") && first.contains("\"parallel5d\""), "{first}");
    assert!(!first.contains("\"pattern\""), "no finish pattern key in either: {first}");

    // The sane door at the loader: a 120-degree lead is past the
    // quadrant the axis law can state, and a 200-degree cone excludes
    // nothing - both are typos about the machine, refused by name.
    let bad = text.replace("\"lead_deg\": 15.0", "\"lead_deg\": 120.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v24 file with a 120-degree lead must not load"
    );
}

#[test]
fn v24_file_steps_to_v25_and_the_swarf_grammar_round_trips() {
    // M5.1 stage D: the swarf family arrives as two more params tags
    // (`swarf`, `advanced_swarf`). Nothing existing changed shape, so
    // step-up is the stamp itself, like every bump since v6.
    let world = new_world();
    project::open(&world, r#"{"version": 24, "entities": []}"#).expect("v24 steps up");
    let p = tmp_path("v25a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v25 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v25 wire grammar, hand-written so the tags are pinned: the
    // swarf's scalar shape is the parallel's minus pattern and plus the
    // axis trio it already had; the ladder's `passes` is a REQUIRED
    // key - an absent one is a corrupt file, not a default, because a
    // silently-1 pass count would replay a roughing spec as a finish.
    let text = r#"{
        "version": 25,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "SW1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"swarf": {
                 "tolerance": 0.1, "stepover": 1.5,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true,
                 "lead_deg": 15.0, "lag_deg": 0.0, "max_tilt_deg": 60.0}}}},
            {"name": "SW2", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"advanced_swarf": {
                 "tolerance": 0.05, "stepover": 2.0,
                 "clearance_z": {"reference": "fixed", "offset": 30.0},
                 "retract_z": {"reference": "fixed", "offset": 40.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 400.0, "plunge_feed": 80.0, "climb": false,
                 "lead_deg": 0.0, "lag_deg": 0.0, "max_tilt_deg": 90.0,
                 "passes": 3}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v25 swarf grammar must read");
    let e = model::find_by_name(&world, "SW1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Swarf { stepover, retract_z, tolerance, lead_deg, max_tilt_deg, .. } = &o.params
    else {
        panic!("swarf params expected, got {:?}", o.params);
    };
    assert_eq!((*tolerance, *stepover, *lead_deg, *max_tilt_deg), (0.1, Some(1.5), 15.0, 60.0));
    assert!(retract_z.is_none(), "absent retract means hop-to-clearance");
    let e = model::find_by_name(&world, "SW2").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::AdvancedSwarf { passes, retract_z, .. } = &o.params else {
        panic!("advanced_swarf params expected, got {:?}", o.params);
    };
    assert_eq!(*passes, 3);
    assert!(retract_z.is_some(), "the stated hop rides through");

    // Write-parse-write is a fixed point; `passes` appears exactly
    // once, because the plain kind's DTO mints no such key.
    let p = tmp_path("v25b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the swarf shapes round-trip byte-identically");
    assert!(first.contains("\"swarf\"") && first.contains("\"advanced_swarf\""), "{first}");
    assert_eq!(first.matches("\"passes\"").count(), 1, "the plain kind must not grow a passes key");

    // The sane doors at the loader: a 9-rung ladder is outside the
    // law's 1..4 by name, and a missing rung count is a corrupt
    // intent, not a default.
    let bad = text.replace("\"passes\": 3", "\"passes\": 9");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v25 file with a 9-pass ladder must not load"
    );
    let holey = text.replace("\"passes\": 3", "");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &holey), Err(ProjectError::Corrupt(_))),
        "advanced_swarf without passes is a corrupt file, not a silent 1"
    );
}

#[test]
fn v25_file_steps_to_v26_and_the_rotary_grammar_round_trips() {
    // M5.1 stage E: the rotary twins arrive as two more params tags
    // (`rotary_contour`, `rotary_parallel`). Nothing existing changed
    // shape - the fitted axis is model data, so the file gains no key
    // for it - and step-up is the stamp itself, like every bump
    // since v6.
    let world = new_world();
    project::open(&world, r#"{"version": 25, "entities": []}"#).expect("v25 steps up");
    let p = tmp_path("v26a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v26 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v26 wire grammar, hand-written so the tags are pinned: the
    // swarf family's shape with the ruling's word swapped for the
    // axis - which is to say, no extra word at all. The contour
    // mints no stepover key (the kind has no such field), and the
    // parallel's absent retract means hop-to-clearance like every
    // raster kind's.
    let text = r#"{
        "version": 26,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "RC1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"rotary_contour": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 600.0, "plunge_feed": 120.0, "climb": true,
                 "lead_deg": 15.0, "lag_deg": -5.0, "max_tilt_deg": 40.0}}}},
            {"name": "RP1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"rotary_parallel": {
                 "tolerance": 0.1, "stepover": 1.2,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "retract_z": {"reference": "fixed", "offset": 30.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": false,
                 "lead_deg": 0.0, "lag_deg": 0.0, "max_tilt_deg": 90.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v26 rotary grammar must read");
    let e = model::find_by_name(&world, "RC1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::RotaryContour { tolerance, retract_z, lead_deg, lag_deg, max_tilt_deg, .. } =
        &o.params
    else {
        panic!("rotary_contour params expected, got {:?}", o.params);
    };
    assert_eq!(*tolerance, 0.05);
    assert!(retract_z.is_none(), "absent retract means hop-to-clearance");
    assert_eq!((*lead_deg, *lag_deg, *max_tilt_deg), (15.0, -5.0, 40.0));
    let e = model::find_by_name(&world, "RP1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::RotaryParallel { stepover, retract_z, .. } = &o.params else {
        panic!("rotary_parallel params expected, got {:?}", o.params);
    };
    assert_eq!(*stepover, Some(1.2));
    assert!(retract_z.is_some(), "the stated hop rides through");

    // Write-parse-write is a fixed point, and `stepover` appears
    // exactly once: the contour's DTO mints no such key.
    let p = tmp_path("v26b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the rotary shapes round-trip byte-identically");
    assert!(first.contains("\"rotary_contour\"") && first.contains("\"rotary_parallel\""), "{first}");
    assert_eq!(first.matches("\"stepover\"").count(), 1, "the contour kind must not grow a stepover key");

    // The sane door at the loader: a 120-degree lag is past the
    // quadrant the pose law can state off the radial - a typo about
    // the machine, refused by name.
    let bad = text.replace("\"lag_deg\": 0.0", "\"lag_deg\": 120.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v26 file with a 120-degree lag must not load"
    );
}

#[test]
fn v26_file_steps_to_v27_and_the_wrap_ladder_grammar_round_trips() {
    // M5.1 stage E2: the rotary rough arrives as one more params tag
    // (`rotary_offset`). Nothing existing changed shape - the axis
    // the ladder wraps is model data, and the kernel's own rapids
    // plane is a law, not a key - and step-up is the stamp itself,
    // like every bump since v6.
    let world = new_world();
    project::open(&world, r#"{"version": 26, "entities": []}"#).expect("v26 steps up");
    let p = tmp_path("v27a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v27 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v27 wire grammar, hand-written so the tags are pinned: the
    // pocket's numbers, radial now - and no `retract_z` key at all,
    // because a stored hop height could only ever be ignored. The
    // absent `entry` means the dive and the absent `max_engagement`
    // the law's 45 degrees, exactly as the pocket's file law reads.
    let text = r#"{
        "version": 27,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "RO1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"rotary_offset": {
                 "tolerance": 0.05, "stepover": 4.0, "stock_to_leave": 0.5,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 3.0,
                 "feed": 800.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "helix_radius": 2.0,
                 "lead_deg": 0.0, "lag_deg": 0.0, "max_tilt_deg": 90.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v27 wrap grammar must read");
    let e = model::find_by_name(&world, "RO1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::RotaryOffset {
        strategy,
        entry,
        optimal_load,
        stepdown,
        stock_to_leave,
        ..
    } = &o.params
    else {
        panic!("rotary_offset params expected, got {:?}", o.params);
    };
    assert_eq!(*strategy, PocketStrategy::Waterline);
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Plunge, "absent entry means the dive");
    assert_eq!(*optimal_load, camcraft_app::load::DEFAULT_OPTIMAL_LOAD, "absent engagement means the law's 45, as a load");
    assert_eq!((*stepdown, *stock_to_leave), (3.0, 0.5));

    // Write-parse-write is a fixed point, and the retract key appears
    // exactly nowhere: the field does not exist to forget.
    let p = tmp_path("v27b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the wrap shape round-trips byte-identically");
    assert!(first.contains("\"rotary_offset\""), "{first}");
    assert_eq!(first.matches("\"retract_z\"").count(), 0, "the kernel's hop plane is no file key");

    // The sane door at the loader: a zero spacing around the bar is a
    // typo no ladder can run - refused by name, never silently widened.
    let bad = text.replace("\"stepover\": 4.0", "\"stepover\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v27 file with a zero stepover must not load"
    );
}

#[test]
fn the_stage_c_machine_fields_round_trip_and_unset_ones_mint_nothing() {
    // The widened grammar rides the SAME DTO with no version bump:
    // fields a file stated round-trip intact, fields it left unstated
    // never appear in the save - a pre-stage-C file and a pre-C save
    // are byte-identical, which is what lets us skip the bump honestly.
    use camcraft_app::machine::Coolant;
    let world = new_world();
    let m = machine::Machine {
        model: "Wide 1".into(),
        vendor: "CamCraft Testbench".into(),
        kind: Some("mill-turn".into()),
        spindle_power: Some(18.5),
        tool_count: Some(40),
        coolants: vec![
            Coolant { name: "Flood".into(), m_code: "M8".into() },
            Coolant { name: "Air".into(), m_code: String::new() },
        ],
        axes: vec![
            machine::AxisLimits { name: "X".into(), ..Default::default() },
            machine::AxisLimits {
                name: "C".into(),
                rotary: true,
                travel: Some((-360.0, 360.0)),
                resolution: Some(0.001),
                ..Default::default()
            },
        ],
        ..Default::default()
    };
    let cmd = camcraft_app::commands::LoadMachine::new(&world, "wide.mch".into(), Some(m.clone()))
        .expect("sane");
    camcraft_app::commands::History::default().execute(&world, cmd);
    let p = tmp_path("stagec_fields");
    project::save(&world, &p).expect("save");
    let text = std::fs::read_to_string(&p).expect("read");
    assert!(text.contains("\"kind\"") && text.contains("mill-turn"), "{text}");
    assert!(text.contains("\"tool_count\"") && text.contains("40"));
    assert!(text.contains("\"rotary\""));
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    let (_, m2) = machine::of(&world2);
    assert_eq!(m2, Some(m), "every stage C number survives");

    // And the unstated machine still mints none of the new keys -
    // the v19 test above proves the old shape; this pins the skipers.
    let plain = tmp_path("stagec_mute");
    let mute = new_world();
    let cmd = camcraft_app::commands::LoadMachine::new(
        &mute,
        "mute.mch".into(),
        Some(machine::Machine::default()),
    )
    .expect("the mute file is a mute machine");
    camcraft_app::commands::History::default().execute(&mute, cmd);
    project::save(&mute, &plain).expect("save mute");
    let mute_text = std::fs::read_to_string(&plain).expect("read");
    for key in ["\"kind\"", "\"spindle_power\"", "\"tool_count\"", "\"coolants\"", "\"rotary\""] {
        assert!(!mute_text.contains(key), "unstated must not mint {key}: {mute_text}");
    }
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(&plain);
}

#[test]
fn v27_file_steps_to_v28_and_the_stamped_ladder_grammar_round_trips() {
    // M5.1 stage E3: the stamped ladder arrives as one more params tag
    // (`multi_axis_rough`) - the pocket's shape minus the `rest_rough`
    // and `feed_opt` switches, plus the pose trio. Nothing existing
    // changed shape; step-up is the stamp itself, like every bump
    // since v6. The hop plane returns here (unlike the wrap's): this
    // kernel runs the pocket's ladder, which reads it.
    let world = new_world();
    project::open(&world, r#"{"version": 27, "entities": []}"#).expect("v27 steps up");
    let p = tmp_path("v28a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v28 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v28 wire grammar, hand-written so the tags are pinned: an
    // explicit `retract_z` (a stated hop), an absent `max_engagement`
    // (the law's 45), `entry` stated Helix. The pose trio rides with
    // the pocket's numbers.
    let text = r#"{
        "version": 28,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "MR1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"multi_axis_rough": {
                 "tolerance": 0.05, "stepover": 4.0, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "retract_z": {"reference": "fixed", "offset": 12.0},
                 "top_z": {"reference": "fixed", "offset": 0.0},
                 "bottom_z": {"reference": "fixed", "offset": -10.0},
                 "stepdown": 5.0,
                 "feed": 800.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "zigzag", "entry": "helix", "helix_radius": 2.0,
                 "lead_deg": 3.0, "lag_deg": 2.0, "max_tilt_deg": 60.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v28 stamped grammar must read");
    let e = model::find_by_name(&world, "MR1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::MultiAxisRough {
        strategy,
        entry,
        optimal_load,
        retract_z,
        lead_deg,
        lag_deg,
        max_tilt_deg,
        ..
    } = &o.params
    else {
        panic!("multi_axis_rough params expected, got {:?}", o.params);
    };
    assert_eq!(*strategy, PocketStrategy::Zigzag);
    assert_eq!(*entry, camcraft_app::machining3d::Entry3D::Helix);
    assert_eq!(*optimal_load, camcraft_app::load::DEFAULT_OPTIMAL_LOAD, "absent engagement means the law's 45, as a load");
    assert!(retract_z.is_some(), "a stated hop must survive as a hop");
    assert_eq!((*lead_deg, *lag_deg, *max_tilt_deg), (3.0, 2.0, 60.0));

    // Write-parse-write is a fixed point, and the hop key appears
    // exactly once: stated, so it is written.
    let p = tmp_path("v28b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the stamped shape round-trips byte-identically");
    assert!(first.contains("\"multi_axis_rough\""), "{first}");
    assert_eq!(first.matches("\"retract_z\"").count(), 1, "the stated hop rides the file once");

    // The sane door at the loader: a 200-degree cone is past the
    // hemisphere the stamp may open into - a typo about the machine,
    // refused by name.
    let bad = text.replace("\"max_tilt_deg\": 60.0", "\"max_tilt_deg\": 200.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v28 file with a 200-degree cone must not load"
    );
}

#[test]
fn v28_file_steps_to_v29_and_shank_avoid_round_trips() {
    // M5.1 stage F: shank avoidance arrives as ONE new params key
    // (`shank_avoid`) inside the three axis-law kinds - a serde
    // default of false, which is exactly the pose law every v28 file
    // ran. Pure step-up, like every bump since v6: no v28 file means
    // anything else by omitting the key.
    let world = new_world();
    project::open(&world, r#"{"version": 28, "entities": []}"#).expect("v28 steps up");
    let p = tmp_path("v29a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v29 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v29 wire grammar, hand-written so the tags are pinned: the
    // rough states nothing new (absent means off), the contour says
    // the dodge is on. One tool wide in the shank so both reads mean
    // a real stack to measure.
    let text = r#"{
        "version": 29,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 8.0, "corner_r": 4.0,
                      "flute_len": 6.0, "shank_d": 10.0, "shank_len": 20.0,
                      "holder_d": 16.0, "holder_len": 10.0,
                      "material_group": "general", "rpm": 4000.0, "feed": 800.0}},
            {"name": "MR1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"multi_axis_rough": {
                 "tolerance": 0.05, "stepover": 4.0, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "fixed", "offset": 0.0},
                 "bottom_z": {"reference": "fixed", "offset": -10.0},
                 "stepdown": 5.0, "feed": 800.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "zigzag", "entry": "plunge", "helix_radius": 2.0,
                 "lead_deg": 0.0, "lag_deg": 0.0, "max_tilt_deg": 60.0}}}
            },
            {"name": "C5", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"contour5d": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 600.0, "plunge_feed": 120.0, "climb": true,
                 "lead_deg": 15.0, "lag_deg": -5.0, "max_tilt_deg": 40.0,
                 "shank_avoid": true}}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v29 axis-law grammar must read");
    let e = model::find_by_name(&world, "MR1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::MultiAxisRough { shank_avoid, .. } = &o.params else {
        panic!("multi_axis_rough params expected, got {:?}", o.params);
    };
    assert!(!*shank_avoid, "an absent key must mean the v28 pose law");
    let e = model::find_by_name(&world, "C5").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Contour5D { shank_avoid, .. } = &o.params else {
        panic!("contour5d params expected, got {:?}", o.params);
    };
    assert!(*shank_avoid, "a stated dodge must read back stated");

    // Write-parse-write is a fixed point, and the stated flag rides
    // the file (it is always written - an always-on dodge must not
    // degrade to a silent default on its way through disk).
    let p = tmp_path("v29b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the flagged shape round-trips byte-identically");
    assert_eq!(
        first.matches("\"shank_avoid\"").count(),
        2,
        "both axis-law kinds state the flag: {first}"
    );

    // The same grammar one version down: a v28 file whose contours
    // carry NO shank_avoid key still reads (the default), steps up,
    // and lands on the off pose law it always had.
    let v28 = text.replace("\"version\": 29", "\"version\": 28").replace(",\n                 \"shank_avoid\": true", "");
    let world3 = new_world();
    project::open(&world3, &v28).expect("a v28 file must step up");
    let e = model::find_by_name(&world3, "C5").expect("op");
    let o = model::operation_of(&world3, &e).expect("operation");
    let OpParams::Contour5D { shank_avoid, .. } = &o.params else {
        panic!("contour5d params expected");
    };
    assert!(!*shank_avoid, "the v28 contour ran with the dodge off - the step-up must agree");
}

#[test]
fn v29_file_steps_to_v30_and_a_sweep_warning_round_trips() {
    // M5.1 stage G: the entity grammar does not move at all - the wire
    // change is one new word in the cached warnings, `sweep_hit`. The
    // step is pure: no v29 file can contain a tag its build never
    // knew, and there is nothing to rewrite. The bump's job is the
    // OTHER direction - a v30 file's sweep verdict is a word an older
    // reader must refuse whole (the version gate) rather than fail
    // serde over inside an entity.
    let world = new_world();
    project::open(&world, r#"{"version": 29, "entities": []}"#).expect("v29 steps up");
    let p = tmp_path("v30a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v30 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v30 warning alphabet, hand-written so the tag is pinned: a
    // contour whose cached path carries the mid-turn verdict. The
    // moves are the v10 test's chain (rapid to the XY, plunge down);
    // the pose that swung lives in the WARNING, not the moves - law
    // (c3) judges the swing the generator recorded, and the cache
    // keeps its verdict.
    let text = r#"{
        "version": 30,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 6.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 1200.0}},
            {"name": "C1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"contour": {
                 "stock_to_leave": 0.0, "clearance_z": 10.0, "retract_z": 8.0,
                 "top_z": 5.0, "bottom_z": 3.0, "stepdown": 8.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "lead": "none", "tolerance": 0.01,
                 "tab_count": 0, "tab_width": 0.0, "tab_height": 0.0}},
                 "cache": {"moves": [
                     {"kind": "rapid", "from": [-50.0, -50.0, 10.0], "to": [0.0, 0.0, 10.0]},
                     {"kind": "plunge", "from": [0.0, 0.0, 10.0], "to": [0.0, 0.0, 3.0]}
                 ], "warnings": [{"sweep_hit": {"at": 1, "part": false}}]}}}
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v30 warning grammar must read");
    let e = model::find_by_name(&world, "C1").expect("op");
    let tp = model::toolpath_of(&world, &e).expect("cached");
    assert_eq!(
        tp.warnings,
        vec![camcraft_app::verify::Warning::SweepHit { at: 1, part: false }],
        "the mid-turn verdict rides the cache"
    );

    // Write-parse-write stays a fixed point, the new word and all.
    let p = tmp_path("v30b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    assert!(
        first.contains("\"sweep_hit\""),
        "a stated verdict is never dropped on its way through disk: {first}"
    );
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the sweep-flagged cache round-trips byte-identically");
}

#[test]
fn v30_file_steps_to_v31_and_the_flow_grammar_round_trips() {
    // M5.1 stage I: the flowline arrives as ONE new params tag
    // (`flow5d`) - the parallel twin's field minus the line choice,
    // whose place the surface's own direction takes. Nothing existing
    // changed shape; step-up is the stamp itself, like every bump
    // since v6. A v30 file can never carry the tag, and a v31 reader
    // of one loses nothing: every key it adds has a default that IS
    // the old document's behavior.
    let world = new_world();
    project::open(&world, r#"{"version": 30, "entities": []}"#).expect("v30 steps up");
    let p = tmp_path("v31a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v31 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v31 wire grammar, hand-written so the tag is pinned: FL1
    // states the spacing, the hop and the dodge; FL2 omits all three
    // and must land on the derived law - absent stepover means the
    // spacing the tolerance cos-law gives, absent retract means no
    // hop, absent shank_avoid means the pose law stage F left.
    let text = r#"{
        "version": 31,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "FL1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"flow5d": {
                 "tolerance": 0.05, "stepover": 4.0,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "retract_z": {"reference": "fixed", "offset": 12.0},
                 "top_z": {"reference": "fixed", "offset": 0.0},
                 "bottom_z": {"reference": "fixed", "offset": -10.0},
                 "feed": 800.0, "plunge_feed": 150.0, "climb": true,
                 "lead_deg": 3.0, "lag_deg": 2.0, "max_tilt_deg": 60.0,
                 "shank_avoid": true}}}
            },
            {"name": "FL2", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"flow5d": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "fixed", "offset": 0.0},
                 "bottom_z": {"reference": "fixed", "offset": -10.0},
                 "feed": 800.0, "plunge_feed": 150.0, "climb": true,
                 "lead_deg": 3.0, "lag_deg": 2.0, "max_tilt_deg": 60.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v31 flow grammar must read");
    let e = model::find_by_name(&world, "FL1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Flow5D {
        stepover,
        retract_z,
        shank_avoid,
        lead_deg,
        lag_deg,
        max_tilt_deg,
        ..
    } = &o.params
    else {
        panic!("flow5d params expected, got {:?}", o.params);
    };
    assert_eq!(*stepover, Some(4.0), "a stated spacing must survive as stated");
    assert!(retract_z.is_some(), "a stated hop must survive as a hop");
    assert!(*shank_avoid, "a stated dodge must read back stated");
    assert_eq!((*lead_deg, *lag_deg, *max_tilt_deg), (3.0, 2.0, 60.0));
    let e = model::find_by_name(&world, "FL2").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Flow5D { stepover, retract_z, shank_avoid, .. } = &o.params else {
        panic!("flow5d params expected, got {:?}", o.params);
    };
    assert_eq!(*stepover, None, "an absent key means the derived law");
    assert!(retract_z.is_none(), "an absent hop means the straight pull-up");
    assert!(!*shank_avoid, "an absent dodge means the stage-F pose law");

    // Write-parse-write is a fixed point, and the stated keys ride the
    // file once each while FL2's silence stays silent (except the
    // flag, always written like the twins').
    let p = tmp_path("v31b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the flow shape round-trips byte-identically");
    assert_eq!(first.matches("\"flow5d\"").count(), 2, "{first}");
    assert_eq!(first.matches("\"stepover\"").count(), 1, "only the stated spacing rides: {first}");
    assert_eq!(first.matches("\"retract_z\"").count(), 1, "only the stated hop rides: {first}");
    assert_eq!(first.matches("\"shank_avoid\"").count(), 2, "the flag is always stated: {first}");

    // The sane door at the loader: a 200-degree cone is past the
    // hemisphere the stamp may open into - a typo about the machine,
    // refused by name, the twins' law read by the third sibling.
    let bad = text.replace("\"max_tilt_deg\": 60.0,\n                 \"shank_avoid\": true", "\"max_tilt_deg\": 200.0,\n                 \"shank_avoid\": true");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v31 file with a 200-degree cone must not load"
    );
}

#[test]
fn v31_file_steps_to_v32_and_the_drive_curve_grammar_round_trips() {
    // M5.1 stage J: the drive-curve contour arrives as ONE new params
    // tag (`multi_axis_contour`) - the flow's field minus the spacing,
    // whose place the picked chains take; no pattern means no stepover
    // on the wire at all. Nothing existing changed shape; step-up is
    // the stamp itself, like every bump since v6. A v31 file can never
    // carry the tag, and a v32 reader of one loses nothing.
    let world = new_world();
    project::open(&world, r#"{"version": 31, "entities": []}"#).expect("v31 steps up");
    let p = tmp_path("v32a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v32 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v32 wire grammar, hand-written so the tag is pinned: MC1
    // states the hop and the dodge; MC2 omits both and must land on
    // the defaults - absent retract means the straight pull-up to
    // clearance between chains, absent shank_avoid the fan law's own
    // vertices, unposed.
    let text = r#"{
        "version": 32,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "MC1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"multi_axis_contour": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "retract_z": {"reference": "fixed", "offset": 12.0},
                 "top_z": {"reference": "fixed", "offset": 6.0},
                 "bottom_z": {"reference": "fixed", "offset": 4.0},
                 "feed": 500.0, "plunge_feed": 120.0, "climb": true,
                 "lead_deg": 0.0, "lag_deg": 0.0, "max_tilt_deg": 60.0,
                 "shank_avoid": true}}}
            },
            {"name": "MC2", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"multi_axis_contour": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "fixed", "offset": 6.0},
                 "bottom_z": {"reference": "fixed", "offset": 4.0},
                 "feed": 500.0, "plunge_feed": 120.0, "climb": true,
                 "lead_deg": 0.0, "lag_deg": 0.0, "max_tilt_deg": 60.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v32 drive-curve grammar must read");
    let e = model::find_by_name(&world, "MC1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::MultiAxisContour {
        retract_z,
        shank_avoid,
        lead_deg,
        lag_deg,
        max_tilt_deg,
        ..
    } = &o.params
    else {
        panic!("multi_axis_contour params expected, got {:?}", o.params);
    };
    assert!(retract_z.is_some(), "a stated hop must survive as a hop");
    assert!(*shank_avoid, "a stated dodge must read back stated");
    assert_eq!((*lead_deg, *lag_deg, *max_tilt_deg), (0.0, 0.0, 60.0));
    let e = model::find_by_name(&world, "MC2").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::MultiAxisContour { retract_z, shank_avoid, .. } = &o.params else {
        panic!("multi_axis_contour params expected, got {:?}", o.params);
    };
    assert!(retract_z.is_none(), "an absent hop means the straight pull-up");
    assert!(!*shank_avoid, "an absent dodge means the fan law's bare vertices");

    // Write-parse-write is a fixed point, and the stated keys ride the
    // file once each while MC2's silence stays silent (except the
    // flag, always written like the family's).
    let p = tmp_path("v32b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the drive-curve shape round-trips byte-identically");
    assert_eq!(first.matches("\"multi_axis_contour\"").count(), 2, "{first}");
    assert!(
        !first.contains("\"stepover\""),
        "the kind that machines picked curves authors no spacing: {first}"
    );
    assert_eq!(first.matches("\"retract_z\"").count(), 1, "only the stated hop rides: {first}");
    assert_eq!(first.matches("\"shank_avoid\"").count(), 2, "the flag is always stated: {first}");

    // The sane door at the loader: a 200-degree cone is past the
    // hemisphere the stamp may open into - a typo about the machine,
    // refused by name, the family's law read by the newest sibling.
    let bad = text.replace("\"max_tilt_deg\": 60.0,\n                 \"shank_avoid\": true", "\"max_tilt_deg\": 200.0,\n                 \"shank_avoid\": true");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &bad), Err(ProjectError::Corrupt(_))),
        "a v32 file with a 200-degree cone must not load"
    );
}

#[test]
fn v32_file_steps_to_v33_and_the_facing_grammar_round_trips() {
    // M5.2 stage A: the turning substrate arrives as one new params
    // tag (`facing`), one new tool-kind word (`turning`) and the
    // inert-defaulted `nose_radius` key - no existing shape changed,
    // so the step-up is the stamp itself. The law's edge is the
    // default: `nose_radius` absent reads 0.0, which every mill kind
    // already meant - but a TURNING tool with no nose is a point
    // that cannot face, and the sane door refuses the default for
    // that kind alone. A v32 file can never carry either tag; a v33
    // reader of one loses nothing.
    let world = new_world();
    project::open(&world, r#"{"version": 32, "entities": []}"#).expect("v32 steps up");
    let p = tmp_path("v33a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v33 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v33 wire grammar, hand-written so the tags are pinned: T0
    // is a v32-shaped ball (no nose key - it must read inert), T1 is
    // the insert with its circle stated, FC1 is the ladder.
    let text = r#"{
        "version": 33,
        "entities": [
            {"name": "T0", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "turning", "diameter": 16.0, "corner_r": 0.0,
                      "nose_radius": 0.8,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 3000.0, "feed": 150.0}},
            {"name": "FC1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"facing": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "fixed", "offset": 16.0},
                 "bottom_z": {"reference": "fixed", "offset": 14.0},
                 "stepdown": 0.5, "feed": 150.0, "plunge_feed": 80.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v33 facing grammar must read");
    let e = model::find_by_name(&world, "T0").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(
        t.nose_radius, 0.0,
        "the absent key lands on the inert default the mills always meant"
    );
    let e = model::find_by_name(&world, "T1").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(t.kind, camcraft_app::tools::ToolKind::Turning);
    assert_eq!(t.nose_radius, 0.8, "the insert's circle rides the file");
    let e = model::find_by_name(&world, "FC1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Facing {
        stepdown,
        feed,
        plunge_feed,
        tolerance,
        ..
    } = &o.params
    else {
        panic!("facing params expected, got {:?}", o.params);
    };
    assert_eq!((*stepdown, *feed, *plunge_feed), (0.5, 150.0, 80.0));
    assert_eq!(*tolerance, 0.05);

    // Write-parse-write is a fixed point, and both new tags are
    // stated wherever a turning number lives.
    let p = tmp_path("v33b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the facing shape round-trips byte-identically");
    assert_eq!(first.matches("\"facing\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"nose_radius\"").count(), 2, "every tool writes the circle - inert or not: {first}");

    // The sane door at the loader: a turning tool that omits the
    // nose reads the 0.0 default, and for THAT kind the default is a
    // point - refused, not silently mounted. A zero infeed is the
    // same lie on the operation's side.
    let point_insert = text.replace("\n                      \"nose_radius\": 0.8,", "");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &point_insert), Err(ProjectError::Corrupt(_))),
        "a turning tool with no stated nose must not load"
    );
    let no_infeed = text.replace("\"stepdown\": 0.5", "\"stepdown\": 0.0");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &no_infeed), Err(ProjectError::Corrupt(_))),
        "a facing ladder with no infeed is not a ladder"
    );
}

#[test]
fn v33_file_steps_to_v34_and_the_profile_grammar_round_trips() {
    // M5.2 stage B: the profile kinds arrive as two new params tags
    // (`profile_rough`, `profile_finish`) and nothing else - the
    // turning tool, its nose and the height DTOs all came with v33,
    // so the step-up is the stamp itself and a v33 file loses
    // nothing being read by a v34 reader.
    let world = new_world();
    project::open(&world, r#"{"version": 33, "entities": []}"#).expect("v33 steps up");
    let p = tmp_path("v34a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v34 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v34 wire grammar, hand-written so the tags are pinned:
    // PR1 is the rung ladder (the only kind that states a leave),
    // PF1 is one ride - the window and the feeds, nothing more.
    let text = r#"{
        "version": 34,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "turning", "diameter": 16.0, "corner_r": 0.0,
                      "nose_radius": 0.8,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 3000.0, "feed": 150.0}},
            {"name": "PR1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"profile_rough": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "fixed", "offset": 16.0},
                 "bottom_z": {"reference": "fixed", "offset": -16.0},
                 "stepdown": 0.8, "stock_to_leave": 0.25,
                 "feed": 150.0, "plunge_feed": 80.0}}}
            },
            {"name": "PF1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"profile_finish": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "fixed", "offset": 16.0},
                 "bottom_z": {"reference": "fixed", "offset": 2.0},
                 "feed": 120.0, "plunge_feed": 60.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v34 profile grammar must read");
    let e = model::find_by_name(&world, "PR1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::ProfileRough {
        stepdown,
        stock_to_leave,
        feed,
        plunge_feed,
        ..
    } = &o.params
    else {
        panic!("rough params expected, got {:?}", o.params);
    };
    assert_eq!((*stepdown, *stock_to_leave, *feed, *plunge_feed), (0.8, 0.25, 150.0, 80.0));
    let e = model::find_by_name(&world, "PF1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::ProfileFinish { feed, plunge_feed, .. } = &o.params else {
        panic!("finish params expected, got {:?}", o.params);
    };
    assert_eq!((*feed, *plunge_feed), (120.0, 60.0));

    // Write-parse-write is a fixed point, and the leave is stated
    // exactly once - the finish ride authors no wall to leave.
    let p = tmp_path("v34b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the profile shapes round-trip byte-identically");
    assert_eq!(first.matches("\"profile_rough\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"profile_finish\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"stock_to_leave\"").count(), 1, "only the ladder leaves stock: {first}");

    // The sane door at the loader, both kinds' edges: a rung ladder
    // with no radial infeed is one rung lying about being many, and
    // a NEGATIVE leave is stock to remove - the kernel's own words
    // at the door, not a surprise at Generate.
    let no_infeed = text.replace("\"stepdown\": 0.8", "\"stepdown\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &no_infeed), Err(ProjectError::Corrupt(_))),
        "a profile rough ladder with no infeed is not a ladder"
    );
    let took_stock = text.replace("\"stock_to_leave\": 0.25", "\"stock_to_leave\": -1.0");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &took_stock), Err(ProjectError::Corrupt(_))),
        "a negative leave must not load"
    );
}

#[test]
fn v34_file_steps_to_v35_and_the_blade_grammar_round_trips() {
    // M5.2 stage C: the blade kinds arrive as three new params tags
    // (`groove_rough`, `groove_finish`, `part`), one new tool-kind
    // word (`groove`) and the inert-defaulted `width` key - the
    // nose_radius law restated a third time: absent reads 0.0,
    // which every non-blade tool already meant, and for the Groove
    // kind alone the 0.0 is a blade that cuts no band and the sane
    // door refuses it. No v34 file can carry the new tags; a v35
    // reader of one loses nothing.
    let world = new_world();
    project::open(&world, r#"{"version": 34, "entities": []}"#).expect("v34 steps up");
    let p = tmp_path("v35a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v35 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v35 wire grammar, hand-written so the tags are pinned: T0
    // is the v34-shaped insert (no width key - it must read inert),
    // T1 is the blade with its kerf stated, and the three kinds
    // stand on the numbers the app suite generates with.
    let text = r#"{
        "version": 35,
        "entities": [
            {"name": "T0", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "turning", "diameter": 16.0, "corner_r": 0.0,
                      "nose_radius": 0.8,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 3000.0, "feed": 150.0}},
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "groove", "diameter": 16.0, "corner_r": 0.0,
                      "width": 2.0,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 2000.0, "feed": 150.0}},
            {"name": "GR1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"groove_rough": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 20.0},
                 "top_z": {"reference": "fixed", "offset": -3.6},
                 "bottom_z": {"reference": "fixed", "offset": -6.8},
                 "stepdown": 0.5, "stock_to_leave": 0.25,
                 "feed": 150.0, "plunge_feed": 60.0}}}
            },
            {"name": "GF1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"groove_finish": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 20.0},
                 "top_z": {"reference": "fixed", "offset": -3.6},
                 "bottom_z": {"reference": "fixed", "offset": -6.8},
                 "feed": 150.0, "plunge_feed": 60.0}}}
            },
            {"name": "PT1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"part": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 20.0},
                 "top_z": {"reference": "fixed", "offset": 16.0},
                 "bottom_z": {"reference": "fixed", "offset": -10.0},
                 "stepdown": 0.5, "feed": 150.0, "plunge_feed": 60.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v35 blade grammar must read");
    let e = model::find_by_name(&world, "T0").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(
        t.width, 0.0,
        "the absent width key lands on the inert default every non-blade tool always meant"
    );
    let e = model::find_by_name(&world, "T1").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(t.kind, camcraft_app::tools::ToolKind::Groove);
    assert_eq!(t.width, 2.0, "the kerf's width rides the file - the blade IS its tool row");
    let e = model::find_by_name(&world, "GR1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::GrooveRough {
        stepdown,
        stock_to_leave,
        feed,
        plunge_feed,
        ..
    } = &o.params
    else {
        panic!("rough params expected, got {:?}", o.params);
    };
    assert_eq!((*stepdown, *stock_to_leave, *feed, *plunge_feed), (0.5, 0.25, 150.0, 60.0));
    let e = model::find_by_name(&world, "GF1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::GrooveFinish { feed, plunge_feed, .. } = &o.params else {
        panic!("finish params expected, got {:?}", o.params);
    };
    assert_eq!((*feed, *plunge_feed), (150.0, 60.0));
    let e = model::find_by_name(&world, "PT1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Part { stepdown, .. } = &o.params else {
        panic!("part params expected, got {:?}", o.params);
    };
    assert_eq!(*stepdown, 0.5, "the parting ladder's infeed rides the file");

    // Write-parse-write is a fixed point, and the new keys are
    // stated wherever a blade number lives: the width on EVERY tool
    // row (inert or not, the nose_radius way), the three tags once
    // each, the leave only where a ladder leaves stock.
    let p = tmp_path("v35b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the blade shapes round-trip byte-identically");
    assert_eq!(first.matches("\"groove_rough\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"groove_finish\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"part\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"width\"").count(), 2, "every tool writes the band - inert or not: {first}");
    assert_eq!(first.matches("\"stock_to_leave\"").count(), 1, "only the rough ladder leaves stock: {first}");

    // The sane doors at the loader: a blade that states no width is
    // a point that cannot groove (the nose's law, refused for that
    // kind alone), and a parting plane with no infeed is one rung
    // lying about being a ladder.
    let dull_blade = text.replace("\n                      \"width\": 2.0,", "");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &dull_blade), Err(ProjectError::Corrupt(_))),
        "a groove tool with no stated width must not load"
    );
    let no_infeed = text.replace("\"stepdown\": 0.5, \"feed\"", "\"stepdown\": 0.0, \"feed\"");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &no_infeed), Err(ProjectError::Corrupt(_))),
        "a parting ladder with no infeed is not a ladder"
    );
}

#[test]
fn v35_file_steps_to_v36_and_the_thread_grammar_round_trips() {
    // M5.2 stage D: the threading kind arrives as one new params tag
    // (`threading`) and one new tool-kind word (`thread`), under the
    // arrival law restated a fifth time: no v35 file can carry them,
    // the V's `width` + `tip_angle` keys already ride every row
    // (inert-defaulted for everything else), and the modeled thread
    // is the mesh every kind already files. A v36 reader of a v35
    // file loses nothing.
    let world = new_world();
    project::open(&world, r#"{"version": 35, "entities": []}"#).expect("v35 steps up");
    let p = tmp_path("v36a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v36 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v36 wire grammar, hand-written so the tags are pinned: T0
    // is the round insert stating neither V key (both must read
    // inert), T1 is the threading insert with its profile stated,
    // and TH1 stands on the numbers the app suite generates with.
    let text = r#"{
        "version": 36,
        "entities": [
            {"name": "T0", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "turning", "diameter": 16.0, "corner_r": 0.0,
                      "nose_radius": 0.8,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 3000.0, "feed": 150.0}},
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "thread", "diameter": 16.0, "corner_r": 0.0,
                      "width": 1.732, "tip_angle": 60.0,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 800.0, "feed": 120.0}},
            {"name": "TH1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"threading": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 24.0},
                 "top_z": {"reference": "fixed", "offset": 14.3},
                 "bottom_z": {"reference": "fixed", "offset": 7.7},
                 "pitch": 2.0, "depth": 1.2, "infeed": 0.4,
                 "feed": 120.0, "plunge_feed": 40.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v36 thread grammar must read");
    let e = model::find_by_name(&world, "T0").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(
        (t.width, t.tip_angle),
        (0.0, 0.0),
        "the absent V keys land on the inert default every non-thread tool always meant"
    );
    let e = model::find_by_name(&world, "T1").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(t.kind, camcraft_app::tools::ToolKind::Thread);
    assert_eq!(
        (t.width, t.tip_angle),
        (1.732, 60.0),
        "the wave's profile rides the file - the thread IS its tool row"
    );
    let e = model::find_by_name(&world, "TH1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Threading {
        pitch,
        depth,
        infeed,
        feed,
        plunge_feed,
        ..
    } = &o.params
    else {
        panic!("threading params expected, got {:?}", o.params);
    };
    assert_eq!(
        (*pitch, *depth, *infeed, *feed, *plunge_feed),
        (2.0, 1.2, 0.4, 120.0, 40.0)
    );

    // Write-parse-write is a fixed point, and the new word is stated
    // exactly where the wave lives: the `threading` tag once, the
    // kind word once, the width on EVERY tool row (inert or not, the
    // nose_radius way restated a fourth time).
    let p = tmp_path("v36b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the thread shapes round-trip byte-identically");
    assert_eq!(first.matches("\"threading\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"thread\"").count(), 1, "the kind word rides the one V tool: {first}");
    assert_eq!(first.matches("\"width\"").count(), 2, "every tool writes the band - inert or not: {first}");

    // The sane doors at the loader: a threading insert that states
    // no width is a point that cannot carry a V (the blade law for
    // that kind alone), and a thread with no pitch is no helix.
    let dull_insert = text.replace("\n                      \"width\": 1.732,", "");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &dull_insert), Err(ProjectError::Corrupt(_))),
        "a threading insert with no stated width must not load"
    );
    let no_pitch = text.replace("\"pitch\": 2.0,", "\"pitch\": 0.0,");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &no_pitch), Err(ProjectError::Corrupt(_))),
        "a thread with no pitch is no helix"
    );

    // #629: the hand. A file from before the key (this one) reads
    // right-hand - the hand its drawing meant; it is then always
    // written, a left-hand thread round-trips, and a word that is no
    // hand refuses the file.
    let hand_of = |w: &flecs_ecs::prelude::World| {
        let e = model::find_by_name(w, "TH1").expect("op");
        match model::operation_of(w, &e).expect("operation").params {
            OpParams::Threading { hand, .. } => hand,
            other => panic!("threading expected, got {other:?}"),
        }
    };
    use camcraft_app::cl::ThreadHand;
    assert_eq!(hand_of(&world), ThreadHand::Right, "an old file's thread is right-hand");
    assert_eq!(first.matches("\"hand\": \"right\"").count(), 1, "the hand is stated: {first}");
    let left = text.replace("\"plunge_feed\": 40.0}", "\"plunge_feed\": 40.0, \"hand\": \"left\"}");
    assert_ne!(left, text);
    let world5 = new_world();
    project::open(&world5, &left).expect("a left-hand thread reads");
    assert_eq!(hand_of(&world5), ThreadHand::Left);
    let p = tmp_path("v36hand");
    project::save(&world5, &p).expect("save");
    let world6 = new_world();
    project::load(&world6, &p).expect("reload");
    let _ = std::fs::remove_file(&p);
    assert_eq!(hand_of(&world6), ThreadHand::Left, "left-hand survives the file");
    let sideways = text.replace("\"plunge_feed\": 40.0}", "\"plunge_feed\": 40.0, \"hand\": \"sideways\"}");
    assert!(project::open(&new_world(), &sideways).is_err(), "no such hand");
}

#[test]
fn v36_file_steps_to_v37_and_the_contour_grammar_round_trips() {
    // M5.2 stage E: the contour kinds arrive as three new params
    // tags (`chamfering`, `taper_turning`, `multi_pass_turning`)
    // and NOTHING ELSE - no new tool kind (the rides stand on the
    // turning insert's nose, a word v36 already files), no new key
    // on any other row. A v36 file cannot carry the tags; a v37
    // reader of one loses nothing. The step-up is the arrival law
    // restated a sixth time.
    let world = new_world();
    project::open(&world, r#"{"version": 36, "entities": []}"#).expect("v36 steps up");
    let p = tmp_path("v37a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v37 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v37 wire grammar, hand-written so the tags are pinned:
    // one turning insert (the nose is the whole tool story) and the
    // three contour operations on the numbers the app suite
    // generates with.
    let text = r#"{
        "version": 37,
        "entities": [
            {"name": "T0", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "turning", "diameter": 16.0, "corner_r": 0.0,
                      "nose_radius": 0.8,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 3000.0, "feed": 150.0}},
            {"name": "CH1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T0", "src": "stock", "params": {"chamfering": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 24.0},
                 "top_z": {"reference": "fixed", "offset": -14.0},
                 "bottom_z": {"reference": "fixed", "offset": -16.0},
                 "angle_deg": 45.0, "feed": 200.0, "plunge_feed": 50.0}}}
            },
            {"name": "TP1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T0", "src": "stock", "params": {"taper_turning": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 48.0},
                 "top_z": {"reference": "fixed", "offset": 40.0},
                 "bottom_z": {"reference": "fixed", "offset": -16.0},
                 "bottom_dia": 20.0, "top_dia": 16.0, "feed": 200.0, "plunge_feed": 50.0}}}
            },
            {"name": "MP1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T0", "src": "stock", "params": {"multi_pass_turning": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 30.0},
                 "top_z": {"reference": "fixed", "offset": 16.0},
                 "bottom_z": {"reference": "fixed", "offset": -16.0},
                 "stepdown": 0.5, "stock_to_leave": 0.25, "passes": 3,
                 "feed": 200.0, "plunge_feed": 50.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v37 contour grammar must read");
    let e = model::find_by_name(&world, "CH1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::Chamfering { angle_deg, feed, plunge_feed, .. } = &o.params else {
        panic!("chamfering params expected, got {:?}", o.params);
    };
    assert_eq!((*angle_deg, *feed, *plunge_feed), (45.0, 200.0, 50.0));
    let e = model::find_by_name(&world, "TP1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::TaperTurning { bottom_dia, top_dia, .. } = &o.params else {
        panic!("taper params expected, got {:?}", o.params);
    };
    assert_eq!((*bottom_dia, *top_dia), (20.0, 16.0));
    let e = model::find_by_name(&world, "MP1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::MultiPassTurning { stepdown, stock_to_leave, passes, .. } = &o.params else {
        panic!("multi-pass params expected, got {:?}", o.params);
    };
    assert_eq!((*stepdown, *stock_to_leave, *passes), (0.5, 0.25, 3));

    // Write-parse-write is a fixed point, and each new word is
    // stated exactly once - the ride's whole tool story is the
    // nose the T0 row already carries.
    let p = tmp_path("v37b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the contour shapes round-trip byte-identically");
    assert_eq!(first.matches("\"chamfering\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"taper_turning\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"multi_pass_turning\"").count(), 1, "{first}");

    // The sane doors at the loader: a cone closed at its TOP end
    // stands on the axis there - no steel reaches, the door keeps
    // the axis point for the bottom end only - and a ladder with
    // no finishing ride leaves the stock standing.
    let axis_point = text.replace("\"bottom_dia\": 20.0, \"top_dia\": 16.0", "\"bottom_dia\": 20.0, \"top_dia\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &axis_point), Err(ProjectError::Corrupt(_))),
        "a taper closed to the axis at its top end must not load"
    );
    let no_rides = text.replace("\"passes\": 3,", "\"passes\": 0,");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &no_rides), Err(ProjectError::Corrupt(_))),
        "a multi-pass program with no finishing ride is not a program"
    );
}

#[test]
fn v37_file_steps_to_v38_and_the_form_grammar_round_trips() {
    // M5.2 stage E2: the crown gauge arrives as ONE new tool-kind
    // word (`form`) and ONE new params tag (`form_turning`) - no
    // new keys: the band rides the width row every tool already
    // writes, the crown the nose_radius row every round tool
    // already writes. A v37 file cannot carry either new name; a
    // v38 reader of one loses nothing. The step-up is the arrival
    // law restated a seventh time.
    let world = new_world();
    project::open(&world, r#"{"version": 37, "entities": []}"#).expect("v37 steps up");
    let p = tmp_path("v38a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v38 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v38 wire grammar, hand-written so the names are pinned:
    // one crown gauge (the story is all TOOL steel - crown 6, band
    // 4) and the form op on the numbers the app suite generates
    // with (the gauge geometry, notably, is NOT among them).
    let text = r#"{
        "version": 38,
        "entities": [
            {"name": "T0", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "form", "diameter": 16.0, "corner_r": 0.0,
                      "nose_radius": 6.0, "width": 4.0,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 1500.0, "feed": 40.0}},
            {"name": "FM1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T0", "src": "stock", "params": {"form_turning": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 24.0},
                 "top_z": {"reference": "fixed", "offset": 13.0},
                 "bottom_z": {"reference": "fixed", "offset": 7.0},
                 "feed": 40.0, "plunge_feed": 20.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v38 form grammar must read");
    let e = model::find_by_name(&world, "T0").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(t.kind, camcraft_app::tools::ToolKind::Form);
    assert_eq!((t.nose_radius, t.width), (6.0, 4.0), "the gauge IS its tool row - crown and band");
    let e = model::find_by_name(&world, "FM1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::FormTurning { feed, plunge_feed, .. } = &o.params else {
        panic!("form turning params expected, got {:?}", o.params);
    };
    assert_eq!((*feed, *plunge_feed), (40.0, 20.0));

    // Write-parse-write is a fixed point, and each new name is
    // stated exactly once - the second is contained in the first,
    // so the WORDS count, not the substrings.
    let p = tmp_path("v38b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the gauge shapes round-trip byte-identically");
    assert_eq!(first.matches("\"form_turning\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"kind\": \"form\"").count(), 1, "the kind word rides the one gauge: {first}");

    // The sane doors at the loader: a gauge that states no band is
    // a crown of no width (the Form arm's is_sane), a band ground
    // past twice its own crown is no steel at the corners, and a
    // form op with no sink is a promise to touch nothing.
    let no_band = text.replace("\"nose_radius\": 6.0, \"width\": 4.0", "\"nose_radius\": 6.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &no_band), Err(ProjectError::Corrupt(_))),
        "a form gauge with no stated band must not load"
    );
    let over_wide = text.replace("\"width\": 4.0", "\"width\": 14.0");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &over_wide), Err(ProjectError::Corrupt(_))),
        "a 14-wide band on a crown of radius 6 has no corners"
    );
    let no_sink = text.replace("\"plunge_feed\": 20.0", "\"plunge_feed\": 0.0");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &no_sink), Err(ProjectError::Corrupt(_))),
        "a form op that never plunges is not an op"
    );
}

#[test]
fn v38_file_steps_to_v39_and_the_bore_grammar_round_trips() {
    // M5.2 stage F: the boring kind arrives as ONE new params tag
    // (`boring_turning`) and no new tool-kind word - `bore` has
    // filed since the drilling days, and its ground nose rides the
    // `nose_radius` key v33 files already carry. The hole the bar
    // rides is the mesh every kind already files, so there is
    // nothing else to name. The step-up is the arrival law restated
    // an eighth time.
    let world = new_world();
    project::open(&world, r#"{"version": 38, "entities": []}"#).expect("v38 steps up");
    let p = tmp_path("v39a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v39 stamp: {saved}"
    );
    let _ = std::fs::remove_file(&p);

    // The v39 wire grammar, hand-written so the names are pinned:
    // one boring bar (the ride is all TOOL steel - the ground nose
    // 0.4) and the bore op on the numbers the app suite generates
    // with (the hole's wall, notably, is NOT among them - it is
    // geometry, and geometry lives in the model).
    let text = r#"{
        "version": 39,
        "entities": [
            {"name": "T0", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "bore", "diameter": 10.0, "corner_r": 0.0,
                      "nose_radius": 0.4,
                      "flute_len": 20.0, "shank_d": 10.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 4000.0, "feed": 400.0}},
            {"name": "BR1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T0", "src": "stock", "params": {"boring_turning": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 60.0},
                 "top_z": {"reference": "fixed", "offset": 40.0},
                 "bottom_z": {"reference": "fixed", "offset": -10.0},
                 "feed": 120.0, "plunge_feed": 40.0}}}
            }
        ]
    }"#;
    let world = new_world();
    project::open(&world, text).expect("the v39 bore grammar must read");
    let e = model::find_by_name(&world, "T0").expect("tool");
    let t = model::tool_of(&world, &e).expect("definition");
    assert_eq!(t.kind, camcraft_app::tools::ToolKind::Bore);
    assert_eq!(t.nose_radius, 0.4, "the bar IS its tool row - the ground nose");
    let e = model::find_by_name(&world, "BR1").expect("op");
    let o = model::operation_of(&world, &e).expect("operation");
    let OpParams::BoringTurning { feed, plunge_feed, .. } = &o.params else {
        panic!("boring turning params expected, got {:?}", o.params);
    };
    assert_eq!((*feed, *plunge_feed), (120.0, 40.0));

    // Write-parse-write is a fixed point, and each new name is
    // stated exactly once - the OLD word (`bore`) counts too: the
    // arrival law is "one new tag, no new kind word", and this is
    // where that is pinned.
    let p = tmp_path("v39b");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the bar shapes round-trip byte-identically");
    assert_eq!(first.matches("\"boring_turning\"").count(), 1, "{first}");
    assert_eq!(first.matches("\"kind\": \"bore\"").count(), 1, "the kind word is the old one: {first}");

    // The sane doors at the loader: a bar that rides on a point is
    // the kernel's door, not the loader's (a drilling cycle's bore
    // files the same 0.0 and is honest steel), so the doors here
    // are the op's own numbers - no sink, no tolerance - and the
    // tag grammar: a name guessed from the label is no name.
    let no_sink = text.replace("\"plunge_feed\": 40.0", "\"plunge_feed\": 0.0");
    let world3 = new_world();
    assert!(
        matches!(project::open(&world3, &no_sink), Err(ProjectError::Corrupt(_))),
        "a bore op that never leads out is not an op"
    );
    let no_tol = text.replace("\"tolerance\": 0.05", "\"tolerance\": 0.0");
    let world4 = new_world();
    assert!(
        matches!(project::open(&world4, &no_tol), Err(ProjectError::Corrupt(_))),
        "a bore ride with no chord tolerance is no ride"
    );
    let guessed = text.replace("\"boring_turning\"", "\"bore_turning\"");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &guessed), Err(ProjectError::Corrupt(_))),
        "the tag is the variant's snake name, not a guess from the label"
    );
}

#[test]
fn v40_part_rows_step_up_silent_and_the_feeder_flags_round_trip() {
    // The G.2 fields grew in with serde defaults and NO version bump,
    // so a current file simply omits them - the same meaning the
    // keys were invented to carry: the feeder says nothing.
    // Built by concatenation, not format!: a JSON template needs its
    // own braces verbatim, and format!'s doubling is a trap worth
    // avoiding outright in prose-shaped strings.
    let silent = |keys: &str| {
        let mut t = String::from(r#"{"version": "#);
        t.push_str(&project::PROJECT_VERSION.to_string());
        t.push_str(
            r#", "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "groove", "diameter": 16.0, "corner_r": 0.0,
                      "width": 2.0,
                      "flute_len": 8.0, "shank_d": 16.0, "shank_len": 60.0,
                      "holder_d": 0.0, "holder_len": 0.0,
                      "material_group": "general", "rpm": 2000.0, "feed": 150.0}},
            {"name": "PT1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"part": {
                 "tolerance": 0.05,
                 "clearance_z": {"reference": "fixed", "offset": 20.0},
                 "top_z": {"reference": "fixed", "offset": 16.0},
                 "bottom_z": {"reference": "fixed", "offset": -10.0},
                 "stepdown": 0.5, "feed": 150.0, "plunge_feed": 60.0"#,
        );
        t.push_str(keys);
        t.push_str("}}}}\n        ]\n}");
        t
    };
    // M5.3 C grew one more key the same way - `bar_transfer` rides
    // the same serde default, so an older row that never heard of
    // the handoff reads silent, exactly as the G.2 row did. M5.3 D
    // grew the probe budget the same way again: absence is 0, the
    // silence that asks no measurement.
    let read = |world: &flecs_ecs::prelude::World| -> (bool, f64, bool, bool, f64) {
        let e = model::find_by_name(world, "PT1").expect("op");
        let o = model::operation_of(world, &e).expect("operation");
        let OpParams::Part { bar_load, bar_advance, bar_unload, bar_transfer, inspect_tol, .. } =
            &o.params
        else {
            panic!("part params expected, got {:?}", o.params);
        };
        (*bar_load, *bar_advance, *bar_unload, *bar_transfer, *inspect_tol)
    };

    let world = new_world();
    project::open(&world, &silent("")).expect("a feeder-less part row must read");
    assert_eq!(read(&world), (false, 0.0, false, false, 0.0), "absence means silence, the seed's own shape");
    // And the save states the learned keys - the fixed point the
    // feed_opt precedent keeps: a second open cannot drift them.
    let p = tmp_path("g2silent");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(saved.contains("\"bar_load\": false"), "{saved}");
    assert!(saved.contains("\"bar_transfer\": false"), "the handoff key rides too: {saved}");
    assert!(saved.contains("\"inspect_tol\": 0.0"), "and the probe budget: {saved}");
    let world2 = new_world();
    project::open(&world2, &saved).expect("reopened");
    assert_eq!(read(&world2), (false, 0.0, false, false, 0.0));

    // Stated keys ride through whole, mm and all.
    let world3 = new_world();
    project::open(
        &world3,
        &silent(", \"bar_load\": true, \"bar_advance\": 25.0, \"bar_unload\": true, \"bar_transfer\": true, \"inspect_tol\": 0.05"),
    )
    .expect("the feeder keys must read");
    assert_eq!(read(&world3), (true, 25.0, true, true, 0.05), "the signals are the file's, verbatim");
    let _ = std::fs::remove_file(&p);
}

#[test]
fn v45_file_steps_to_v46_and_the_ladder_grammar_round_trips() {
    // M2.5 A's pure step-up: the v45 finish3d row grows one optional
    // key - `steep_stepover` - whose absence decodes to the scallop
    // law, exactly the ladder every v45 file ran. No entity moved, so
    // a v45 file reads with the same answer it wrote, and the only
    // change is the stamp. The two new pattern words are the arrival
    // law's: a v45 file cannot contain them.
    let finish = |keys: &str| {
        let mut t = String::from(
            r#"{
        "version": 45,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true"#,
        );
        t.push_str(keys);
        t.push_str("}}}}\n        ]\n    }");
        t
    };
    let read = |world: &flecs_ecs::prelude::World| {
        let e = model::find_by_name(world, "F1").expect("op");
        let o = model::operation_of(world, &e).expect("operation");
        let OpParams::Finish3D { pattern, steep_stepover, .. } = &o.params else {
            panic!("finish3d params expected, got {:?}", o.params);
        };
        (*pattern, *steep_stepover)
    };

    let world = new_world();
    project::open(&world, &finish("")).expect("a ladder-less v45 file must migrate");
    let p = tmp_path("v46a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the rung's stamp: {saved}"
    );
    assert_eq!(
        read(&world),
        (camcraft_app::finishing3d::FinishPattern::Waterline, None),
        "absence is the waterline at the scallop's own spacing"
    );
    assert!(!saved.contains("steep_stepover"), "None is absent, not null: {saved}");

    // Stated, the pair round-trips: the word and its number, through
    // save, reload, resave - a fixed point like every grammar before it.
    let stated = finish(", \"pattern\": \"steep_shallow\", \"steep_stepover\": 2.5");
    let world2 = new_world();
    project::open(&world2, &stated).expect("the ladder grammar must read");
    assert_eq!(
        read(&world2),
        (camcraft_app::finishing3d::FinishPattern::SteepShallow, Some(2.5)),
        "the authored ladder is the file's, verbatim"
    );
    project::save(&world2, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    assert!(first.contains("\"steep_shallow\""), "the tag is the word: {first}");
    assert!(first.contains("\"steep_stepover\": 2.5"), "{first}");
    let world3 = new_world();
    project::load(&world3, &p).expect("reload");
    project::save(&world3, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    assert_eq!(first, second, "the ladder shape round-trips byte-identically");

    // The other new word, and the nonsense the loader refuses.
    let world4 = new_world();
    project::open(&world4, &finish(", \"pattern\": \"flat\""))
        .expect("the floors-only word must read");
    assert_eq!(read(&world4).0, camcraft_app::finishing3d::FinishPattern::Flat);
    let _ = std::fs::remove_file(&p);
    let bad = finish(", \"pattern\": \"steep_shallow\", \"steep_stepover\": 0.0");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &bad), Err(ProjectError::Corrupt(_))),
        "a ladder that does not advance is refused, not defaulted"
    );
}

#[test]
fn v46_file_steps_to_v47_and_the_crease_grammar_round_trips() {
    // M2.5 B's pure step-up: the v46 finish3d row grows one optional
    // key - `pencil_reference` - whose absence decodes to "no
    // reference ball named", exactly the answer every v46 file ran.
    // No entity moved, so a v46 file reads with the same answer it
    // wrote, and the only change is the stamp. The two new pattern
    // words are the arrival law's: a v46 file cannot contain them.
    let finish = |keys: &str| {
        let mut t = String::from(
            r#"{
        "version": 46,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true"#,
        );
        t.push_str(keys);
        t.push_str("}}}}\n        ]\n    }");
        t
    };
    let read = |world: &flecs_ecs::prelude::World| {
        let e = model::find_by_name(world, "F1").expect("op");
        let o = model::operation_of(world, &e).expect("operation");
        let OpParams::Finish3D { pattern, pencil_reference, .. } = &o.params else {
            panic!("finish3d params expected, got {:?}", o.params);
        };
        (*pattern, *pencil_reference)
    };

    let world = new_world();
    project::open(&world, &finish("")).expect("a reference-less v46 file must migrate");
    let p = tmp_path("v47a");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "the migration writes the v47 stamp: {saved}"
    );
    assert_eq!(
        read(&world),
        (camcraft_app::finishing3d::FinishPattern::Waterline, None),
        "absence is the waterline with every corner unhidden"
    );
    assert!(!saved.contains("pencil_reference"), "None is absent, not null: {saved}");

    // Stated, the pair round-trips: the word and its number, through
    // save, reload, resave - a fixed point like every grammar before it.
    let stated = finish(", \"pattern\": \"pencil\", \"pencil_reference\": 8.0");
    let world2 = new_world();
    project::open(&world2, &stated).expect("the v47 crease grammar must read");
    assert_eq!(
        read(&world2),
        (camcraft_app::finishing3d::FinishPattern::Pencil, Some(8.0)),
        "the authored pen is the file's, verbatim"
    );
    project::save(&world2, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    assert!(first.contains("\"pencil\""), "the tag is the word: {first}");
    assert!(first.contains("\"pencil_reference\": 8.0"), "{first}");
    let world3 = new_world();
    project::load(&world3, &p).expect("reload");
    project::save(&world3, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    assert_eq!(first, second, "the crease shape round-trips byte-identically");

    // The other new word, the mute key on Corner, and the nonsense
    // the loader refuses.
    let world4 = new_world();
    project::open(&world4, &finish(", \"pattern\": \"corner\", \"pencil_reference\": 99.0"))
        .expect("the two-face word must read");
    assert_eq!(
        read(&world4),
        (camcraft_app::finishing3d::FinishPattern::Corner, Some(99.0)),
        "the file may hold an inert pen; Corner reads it and ignores it"
    );
    let _ = std::fs::remove_file(&p);
    let bad = finish(", \"pattern\": \"pencil\", \"pencil_reference\": 0.0");
    let world5 = new_world();
    assert!(
        matches!(project::open(&world5, &bad), Err(ProjectError::Corrupt(_))),
        "a pen that cuts nothing is refused, not defaulted"
    );
}

#[test]
fn the_stage_c_pattern_words_arrive_as_words_not_a_version() {
    // M2.5 C: "ramp", "morph_spiral" and "geodesic" join the grammar
    // as words, not as a version - no key moved and no entity
    // changed, so a file that never spelled them reads exactly as it
    // did (the scallop's own arrival law). The twins round-trip
    // through the save/load/save fixed point, and an unknown spelling
    // is still refused, never defaulted.
    let finish = |keys: &str| {
        let mut t = String::from(
            r#"{
        "version": 47,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "F1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"finish3d": {
                 "tolerance": 0.1,
                 "clearance_z": {"reference": "fixed", "offset": 25.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "feed": 500.0, "plunge_feed": 100.0, "climb": true"#,
        );
        t.push_str(keys);
        t.push_str("}}}}\n        ]\n    }");
        t
    };
    for (word, want) in [
        ("ramp", camcraft_app::finishing3d::FinishPattern::Ramp),
        (
            "morph_spiral",
            camcraft_app::finishing3d::FinishPattern::MorphSpiral,
        ),
        (
            "geodesic",
            camcraft_app::finishing3d::FinishPattern::Geodesic,
        ),
        (
            "blend",
            camcraft_app::finishing3d::FinishPattern::Blend,
        ),
        (
            "morph",
            camcraft_app::finishing3d::FinishPattern::Morph,
        ),
        (
            "flow",
            camcraft_app::finishing3d::FinishPattern::Flow,
        ),
    ] {
        let t = finish(&format!(", \"pattern\": \"{word}\""));
        let w = new_world();
        project::open(&w, &t).expect("the stage C spellings must read");
        let e = model::find_by_name(&w, "F1").expect("op");
        let o = model::operation_of(&w, &e).expect("operation");
        let OpParams::Finish3D { pattern, .. } = &o.params else {
            panic!("finish3d params expected, got {:?}", o.params);
        };
        assert_eq!(*pattern, want, "the {word} word must decode to its variant");
        let p = tmp_path("stagec");
        project::save(&w, &p).expect("save");
        let first = std::fs::read_to_string(&p).expect("read");
        assert!(first.contains(&format!("\"pattern\": \"{word}\"")), "{word}: {first}");
        let w2 = new_world();
        project::load(&w2, &p).expect("reload");
        project::save(&w2, &p).expect("resave");
        let second = std::fs::read_to_string(&p).expect("reread");
        let _ = std::fs::remove_file(&p);
        assert_eq!(first, second, "the {word} shape round-trips byte-identically");
    }
    // The arrival law's other half: a file that never spelled the new
    // words reads untouched - the plain v47 body above with no
    // pattern key at all is the waterline, exactly as it always was.
    let w3 = new_world();
    project::open(&w3, &finish("")).expect("a wordless v47 file must read as before");
    let e = model::find_by_name(&w3, "F1").expect("op");
    let o = model::operation_of(&w3, &e).expect("operation");
    let OpParams::Finish3D { pattern, .. } = &o.params else {
        panic!("finish3d params expected");
    };
    assert_eq!(*pattern, camcraft_app::finishing3d::FinishPattern::Waterline);
}

#[test]
fn the_deburr_params_grammar_round_trips() {
    // M2.5 E B's arrival word: `deburr` is the variant's automatic
    // snake_case tag, and the six keys are the kind's whole content -
    // the one authored number is `width` (the tip's depth follows from
    // it and the TOOL's cone, the tool-free rule), and there is no
    // retract key to be absent because the kernel hops every run to
    // the clearance itself. A v47 file cannot contain this tag (the
    // arrival law), so the body is stamped at the build's own version.
    let body = |width: &str| {
        format!(
            r#"{{
        "version": {},
        "entities": [
            {{"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {{"kind": "taper", "diameter": 6.0, "corner_r": 0.0, "tip_angle": 90.0,
                      "flute_len": 10.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 10000.0, "feed": 2000.0}}}},
            {{"name": "D1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {{"tool": "T1", "src": "stock", "params": {{"deburr": {{
                 "width": {width},
                 "clearance_z": {{"reference": "fixed", "offset": 25.0}},
                 "top_z": {{"reference": "top_of_model", "offset": 0.0}},
                 "bottom_z": {{"reference": "bottom_of_model", "offset": 0.0}},
                 "feed": 500.0, "plunge_feed": 100.0
             }}}}}}}}
        ]
    }}"#,
            project::PROJECT_VERSION
        )
    };
    let read = |world: &flecs_ecs::prelude::World| {
        let e = model::find_by_name(world, "D1").expect("op");
        let o = model::operation_of(world, &e).expect("operation");
        let OpParams::Deburr {
            width,
            clearance_z,
            top_z,
            bottom_z,
            feed,
            plunge_feed,
        } = &o.params
        else {
            panic!("deburr params expected, got {:?}", o.params);
        };
        (*width, *clearance_z, *top_z, *bottom_z, *feed, *plunge_feed)
    };

    let world = new_world();
    project::open(&world, &body("0.5")).expect("the deburr word must read");
    assert_eq!(
        read(&world),
        (
            0.5,
            Height::fixed(25.0),
            Height::of(
                camcraft_app::operation::HeightReference::TopOfModel,
                0.0
            ),
            Height::of(
                camcraft_app::operation::HeightReference::BottomOfModel,
                0.0
            ),
            500.0,
            100.0
        ),
        "the six keys land as authored"
    );
    let p = tmp_path("deburr");
    project::save(&world, &p).expect("save");
    let first = std::fs::read_to_string(&p).expect("read");
    assert!(first.contains("\"deburr\""), "the tag is the word: {first}");
    assert!(first.contains("\"width\": 0.5"), "{first}");
    let world2 = new_world();
    project::load(&world2, &p).expect("reload");
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(first, second, "the deburr shape round-trips byte-identically");

    // A width that breaks nothing is no deburr: the door's law reads
    // the file too, and the loader refuses rather than defaulting.
    let world3 = new_world();
    assert!(
        matches!(
            project::open(&world3, &body("-1.0")),
            Err(ProjectError::Corrupt(_))
        ),
        "a negative break width is refused, not defaulted"
    );
}

#[test]
fn the_shell_key_round_trips_and_absent_means_the_ladder() {
    // M2.5 F B rides the wire like the slot's entry family (M8.2 C):
    // one appended #[serde(default)] key, no PROJECT_VERSION bump -
    // a file that never heard of shells has the key absent, and the
    // default (off) is exactly the plain ladder it generated. Stated,
    // the flag round-trips; the file writes it in both states, so
    // save-load-save is a fixed point.
    let body = |shell: &str| {
        // The v45 ladder test's idiom: a plain raw string (single
        // braces, the grammar pinned by hand) plus a token for the
        // version - the round-trip asserts must state the head, never
        // pin a literal that future bumps carry past.
        let mut t = String::from(
            r#"{
        "version": PROJECT_VERSION,
        "entities": [
            {"name": "T1", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {"kind": "ball_nose", "diameter": 6.0, "corner_r": 3.0,
                      "flute_len": 15.0, "shank_d": 8.0, "shank_len": 40.0,
                      "holder_d": 44.0, "holder_len": 35.0,
                      "material_group": "general", "rpm": 9000.0, "feed": 1500.0}},
            {"name": "P3", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {"tool": "T1", "src": "stock", "params": {"pocket3d": {
                 "stepover": 2.5, "stock_to_leave": 0.0,
                 "clearance_z": {"reference": "fixed", "offset": 10.0},
                 "top_z": {"reference": "top_of_model", "offset": 0.0},
                 "bottom_z": {"reference": "bottom_of_model", "offset": 0.0},
                 "stepdown": 1.5, "feed": 900.0, "plunge_feed": 150.0, "climb": true,
                 "strategy": "waterline", "entry": "plunge", "helix_radius": 1.0,
                 "rest_rough": false, "max_engagement": 40.0, "feed_opt": false"#,
        );
        t = t.replace(
            "PROJECT_VERSION",
            &project::PROJECT_VERSION.to_string(),
        );
        t.push_str(shell);
        t.push_str("}}}}\n        ]\n    }");
        t
    };
    let shell_of = |world: &flecs_ecs::prelude::World| {
        let e = model::find_by_name(world, "P3").expect("op");
        let o = model::operation_of(world, &e).expect("operation");
        let OpParams::Pocket3D { shell, feed_opt, rest_rough, .. } = &o.params else {
            panic!("pocket3d params expected, got {:?}", o.params);
        };
        (*shell, *feed_opt, *rest_rough)
    };

    // Stated on: the key reads, and the file keeps it.
    let world = new_world();
    project::open(&world, &body(", \"shell\": true")).expect("the shell key must read");
    assert_eq!(
        shell_of(&world),
        (true, false, false),
        "the stated shell lands, its neighbours untouched"
    );
    let p = tmp_path("shell");
    project::save(&world, &p).expect("save");
    let saved = std::fs::read_to_string(&p).expect("read");
    assert!(saved.contains("\"shell\": true"), "{saved}");

    // Absent means off: the same file with the key cut still opens,
    // and opens as the ladder every pre-shell file ran.
    // The strip is JSON surgery, not a string hunt: project::save
    // pretty-prints, so the key sits on its own line and a `, "shell"
    // ...` replace would miss it - the way an old file never wrote it.
    // get_mut, never index chains: mutable indexing UPGRADES a missing
    // key through Null into an object, which would graft a vestigial
    // `operation: {params: {pocket3d: null}}` onto the tool row - and
    // the loader, rightly, refuses that shape by name.
    let mut doc: serde_json::Value = serde_json::from_str(&saved).expect("the save is JSON");
    for ent in doc["entities"].as_array_mut().expect("entities") {
        if let Some(p3) = ent
            .get_mut("operation")
            .and_then(|o| o.get_mut("params"))
            .and_then(|p| p.get_mut("pocket3d"))
            .and_then(|c| c.as_object_mut())
        {
            p3.remove("shell");
        }
    }
    let stripped = serde_json::to_string(&doc).expect("reserialize");
    assert!(
        !stripped.contains("\"shell\""),
        "the strip must reach the key: {stripped}"
    );
    let world2 = new_world();
    project::open(&world2, &stripped).expect("a shell-less file must still read");
    assert!(!shell_of(&world2).0, "absence is the plain ladder");

    // Fixed point: off writes false, and false reads back off - a
    // second cycle cannot drift the answer.
    project::save(&world2, &p).expect("resave");
    let second = std::fs::read_to_string(&p).expect("reread");
    assert!(second.contains("\"shell\": false"), "{second}");
    let world3 = new_world();
    project::load(&world3, &p).expect("reload");
    project::save(&world3, &p).expect("resave");
    let third = std::fs::read_to_string(&p).expect("reread");
    let _ = std::fs::remove_file(&p);
    assert_eq!(second, third, "the shell shape round-trips byte-identically");
}

/// #630: the lathe tool orientation is additive - an unstated one is
/// never written (older files read byte for byte as before and take
/// the kind's default), a stated one round-trips, and an off-chart code
/// is refused at read.
#[test]
fn a_lathe_orientation_round_trips_and_old_tools_take_the_default() {
    let path = tmp_path("orient");
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, "TN", catalog_tool("TN-1600-0R8")).expect("insert"));
    let stated = Tool { orientation: Some(6), ..catalog_tool("TN-1600-0R8") };
    history.execute(&world, AddTool::new(&world, "TN6", stated).expect("stated insert"));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert_eq!(text.matches("\"orientation\"").count(), 1, "only the stated tool writes the key: {text}");

    let back = new_world();
    project::load(&back, &path).expect("load");
    let tool = |n: &str| model::find_by_name(&back, n).and_then(|e| model::tool_of(&back, &e)).expect("tool");
    assert_eq!(tool("TN").orientation, None, "an unstated orientation stays unstated");
    assert_eq!(tool("TN").lathe_orientation(), Some(2), "and reads as the insert's default");
    assert_eq!(tool("TN6").orientation, Some(6));
    project::save(&back, &path).expect("resave");
    assert_eq!(std::fs::read_to_string(&path).expect("reread"), text, "byte-identical round trip");

    let bad = text.replace("\"orientation\": 6", "\"orientation\": 12");
    assert_ne!(bad, text, "the fixture carries the key");
    let _ = std::fs::remove_file(&path);
    assert!(matches!(project::open(&new_world(), &bad), Err(ProjectError::Corrupt(_))), "orientation 12 is refused");
}

/// #641: a grooving blade's touched-off corner rides the same additive
/// key - an older blade (no key) reads as the left-corner default, a
/// stated corner round-trips byte for byte.
#[test]
fn a_blade_corner_round_trips_and_old_blades_take_the_left_corner() {
    let path = tmp_path("blade-corner");
    let world = new_world();
    let mut history = History::default();
    history.execute(&world, AddTool::new(&world, "GT", catalog_tool("GT-0300")).expect("blade"));
    let right = Tool { orientation: Some(1), ..catalog_tool("GT-0300") };
    history.execute(&world, AddTool::new(&world, "GT1", right).expect("right-corner blade"));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert_eq!(text.matches("\"orientation\"").count(), 1, "only the stated blade writes the key: {text}");

    let back = new_world();
    project::load(&back, &path).expect("load");
    let tool = |n: &str| model::find_by_name(&back, n).and_then(|e| model::tool_of(&back, &e)).expect("tool");
    assert_eq!(tool("GT").orientation, None, "an older blade states no corner");
    assert_eq!(tool("GT").lathe_orientation(), Some(2), "and reads as the left (-Z) corner");
    assert_eq!(tool("GT1").orientation, Some(1));
    project::save(&back, &path).expect("resave");
    let again = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(again, text, "byte-identical round trip");
}
