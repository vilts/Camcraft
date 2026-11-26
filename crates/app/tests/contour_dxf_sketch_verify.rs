//! #1864: 2D Contour on DXF sketches - ray picking with occlusion,
//! placement/height/verify checks without a solid model, and MCP workflow.
//!
//! Gate map (M4.4): covers app::pick (feature_pick, sketch_pick, feature_sel),
//! app::mcp (features, pick), app::operation (contour_sketch, heights_sel, stamp),
//! app::sketch_model.
//! Provenance: our own fixtures over our own pick engine, verifier, and MCP
//! server; DXF geometry uses public Autodesk ASCII DXF group codes.

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{
    AddTool, Command, CommandError, CreateOperation, DxfOptions, EditOperationParams,
    GenerateOperation, History, ImportDxf, ImportModel, MoveSketch, SetStock, TagFixture,
};
use camcraft_app::geometry::sketch_chains::ChainId;
use camcraft_app::geometry::{Point3, cube};
use camcraft_app::machining2d::Lead;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{
    self, FeatureSel, GeoSel, Height, HeightReference, OpParams, OpStatus, OpType, SrcRef,
};
use camcraft_app::pick::{PickKinds, PickQuery, PickRay, PickTarget, Projection, pick_selection};
use camcraft_app::project;
use camcraft_app::schema::Location;
use camcraft_app::sketch;
use camcraft_app::sketch_model;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TOOL: &str = "EM-0400-2FL";

fn tmp_dir(tag: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-1864-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

fn plate_dxf() -> &'static str {
    "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n\
     0\nLWPOLYLINE\n8\ncut\n90\n4\n70\n1\n10\n0\n20\n0\n10\n120\n20\n0\n10\n120\n20\n80\n10\n0\n20\n80\n\
     0\nCIRCLE\n8\nholes\n10\n25\n20\n55\n40\n8\n\
     0\nCIRCLE\n8\nholes\n10\n95\n20\n55\n40\n6\n\
     0\nLINE\n8\nholes\n10\n45\n20\n20\n11\n75\n21\n20\n\
     0\nARC\n8\nholes\n10\n75\n20\n25\n40\n5\n50\n270\n51\n90\n\
     0\nLINE\n8\nholes\n10\n75\n20\n30\n11\n45\n21\n30\n\
     0\nARC\n8\nholes\n10\n45\n20\n25\n40\n5\n50\n90\n51\n270\n\
     0\nLINE\n8\nopen\n10\n20\n20\n10\n11\n100\n21\n10\n\
     0\nENDSEC\n0\nEOF\n"
}

fn write_dxf(dir: &Path, name: &str, text: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, text).expect("write dxf");
    path
}

fn run(world: &World, history: &mut History, cmd: Result<impl Command + 'static, CommandError>, what: &str) {
    history.execute(world, cmd.unwrap_or_else(|e| panic!("{what}: {e}")));
}

fn sheet_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Box { dx: 300.0, dy: 240.0, dz: 10.0 };
    run(&world, &mut history, SetStock::new(&world, shape, Location::default()), "sheet stock");
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("catalog tool");
    run(&world, &mut history, AddTool::new(&world, TOOL, tool), "tool");
    run(&world, &mut history, ImportDxf::from_bytes(&world, None, "plate.dxf", plate_dxf().as_bytes(), &DxfOptions::default()), "import dxf");
    (world, history)
}

fn stock_contour_params() -> OpParams {
    let mut params = operation::seed_params(OpType::Contour.ordinal());
    if let OpParams::Contour {
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        stepdown,
        lead,
        tolerance,
        ..
    } = &mut params
    {
        *clearance_z = Height::of(HeightReference::TopOfStock, 10.0);
        *retract_z = Height::of(HeightReference::TopOfStock, 5.0);
        *top_z = Height::of(HeightReference::TopOfStock, 0.0);
        *bottom_z = Height::of(HeightReference::TopOfStock, -2.0);
        *stepdown = 2.0;
        *lead = Lead::None;
        *tolerance = 0.005;
    }
    params
}

struct PlateIds {
    big_circle: ChainId,
    slot: ChainId,
    open_line: ChainId,
}

fn plate_ids(world: &World) -> PlateIds {
    let (_, sk) = sketch::find(world, "plate").expect("plate sketch");
    let mut big_circle = None;
    let mut slot = None;
    for ch in sk.data.chains().by_layer("holes") {
        let len = ch.length();
        if (len - std::f64::consts::TAU * 8.0).abs() < 0.1 {
            big_circle = Some(ch.id());
        } else if (len - std::f64::consts::TAU * 6.0).abs() >= 0.1 {
            slot = Some(ch.id());
        }
    }
    let open = sk.data.chains().by_layer("open");
    PlateIds {
        big_circle: big_circle.expect("R8 circle"),
        slot: slot.expect("stadium slot"),
        open_line: open[0].id(),
    }
}

fn mcp_call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("mcp reply");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let body = if is_err {
        result["content"][0]["text"].clone()
    } else {
        result["structuredContent"].clone()
    };
    (is_err, body)
}

#[test]
fn ray_pick_hits_sketch_line_arc_circle_and_respects_solid_occlusion() {
    let (world, mut history) = sheet_world();
    let ids = plate_ids(&world);
    let skip_none = |_: &str| false;

    let q_line = PickQuery {
        ray: PickRay { origin: [60.0, 10.2, 50.0], dir: [0.0, 0.0, -1.0], projection: Projection::Ortho },
        radius: 1.0,
        radius_per_depth: 0.0,
        kinds: PickKinds::EDGES,
        through: false,
    };
    let (p, sel) = pick_selection(&world, &q_line, camcraft_app::pick::EdgeUnit::Run, skip_none).expect("line pick");
    assert_eq!(p.entity, "plate");
    assert_eq!(p.target, PickTarget::SketchChain { id: ids.open_line });
    assert_eq!(sel, Some(FeatureSel::SketchChain(ids.open_line)));

    let target = [33.0, 55.0, 10.0];
    let eye = [33.0, 25.0, 50.0];
    let q_circle = PickQuery {
        ray: PickRay {
            origin: eye,
            dir: [target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]],
            projection: Projection::Perspective,
        },
        radius: 1.5,
        radius_per_depth: 0.02,
        kinds: PickKinds::EDGES,
        through: false,
    };
    let (p, _) = pick_selection(&world, &q_circle, camcraft_app::pick::EdgeUnit::Run, skip_none).expect("circle pick");
    assert_eq!(p.target, PickTarget::SketchChain { id: ids.big_circle });

    let q_arc = PickQuery {
        ray: PickRay { origin: [79.8, 25.0, 40.0], dir: [0.0, 0.0, -1.0], projection: Projection::Ortho },
        radius: 1.0,
        radius_per_depth: 0.0,
        kinds: PickKinds::EDGES,
        through: false,
    };
    let (p, _) = pick_selection(&world, &q_arc, camcraft_app::pick::EdgeUnit::Run, skip_none).expect("slot arc pick");
    assert_eq!(p.target, PickTarget::SketchChain { id: ids.slot });

    let q_miss = PickQuery {
        ray: PickRay { origin: [-50.0, -50.0, 40.0], dir: [0.0, 0.0, -1.0], projection: Projection::Ortho },
        radius: 1.0,
        radius_per_depth: 0.0,
        kinds: PickKinds::EDGES,
        through: false,
    };
    assert!(pick_selection(&world, &q_miss, camcraft_app::pick::EdgeUnit::Run, skip_none).is_none());

    let cover = cube(Point3::new(50.0, 0.0, 12.0), Point3::new(70.0, 20.0, 18.0));
    run(&world, &mut history, Ok::<_, CommandError>(ImportModel::from_mesh("cover", cover)), "cover");
    let blocked = pick_selection(&world, &q_line, camcraft_app::pick::EdgeUnit::Run, skip_none);
    assert!(blocked.is_none_or(|(pk, _)| pk.target != PickTarget::SketchChain { id: ids.open_line }), "solid must occlude sketch curve");
    let mut q_through = q_line;
    q_through.through = true;
    let (pk, _) = pick_selection(&world, &q_through, camcraft_app::pick::EdgeUnit::Run, skip_none).expect("through picks curve");
    assert_eq!(pk.target, PickTarget::SketchChain { id: ids.open_line });
}

#[test]
fn move_sketch_stock_shift_unresolved_model_height_and_verify_checks() {
    let (world, mut history) = sheet_world();
    let ids = plate_ids(&world);

    let mut p_model = stock_contour_params();
    if let OpParams::Contour { top_z, .. } = &mut p_model {
        *top_z = Height::of(HeightReference::TopOfModel, 0.0);
    }
    run(&world, &mut history, CreateOperation::new(&world, "ModelH", TOOL, SrcRef::Entity("plate".into()), p_model), "create model height op");
    let err = GenerateOperation::new(&world, "ModelH").err().expect("TopOfModel must refuse on sketch");
    assert!(err.to_string().contains(sketch_model::NO_SOLID_HEIGHT), "{err}");

    let mut p_sel = stock_contour_params();
    if let OpParams::Contour { top_z, bottom_z, .. } = &mut p_sel {
        *top_z = Height::of(HeightReference::TopOfSelection, 0.0);
        *bottom_z = Height::of(HeightReference::TopOfSelection, -2.0);
    }
    run(
        &world,
        &mut history,
        CreateOperation::new(
            &world,
            "CircleCut",
            TOOL,
            SrcRef::Selection(vec![GeoSel { entity: "plate".into(), feature: FeatureSel::SketchChain(ids.big_circle) }]),
            p_sel.clone(),
        ),
        "create TopOfSelection contour",
    );
    run(&world, &mut history, GenerateOperation::new(&world, "CircleCut"), "gen circle");
    let op_e = model::find_by_name(&world, "CircleCut").unwrap();
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok);

    run(&world, &mut history, MoveSketch::new(&world, "plate", 15.0, -5.0, 0.0), "move sketch");
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Stale, "MoveSketch must invalidate digest");
    assert!(history.undo(&world));
    assert_eq!(operation::status_of(&world, &op_e), OpStatus::Ok, "undo MoveSketch restores fresh");
    assert!(history.redo(&world));
    run(&world, &mut history, GenerateOperation::new(&world, "CircleCut"), "regen moved circle");
    let c_moved = model::toolpath_of(&world, &op_e).unwrap();
    assert!(c_moved.path.moves.iter().filter_map(|m| m.arc).any(|a| {
        (a.center.x - (25.0 + 15.0)).abs() < 0.02 && (a.center.y - (55.0 - 5.0)).abs() < 0.02
    }), "circle center should shift by MoveSketch delta");

    let clamp = cube(Point3::new(34.0, 44.0, 8.0), Point3::new(46.0, 56.0, 15.0));
    run(&world, &mut history, Ok::<_, CommandError>(ImportModel::from_mesh("clamp", clamp)), "import clamp");
    run(&world, &mut history, TagFixture::new(&world, "clamp", None), "tag fixture");
    run(&world, &mut history, GenerateOperation::new(&world, "CircleCut"), "regen against fixture");
    let with_fixture = model::toolpath_of(&world, &op_e).unwrap();
    assert!(!with_fixture.warnings.is_empty(), "fixture in cut path must trigger verify warning");
    assert!(history.undo(&world));

    let mut p_low_rapid = p_sel;
    if let OpParams::Contour {
        clearance_z,
        retract_z,
        top_z,
        bottom_z,
        ..
    } = &mut p_low_rapid
    {
        *clearance_z = Height::of(HeightReference::TopOfStock, -1.0);
        *retract_z = Height::of(HeightReference::TopOfStock, -2.0);
        *top_z = Height::of(HeightReference::TopOfStock, -4.0);
        *bottom_z = Height::of(HeightReference::TopOfStock, -6.0);
    }
    run(&world, &mut history, EditOperationParams::new(&world, "CircleCut", p_low_rapid), "low rapid edit");
    run(&world, &mut history, GenerateOperation::new(&world, "CircleCut"), "gen low rapid");
    let low_cache = model::toolpath_of(&world, &op_e).unwrap();
    assert!(!low_cache.warnings.is_empty(), "low rapid below stock top must trigger stock-rapid warning");
}

#[test]
fn mcp_sketch_workflow_picks_chains_layers_sets_overrides_and_generates() {
    let dir = tmp_dir("mcp-1864");
    let dxf_path = write_dxf(&dir, "plate.dxf", plate_dxf());

    let mut s = Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    s.handle(&init).expect("init");

    let (err, r) = mcp_call(&mut s, 1, "set_stock", json!({ "box": { "dx": 300.0, "dy": 240.0, "dz": 10.0 } }));
    assert!(!err, "set_stock failed: {r}");
    let (err, r) = mcp_call(&mut s, 2, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    assert!(!err, "add_tool failed: {r}");
    let (err, r) = mcp_call(&mut s, 3, "import_dxf", json!({ "path": dxf_path.to_string_lossy() }));
    assert!(!err, "import_dxf failed: {r}");

    let (err, feat) = mcp_call(&mut s, 4, "list_features", json!({ "model": "plate" }));
    assert!(!err, "list_features failed: {feat}");
    assert_eq!(feat["kind"], json!("sketch"));
    let chains = feat["chains"].as_array().expect("chains array");
    let layers = feat["layers"].as_array().expect("layers array");
    assert_eq!(chains.len(), 5, "outer + 2 circles + slot + open line: {feat}");
    assert_eq!(layers.len(), 3, "cut, holes, open: {feat}");

    let circle_id = chains
        .iter()
        .find(|c| c["layer"] == json!("holes") && c["closed"] == json!(true))
        .and_then(|c| c["id"].as_str())
        .expect("sketch_chain id")
        .to_owned();
    assert!(circle_id.starts_with("sketch_chain:"));

    let holes_layer_id = layers
        .iter()
        .find(|l| l["layer"] == json!("holes"))
        .and_then(|l| l["id"].as_str())
        .expect("sketch_layer id")
        .to_owned();
    assert_eq!(holes_layer_id, "sketch_layer:holes");

    let (err, r) = mcp_call(&mut s, 5, "add_operation", json!({
        "kind": "2D Contour",
        "tool": "EM-0400-2FL",
        "source": "plate",
        "name": "Cut",
        "params": &project::params_to_json(&stock_contour_params())["contour"]
    }));
    assert!(!err, "add_operation failed: {r}");

    let (err, geom_res) = mcp_call(&mut s, 6, "set_operation_geometry", json!({
        "operation": "Cut",
        "picks": [
            { "model": "plate", "id": holes_layer_id },
            { "model": "plate", "id": circle_id, "side": "outside", "reverse": true }
        ]
    }));
    assert!(!err, "set_operation_geometry failed: {geom_res}");
    assert_eq!(geom_res["chain_overrides"].as_array().map(Vec::len), Some(1));

    let (err, gen_res) = mcp_call(&mut s, 7, "generate", json!({ "name": "Cut" }));
    assert!(!err, "MCP generate failed: {gen_res}");

    let (err, view) = mcp_call(&mut s, 8, "get_operation_geometry", json!({ "operation": "Cut" }));
    assert!(!err, "get_operation_geometry failed: {view}");
    assert_eq!(view["chain_overrides"][0]["side"], json!("outside"));
    assert_eq!(view["chain_overrides"][0]["reverse"], json!(true));
}
