//! #258 / #2190: document/setup per-post `PostScriptOptions` persistence, undo/redo,
//! post-switch isolation, verification disagreement check, CLI `--post-option`,
//! MCP `get_post_options` / `set_post_options` integration, and named-post
//! `post` / `cam.post` handling.
//!
//! Provenance: our own fixtures over our own commands; no vendor material.
//!
//! Gate map (M4.4): covers app::posting, app::posting::mounts, app::setup_post_options,
//! app::script, app::mcp.

use std::collections::BTreeMap;
use std::path::PathBuf;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SelectPost, SetPostOptions,
    SetPostScriptOptions, SetSetupPostScriptOptions, SetStock,
};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::options::OptionValue;
use camcraft_app::schema::{BetweenSetups, PostOptions, PostRef, PostScriptOptions};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{cli, mcp::Server, posting, project, script, setup_post_options};

const TOOL: &str = "EM-0400-2FL";

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false, blend_radius: None,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        picks: Default::default(),
    }
}

fn add_setup_with_face(world: &World, h: &mut History, op_name: &str) {
    h.execute(world, CreateSetup::new(world, None, None).expect("setup"));
    let stock = SetStock::new(world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(world, stock.expect("stock"));
    let op = CreateOperation::new(world, op_name, TOOL, SrcRef::Stock, face());
    h.execute(world, op.expect("op"));
    h.execute(world, GenerateOperation::new(world, op_name).expect("gen"));
}

fn single_face_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    let op = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face());
    h.execute(&world, op.expect("op"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("gen"));
    (world, h)
}

fn tmp(tag: &str, ext: &str) -> PathBuf {
    std::env::temp_dir().join(format!("camcraft-258-{tag}-{}.{ext}", std::process::id()))
}

#[test]
fn document_and_setup_script_options_undo_redo_and_keep_per_post_memory() {
    let (world, mut h) = single_face_doc();
    let fanuc = PostRef::Builtin;
    let dnm = PostRef::Named("Doosan DNM".into());

    // Invalid option key and bad value refuse cleanly without touching history.
    let before_done = h.done_len();
    let mut bad = BTreeMap::new();
    bad.insert("noSuchOption".into(), OptionValue::Bool(true));
    let err = SetPostScriptOptions::for_post(&world, &fanuc, bad).expect_err("unknown refused");
    assert!(err.to_string().contains("noSuchOption"), "{err}");
    assert_eq!(h.done_len(), before_done);

    // Set Fanuc option.
    let mut fanuc_ov = BTreeMap::new();
    fanuc_ov.insert("showSequenceNumbers".into(), OptionValue::Str("every-block".into()));
    fanuc_ov.insert("sequenceNumberStart".into(), OptionValue::Int(50));
    h.execute(
        &world,
        SetPostScriptOptions::for_post(&world, &fanuc, fanuc_ov.clone()).expect("fanuc set"),
    );

    // Post writes N50 sequence numbers!
    let nc = tmp("seq", "nc");
    let posted = posting::post_setup(&world, &nc).expect("post succeeds");
    assert!(posted.program.contains("N50 "), "expected N50 sequence lines: {}", posted.program);

    // Switch to Doosan DNM, set Doosan smoothingLevel without disturbing Fanuc's saved choices.
    h.execute(&world, SelectPost::new(&world, dnm.clone()).expect("select dnm"));
    let mut dnm_ov = BTreeMap::new();
    dnm_ov.insert("smoothingLevel".into(), OptionValue::Int(8));
    h.execute(
        &world,
        SetPostScriptOptions::for_post(&world, &dnm, dnm_ov.clone()).expect("dnm set"),
    );
    h.execute(&world, SelectPost::new(&world, fanuc.clone()).expect("back to fanuc"));

    let key_fanuc = setup_post_options::post_key(&fanuc);
    let key_dnm = setup_post_options::post_key(&dnm);
    let doc_opts = setup_post_options::script_document(&world);
    assert_eq!(doc_opts.for_post(&key_fanuc).get("showSequenceNumbers"), fanuc_ov.get("showSequenceNumbers"));
    assert_eq!(doc_opts.for_post(&key_dnm).get("smoothingLevel"), dnm_ov.get("smoothingLevel"));

    // Undo and redo round-trip cleanly.
    h.undo(&world);
    h.undo(&world);
    let after_undo = setup_post_options::script_document(&world);
    assert!(!after_undo.for_post(&key_dnm).contains_key("smoothingLevel"));
    h.redo(&world);
    let after_redo = setup_post_options::script_document(&world);
    assert_eq!(after_redo.for_post(&key_dnm).get("smoothingLevel"), dnm_ov.get("smoothingLevel"));

    let _ = std::fs::remove_file(&nc);
}

#[test]
fn project_round_trips_script_options_and_flags_multi_setup_disagreement() {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("tool"));
    add_setup_with_face(&world, &mut h, "Face1");
    add_setup_with_face(&world, &mut h, "Face2");

    let fanuc = PostRef::Builtin;
    let mut doc_ov = BTreeMap::new();
    doc_ov.insert("writeToolList".into(), OptionValue::Bool(true));
    h.execute(
        &world,
        SetPostScriptOptions::for_post(&world, &fanuc, doc_ov).expect("doc set"),
    );

    let mut setup2_ov = BTreeMap::new();
    setup2_ov.insert("writeToolList".into(), OptionValue::Bool(false));
    setup2_ov.insert("showSequenceNumbers".into(), OptionValue::Str("every-block".into()));
    let mut next_setup2 = PostScriptOptions::default();
    next_setup2.set_for_post(&setup_post_options::post_key(&fanuc), setup2_ov);
    h.execute(
        &world,
        SetSetupPostScriptOptions::for_setup(&world, "Setup2", Some(next_setup2)).expect("setup2 set"),
    );

    // Setup1 still follows document (`None` own), Setup2 owns its overrides.
    assert!(setup_post_options::script_own(&world, Some("Setup1")).is_none());
    assert!(setup_post_options::script_own(&world, Some("Setup2")).is_some());

    // Combined single-file post must refuse by setup name because Setup1 and Setup2 disagree on Fanuc's script options.
    let problems = setup_post_options::program_problems(&world);
    assert!(!problems.is_empty(), "disagreeing script options must fail combined program check");

    // Project save/load preserves both document and Setup2 overrides.
    let text = project::save_text(&world).expect("save");
    let reloaded = new_world();
    project::open(&reloaded, &text).expect("open");
    assert_eq!(
        setup_post_options::script_document(&reloaded),
        setup_post_options::script_document(&world)
    );
    assert_eq!(
        setup_post_options::script_own(&reloaded, Some("Setup2")),
        setup_post_options::script_own(&world, Some("Setup2"))
    );

    // Switching to FilePerSetup resolves the multi-setup conflict and posts separate clean files.
    let cur = setup_post_options::document(&world);
    h.execute(
        &world,
        SetPostOptions::new(&world, PostOptions { between_setups: BetweenSetups::FilePerSetup, ..cur })
            .expect("file per setup"),
    );
    assert!(setup_post_options::program_problems(&world).is_empty());
}

#[test]
fn mcp_and_cli_expose_and_validate_declared_post_options() {
    let mut server = Server::new();
    let req = |s: &mut Server, id: i64, method: &str, params: Value| {
        s.handle(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .expect("reply")
    };
    req(&mut server, 1, "initialize", json!({ "protocolVersion": "2025-06-18" }));

    let got = req(&mut server, 2, "tools/call", json!({ "name": "get_post_options", "arguments": {} }));
    let sc = &got["result"]["structuredContent"];
    assert!(sc["script_schema"].is_array(), "missing script_schema: {sc}");
    assert!(sc["script_options"].is_object(), "missing script_options: {sc}");

    let set_ok = req(
        &mut server,
        3,
        "tools/call",
        json!({
            "name": "set_post_options",
            "arguments": { "script_options": { "showSequenceNumbers": "tool-change", "writeMachine": true } }
        }),
    );
    assert_ne!(set_ok["result"]["isError"], json!(true), "{set_ok}");
    assert_eq!(
        set_ok["result"]["structuredContent"]["script_options"]["showSequenceNumbers"],
        json!("tool-change")
    );
    assert_eq!(
        set_ok["result"]["structuredContent"]["script_options"]["writeMachine"],
        json!(true)
    );

    let set_bad = req(
        &mut server,
        4,
        "tools/call",
        json!({
            "name": "set_post_options",
            "arguments": { "script_options": { "noSuchOpt": 1 } }
        }),
    );
    assert_eq!(set_bad["result"]["isError"], json!(true));
    assert!(set_bad["result"]["content"][0]["text"].as_str().unwrap_or("").contains("noSuchOpt"));

    // CLI `process --post-option` check.
    let (world, _) = single_face_doc();
    let proj = tmp("cli-opt", "camproj");
    let out = tmp("cli-opt", "nc");
    project::save(&world, &proj).expect("save proj");

    let args: Vec<String> = [
        "process",
        &proj.display().to_string(),
        "--post-option",
        "showSequenceNumbers=every-block",
        "-o",
        &out.display().to_string(),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let code = cli::run(&args);
    assert_eq!(code, 0, "cli process with --post-option must exit 0");
    let posted = std::fs::read_to_string(&out).expect("nc output");
    assert!(posted.contains("N10 "), "expected N10 from CLI --post-option: {posted}");

    let bad_args: Vec<String> = [
        "process",
        &proj.display().to_string(),
        "--post-option",
        "nonexistentOpt=true",
        "-o",
        &out.display().to_string(),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let bad_code = cli::run(&bad_args);
    assert_ne!(bad_code, 0, "unknown --post-option must exit non-zero");

    let _ = std::fs::remove_file(&proj);
    let _ = std::fs::remove_file(&out);
}

fn mcp_req(server: &mut Server, id: i64, name: &str, arguments: Value) -> Value {
    server
        .handle(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": { "name": name, "arguments": arguments }
        }))
        .expect("mcp reply")
}

fn mcp_ok(server: &mut Server, id: &mut i64, name: &str, arguments: Value) -> Value {
    let cur = *id;
    *id += 1;
    let reply = mcp_req(server, cur, name, arguments);
    assert_ne!(reply["result"]["isError"], json!(true), "{name} failed: {reply}");
    reply["result"]["structuredContent"].clone()
}

fn setup_issue_2190_mcp_doc(server: &mut Server, id: &mut i64) {
    mcp_ok(server, id, "load_machine", json!({ "builtin": "VMC 3-axis" }));
    mcp_ok(server, id, "set_stock", json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }));
    mcp_ok(server, id, "add_tool", json!({ "catalog": "EM-1000-4FL" }));
    mcp_ok(
        server,
        id,
        "add_operation",
        json!({
            "kind": "face",
            "tool": "EM-1000-4FL",
            "name": "F1",
            "params": { "blend_turns": true }
        }),
    );
    mcp_ok(server, id, "generate", json!({}));
    mcp_ok(
        server,
        id,
        "set_post_options",
        json!({
            "script_options": {
                "showSequenceNumbers": "every-block",
                "writeToolList": true,
                "safeRetract": "G53",
                "useRadiusArcs": true
            }
        }),
    );
}

#[test]
fn mcp_post_with_post_name_applies_matching_stored_script_options() {
    let mut server = Server::new();
    let _ = server.handle(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" }
    }));
    let mut id = 2;
    setup_issue_2190_mcp_doc(&mut server, &mut id);

    let named_nc = tmp("2190-named", "nc");
    let doc_nc = tmp("2190-doc", "nc");

    let named_res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": named_nc.display().to_string(), "post": "Fanuc" }),
    );
    let doc_res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": doc_nc.display().to_string() }),
    );

    assert!(named_res.get("warning").is_none(), "unexpected warning: {named_res}");
    assert!(doc_res.get("warning").is_none(), "unexpected warning: {doc_res}");

    let named_text = std::fs::read_to_string(&named_nc).expect("read named.nc");
    let doc_text = std::fs::read_to_string(&doc_nc).expect("read doc.nc");
    assert_eq!(named_text, doc_text, "named Fanuc post must match document Fanuc post");
    assert!(named_text.contains("N10 "), "expected sequence numbers: {named_text}");
    assert!(named_text.contains("(T1 - "), "expected tool list: {named_text}");
    assert!(named_text.contains("G53 G00 Z0."), "expected G53 safe retract: {named_text}");
    assert!(!named_text.contains("G28"), "must not fall back to G28: {named_text}");
    let arc_lines: Vec<&str> = named_text
        .lines()
        .filter(|l| l.contains("G02 ") || l.contains("G03 "))
        .collect();
    assert!(!arc_lines.is_empty(), "expected arc lines with blend_turns: {named_text}");
    assert!(
        arc_lines.iter().all(|l| l.contains(" R") && !l.contains(" I") && !l.contains(" J")),
        "expected R arcs (useRadiusArcs: true), got: {arc_lines:?}"
    );

    let _ = std::fs::remove_file(&named_nc);
    let _ = std::fs::remove_file(&doc_nc);
}

#[test]
fn mcp_post_with_different_post_warns_about_unapplied_stored_script_options() {
    let mut server = Server::new();
    let _ = server.handle(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" }
    }));
    let mut id = 2;

    // Fresh doc with NO stored script_options: posting with another post emits no warning.
    mcp_ok(&mut server, &mut id, "load_machine", json!({ "builtin": "VMC 3-axis" }));
    mcp_ok(&mut server, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }));
    mcp_ok(&mut server, &mut id, "add_tool", json!({ "catalog": "EM-1000-4FL" }));
    mcp_ok(&mut server, &mut id, "add_operation", json!({ "kind": "face", "tool": "EM-1000-4FL", "name": "F1" }));
    mcp_ok(&mut server, &mut id, "generate", json!({}));
    let clean_haas = tmp("2190-clean-haas", "nc");
    let clean_res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": clean_haas.display().to_string(), "post": "Haas" }),
    );
    assert!(clean_res.get("warning").is_none(), "no stored options => no warning: {clean_res}");
    let _ = std::fs::remove_file(&clean_haas);

    // Now store Fanuc script_options, and post as Haas and as a custom .cps file: both warn clearly.
    mcp_ok(
        &mut server,
        &mut id,
        "set_post_options",
        json!({ "script_options": { "safeRetract": "G53", "useRadiusArcs": true } }),
    );
    let haas_nc = tmp("2190-haas-warn", "nc");
    let haas_res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": haas_nc.display().to_string(), "post": "Haas" }),
    );
    let warn = haas_res["warning"].as_str().unwrap_or("");
    assert!(
        warn.contains("not applied")
            && warn.contains("Fanuc")
            && warn.contains("safeRetract")
            && warn.contains("useRadiusArcs"),
        "expected warning naming unapplied post and keys, got: {haas_res}"
    );
    let _ = std::fs::remove_file(&haas_nc);

    let custom_cps = tmp("2190-custom", "cps");
    let fanuc_src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/posts/fanuc.cps"))
        .expect("read fanuc.cps");
    std::fs::write(&custom_cps, &fanuc_src).expect("write custom cps");
    let custom_nc = tmp("2190-custom-warn", "nc");
    let custom_res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": custom_nc.display().to_string(), "post": custom_cps.display().to_string() }),
    );
    let cwarn = custom_res["warning"].as_str().unwrap_or("");
    assert!(
        cwarn.contains("not applied")
            && cwarn.contains("Fanuc")
            && cwarn.contains("safeRetract")
            && cwarn.contains("useRadiusArcs"),
        "expected warning when posting via unattached .cps file, got: {custom_res}"
    );
    let _ = std::fs::remove_file(&custom_cps);
    let _ = std::fs::remove_file(&custom_nc);
}

#[test]
fn mcp_post_with_cps_file_path_matching_setup_applies_stored_script_options() {
    let custom_cps = tmp("2190-setup-file", "cps");
    let fanuc_src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/posts/fanuc.cps"))
        .expect("read fanuc.cps");
    std::fs::write(&custom_cps, &fanuc_src).expect("write custom cps");

    let mut server = Server::new();
    let _ = server.handle(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" }
    }));
    let mut id = 2;
    mcp_ok(&mut server, &mut id, "add_setup", json!({ "name": "Setup1" }));
    mcp_ok(&mut server, &mut id, "load_machine", json!({ "builtin": "VMC 3-axis" }));
    mcp_ok(&mut server, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }));
    mcp_ok(
        &mut server,
        &mut id,
        "set_setup_post",
        json!({ "setup": "Setup1", "post": custom_cps.display().to_string() }),
    );
    mcp_ok(&mut server, &mut id, "add_tool", json!({ "catalog": "EM-1000-4FL" }));
    mcp_ok(&mut server, &mut id, "add_operation", json!({ "kind": "face", "tool": "EM-1000-4FL", "name": "F1" }));
    mcp_ok(&mut server, &mut id, "generate", json!({}));
    mcp_ok(
        &mut server,
        &mut id,
        "set_post_options",
        json!({
            "setup": "Setup1",
            "script_options": { "safeRetract": "G53", "showSequenceNumbers": "every-block" }
        }),
    );

    let out_nc = tmp("2190-file-match", "nc");
    let res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": out_nc.display().to_string(), "post": custom_cps.display().to_string() }),
    );
    assert!(res.get("warning").is_none(), "matching .cps file must not warn: {res}");
    let text = std::fs::read_to_string(&out_nc).expect("read out_nc");
    assert!(text.contains("G53 G00 Z0.") && text.contains("N10 "), "{text}");

    // Same .cps file addressed through a non-identical path spelling (e.g. parent/./file.cps) also matches.
    let alt_path = custom_cps
        .parent()
        .expect("parent")
        .join(".")
        .join(custom_cps.file_name().expect("file name"));
    let alt_nc = tmp("2190-file-alt", "nc");
    let alt_res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": alt_nc.display().to_string(), "post": alt_path.display().to_string() }),
    );
    assert!(alt_res.get("warning").is_none(), "canonical .cps path must match without warning: {alt_res}");
    let alt_text = std::fs::read_to_string(&alt_nc).expect("read alt_nc");
    assert_eq!(alt_text, text);

    let _ = std::fs::remove_file(&custom_cps);
    let _ = std::fs::remove_file(&out_nc);
    let _ = std::fs::remove_file(&alt_nc);
}

#[test]
fn cam_post_and_cli_and_multi_setup_handle_named_post_script_options() {
    // Multi-setup with Setup1 on Fanuc (G53) and Setup2 on Haas (every-block).
    let mut server = Server::new();
    let _ = server.handle(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" }
    }));
    let mut id = 2;
    mcp_ok(&mut server, &mut id, "load_machine", json!({ "builtin": "VMC 3-axis" }));
    mcp_ok(&mut server, &mut id, "add_setup", json!({ "name": "Setup1" }));
    mcp_ok(&mut server, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }));
    mcp_ok(&mut server, &mut id, "add_tool", json!({ "catalog": "EM-1000-4FL" }));
    mcp_ok(&mut server, &mut id, "add_operation", json!({ "kind": "face", "tool": "EM-1000-4FL", "name": "F1" }));
    mcp_ok(&mut server, &mut id, "add_setup", json!({ "name": "Setup2" }));
    mcp_ok(&mut server, &mut id, "set_stock", json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }));
    mcp_ok(&mut server, &mut id, "set_setup_post", json!({ "setup": "Setup2", "post": "Haas" }));
    mcp_ok(&mut server, &mut id, "add_operation", json!({ "kind": "face", "tool": "EM-1000-4FL", "name": "F2" }));
    mcp_ok(&mut server, &mut id, "generate", json!({}));
    mcp_ok(
        &mut server,
        &mut id,
        "set_post_options",
        json!({ "setup": "Setup1", "script_options": { "safeRetract": "G53" } }),
    );
    mcp_ok(
        &mut server,
        &mut id,
        "set_post_options",
        json!({ "setup": "Setup2", "script_options": { "showSequenceNumbers": "every-block" } }),
    );

    let multi_nc = tmp("2190-multi", "nc");
    let multi_res = mcp_ok(
        &mut server,
        &mut id,
        "post",
        json!({ "output": multi_nc.display().to_string(), "post": "Fanuc" }),
    );
    let multi_text = std::fs::read_to_string(&multi_nc).expect("read multi.nc");
    assert!(multi_text.contains("G53 G00 Z0."), "Setup1 Fanuc G53 applies: {multi_text}");
    let mwarn = multi_res["warning"].as_str().unwrap_or("");
    assert!(
        mwarn.contains("Setup2") && mwarn.contains("Haas") && mwarn.contains("showSequenceNumbers"),
        "expected warning for Setup2 Haas options, got: {multi_res}"
    );
    let _ = std::fs::remove_file(&multi_nc);

    // Script cam.post + CLI process --cps Fanuc.
    let (world, mut h) = single_face_doc();
    let mut fanuc_ov = BTreeMap::new();
    fanuc_ov.insert("safeRetract".into(), OptionValue::Str("G53".into()));
    fanuc_ov.insert("showSequenceNumbers".into(), OptionValue::Str("every-block".into()));
    h.execute(
        &world,
        SetPostScriptOptions::for_post(&world, &PostRef::Builtin, fanuc_ov).expect("set"),
    );
    let proj = tmp("2190-cli", "camproj");
    let script_nc = tmp("2190-script", "nc");
    let haas_nc = tmp("2190-script-haas", "nc");
    let cli_nc = tmp("2190-cli", "nc");
    project::save(&world, &proj).expect("save");

    let out_match = script::run(&format!(
        "cam.load({}); cam.post({{ to: {}, cps: \"Fanuc\" }})",
        json!(proj.display().to_string()),
        json!(script_nc.display().to_string()),
    ));
    assert!(out_match.ok, "{:?}", out_match.errors);
    assert!(out_match.result.get("warning").is_none(), "{:?}", out_match.result);
    let script_text = std::fs::read_to_string(&script_nc).expect("script nc");
    assert!(script_text.contains("G53 G00 Z0.") && script_text.contains("N10 "), "{script_text}");

    let out_mismatch = script::run(&format!(
        "cam.load({}); const posted = cam.post({{ to: {}, cps: \"Haas\", program: true }}); ({{ ...posted, report: cam.report() }})",
        json!(proj.display().to_string()),
        json!(haas_nc.display().to_string()),
    ));
    assert!(out_mismatch.ok, "{:?}", out_mismatch.errors);
    // Conflict resolution must retain the shared header and first-file bookkeeping alongside the warning.
    let haas_text = std::fs::read_to_string(&haas_nc).expect("script Haas nc");
    assert_eq!(out_mismatch.result["path"], json!(haas_nc.display().to_string()));
    assert_eq!(out_mismatch.result["ops"], json!(1));
    assert_eq!(out_mismatch.result["lines"], json!(haas_text.lines().count()));
    assert_eq!(out_mismatch.result["program"], json!(haas_text));
    assert_eq!(out_mismatch.result["report"]["posted"], out_mismatch.result["path"]);
    assert!(
        out_mismatch.result["warning"].as_str().is_some_and(|w| w.contains("Fanuc") && w.contains("safeRetract")),
        "{:?}",
        out_mismatch.result
    );
    assert!(
        out_mismatch.logs.iter().any(|l| l.contains("Fanuc") && l.contains("safeRetract")),
        "expected warning in script logs for CLI visibility: {:?}",
        out_mismatch.logs
    );

    let args: Vec<String> = [
        "process",
        &proj.display().to_string(),
        "--cps",
        "Fanuc",
        "-o",
        &cli_nc.display().to_string(),
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(cli::run(&args), 0);
    let cli_text = std::fs::read_to_string(&cli_nc).expect("cli nc");
    assert!(cli_text.contains("G53 G00 Z0.") && cli_text.contains("N10 "), "{cli_text}");

    let _ = std::fs::remove_file(&proj);
    let _ = std::fs::remove_file(&script_nc);
    let _ = std::fs::remove_file(&haas_nc);
    let _ = std::fs::remove_file(&cli_nc);

    // Multi-mount (`file-per-setup`) with named `post: "Fanuc"` applies each setup's own Fanuc options.
    let mut fps_server = Server::new();
    let _ = fps_server.handle(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" }
    }));
    let mut fid = 2;
    mcp_ok(&mut fps_server, &mut fid, "load_machine", json!({ "builtin": "VMC 3-axis" }));
    mcp_ok(&mut fps_server, &mut fid, "add_setup", json!({ "name": "Setup1" }));
    mcp_ok(&mut fps_server, &mut fid, "set_stock", json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }));
    mcp_ok(&mut fps_server, &mut fid, "add_tool", json!({ "catalog": "EM-1000-4FL" }));
    mcp_ok(&mut fps_server, &mut fid, "add_operation", json!({ "kind": "face", "tool": "EM-1000-4FL", "name": "F1" }));
    mcp_ok(&mut fps_server, &mut fid, "add_setup", json!({ "name": "Setup2" }));
    mcp_ok(&mut fps_server, &mut fid, "set_stock", json!({ "box": { "dx": 60, "dy": 40, "dz": 10 } }));
    mcp_ok(&mut fps_server, &mut fid, "add_operation", json!({ "kind": "face", "tool": "EM-1000-4FL", "name": "F2" }));
    mcp_ok(&mut fps_server, &mut fid, "generate", json!({}));
    mcp_ok(
        &mut fps_server,
        &mut fid,
        "set_post_options",
        json!({ "setup": "Setup1", "script_options": { "writeToolList": true } }),
    );
    mcp_ok(
        &mut fps_server,
        &mut fid,
        "set_post_options",
        json!({ "setup": "Setup2", "script_options": { "safeRetract": "G53" } }),
    );
    let fps_out = tmp("2190-fps", "nc");
    let fps_res = mcp_ok(
        &mut fps_server,
        &mut fid,
        "post",
        json!({
            "output": fps_out.display().to_string(),
            "post": "Fanuc",
            "between_setups": "file-per-setup"
        }),
    );
    assert!(fps_res.get("warning").is_none(), "file-per-setup matching Fanuc must not warn: {fps_res}");
    let files = fps_res["files"].as_array().expect("files array");
    assert_eq!(files.len(), 2, "{fps_res}");
    let p1 = PathBuf::from(files[0].as_str().expect("p1"));
    let p2 = PathBuf::from(files[1].as_str().expect("p2"));
    let s1_text = std::fs::read_to_string(&p1).expect("read setup1");
    let s2_text = std::fs::read_to_string(&p2).expect("read setup2");
    assert!(s1_text.contains("(T1 - ") && !s1_text.contains("G53 G00 Z0."), "{s1_text}");
    assert!(!s2_text.contains("(T1 - ") && s2_text.contains("G53 G00 Z0."), "{s2_text}");
    let _ = std::fs::remove_file(&p1);
    let _ = std::fs::remove_file(&p2);
}
