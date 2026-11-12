//! #1054: the STEP facet budget is a setting carried with the import. The
//! default imports exactly as before; a lowered budget flags reduced
//! fidelity (never a silent pass), a raised one meshes finer and clears
//! it; out-of-bounds budgets are refused by name over the command, the
//! script verb and MCP; the budget lands with the model through undo,
//! delete and the project file, and an older file loads at the default.
//!
//! Gate map (#1054): covers app::geometry::import::step_budget,
//! app::commands (import_model, update_model, delete_entity),
//! app::model_fidelity (budget_of, set_budget), app::project (the
//! `facet_budget` key), app::script (cam.import), app::mcp (import_model,
//! get_job).
//!
//! Provenance: our own pins over our own setting; no vendor material.

use camcraft_app::commands::{DeleteEntity, History, ImportModel, UpdateModel};
use camcraft_app::geometry::import::{step, step_budget};
use camcraft_app::mcp::Server;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_fidelity, project, script};
use flecs_ecs::prelude::World;
use serde_json::{json, Value};

const LOW: u64 = 2_000;
/// Far above the fixture's full-fidelity mesh, still under the default.
const RAISED: u64 = 5_000_000;

fn models(n: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(n)
}

fn step_path() -> std::path::PathBuf {
    models("Milling-test.step")
}

fn budget(w: &World, name: &str) -> Option<u64> {
    model::find_by_name(w, name).and_then(|e| model_fidelity::budget_of(w, &e))
}

fn chord(w: &World, name: &str) -> Option<f64> {
    model::find_by_name(w, name).and_then(|e| model_fidelity::chord_of(w, &e))
}

#[test]
fn the_default_budget_imports_exactly_as_before() {
    assert_eq!(step_budget::DEFAULT, 30_000_000, "the default is the cap every import ran under");
    let plain = ImportModel::new(&step_path()).expect("imports");
    let stated = ImportModel::within(&step_path(), Some(step_budget::DEFAULT)).expect("imports");
    assert_eq!(plain.mesh.facets, stated.mesh.facets, "the same mesh, facet for facet");
    assert_eq!((plain.coarse_mm, stated.coarse_mm), (None, None));
    assert_eq!((plain.facet_budget, stated.facet_budget), (None, None), "the default records nothing");
}

#[test]
fn a_lowered_budget_flags_and_a_raised_one_clears_through_undo_and_delete() {
    let world = new_world();
    let mut h = History::default();
    let low = ImportModel::within(&step_path(), Some(LOW)).expect("imports at reduced fidelity");
    let low_chord = low.coarse_mm.expect("a smaller budget flags, never passes silently");
    assert!(low_chord > step::CHORD_CAP, "{low_chord}");
    assert_eq!(low.facet_budget, Some(LOW));
    let low_facets = low.mesh.facets.len();
    h.execute(&world, low.renamed("part"));
    assert_eq!((budget(&world, "part"), chord(&world, "part")), (Some(LOW), Some(low_chord)));

    // The same file again, under a raised budget: finer, and full fidelity.
    let high = ImportModel::within(&step_path(), Some(RAISED)).expect("imports");
    assert!(high.mesh.facets.len() > low_facets, "a raised budget meshes finer");
    assert_eq!(high.coarse_mm, None, "and clears the flag");
    let upd = UpdateModel::new(&world, "part", high.mesh).expect("update").with_fidelity(high.coarse_mm).with_budget(high.facet_budget);
    h.execute(&world, upd);
    assert_eq!((budget(&world, "part"), chord(&world, "part")), (Some(RAISED), None));
    // Undo trades the budget back with the facets; redo forward again.
    h.undo(&world);
    assert_eq!((budget(&world, "part"), chord(&world, "part")), (Some(LOW), Some(low_chord)));
    h.redo(&world);
    assert_eq!(budget(&world, "part"), Some(RAISED));
    // A delete's undo brings the record back.
    h.execute(&world, DeleteEntity::new(&world, "part").expect("delete"));
    assert_eq!(budget(&world, "part"), None);
    h.undo(&world);
    assert_eq!(budget(&world, "part"), Some(RAISED));
}

#[test]
fn an_out_of_bounds_budget_is_refused_by_name_before_parsing() {
    for bad in [0, step_budget::MIN - 1, step_budget::CEILING + 1] {
        let err = ImportModel::within(&step_path(), Some(bad)).err().expect("refused").to_string();
        assert!(err.contains("STEP facet budget") && err.contains(&bad.to_string()), "{err}");
    }
    // A budget for a mesh file would be honoured by nothing: refused too.
    let err = ImportModel::within(&models("cube.stl"), Some(LOW)).err().expect("refused").to_string();
    assert!(err.contains("STEP facet budget") && err.contains("STEP files only"), "{err}");
}

#[test]
fn the_script_verb_takes_the_budget_and_refuses_one_out_of_bounds() {
    let path = json!(step_path().display().to_string()).to_string();
    let out = script::run(&format!("cam.import({path}, null, {LOW})"));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["model"], json!("Milling-test"));
    assert_eq!(out.result["facet_budget"], json!(LOW));
    assert!(out.result["reduced_fidelity"]["approximation_mm"].as_f64().is_some_and(|mm| mm > step::CHORD_CAP), "{}", out.result);

    let out = script::run(&format!("cam.import({path}, \"m\", {RAISED})"));
    assert!(out.ok, "{:?}", out.errors);
    assert!(out.result.get("reduced_fidelity").is_none(), "{}", out.result);

    for bad in ["10", "1e12", "2000.5", "\"2000\""] {
        let out = script::run(&format!("cam.import({path}, \"m\", {bad})"));
        assert!(!out.ok, "{bad}");
        assert!(out.errors[0].contains("STEP facet budget"), "{bad}: {:?}", out.errors);
    }
}

struct Client {
    server: Server,
    next: i64,
}

impl Client {
    fn call(&mut self, name: &str, args: Value) -> Value {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
        self.server.handle(&msg).expect("a request gets a response")["result"].clone()
    }

    fn ok(&mut self, name: &str, args: Value) -> Value {
        let result = self.call(name, args);
        assert!(result["isError"] != json!(true), "{name} refused: {result}");
        result["structuredContent"].clone()
    }
}

#[test]
fn mcp_import_model_takes_the_budget_and_get_job_reports_it() {
    let path = step_path().to_string_lossy().into_owned();
    let mut c = Client { server: Server::new(), next: 1 };
    let low = c.ok("import_model", json!({ "path": path, "name": "part", "facet_budget": LOW }));
    assert_eq!(low["facet_budget"], json!(LOW));
    assert!(low["reduced_fidelity"]["approximation_mm"].as_f64().is_some_and(|mm| mm > step::CHORD_CAP), "{low}");
    let note = low["reduced_fidelity"]["note"].as_str().expect("a sentence");
    assert!(note.contains("larger STEP facet budget"), "the refusal names a setting that exists: {note}");

    // Re-imported under a raised budget, the same model clears.
    let high = c.ok("import_model", json!({ "path": path, "name": "part", "facet_budget": RAISED }));
    assert!(high.get("reduced_fidelity").is_none(), "{high}");
    let job = c.ok("get_job", json!({}));
    let part = job["models"].as_array().and_then(|m| m.iter().find(|m| m["name"] == json!("part"))).expect("listed");
    assert_eq!(part["facet_budget"], json!(RAISED));

    for bad in [json!(5), json!(step_budget::CEILING + 1), json!(-1), json!(2000.5), json!("2000")] {
        let r = c.call("import_model", json!({ "path": path, "name": "x", "facet_budget": bad }));
        assert_eq!(r["isError"], json!(true), "{bad}: {r}");
        assert!(r.to_string().contains("STEP facet budget"), "{bad}: {r}");
    }
}

#[test]
fn the_budget_round_trips_through_the_project_and_an_older_file_reads_the_default() {
    let dir = std::env::temp_dir();
    let file = |tag: &str| dir.join(format!("camcraft-1054-{tag}-{}.camproj", std::process::id()));
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::within(&step_path(), Some(LOW)).expect("imports").renamed("low"));
    h.execute(&world, ImportModel::new(&step_path()).expect("imports").renamed("plain"));
    let (a, b) = (file("a"), file("b"));
    project::save(&world, &a).expect("save");
    let text = std::fs::read_to_string(&a).expect("read");
    assert_eq!(text.matches("\"facet_budget\"").count(), 1, "only the non-default import records a budget");
    let loaded = new_world();
    project::load(&loaded, &a).expect("load");
    assert_eq!((budget(&loaded, "low"), budget(&loaded, "plain")), (Some(LOW), None));
    assert_eq!(chord(&loaded, "low"), chord(&world, "low"), "the flag rides beside it, unchanged");
    project::save(&loaded, &b).expect("save again");
    assert_eq!(text, std::fs::read_to_string(&b).expect("read"), "saved back unchanged");

    // A hand-edited budget out of bounds is refused by name, not loaded.
    let bad = text.replace(&format!("\"facet_budget\": {LOW}"), "\"facet_budget\": 7");
    assert_ne!(bad, text, "the key is written as expected");
    std::fs::write(&b, bad).expect("write");
    let err = project::load(&new_world(), &b).expect_err("refused").to_string();
    assert!(err.contains("STEP facet budget 7"), "{err}");
    for p in [a, b] {
        let _ = std::fs::remove_file(p);
    }
}
