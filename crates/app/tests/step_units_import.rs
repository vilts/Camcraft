//! #1874: a STEP import states the length unit that the model was read
//! in, and a STEP file that states no length unit is never read in a
//! guessed one. Without a unit from the caller it is refused by name; with
//! one it reads in that unit. A file that states its own unit keeps it,
//! and the note says that the caller's unit is not used. A unit given for
//! a mesh file is refused by name.
//! Provenance: our own test design over our own fixtures (`box.step`
//! mutated in memory, `short_units_inch.step`, `sample_ap214.step`); the
//! unit records are ISO 10303-41. No HSMWorks material consulted.
//!
//! Gate map (#1874): covers camcraft_app::commands::import_model,
//! camcraft_app::geometry::import::step (unit_note), camcraft_app::mcp
//! (import_model `units`), camcraft_app::script (cam.import).

use std::path::{Path, PathBuf};

use camcraft_app::commands::{CommandError, ImportModel};
use camcraft_app::geometry::ImportError;
use camcraft_app::units::Unit;
use serde_json::json;

fn model(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn text(name: &str) -> String {
    std::fs::read_to_string(model(name)).expect("fixture").replace("\r\n", "\n")
}

fn cut(text: String, from: &str, to: &str) -> String {
    assert!(text.contains(from), "fixture edit no longer applies: {from}");
    text.replacen(from, to, 1)
}

/// `box.step` with no length unit: the unit, the uncertainty that names
/// it, and their places in the context are taken out.
fn unitless() -> Vec<u8> {
    let t = cut(text("box.step"), "    GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#15))\n", "");
    let t = cut(t, "GLOBAL_UNIT_ASSIGNED_CONTEXT((#12, #13, #14))", "GLOBAL_UNIT_ASSIGNED_CONTEXT((#13, #14))");
    let t = cut(t, "#12 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );\n", "");
    cut(t, "#15 = UNCERTAINTY_MEASURE_WITH_UNIT(1.0E-6, #12, 'distance_accuracy_value','confusion accuracy');\n", "").into_bytes()
}

fn width(cmd: &ImportModel) -> f64 {
    let b = cmd.mesh.bounds().expect("a mesh");
    b.max.x - b.min.x
}

fn unit_note(cmd: &ImportModel) -> &str {
    cmd.notes.iter().find(|n| n.starts_with("length unit")).map_or("", String::as_str)
}

#[test]
fn a_step_file_with_no_length_unit_is_refused_by_name_without_a_unit() {
    let got = ImportModel::from_bytes("part.step", &unitless(), None);
    assert!(
        matches!(got, Err(CommandError::Import(ImportError::StepUnitsMissing))),
        "a unitless file is refused, never read in millimetres: {:?}",
        got.as_ref().err()
    );
    let msg = got.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(msg.contains("states no length unit") && msg.contains("mm or in"), "the refusal says what to state: {msg}");
}

#[test]
fn a_unit_stated_at_import_reads_the_unitless_file_in_it_and_says_so() {
    let mm = ImportModel::from_bytes_in("part.step", &unitless(), None, Some(Unit::Millimeters)).expect("mm import");
    let inch = ImportModel::from_bytes_in("part.step", &unitless(), None, Some(Unit::Inches)).expect("inch import");
    let ratio = width(&inch) / width(&mm);
    assert!((ratio - 25.4).abs() < 1e-6, "an inch part is 25.4 times as wide: {ratio}");
    assert_eq!(unit_note(&inch), "length unit: in (stated at import; the file states no length unit)");
    assert_eq!(unit_note(&mm), "length unit: mm (stated at import; the file states no length unit)");
}

#[test]
fn a_file_that_states_its_unit_says_it_and_keeps_it_over_the_callers() {
    let data = std::fs::read(model("box.step")).expect("box.step");
    let own = ImportModel::from_bytes("box.step", &data, None).expect("box imports");
    assert_eq!(unit_note(&own), "length unit: mm (stated by the file)");
    let asked = ImportModel::from_bytes_in("box.step", &data, None, Some(Unit::Inches)).expect("box imports");
    assert_eq!(unit_note(&asked), "length unit: mm (stated by the file; the in stated at import is not used)");
    assert!((width(&asked) - width(&own)).abs() < 1e-9, "the file's unit wins");
    let inch = ImportModel::within(&model("short_units_inch.step"), None).expect("inch file imports");
    assert_eq!(unit_note(&inch), "length unit: in (stated by the file)");
    let metre = ImportModel::within(&model("sample_ap214.step"), None).expect("metre file imports");
    assert_eq!(unit_note(&metre), "length unit: m (stated by the file)");
}

#[test]
fn a_unit_for_a_mesh_file_is_refused_by_name_before_parsing() {
    let got = ImportModel::from_bytes_in("part.stl", b"not even read", None, Some(Unit::Inches));
    let msg = got.err().map(|e| e.to_string()).unwrap_or_default();
    assert!(msg.contains("applies to STEP files only"), "{msg}");
}

#[test]
fn mcp_import_model_takes_the_unit_and_refuses_without_it() {
    let dir = std::env::temp_dir().join(format!("camcraft-step-units-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let file = dir.join("unitless.step");
    std::fs::write(&file, unitless()).expect("write the fixture");
    let path = file.to_string_lossy().into_owned();
    let call = |server: &mut camcraft_app::mcp::Server, args: serde_json::Value| {
        server
            .handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "import_model", "arguments": args } }))
            .expect("a reply")
    };
    let mut server = camcraft_app::mcp::Server::new();
    let refused = call(&mut server, json!({ "path": path }));
    assert_eq!(refused["result"]["isError"], json!(true), "{refused}");
    assert!(refused.to_string().contains("states no length unit"), "{refused}");
    let bad = call(&mut server, json!({ "path": path, "units": "cm" }));
    assert!(bad.to_string().contains("must be \\\"mm\\\" or \\\"in\\\""), "{bad}");
    let ok = call(&mut server, json!({ "path": path, "units": "in" }));
    let body = ok["result"]["content"][0]["text"].as_str().unwrap_or_default();
    assert!(body.contains("stated at import; the file states no length unit"), "{ok}");
    let _ = std::fs::remove_dir_all(&dir);
}
