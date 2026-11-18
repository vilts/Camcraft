//! A Solidcraft `.sc` design opens as a model (#1456): its cached final
//! body, meshed by the kernel's machining mesh, lands through the one model
//! import (Model > Open, viewport drop and MCP `import_model` all call it).
//! A damaged or unsupported `.sc` is refused by name, with the kernel
//! reader's own reason, and enters no history.
//!
//! Gate map (M4.4): covers camcraft_geometry::solidcraft,
//! app::geometry::import, app::commands::import_model, app::mcp::tools.

use std::path::{Path, PathBuf};

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::import;
use camcraft_app::mcp::Server;
use serde_json::{json, Value};

fn sample() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models/solidcraft_for_beginners.sc")
}

fn tmp(name: &str, bytes: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-sc-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let path = dir.join(name);
    std::fs::write(&path, bytes).expect("write tmp file");
    path
}

#[test]
#[ignore = "heavy lane (#1456 Solidcraft sample): meshes the sample at the 0.005 mm chord, ~100 s in debug - run with --release -- --ignored"]
fn the_sample_design_imports_at_its_design_box() {
    let graded = import::load_model_graded(&sample()).expect("the Solidcraft sample must import");
    let b = graded.mesh.bounds().expect("a non-empty mesh");
    // The design's own box (the kernel pin reads the same numbers).
    for (got, want) in [
        (b.min.x, -62.5),
        (b.min.y, -62.5),
        (b.min.z, 0.0),
        (b.max.x, 62.5),
        (b.max.y, 62.5),
        (b.max.z, 85.0),
    ] {
        assert!((got - want).abs() < 0.01, "bounds {b:?}");
    }
    assert!(graded.open.is_none(), "the machining mesh is watertight");
    assert!(graded.coarse_mm.is_none(), "meshed at the importer's chord cap");
    let cmd = ImportModel::new(&sample()).expect("the import command takes the sample");
    assert_eq!(cmd.name, "solidcraft_for_beginners");
}

#[test]
fn damaged_and_unsupported_sc_files_are_refused_by_name() {
    let bytes = std::fs::read(sample()).expect("read the sample");
    let not_zip = tmp("text.sc", b"this is not a Solidcraft design");
    let cut = tmp("cut.sc", &bytes[..bytes.len() / 2]);
    let empty = tmp("empty.sc", b"");
    for (path, want) in [(&not_zip, "Solidcraft"), (&cut, "Solidcraft"), (&empty, "empty")] {
        let err = match import::load_model(path) {
            Ok(_) => panic!("{} must be refused", path.display()),
            Err(e) => e.to_string(),
        };
        assert!(err.contains(want), "{}: {err}", path.display());
    }
    // The kernel reader's own reason is in the message, not a generic one.
    let err = import::load_model(&not_zip).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("import_solidcraft") || err.contains("ZIP") || err.contains("zip"), "{err}");
}

#[test]
fn a_facet_budget_is_refused_for_a_solidcraft_design() {
    let err = ImportModel::within(&sample(), Some(1_000_000)).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("STEP files only"), "{err}");
}

fn call(server: &mut Server, id: i64, name: &str, args: Value) -> Value {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    server.handle(&msg).expect("a request gets a response")["result"].clone()
}

fn server() -> Server {
    let mut server = Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    server.handle(&init).expect("initialize");
    server
}

fn models(server: &mut Server) -> usize {
    let job = call(server, 9, "get_job", json!({}));
    job["structuredContent"]["models"].as_array().map_or(0, Vec::len)
}

#[test]
fn mcp_import_model_refuses_a_damaged_solidcraft_design_by_name() {
    let mut server = server();
    let bad = tmp("bad.sc", b"PK\x03\x04 not really");
    let refused = call(&mut server, 1, "import_model", json!({ "path": bad.display().to_string() }));
    assert_eq!(refused["isError"], json!(true), "{refused}");
    let text = refused["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("Solidcraft design refused"), "{text}");
    assert_eq!(models(&mut server), 0, "a refused design lands no model");
}

#[test]
#[ignore = "heavy lane (#1456 Solidcraft sample): meshes the sample at the 0.005 mm chord, ~100 s in debug - run with --release -- --ignored"]
fn mcp_import_model_opens_a_solidcraft_design() {
    let mut server = server();
    let ok = call(&mut server, 1, "import_model", json!({ "path": sample().display().to_string() }));
    assert_ne!(ok["isError"], json!(true), "{ok}");
    assert_eq!(models(&mut server), 1, "the design landed as one model");
}
