//! #1617: a closed, gap-free STEP solid whose faces are all wound inward
//! (consistently, so no face disagrees with a neighbour) imports with a
//! positive signed volume, and the import says it turned it - in
//! `Graded::notes` and in the MCP `import_model` result. A lone curved face
//! (an open shell) has no inside and is never turned.
//!
//! Gate map (#1617): covers app::geometry::import (parse_model_graded,
//! step_outward), app::commands::import_model (notes), app::script::api
//! (v_import notes) through app::mcp (import_model).
//!
//! Provenance: our own box and dome fixtures with their senses reversed
//! by hand; signed volume by the divergence theorem. No vendor material.

use std::path::{Path, PathBuf};

use camcraft_app::geometry::import::{Format, parse_model_graded};
use camcraft_app::geometry::Triangle;
use camcraft_app::mcp::Server;
use serde_json::{Value, json};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixture(name)).expect("fixture")
}

/// Every FACE_SURFACE and FACE_BOUND sense of `text` reversed.
fn reversed(text: &str) -> String {
    text.lines()
        .map(|l| {
            if l.contains("FACE_SURFACE(") || l.contains("FACE_BOUND(") {
                l.replace(".T.)", ".X.)").replace(".F.)", ".T.)").replace(".X.)", ".F.)")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn volume(t: &[Triangle]) -> f64 {
    t.iter().map(|t| t.a.dot(t.b.cross(t.c)) / 6.0).sum()
}

#[test]
fn an_inward_closed_box_is_turned_outward_and_said() {
    let plain = parse_model_graded(read("box.step").as_bytes(), Format::Step).expect("plain box");
    assert!(plain.notes.is_empty(), "nothing changed, nothing said: {:?}", plain.notes);
    let v_plain = volume(&plain.mesh.facets);
    assert!(v_plain > 0.0, "the fixture is outward: {v_plain}");

    let inward = reversed(&read("box.step"));
    assert_ne!(inward, read("box.step"), "the senses were reversed");
    let got = parse_model_graded(inward.as_bytes(), Format::Step).expect("inward box imports");
    let v = volume(&got.mesh.facets);
    assert!((v - v_plain).abs() <= 1e-6 * v_plain.abs(), "turned outward: {v} vs {v_plain}");
    assert_eq!(got.notes.len(), 1, "{:?}", got.notes);
    let note = got.notes.first().map(String::as_str).unwrap_or_default();
    assert!(note.starts_with("1 closed solid(s) wound inward (negative volume) were turned outward"), "{note}");
    assert!(note.contains(&format!("{} facet(s) reversed", got.mesh.facets.len())), "{note}");
}

/// The dome's B-spline face alone, in an open shell, wound either way: an
/// open, curved face has no inside, so its winding is left as the file says.
#[test]
fn a_lone_curved_face_is_left_alone() {
    let text = read("bspline_dome.step");
    let shell = "#17 = CLOSED_SHELL('', (#18, #25, #32, #39, #46, #53));";
    assert_eq!(text.matches(shell).count(), 1, "fixture layout changed");
    // A solid brep naming the shell makes main's closed-shell check judge it
    // (#1576), so the lone face is held by a surface model instead.
    let brep = "#16 = MANIFOLD_SOLID_BREP('', #17);";
    assert_eq!(text.matches(brep).count(), 1, "fixture layout changed");
    let lone = text.replace(shell, "#17 = OPEN_SHELL('', (#53));").replace(brep, "#16 = SHELL_BASED_SURFACE_MODEL('', (#17));");
    for (label, data) in [("as written", lone.clone()), ("reversed", reversed(&lone))] {
        let got = parse_model_graded(data.as_bytes(), Format::Step).expect("a lone face imports");
        assert!(!got.mesh.facets.is_empty(), "{label}: the face meshed");
        assert!(got.notes.is_empty(), "{label}: an open face is never turned: {:?}", got.notes);
    }
    let a = parse_model_graded(lone.as_bytes(), Format::Step).expect("as written");
    let b = parse_model_graded(reversed(&lone).as_bytes(), Format::Step).expect("reversed");
    // Left alone means each keeps the file's own winding: the two are mirror
    // images in winding, not both forced one way.
    let (na, nb) = (a.mesh.facets.first().map(Triangle::normal), b.mesh.facets.first().map(Triangle::normal));
    if let (Some(na), Some(nb)) = (na, nb) {
        assert!(na.dot(nb) < 0.0, "the reversed face keeps its reversed winding: {na:?} {nb:?}");
    }
}

fn call(server: &mut Server, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    assert_ne!(reply["result"]["isError"], json!(true), "{name} refused: {reply}");
    reply["result"]["structuredContent"].clone()
}

#[test]
fn the_mcp_import_result_carries_the_note() {
    let dir = std::env::temp_dir().join(format!("camcraft-import-outward-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (bad, good) = (dir.join("inward.step"), dir.join("plain.step"));
    std::fs::write(&bad, reversed(&read("box.step"))).expect("write");
    std::fs::copy(fixture("box.step"), &good).expect("copy");

    let mut server = Server::new();
    let got = call(&mut server, "import_model", json!({ "path": bad.display().to_string() }));
    let notes = got["notes"].as_array().cloned().unwrap_or_default();
    assert_eq!(notes.len(), 1, "{got}");
    assert!(notes[0].as_str().unwrap_or_default().contains("turned outward"), "{got}");
    let plain = call(&mut server, "import_model", json!({ "path": good.display().to_string() }));
    assert!(plain.get("notes").is_none(), "nothing changed, nothing said: {plain}");
    let _ = std::fs::remove_dir_all(&dir);
}
