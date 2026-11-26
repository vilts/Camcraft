//! #3361: 3D Pocket "Pass starts at" (Auto / Inside / Outside), the
//! shared parameter of #3316. The generate arm hands the value to the
//! level ladder: on the 40 mm block in its 70 mm stock (the block is the
//! level's hole), Waterline with Inside starts at the part wall and
//! Outside at the outer edge of the level, while Auto keeps its order.
//! The option rides the undoable EditOperationParams through MCP
//! `edit_operation` (undo and redo) and the project file: Auto is absent
//! from both, and a file without the key reads Auto.
//!
//! Gate map: covers app::operation (generate's Pocket3D arm, params
//! digest), app::mcp (edit_operation, undo, redo), app::project (params,
//! params_impl), motion::machining3d (hsm).
//! Provenance: our own regression of the maintainer's request; no vendor
//! material consulted.

mod area_support;
mod common;

use area_support::*;
use camcraft_app::machining2d::PassStart;
use camcraft_app::mcp::Server;
use serde_json::{Value, json};

/// A whole-model Waterline 3D Pocket: one level at Z 5 around the block.
fn waterline(start: PassStart) -> OpParams {
    let mut p = operation::retype_params(&pocket(10.0, 5.0), OpType::Pocket3D.ordinal());
    if let OpParams::Pocket3D { strategy, pass_start, stepdown, entry, .. } = &mut p {
        (*strategy, *pass_start, *stepdown, *entry) = (PocketStrategy::Waterline, start, 10.0, Entry3D::Plunge);
    } else {
        panic!("not a 3D Pocket: {p:?}");
    }
    p
}

/// The Chebyshev distance from the block's centre of the first cutting
/// feed at the level.
fn first_cut(t: &Toolpath) -> f64 {
    let m = t.moves.iter().find(|m| m.kind == MoveKind::Feed && (m.from.z - 5.0).abs() < 1e-6 && (m.to.z - 5.0).abs() < 1e-6);
    m.map_or(f64::NAN, |m| m.from.x.abs().max(m.from.y.abs()))
}

#[test]
fn the_generate_arm_hands_pass_start_to_the_3d_ladder() {
    let world = block_world();
    let generate = |s: PassStart| {
        let o = op(SrcRef::Entity("part".into()), waterline(s), AreaControls::default());
        run(&world, &o).unwrap_or_else(|e| panic!("{s:?} generates: {e}"))
    };
    let (auto, aw) = generate(PassStart::Auto);
    let (inside, iw) = generate(PassStart::Inside);
    let (outside, ow) = generate(PassStart::Outside);
    assert_eq!((iw, ow), (aw, aw), "no side adds a verify warning");
    let (fa, fi, fo) = (first_cut(&auto), first_cut(&inside), first_cut(&outside));
    assert!((fi - (20.0 + R)).abs() < 0.5, "Inside starts on the part wall ring: {fi}");
    assert!(fo > fi + 5.0 && fo > fa, "Outside starts at the outer edge: {fo} (Inside {fi}, Auto {fa})");
    assert!((fa - fi).abs() > 0.5, "Auto keeps its own order: {fa}");
    for (what, t) in [("inside", &inside), ("outside", &outside)] {
        assert!(t.notes.iter().any(|n| n.starts_with(camcraft_app::machining2d::PASS_START_NOTE)), "{what} names its start: {:?}", t.notes);
    }
    assert!(!auto.notes.iter().any(|n| n.starts_with(camcraft_app::machining2d::PASS_START_NOTE)), "Auto says nothing");
}

/// The project file: Auto writes no key, Inside and Outside their word,
/// a file without the key reads Auto, and each value reads back.
#[test]
fn pocket3d_pass_start_rides_the_project_file() {
    let auto = waterline(PassStart::Auto);
    let wire = project::params_to_json(&auto);
    assert_eq!(wire["pocket3d"].get("pass_start"), None, "Auto stays off the wire: {wire}");
    assert_eq!(project::params_from_json(&wire).expect("an old file reads"), auto, "absent is Auto");
    for (s, word) in [(PassStart::Inside, "inside"), (PassStart::Outside, "outside")] {
        let p = waterline(s);
        let wire = project::params_to_json(&p);
        assert_eq!(wire["pocket3d"].get("pass_start").and_then(Value::as_str), Some(word), "{wire}");
        assert_eq!(project::params_from_json(&wire).expect("reads back"), p);
    }
    let mut bad = project::params_to_json(&auto);
    bad["pocket3d"]["pass_start"] = json!("sideways");
    assert!(project::params_from_json(&bad).is_err(), "an unknown word is refused");
}

fn call(s: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    s.handle(&msg).expect("a response")["result"].clone()
}

fn fig8_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/fig8.step")
}

/// MCP: the generic `edit_operation` sets the value, refuses an unknown
/// word, never writes `auto`, and undo and redo walk the history.
#[test]
fn mcp_sets_pocket3d_pass_start_and_undo_redo_walk_it() {
    let mut s = Server::new();
    let ok = |r: &Value| r["isError"] != json!(true);
    let imp = call(&mut s, 1, "import_model", json!({ "path": fig8_path().display().to_string() }));
    assert!(ok(&imp), "{imp}");
    assert!(ok(&call(&mut s, 2, "add_tool", json!({ "catalog": TOOL }))));
    let add = call(&mut s, 3, "add_operation", json!({ "kind": "3D Pocket", "tool": TOOL, "name": "P3", "source": "fig8" }));
    assert!(ok(&add), "{add}");
    assert!(!add.to_string().contains("pass_start"), "Auto is absent: {add}");
    let edit = call(&mut s, 4, "edit_operation", json!({ "name": "P3", "params": { "pass_start": "inside" } }));
    assert!(edit.to_string().contains("\"pass_start\":\"inside\""), "{edit}");
    let bad = call(&mut s, 5, "edit_operation", json!({ "name": "P3", "params": { "pass_start": "sideways" } }));
    assert_eq!(bad["isError"], json!(true), "an unknown word is refused: {bad}");
    let auto = call(&mut s, 6, "edit_operation", json!({ "name": "P3", "params": { "pass_start": "auto" } }));
    assert!(ok(&auto), "auto is a known word: {auto}");
    assert!(!auto.to_string().contains("pass_start"), "Auto is not written: {auto}");
    assert!(ok(&call(&mut s, 7, "undo", json!({}))));
    let job = call(&mut s, 8, "get_job", json!({})).to_string();
    assert!(job.contains("\"pass_start\":\"inside\""), "undo restores Inside: {job}");
    assert!(ok(&call(&mut s, 9, "redo", json!({}))));
    let job = call(&mut s, 10, "get_job", json!({})).to_string();
    assert!(!job.contains("pass_start"), "redo puts Auto back: {job}");
}
