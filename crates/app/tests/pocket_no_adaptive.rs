//! #1517: Adaptive clearing lives in the Adaptive operations only.
//! 2D Pocket no longer accepts or offers the Adaptive strategy; pre-v64
//! projects carrying Pocket + Adaptive migrate cleanly to 2D Adaptive
//! (Spiral), and any un-carriable parameter (`rest`, `predrilled`) is
//! turned off, noted on load, marked needs review, and blocks posting
//! until reviewed.
//!
//! Provenance: our own domain rules over public operation kinds and
//! persistence contract; no vendor code consulted.
//!
//! Gate map (#1517): covers app::operation (types::sane, generate),
//! app::commands (operation::create, operation::edit, operation::meta),
//! app::project (file, apply, entity, validate, pocket_adaptive_migrate),
//! app::posting, app::mcp, app::script.

mod common;

use common::gen_to_cltext;

use camcraft_app::commands::{
    AddTool, ChangeOperationKind, CreateOperation, EditOperationParams, GenerateOperation, History,
    MarkOperationReviewed, PasteOperation, SetStock,
};
use camcraft_app::links::Links;
use camcraft_app::machining2d::{AdaptiveMethod, PocketStrategy};
use camcraft_app::machining3d::Entry3D;
use camcraft_app::mcp::Server;
use camcraft_app::model;
use camcraft_app::operation::{self, Height, KindInfo, OpParams, OpType, SrcRef};
use camcraft_app::posting;
use camcraft_app::project::{self, ProjectError};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;

use serde_json::{Value, json};
use std::path::PathBuf;

fn catalog_tool(name: &str) -> Tool {
    catalog()
        .into_iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| t)
        .expect("catalog entry")
}

fn setup_world() -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = StockShape::Box { dx: 12.0, dy: 12.0, dz: 20.0 };
    history.execute(&world, SetStock::new(&world, stock, Location::default()).expect("box stock"));
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", catalog_tool("EM-0400-2FL")).expect("tool"));
    (world, history)
}

fn pocket_params(strategy: PocketStrategy) -> OpParams {
    OpParams::Pocket { pass_start: Default::default(),
        links: Links::PLANAR,
        stepover: 2.0,
        stock_to_leave: 0.0,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(18.0),
        stepdown: 5.0,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        strategy,
        entry: Entry3D::Helix,
        helix_radius: 1.5,
        optimal_load: 0.45,
        blend_turns: false,
        blend_radius: None,
        rest: None,
        tilt: None,
        corner_relief: Default::default(),
        picks: Default::default(),
    }
}

fn raw_operation(params: OpParams) -> camcraft_app::schema::Operation {
    camcraft_app::schema::Operation {
        tool: "EM-0400-2FL".into(),
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

fn v63_project_with_pocket_adaptive(extra: &str) -> String {
    let extra = if extra.is_empty() { String::new() } else { format!(", {extra}") };
    format!(
        r#"{{
        "version": 63,
        "entities": [
            {{"name": "Stock", "translation": [0.0, 0.0, 0.0], "kind": "stock",
             "stock": {{"box": {{"dx": 12.0, "dy": 12.0, "dz": 20.0}}}}}},
            {{"name": "EM-0400-2FL", "translation": [0.0, 0.0, 0.0], "kind": "tool",
             "tool": {{"kind": "end_mill", "diameter": 4.0, "corner_r": 0.0,
                      "flute_len": 12.0, "shank_d": 4.0, "shank_len": 35.0,
                      "holder_d": 32.0, "holder_len": 30.0,
                      "material_group": "general", "rpm": 12000.0, "feed": 800.0}}}},
            {{"name": "P1", "translation": [0.0, 0.0, 0.0], "kind": "operation",
             "operation": {{"tool": "EM-0400-2FL", "src": "stock", "params": {{"pocket": {{
                 "stepover": 2.0, "stock_to_leave": 0.0, "clearance_z": 25.0,
                 "top_z": 20.0, "bottom_z": 18.0, "stepdown": 5.0,
                 "feed": 800.0, "plunge_feed": 200.0, "climb": true,
                 "strategy": "adaptive", "entry": "helix", "helix_radius": 1.5,
                 "optimal_load": 0.45, "blend_turns": true, "blend_radius": 0.4{extra}
             }}}}}}}}
        ]
    }}"#
    )
}

fn mcp_call(s: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let reply = s
        .handle(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } }))
        .expect("mcp reply");
    let r = &reply["result"];
    let is_err = r["isError"] == json!(true);
    let body = if is_err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() };
    (is_err, body)
}

#[test]
fn pocket_adaptive_is_refused_at_every_door() {
    let bad = pocket_params(PocketStrategy::Adaptive);
    let why = bad.named_refusal().expect("Pocket + Adaptive must carry a named refusal");
    assert!(why.to_ascii_lowercase().contains("adaptive") && why.contains("2D Adaptive"), "{why}");
    assert!(!bad.is_sane(), "Pocket + Adaptive must fail is_sane");

    let (world, mut history) = setup_world();
    let err = CreateOperation::new(&world, "P1", "EM-0400-2FL", SrcRef::Stock, bad.clone())
        .err().expect("CreateOperation must refuse Pocket + Adaptive");
    assert!(err.to_string().contains("2D Adaptive"), "CreateOperation: {err}");

    let err = PasteOperation::new(&world, "P_copy", raw_operation(bad.clone()), Vec::new())
        .err().expect("PasteOperation must refuse Pocket + Adaptive");
    assert!(err.to_string().contains("2D Adaptive"), "PasteOperation: {err}");

    history.execute(
        &world,
        CreateOperation::new(&world, "P1", "EM-0400-2FL", SrcRef::Stock, pocket_params(PocketStrategy::Auto))
            .expect("valid Pocket creates"),
    );
    let err = EditOperationParams::new(&world, "P1", bad.clone())
        .err().expect("EditOperationParams must refuse Pocket + Adaptive");
    assert!(err.to_string().contains("2D Adaptive"), "EditOperationParams: {err}");

    // Direct generator call must also refuse rather than cut Pocket + Adaptive.
    let gen_err = operation::generate(&world, &raw_operation(bad), "")
        .expect_err("operation::generate must refuse Pocket + Adaptive");
    assert!(gen_err.to_string().contains("2D Adaptive"), "generate: {gen_err}");

    // MCP add_operation and edit_operation both refuse Pocket + Adaptive.
    let mut s = Server::new();
    assert!(!mcp_call(&mut s, 1, "set_stock", json!({ "box": { "dx": 12.0, "dy": 12.0, "dz": 20.0 } })).0);
    assert!(!mcp_call(&mut s, 2, "add_tool", json!({ "catalog": "EM-0400-2FL" })).0);
    let (err, body) = mcp_call(
        &mut s, 3, "add_operation",
        json!({ "kind": "pocket", "tool": "EM-0400-2FL", "name": "P_mcp", "params": { "strategy": "adaptive" } }),
    );
    assert!(err && body.as_str().is_some_and(|t| t.contains("2D Adaptive")), "MCP add_operation: {body}");

    assert!(!mcp_call(&mut s, 4, "add_operation", json!({ "kind": "pocket", "tool": "EM-0400-2FL", "name": "P_mcp" })).0);
    let (err, body) = mcp_call(
        &mut s, 5, "edit_operation",
        json!({ "name": "P_mcp", "params": { "strategy": "adaptive" } }),
    );
    assert!(err && body.as_str().is_some_and(|t| t.contains("2D Adaptive")), "MCP edit_operation: {body}");

    // Retyping Adaptive2D to Pocket lands on a legal Pocket strategy, never Adaptive.
    let adp_idx = KindInfo::ALL.iter().position(|k| k.ty == OpType::Adaptive2D).expect("Adaptive2D");
    let pkt_idx = KindInfo::ALL.iter().position(|k| k.ty == OpType::Pocket).expect("Pocket");
    history.execute(&world, ChangeOperationKind::new(&world, "P1", adp_idx).expect("retype to Adaptive2D"));
    history.execute(&world, ChangeOperationKind::new(&world, "P1", pkt_idx).expect("retype to Pocket"));
    let op = model::operation_of(&world, &model::find_by_name(&world, "P1").expect("P1")).expect("op");
    assert!(matches!(op.params, OpParams::Pocket { strategy: PocketStrategy::Auto, .. }));
}

#[test]
fn v63_pocket_adaptive_opens_as_adaptive2d_with_identical_toolpath() {
    let v63 = v63_project_with_pocket_adaptive("");
    let world = new_world();
    let notes = project::open_noted(&world, &v63).expect("v63 Pocket Adaptive must migrate on open");
    assert!(
        notes.iter().any(|n| n.contains("P1") && n.contains("2D Adaptive")),
        "migration notes: {notes:?}"
    );
    // Main already persisted tilt bases, RTCP, WCS flips, and drill
    // break-through semantics through v68.
    // Every pre-retirement version must still migrate the retired strategy.
    const { assert!(project::PROJECT_VERSION >= 69) };
    for version in 64..=68 {
        let legacy = v63.replace("\"version\": 63", &format!("\"version\": {version}"));
        let legacy_world = new_world();
        let legacy_notes = project::open_noted(&legacy_world, &legacy)
            .unwrap_or_else(|e| panic!("v{version} Pocket Adaptive must migrate: {e}"));
        assert!(
            legacy_notes.iter().any(|n| n.contains("P1") && n.contains("2D Adaptive")),
            "v{version} migration notes: {legacy_notes:?}"
        );
        assert!(matches!(
            model::operation_of(&legacy_world, &model::find_by_name(&legacy_world, "P1").expect("P1")).expect("op").params,
            OpParams::Adaptive2D { method: AdaptiveMethod::Spiral, .. }
        ));
    }
    let e = model::find_by_name(&world, "P1").expect("migrated op P1");
    let op = model::operation_of(&world, &e).expect("operation");
    let OpParams::Adaptive2D {
        optimal_load,
        method,
        entry,
        helix_radius,
        blend_turns,
        blend_radius,
        ..
    } = &op.params
    else {
        panic!("expected migrated Adaptive2D, got {:?}", op.params);
    };
    assert_eq!(*method, AdaptiveMethod::Spiral);
    assert_eq!(*optimal_load, 0.45);
    assert_eq!(*entry, Entry3D::Helix);
    assert_eq!(*helix_radius, 1.5);
    assert!(*blend_turns);
    assert_eq!(*blend_radius, Some(0.4));

    let mut history = History::default();
    history.execute(&world, GenerateOperation::new(&world, "P1").expect("generate migrated"));
    let migrated_cl = gen_to_cltext(&model::toolpath_of(&world, &e).expect("path").path);

    // Recorded by opening this unchanged v63 fixture with the unmigrated
    // Pocket+Adaptive generator on upstream/main e2ed62d0b (after #2004 / PR
    // #2013 dropped the air-cutting Top Z=20 level when stock_top <= top_z,
    // leaving only the Z=18 level). This expectation never reads migrated
    // params.
    assert_eq!(
        migrated_cl,
        include_str!("golden/pocket_adaptive_v63.cl"),
        "migration must preserve the independent legacy Pocket+Adaptive CL record"
    );

    let path: PathBuf =
        std::env::temp_dir().join(format!("camcraft_1517_clean_{}.camproj", std::process::id()));
    project::save(&world, &path).expect("save");
    let saved = std::fs::read_to_string(&path).expect("read saved");
    let _ = std::fs::remove_file(&path);
    assert!(
        saved.contains(&format!("\"version\": {}", project::PROJECT_VERSION)),
        "saved with current version: {saved}"
    );
    assert!(
        !saved.contains("\"pocket\""),
        "saved project must not retain retired pocket params: {saved}"
    );
}

#[test]
fn v63_uncarriable_params_mark_needs_review_and_block_posting() {
    let v63_rest = v63_project_with_pocket_adaptive(
        r#""rest": {"previous_tool_diameter": 10.0, "previous_corner_radius": 0.0, "previous_stock_to_leave": 0.0, "allowance": 0.0}"#,
    );
    let w_rest = new_world();
    let notes = project::open_noted(&w_rest, &v63_rest).expect("v63 with rest opens");
    assert!(
        notes
            .iter()
            .any(|n| n.contains("P1") && n.to_ascii_lowercase().contains("rest") && n.contains("review")),
        "rest load note: {notes:?}"
    );
    let e = model::find_by_name(&w_rest, "P1").expect("P1");
    assert!(
        matches!(model::operation_of(&w_rest, &e).expect("op").params, OpParams::Adaptive2D { .. }),
        "rest Pocket Adaptive still migrates to Adaptive2D"
    );

    let mut h = History::default();
    h.execute(&w_rest, GenerateOperation::new(&w_rest, "P1").expect("generate succeeds after rest is turned off"));
    let nc = std::env::temp_dir().join(format!("camcraft_1517_rest_{}.nc", std::process::id()));
    let post_err = posting::post_setup(&w_rest, &nc).expect_err("needs_review must block posting");
    assert!(
        post_err.contains("P1") && post_err.to_ascii_lowercase().contains("review"),
        "post error must cite P1 and review: {post_err}"
    );
    // Saving and reloading preserves needs_review and keeps blocking post until cleared.
    let saved_rest = std::env::temp_dir().join(format!("camcraft_1517_rest_{}.camproj", std::process::id()));
    project::save(&w_rest, &saved_rest).expect("save needs_review");
    let w_reloaded = new_world();
    project::load(&w_reloaded, &saved_rest).expect("reload needs_review");
    let _ = std::fs::remove_file(&saved_rest);
    assert!(
        !model::operation_meta_of(&w_reloaded, &model::find_by_name(&w_reloaded, "P1").expect("P1")).needs_review.is_empty(),
        "needs_review persists across save/load"
    );
    let post_err2 = posting::post_setup(&w_reloaded, &nc).expect_err("reloaded needs_review still blocks");
    assert!(post_err2.contains("P1") && post_err2.to_ascii_lowercase().contains("review"));
    h.execute(&w_rest, MarkOperationReviewed::new(&w_rest, "P1").expect("mark reviewed"));
    posting::post_setup(&w_rest, &nc).expect("reviewed op posts cleanly");
    h.undo(&w_rest);
    assert!(posting::post_setup(&w_rest, &nc).is_err(), "undo restores needs_review");
    let _ = std::fs::remove_file(&nc);

    let v63_predrilled = v63_project_with_pocket_adaptive(
        r#""links": {"approach": {"predrilled": true}}"#,
    );
    let proj_path = std::env::temp_dir().join(format!("camcraft_1517_pre_{}.camproj", std::process::id()));
    std::fs::write(&proj_path, &v63_predrilled).expect("write v63 predrilled");

    let mut s = Server::new();
    let (err, opened) = mcp_call(
        &mut s,
        1,
        "open_project",
        json!({ "path": proj_path.to_string_lossy() }),
    );
    let _ = std::fs::remove_file(&proj_path);
    assert!(!err, "open_project must succeed: {opened}");
    assert!(
        opened["notes"].to_string().to_ascii_lowercase().contains("pre-drilled"),
        "open_project reports migration notes: {opened}"
    );

    let (err, job) = mcp_call(&mut s, 2, "get_job", json!({}));
    assert!(!err, "get_job: {job}");
    let review_str = job["operations"][0]["needs_review"].to_string();
    assert!(
        review_str.to_ascii_lowercase().contains("pre-drilled")
            || review_str.to_ascii_lowercase().contains("predrilled"),
        "get_job must expose dropped pre-drilled review reason: {job}"
    );

    let (err, generated) = mcp_call(&mut s, 3, "generate", json!({}));
    assert!(!err, "generate: {generated}");
    let (err, ver) = mcp_call(&mut s, 4, "verify", json!({}));
    assert!(!err, "verify: {ver}");
    assert_eq!(
        ver["ok"],
        json!(false),
        "verify must refuse while P1 needs review: {ver}"
    );

    let nc_mcp = std::env::temp_dir().join(format!("camcraft_1517_mcp_{}.nc", std::process::id()));
    let (err, post_mcp) = mcp_call(&mut s, 5, "post", json!({ "output": nc_mcp.to_string_lossy() }));
    assert!(err, "MCP post refuses while P1 needs review: {post_mcp}");
    let (err, rev) = mcp_call(&mut s, 6, "mark_operation_reviewed", json!({ "name": "P1" }));
    assert!(!err, "mark_operation_reviewed: {rev}");
    let (err, rev2) = mcp_call(&mut s, 7, "mark_operation_reviewed", json!({ "name": "P1" }));
    assert!(err, "already reviewed refuses by name: {rev2}");
    let (err, ver2) = mcp_call(&mut s, 8, "verify", json!({}));
    assert!(!err && ver2["ok"] == json!(true), "verify ok after review: {ver2}");
    let (err, post_ok) = mcp_call(&mut s, 9, "post", json!({ "output": nc_mcp.to_string_lossy() }));
    let _ = std::fs::remove_file(&nc_mcp);
    assert!(!err, "MCP post succeeds after review: {post_ok}");
}

#[test]
fn current_version_file_with_pocket_adaptive_is_refused_as_corrupt() {
    let future = v63_project_with_pocket_adaptive("").replace(
        "\"version\": 63",
        &format!("\"version\": {}", project::NEWEST_PROJECT_VERSION + 1),
    );
    assert!(matches!(project::open(&new_world(), &future), Err(ProjectError::UnsupportedVersion(_))),
        "future project formats must be refused before migration");
    let v_cur = v63_project_with_pocket_adaptive("").replace(
        "\"version\": 63",
        &format!("\"version\": {}", project::PROJECT_VERSION),
    );
    let world = new_world();
    let res = project::open(&world, &v_cur);
    assert!(
        matches!(res, Err(ProjectError::Corrupt(_))),
        "current-version file carrying retired Pocket + Adaptive must be refused as corrupt, got: {res:?}"
    );
}
