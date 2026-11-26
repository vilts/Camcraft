//! #1610: a STEP closed shell that meshes open lands as an INCOMPLETE
//! model - loadable for viewing and setup, its hole named - and every
//! operation on it, and the program verdict, is refused by name at the
//! post. The hole rides undo, update and the project file; a whole
//! re-import clears it. A loader that cannot land a model incomplete (a
//! mesh-only import, a fixture, a tool body) still refuses the file.
//!
//! Fixture: `testdata/models/fillet_corner.step` with its corner blend's
//! weights bent, as in `import_step_closed_shell.rs`: since #917 the reader
//! refuses it by name at read. The incomplete model is pinned with a
//! synthetic hole (`incomplete`); the kernel's partial mesh, which names
//! each left-out face, is pinned in `crates/kernel/tests/machining_mesh_open.rs`.
//!
//! Gate map (#1610): covers app::mesh_open, app::model_fidelity
//! (op_problem, verdict_tol, set), app::commands::import_model,
//! app::commands::update_model, app::commands::delete_entity,
//! app::project (wire, apply, validate: `mesh_open`), app::posting
//! (verification_report, post_setup), app::geometry::import (the `*_open`
//! loaders, step_closed).
//!
//! Provenance: our own pins over our own laws; no vendor material.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use camcraft_app::commands::{
    AddTool, CreateOperation, DeleteEntity, GenerateOperation, History, ImportModel, SetStock, TagFixture, UpdateModel,
};
use camcraft_app::geometry::import::{self, Format};
use camcraft_app::geometry::{cube, Mesh, Point3};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{mesh_open, model, model_fidelity, posting, project};
use flecs_ecs::prelude::World;

const MILL: &str = "EM-1000-4FL";
const HOLE: &str = "closed shell #7 (6 faces) meshes open: 4 boundary edges lie more than 1.00e-2 mm from any other face";
const SAID: &str = "imported incomplete - closed shell #7 (6 faces) meshes open";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// `fillet_corner.step` with its corner blend bent so truck drops the blend.
fn bent() -> String {
    let text = std::fs::read_to_string(fixture("fillet_corner.step")).expect("fixture");
    let weights = "RATIONAL_B_SPLINE_SURFACE(((1.0,1.0,2.0),";
    assert_eq!(text.matches(weights).count(), 1, "fixture layout changed");
    text.replace(weights, "RATIONAL_B_SPLINE_SURFACE(((1.0E300,1.0E-300,2.0),")
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

fn brick() -> Mesh {
    cube(Point3::new(-20.0, -10.0, 0.0), Point3::new(20.0, 10.0, 10.0))
}

fn face() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    }
}

/// An incomplete import, as `ImportModel` builds it from a graded mesh
/// whose `open` names a hole.
fn incomplete(name: &str) -> ImportModel {
    let mut cmd = ImportModel::from_mesh(name, brick()).with_fidelity(Some(model_fidelity::UNKNOWN_MM));
    cmd.hole = Some(HOLE.to_string());
    cmd
}

/// The incomplete "part", a box stock, and a Face op on it, generated.
fn faced_part() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, incomplete("part"));
    h.execute(&world, SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 30.0, dz: 12.0 }, Default::default()).expect("stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == MILL).map(|(_, t)| t).expect("catalog mill");
    h.execute(&world, AddTool::new(&world, MILL, tool).expect("tool"));
    h.execute(&world, CreateOperation::new(&world, "OP", MILL, SrcRef::Entity("part".into()), face()).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "OP").expect("generate"));
    (world, h)
}

fn hole(world: &World, name: &str) -> Option<String> {
    model::find_by_name(world, name).and_then(|e| mesh_open::hole_of(world, &e))
}

/// The reader's refusal of the bent blend (#917): the item, the face and
/// the weight by name, at read, before any mesh.
fn bent_weight_refused(msg: &str) {
    assert!(msg.contains("first #1289 at read: face #1228: weight: 1"), "{msg}");
    assert!(msg.contains("out of range"), "{msg}");
}

#[test]
fn a_bent_closed_shell_is_refused_at_read_by_every_loader() {
    // #917: the solidcore reader refuses the bent weight itself, so no STEP
    // text reaches a partial mesh at the pinned solidcore commit (that path
    // is pinned at the kernel, `machining_mesh_open`). The model import
    // that could land a model incomplete refuses it by name too: a file the
    // reader cannot read lands no model.
    let text = bent();
    let open = within(Duration::from_secs(90), move || ImportModel::from_bytes("bent.step", text.as_bytes(), None));
    let msg = open.err().map(|e| e.to_string()).expect("the model import refuses a weight out of range");
    bent_weight_refused(&msg);
    // A loader that cannot land a model incomplete refuses it the same way.
    let text = bent();
    let strict = within(Duration::from_secs(90), move || {
        import::parse_model_graded(text.as_bytes(), Format::Step)
    });
    bent_weight_refused(&strict.expect_err("the graded loader refuses it").to_string());
    // The path loader a fixture or a tool body reads through, too.
    let file = std::env::temp_dir().join(format!("camcraft-1610-{}.step", std::process::id()));
    std::fs::write(&file, bent()).expect("write");
    let path = file.clone();
    let strict = within(Duration::from_secs(90), move || import::load_model_graded(&path));
    let _ = std::fs::remove_file(&file);
    bent_weight_refused(&strict.expect_err("the graded path loader refuses it").to_string());
}

#[test]
fn every_operation_on_an_incomplete_model_is_refused_at_the_post_naming_the_hole() {
    let (world, _h) = faced_part();
    let report = posting::verification_report(&world);
    let want = format!("OP: model 'part' was {SAID}");
    assert!(report.problems.iter().any(|p| p.starts_with(&want) && p.contains(HOLE)), "{:?}", report.problems);
    let not_judged = report.not_judged.expect("the program sim gives no verdict");
    assert!(not_judged.contains(SAID), "{not_judged}");
    let out = std::env::temp_dir().join(format!("camcraft-1610-{}.nc", std::process::id()));
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    let Err(err) = posted else { panic!("the post went out against a model with walls missing") };
    assert!(err.contains(SAID), "{err}");
    // Not the pre-v59 note: the refusal is about the hole.
    assert!(!err.contains("saved before CamCraft recorded"), "{err}");
    // The program verdict names it too, with no operation to name.
    let bare = new_world();
    History::default().execute(&bare, incomplete("part"));
    let why = model_fidelity::verdict_tol(&bare).expect_err("no verdict against a mesh with walls missing");
    assert!(why.starts_with(&format!("model 'part' was {SAID}")), "{why}");
    // And a fixture made from an incomplete model.
    let mut h = History::default();
    h.execute(&world, incomplete("vise"));
    h.execute(&world, TagFixture::new(&world, "vise", None).expect("fixture"));
    let gate = posting::verification_gate(&world);
    assert!(gate.iter().any(|p| p.starts_with(&format!("fixture 'vise' was {SAID}"))), "{gate:?}");
}

#[test]
fn the_hole_rides_undo_and_a_whole_reimport_clears_it() {
    let (world, mut h) = faced_part();
    h.execute(&world, DeleteEntity::new(&world, "OP").expect("delete op"));
    h.execute(&world, DeleteEntity::new(&world, "part").expect("delete part"));
    h.undo(&world);
    assert_eq!(hole(&world, "part").as_deref(), Some(HOLE), "an undone delete brings the hole back");
    // A whole mesh in place: the hole and the mark go; undo brings both back.
    h.execute(&world, UpdateModel::new(&world, "part", brick()).expect("update"));
    assert_eq!(hole(&world, "part"), None);
    assert_eq!(model::find_by_name(&world, "part").and_then(|e| model_fidelity::chord_of(&world, &e)), None);
    h.undo(&world);
    assert_eq!(hole(&world, "part").as_deref(), Some(HOLE), "an undone update brings the hole back");
    // An incomplete mesh in place lands its own hole.
    let other = "closed shell #9 (4 faces) meshes open";
    h.execute(&world, UpdateModel::new(&world, "part", brick()).expect("update").with_fidelity(Some(model_fidelity::UNKNOWN_MM)).with_hole(Some(other.into())));
    assert_eq!(hole(&world, "part").as_deref(), Some(other));
}

#[test]
fn the_hole_rides_the_project_file_and_is_refused_without_its_mark() {
    let (world, _h) = faced_part();
    let dir = std::env::temp_dir();
    let file = |tag: &str| dir.join(format!("camcraft-1610-{tag}-{}.camproj", std::process::id()));
    let (a, b, c) = (file("a"), file("b"), file("c"));
    project::save(&world, &a).expect("save");
    let text = std::fs::read_to_string(&a).expect("read");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    let row = doc["entities"].as_array().and_then(|r| r.iter().find(|e| e["name"] == "part")).cloned().expect("part row");
    assert_eq!(row["mesh_open"], serde_json::json!(HOLE), "{row}");
    assert_eq!(row["fidelity_unknown"], serde_json::json!(true), "{row}");
    let loaded = new_world();
    project::load(&loaded, &a).expect("load");
    assert_eq!(hole(&loaded, "part").as_deref(), Some(HOLE));
    project::save(&loaded, &b).expect("save again");
    assert_eq!(text, std::fs::read_to_string(&b).expect("read"), "byte for byte");
    // A hole without the mark is corrupt, never a whole model with a note.
    let bad = text.replacen("\"fidelity_unknown\": true", "\"fidelity_unknown\": false", 1);
    assert_ne!(bad, text, "file layout changed");
    std::fs::write(&c, bad).expect("write");
    let err = project::load(&new_world(), &c).expect_err("a hole without its mark is corrupt");
    assert!(err.to_string().contains("names a mesh hole but is not a model marked fidelity unknown"), "{err}");
    for f in [a, b, c] {
        let _ = std::fs::remove_file(f);
    }
}
