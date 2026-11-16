//! #1573: a STEP shell the file declares closed must mesh closed, or the
//! import is refused by name; a face the kernel returns no mesh for is
//! refused by name on its own. Never a model with a wall missing.
//!
//! Fixture: `testdata/models/fillet_corner.step` with its corner blend's
//! weights bent (`1E300, 1E-300`), which truck cannot place a boundary on:
//! it returns no polygon for the blend and the rest of the shell imported
//! open, with no refusal, before this law.
//!
//! `box.step` with one face's `same_sense` flipped is a face wound against
//! its neighbours: closed geometry, fixed at import, never called open
//! (#1573 review: two healthy CAD exports were refused that way).
//!
//! Gate map (M4.4): covers app::geometry::import::step,
//! app::geometry::import::step_closed, app::geometry::import::step_seams,
//! app::geometry::import::step_orient, app::geometry::import::step_facets,
//! app::commands::import_model (notes), app::script::api (v_import notes).

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use camcraft_app::geometry::import::{self, step, Format, ImportError};
use camcraft_app::mcp::Server;
use serde_json::{json, Value};

fn model(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

fn within<R: Send + 'static>(limit: Duration, f: impl FnOnce() -> R + Send + 'static) -> R {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(limit) {
        Ok(r) => r,
        Err(_) => panic!("import did not finish within {limit:?}"),
    }
}

/// The fixture with its corner blend bent so truck drops the blend face.
fn bent() -> String {
    let text = std::fs::read_to_string(model("fillet_corner.step")).expect("fixture");
    let weights = "RATIONAL_B_SPLINE_SURFACE(((1.0,1.0,2.0),";
    assert_eq!(text.matches(weights).count(), 1, "fixture layout changed");
    assert!(text.contains("#1288=CLOSED_SHELL('',("), "fixture layout changed");
    text.replace(weights, "RATIONAL_B_SPLINE_SURFACE(((1.0E300,1.0E-300,2.0),")
}

fn import(text: String) -> Result<camcraft_app::geometry::Mesh, ImportError> {
    within(Duration::from_secs(90), move || {
        step::with_face_eval_cap(200_000, || import::parse_model(text.as_bytes(), Format::Step))
    })
}

#[test]
fn a_closed_shell_that_meshes_open_is_refused_by_name() {
    let err = import(bent()).expect_err("a closed shell with a face missing must not import");
    let msg = err.to_string();
    assert!(msg.contains("closed shell #1288 (10 faces) meshes open"), "{msg}");
    assert!(msg.contains("boundary edges lie more than"), "{msg}");
    assert!(msg.contains("mm from any other face, most on face #"), "{msg}");
    assert!(msg.contains(" mm; the first from ("), "{msg}");
    assert!(msg.contains("got no mesh from the kernel"), "the dropped face is named: {msg}");
}

#[test]
fn a_face_the_kernel_cannot_mesh_is_refused_in_an_open_shell_too() {
    // Declared open, and no solid naming it: the closed-shell law does not
    // apply, but a face left out is still a wall the file has and the mesh
    // does not.
    let text = bent()
        .replace("#1288=CLOSED_SHELL(", "#1288=OPEN_SHELL(")
        .replace("#1289=MANIFOLD_SOLID_BREP('',#1288);", "#1289=SHELL_BASED_SURFACE_MODEL('',(#1288));");
    let err = import(text).expect_err("a dropped face must not import silently");
    let msg = err.to_string();
    assert!(msg.contains("in shell #1288 could not be tessellated"), "{msg}");
    assert!(msg.contains("returned no mesh for it"), "{msg}");
}

#[test]
fn the_outer_shell_of_a_solid_is_closed_whatever_its_keyword() {
    // A MANIFOLD_SOLID_BREP's outer shell is closed by the schema, even
    // when the file spells it OPEN_SHELL: the gap is still refused as one.
    let text = bent().replace("#1288=CLOSED_SHELL(", "#1288=OPEN_SHELL(");
    assert!(text.contains("#1289=MANIFOLD_SOLID_BREP('',#1288);"), "fixture layout changed");
    let msg = import(text).expect_err("a solid with a face missing must not import").to_string();
    assert!(msg.contains("closed shell #1288 (10 faces) meshes open"), "{msg}");
}

/// Signed volume of a mesh (divergence theorem), as tetrahedra to a point
/// off every face's plane: positive when it is wound outward, and every
/// face counts, so one face wound inward changes it.
fn volume(m: &camcraft_app::geometry::Mesh) -> f64 {
    let o = camcraft_app::geometry::Point3::new(-3.0, -7.0, -11.0);
    m.facets.iter().map(|t| (t.a - o).dot((t.b - o).cross(t.c - o)) / 6.0).sum()
}

/// `box.step` with face #32 wound against its neighbours.
fn reversed_box() -> String {
    let text = std::fs::read_to_string(model("box.step")).expect("fixture");
    let face = "#32 = FACE_SURFACE('', (#33), #90, .T.);";
    assert_eq!(text.matches(face).count(), 1, "fixture layout changed");
    let bound = "#33 = FACE_BOUND('', #34, .T.);";
    assert_eq!(text.matches(bound).count(), 1, "fixture layout changed");
    text.replace(face, "#32 = FACE_SURFACE('', (#33), #90, .F.);").replace(bound, "#33 = FACE_BOUND('', #34, .F.);")
}

#[test]
fn a_face_wound_against_its_neighbours_imports_flipped_not_refused() {
    let good = import(std::fs::read_to_string(model("box.step")).expect("fixture")).expect("box.step imports");
    let mesh = import(reversed_box()).expect("a reversed face is closed geometry: imported, not refused");
    assert_eq!(mesh.facets.len(), good.facets.len());
    let (v, w) = (volume(&mesh), volume(&good));
    assert!(w > 0.0, "box.step is wound outward: {w}");
    assert!((v - w).abs() <= w * 1e-9, "the reversed face is flipped back outward: {v} vs {w}");
}

#[test]
fn every_solid_step_fixture_still_imports() {
    for name in [
        "box.step",
        "bspline_dome.step",
        "fillet_corner.step",
        "Milling-test.step",
        "Milling-test-inch.step",
        "sample_ap214.step",
        "lathe_holder_offset.step",
        "tool_csys_box_iso13399.step",
    ] {
        let path = model(name);
        let r = within(Duration::from_secs(240), move || import::load_model(&path));
        assert!(r.is_ok(), "{name}: {:?}", r.err());
    }
}

/// One MCP `tools/call`, in process: the structured answer, or the refusal.
fn mcp_import(path: &Path) -> Value {
    let mut server = Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    server.handle(&init).expect("initialize answers");
    let args = json!({ "path": path.display().to_string(), "name": "box" });
    let call = json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": { "name": "import_model", "arguments": args } });
    let reply = server.handle(&call).expect("tools/call answers");
    assert_ne!(reply["result"]["isError"], json!(true), "import_model refused: {reply}");
    reply["result"]["structuredContent"].clone()
}

#[test]
fn a_flipped_face_is_named_in_the_import_result_and_the_mcp_answer() {
    // #1576 review: a flip is said by name where the user and an agent
    // read the import, not only in the log.
    let bad = reversed_box();
    let graded = within(Duration::from_secs(90), move || import::parse_model_graded(bad.as_bytes(), Format::Step))
        .expect("a reversed face imports");
    assert_eq!(graded.notes.len(), 1, "{:?}", graded.notes);
    assert!(graded.notes[0].contains("1 face(s) wound against their neighbours were flipped: face #32 ("), "{:?}", graded.notes);
    let dir = std::env::temp_dir().join(format!("camcraft-closed-shell-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let (bad_path, good_path) = (dir.join("reversed.step"), dir.join("good.step"));
    std::fs::write(&bad_path, reversed_box()).expect("write");
    std::fs::copy(model("box.step"), &good_path).expect("copy");
    let said = mcp_import(&bad_path);
    let notes = said["notes"].as_array().cloned().unwrap_or_default();
    assert_eq!(notes.len(), 1, "{said}");
    assert!(notes[0].as_str().unwrap_or_default().contains("flipped: face #32 ("), "{said}");
    let quiet = mcp_import(&good_path);
    assert!(quiet.get("notes").is_none(), "nothing changed, nothing said: {quiet}");
    let _ = std::fs::remove_dir_all(&dir);
}
