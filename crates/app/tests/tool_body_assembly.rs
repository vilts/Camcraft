//! Issue #361 / #1040: modular tool bodies assembled from several
//! vendor models (clamping unit, adapter, cutting head). Pieces mate by
//! ISO 13399 coordinate systems (MCS <-> CSW) or explicit placement,
//! combine into one judged/drawn ToolBody, survive .camproj and
//! .camtools roundtrips, and remove in one undoable step.
//!
//! Gate map (M4.4): covers app::tool_body_assembly, commands::tool_body,
//! app::mcp::body_tools, project::tool_body_wire, app::tool_library.
//! Lane shape (M4.4): all tests here run in seconds.
//!
//! Provenance: fixtures `modular_lathe_head.step` and
//! `modular_clamp_unit.step` are hand-written Part 21 boxes we created;
//! no vendor file consulted.

mod common;

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{
    CreateOperation, GenerateOperation, History, ImportModel, ImportTools,
    LoadMachine, SetToolBody,
};
use camcraft_app::geometry::import::step;
use camcraft_app::mcp::Server;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::project;
use camcraft_app::schema::PostRef;
use camcraft_app::tool_body::{self, BodyFrame};
use camcraft_app::tool_body_assembly::PiecePlace;
use camcraft_app::verify::Warning;
use camcraft_app::{model, posting, tool_body_view, tool_library};
use camcraft_tools::lathe_holder::LathePlacement;

use common::bore::*;

const INSERT: &str = "TN-1600-0R8";
const MILL: &str = "EM-0600-4FL";

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn od_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    add_tool(&world, &mut history, INSERT);
    // Stepped part: r 30 shoulder below z 0, r 6 neck above to z 20.
    let part = revolve(&[(0.0, -16.0), (30.0, -16.0), (30.0, 0.0), (6.0, 0.0), (6.0, 20.0), (0.0, 20.0)]);
    history.execute(&world, ImportModel::from_mesh("step", part));
    (world, history)
}

fn finish() -> OpParams {
    OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    }
}

fn regen(world: &World, history: &mut History, name: &str) {
    history.execute(world, GenerateOperation::new(world, name).expect("regenerate"));
}

fn holder_hits(warns: &[Warning]) -> Vec<(usize, bool, f64)> {
    warns
        .iter()
        .filter_map(|w| match *w {
            Warning::HolderHit { at, part, short_mm } => Some((at, part, short_mm)),
            _ => None,
        })
        .collect()
}

struct McpClient {
    server: Server,
    next: i64,
}

impl McpClient {
    fn new() -> Self {
        let mut c = Self { server: Server::new(), next: 1 };
        c.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
        c
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
        self.server.handle(&msg).expect("a request gets a response")
    }

    fn call(&mut self, name: &str, args: Value) -> (bool, Value) {
        let reply = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = &reply["result"];
        assert!(result.is_object(), "tools/call answers with a result: {reply}");
        let is_err = result["isError"] == json!(true);
        let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
        (is_err, body)
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let (err, body) = self.call(name, args);
        assert!(!err, "{name} refused: {body}");
        body
    }

    fn refused(&mut self, name: &str, args: Value) -> String {
        let (err, body) = self.call(name, args);
        assert!(err, "{name} should have refused, got {body}");
        body.as_str().unwrap_or_default().to_owned()
    }
}

#[test]
fn lathe_head_and_clamping_unit_mate_by_iso13399_csys_and_judge_together() {
    let (world, mut history) = od_world();
    let head = testdata("modular_lathe_head.step");
    let clamp = testdata("modular_clamp_unit.step");
    history.execute(&world, CreateOperation::new(&world, "F1", INSERT, SrcRef::Entity("step".into()), finish()).expect("create"));
    posting::post_set(&world, &PostRef::Named("Fanuc Lathe".into()));

    // Piece 0 alone: head spans x 2..18, z 1..12 in turning frame -> clear of r 30 shoulder at z <= 0.
    let place = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };
    history.execute(&world, SetToolBody::lathe_from_file(&world, INSERT, &head, place).expect("head lands"));
    regen(&world, &mut history, "F1");
    let head_only = cache(&world, "F1").expect("cached");
    assert!(holder_hits(&head_only.warnings).is_empty(), "head alone stands clear: {:?}", head_only.warnings);

    // Attach clamping unit via unambiguous auto-mate (head MCS <-> clamp CSW, with 90-deg rotation).
    history.execute(
        &world,
        SetToolBody::attach_piece(&world, INSERT, &clamp, PiecePlace::auto_csys()).expect("clamp mates onto head"),
    );
    let e = model::find_by_name(&world, INSERT).expect("tool");
    let body = tool_body::body_of(&world, &e).expect("assembled body");
    assert_eq!(body.pieces.len(), 2);
    assert_eq!(body.source, "modular_lathe_head.step + modular_clamp_unit.step");

    // Combined body now reaches z -8..15, r 24..54 at tip z=2 -> strikes the r 30 shoulder!
    regen(&world, &mut history, "F1");
    let assembled = cache(&world, "F1").expect("cached");
    assert!(!holder_hits(&assembled.warnings).is_empty(), "clamping unit behind the head must hit the shoulder");
    let out = std::env::temp_dir().join(format!("camcraft-361-asm-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_err(), "holder hit from attached clamping unit must block post");

    // Removing the clamping unit is one undoable step and restores clearance.
    history.execute(&world, SetToolBody::remove_piece(&world, INSERT, 1).expect("remove clamp"));
    let after_rm = tool_body::body_of(&world, &e).expect("head remains");
    assert_eq!(after_rm.pieces.len(), 1);
    regen(&world, &mut history, "F1");
    assert!(holder_hits(&cache(&world, "F1").expect("cached").warnings).is_empty());

    // Undo restores the 2-piece assembly (past regen + remove_piece) and the collision.
    history.undo(&world);
    history.undo(&world);
    assert_eq!(tool_body::body_of(&world, &e).expect("restored").pieces.len(), 2);
    regen(&world, &mut history, "F1");
    assert!(!holder_hits(&cache(&world, "F1").expect("cached").warnings).is_empty());
}

#[test]
fn three_piece_spindle_assembly_persists_in_camproj_and_camtools_and_draws_all_pieces() {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, MILL);

    let head = testdata("tool_csys_box_iso13399.step");
    let unit = testdata("modular_clamp_unit.step");

    // Piece 0: milling head placed by CIP (z 35..75, x/y -10..10, r = 14.142).
    history.execute(&world, SetToolBody::from_step_auto(&world, MILL, &head).expect("head lands"));
    // Piece 1: adapter mated onto head's MCS using CSW (z 35..65, x -8.5..14.5, y -12..12 -> r = 18.821).
    history.execute(
        &world,
        SetToolBody::attach_piece(&world, MILL, &unit, PiecePlace::csys(Some(0), "MCS", "CSW")).expect("adapter mates"),
    );
    // Piece 2: clamping unit mated onto adapter's MCS using CSW (z 65..95).
    history.execute(
        &world,
        SetToolBody::attach_piece(&world, MILL, &unit, PiecePlace::csys(Some(1), "MCS", "CSW")).expect("clamp mates"),
    );

    let e = model::find_by_name(&world, MILL).expect("tool");
    let tool = model::tool_of(&world, &e).expect("tool record");
    let body = tool_body::body_of(&world, &e).expect("assembled body");
    assert_eq!(body.pieces.len(), 3);
    let BodyFrame::Spindle { profile, .. } = &body.frame else { panic!("expected spindle frame") };
    assert!((profile.top() - 95.0).abs() < 1e-6, "top should reach 95 mm, got {}", profile.top());
    let max_r = profile.bands().iter().map(|b| b.r).fold(0.0, f64::max);
    assert!(max_r > 18.8, "adapter/clamp corners widen envelope, got {max_r}");

    // Displayed assembly includes all 3 pieces' facets.
    let drawn = tool_body_view::assembly(&tool, &body);
    assert!(drawn.len() > body.pieces[0].facets.len(), "draws more than piece 0 alone");

    // Project save & reload preserves all 3 pieces and their tool-frame CSYS.
    let saved = serde_json::to_string(&project::collect(&world)).expect("save camproj");
    let world2 = camcraft_app::world::new_world();
    project::open(&world2, &saved).expect("open camproj");
    let e2 = model::find_by_name(&world2, MILL).expect("reloaded tool");
    let body2 = tool_body::body_of(&world2, &e2).expect("reloaded body");
    assert_eq!(body2.pieces.len(), 3);
    assert_eq!(body2.frame, body.frame);

    // .camtools export & import preserves all 3 pieces.
    let lib_json = tool_library::export_tool_library(&world).expect("export .camtools");
    let parsed = tool_library::parse_tool_library(&lib_json).expect("parse .camtools");
    let world3 = camcraft_app::world::new_world();
    let mut h3 = History::default();
    h3.execute(&world3, ImportTools::new(&world3, parsed).expect("import tools"));
    let e3 = model::find_by_name(&world3, MILL).expect("imported tool");
    let body3 = tool_body::body_of(&world3, &e3).expect("imported body");
    assert_eq!(body3.pieces.len(), 3);
    assert_eq!(body3.frame, body.frame);
}

#[test]
fn modular_assembly_refuses_ambiguous_missing_and_over_budget_pieces_by_name() {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, MILL);
    add_tool(&world, &mut history, INSERT);

    let head = testdata("tool_csys_box_iso13399.step");
    let unit = testdata("modular_clamp_unit.step");
    history.execute(&world, SetToolBody::from_step_auto(&world, MILL, &head).expect("head"));

    // Ambiguous auto-mate: head has MCS + CSW1_2 and clamp has MCS + CSW.
    let err = SetToolBody::attach_piece(&world, MILL, &unit, PiecePlace::auto_csys()).expect_err("ambiguous");
    assert!(err.to_string().contains("ambiguous"), "{err}");

    // Missing CSYS name.
    let err = SetToolBody::attach_piece(&world, MILL, &unit, PiecePlace::csys(Some(0), "NO_SUCH", "CSW")).expect_err("missing target");
    assert!(err.to_string().contains("NO_SUCH"), "{err}");

    // Frame mismatch.
    let place = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };
    let err = SetToolBody::attach_piece(&world, MILL, &unit, PiecePlace::Lathe(place)).expect_err("frame mismatch");
    assert!(err.to_string().contains("spindle"), "{err}");

    // Remove out-of-range or only piece refuses.
    let err = SetToolBody::remove_piece(&world, MILL, 5).expect_err("out of range");
    assert!(err.to_string().contains("out of range"), "{err}");
    let err = SetToolBody::remove_piece(&world, MILL, 0).expect_err("only piece");
    assert!(err.to_string().contains("only piece"), "{err}");

    // Over-budget STEP piece refused by name at tool_body_file.
    step::with_facet_budget(2_000, || {
        let err = SetToolBody::attach_piece(&world, MILL, &testdata("Milling-test.step"), PiecePlace::Spindle { axis: [0.0, 0.0, 1.0], offset: 60.0 }).expect_err("coarse");
        assert!(err.to_string().contains("too detailed"), "{err}");
    });
}

#[test]
fn mcp_attach_and_remove_tool_body_piece_roundtrips_and_undoes() {
    let mut c = McpClient::new();
    c.ok("add_tool", json!({ "catalog": INSERT }));
    let head = testdata("modular_lathe_head.step").display().to_string();
    let clamp = testdata("modular_clamp_unit.step").display().to_string();

    c.ok("set_lathe_tool_body", json!({
        "tool": INSERT,
        "path": head,
        "radial": [1, 0, 0],
        "along": [0, 0, 1],
        "tip": [0, 0, 0]
    }));
    let attached = c.ok("attach_tool_body_piece", json!({
        "tool": INSERT,
        "path": clamp
    }));
    assert_eq!(attached["body"]["pieces"].as_array().map(Vec::len), Some(2), "{attached}");

    let got = c.ok("get_tool_body", json!({ "tool": INSERT }));
    assert_eq!(got["body"]["pieces"].as_array().map(Vec::len), Some(2));

    let removed = c.ok("remove_tool_body_piece", json!({ "tool": INSERT, "index": 1 }));
    assert_eq!(removed["body"]["pieces"].as_array().map(Vec::len), Some(1));

    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_tool_body", json!({ "tool": INSERT }))["body"]["pieces"].as_array().map(Vec::len), Some(2));

    let e = c.refused("remove_tool_body_piece", json!({ "tool": INSERT, "index": 9 }));
    assert!(e.contains("out of range"), "{e}");
}


#[test]
fn inconsistent_saved_piece_coordinates_are_refused_in_camproj_and_camtools() {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    add_tool(&world, &mut history, MILL);
    history.execute(
        &world,
        SetToolBody::from_step_auto(&world, MILL, &testdata("tool_csys_box_iso13399.step")).expect("head"),
    );
    history.execute(
        &world,
        SetToolBody::attach_piece(
            &world, MILL, &testdata("modular_clamp_unit.step"), PiecePlace::csys(Some(0), "MCS", "CSW"),
        ).expect("adapter"),
    );

    // Keep facet counts unchanged while shrinking the retained head. Previously
    // loading passed, but removing the adapter rebuilt an unsafe smaller body.
    fn shrink_piece(value: &mut Value) -> bool {
        match value {
            Value::Object(object) => {
                if let Some(pieces) = object.get_mut("pieces").and_then(Value::as_array_mut) {
                    let facets = pieces[0]["facets"].as_array_mut().expect("piece facets");
                    for facet in facets {
                        for vertex in facet.as_array_mut().expect("vertices") {
                            for coordinate in vertex.as_array_mut().expect("coordinates").iter_mut().take(2) {
                                *coordinate = json!(coordinate.as_f64().expect("coordinate") * 0.1);
                            }
                        }
                    }
                    return true;
                }
                object.values_mut().any(shrink_piece)
            }
            Value::Array(values) => values.iter_mut().any(shrink_piece),
            _ => false,
        }
    }

    let mut saved = serde_json::to_value(project::collect(&world)).expect("save project");
    assert!(shrink_piece(&mut saved), "project contains modular pieces");
    let reopened = camcraft_app::world::new_world();
    let err = project::open(&reopened, &saved.to_string()).expect_err("inconsistent project refused");
    assert!(err.to_string().contains(MILL), "{err}");
    assert!(err.to_string().contains("modular piece coordinates"), "{err}");

    let exported = tool_library::export_tool_library(&world).expect("export tools");
    let mut library: Value = serde_json::from_str(&exported).expect("library JSON");
    assert!(shrink_piece(&mut library), "library contains modular pieces");
    let err = tool_library::parse_tool_library(&library.to_string()).expect_err("inconsistent library refused");
    assert!(err.to_string().contains(MILL), "{err}");
    assert!(err.to_string().contains("modular piece coordinates"), "{err}");
}
