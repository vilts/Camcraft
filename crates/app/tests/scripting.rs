//! The job-script host (M5.3 F): laws, not plumbing. What must stay
//! true: a script's post is the APP's post (same bytes, not a batch
//! dialect of it); a refusal is always recorded, even caught; the
//! bindings file and the verb table cannot drift; and nothing the
//! engine chokes on ever crashes the process - a bad source comes
//! back as an error line with a place in it.
//!
//! Gate map (M4.4): covers app::script, app::mcp, app::posting, app::project,
//! app::commands, app::operation, app::tool_list, app::tool_library,
//! app::world.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, RenameOperation, SetStock};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::project;
use camcraft_app::script;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp(tag: &str, ext: &str) -> PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-script-{tag}-{}.{}", std::process::id(), ext));
    let _ = std::fs::remove_file(&p);
    p
}

/// The fixture both lanes share: box stock and one clean facing op,
/// the same parameters the posting tests post successfully - so any
/// refusal here is a scripting bug, not a generator one.
fn fixture() -> World {
    let world = new_world();
    let mut history = History::default();
    let cmd = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, cmd.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("catalog tool"));
    let params = OpParams::Face { holes: Default::default(), blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let cmd = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, cmd.expect("create"));
    world
}

fn json_str(p: &Path) -> String {
    json!(&p.display().to_string()).to_string()
}

#[test]
fn an_empty_script_is_a_clean_run() {
    let out = script::run("");
    assert!(out.ok, "{:?}", out.errors);
    assert!(out.result.is_null());
    assert!(out.logs.is_empty() && out.errors.is_empty());
}

#[test]
fn a_syntax_error_refuses_with_a_place_in_it() {
    let out = script::run("var x = @;");
    assert!(!out.ok, "a bad source must refuse");
    let text = out.errors.join("\n");
    assert!(text.contains("line"), "the refusal must say where: {text}");
}

#[test]
fn a_caught_refusal_is_still_recorded() {
    // The law: swallowing an Error must not make the job clean - the
    // batch report is the host's record, not the script's.
    let out = script::run("try { cam.operations(\"extra\") } catch (e) { cam.log(\"got: \" + e.message) }");
    assert!(!out.ok, "{:?}", out.errors);
    assert!(out.errors.iter().any(|l| l.starts_with("operations:")), "{:?}", out.errors);
    assert!(out.logs[0].starts_with("got: operations:"), "{:?}", out.logs);
}

#[test]
fn an_unknown_option_names_the_key_that_typed_wrong() {
    let nc = tmp("opt", "nc");
    let out = script::run(&format!("cam.post({{ to: {}, nosuch: 1 }})", json_str(&nc)));
    assert!(!out.ok);
    assert!(out.errors[0].contains("unknown option \"nosuch\""), "{:?}", out.errors);
    assert!(!nc.exists(), "a refusal writes nothing");
}

#[test]
fn a_post_needs_where_to_put_the_program() {
    let out = script::run("cam.post({})");
    assert!(!out.ok);
    assert!(out.errors[0].contains("\"to\""), "{:?}", out.errors);
}

#[test]
fn a_bare_save_without_a_document_refuses() {
    let out = script::run("cam.save()");
    assert!(!out.ok);
    assert!(out.errors[0].contains("no document"), "{:?}", out.errors);
}

#[test]
fn a_logged_object_reads_as_json() {
    let out = script::run("cam.log(\"a\", { b: 1 }, 2)");
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.logs, vec!["a".to_string(), "{\"b\":1}".to_string(), "2".to_string()]);
}

#[test]
fn cam_result_parks_the_last_verb() {
    let camp = tmp("park", "camp");
    let world = fixture();
    project::save(&world, &camp).expect("save fixture");
    let out = script::run(&format!("cam.load({}); cam.result().operations[0].name", json_str(&camp)));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result, json!("Face1"));
}

#[test]
fn verify_on_an_ungenerated_document_refuses_to_judge() {
    let camp = tmp("never", "camp");
    let world = fixture();
    project::save(&world, &camp).expect("save fixture");
    let out = script::run(&format!("cam.load({}); cam.verify()", json_str(&camp)));
    // verify() is a question, not a refusal: the run stands, and it
    // answers honestly that there is nothing to judge yet.
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["ok"], json!(false), "a stale job is not a passed job");
    let v = out.result.pointer("/gate/skipped").expect("skipped says why");
    assert!(v.is_string(), "{v}");
    assert_eq!(out.result.pointer("/operations/0/status"), Some(&json!("never")));
    // The never-generated program also cannot post, by name.
    let nc = tmp("neverpost", "nc");
    let out = script::run(&format!(
        "cam.load({}); cam.post({{ to: {} }})",
        json_str(&camp),
        json_str(&nc)
    ));
    assert!(!out.ok);
    assert!(out.errors[0].contains("no toolpath yet"), "{:?}", out.errors);
    assert!(!nc.exists());
}

#[test]
fn a_scripts_post_is_the_apps_post_byte_for_byte() {
    // The law of the seam, measured: the batch lane and the UI post
    // the same document with the same bytes, because both call the
    // same door. A drift here means the host grew its own dialect.
    let camp = tmp("same", "camp");
    let world = fixture();
    project::save(&world, &camp).expect("save fixture");

    let nc_script = tmp("same-script", "nc");
    let nc_app = tmp("same-app", "nc");
    let script_src = format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {} }}); cam.report()",
        json_str(&camp),
        json_str(&nc_script)
    );
    let out = script::run(&script_src);
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["operations"][0]["status"], json!("ok"));
    assert_eq!(out.result["posted"], json!(nc_script.display().to_string()));
    assert!(out.result["minutes"].is_number(), "report carries the time law");

    // The app lane: same file, same commands, the UI's own post call.
    let world_b = new_world();
    project::load(&world_b, &camp).expect("load fixture");
    let mut history_b = History::default();
    let cmd = GenerateOperation::new(&world_b, "Face1").expect("generate");
    history_b.execute(&world_b, cmd);
    posting::post_setup(&world_b, &nc_app).expect("app post");

    let a = std::fs::read(&nc_script).expect("script wrote it");
    let b = std::fs::read(&nc_app).expect("app wrote it");
    assert_eq!(a, b, "the batch lane and the app must post one program");
    // The text round trip the script's `program: true` promises:
    let out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {}, program: true }}).program",
        json_str(&camp),
        json_str(&nc_script)
    ));
    assert!(out.ok, "{:?}", out.errors);
    let Value::String(text) = &out.result else { panic!("program text expected: {:?}", out.result) };
    assert_eq!(text.as_bytes(), &a[..]);
}

#[test]
fn named_generate_forces_even_a_fresh_operation() {
    let camp = tmp("force", "camp");
    let world = fixture();
    project::save(&world, &camp).expect("save fixture");
    let out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.generate(\"Face1\").length",
        json_str(&camp)
    ));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result, json!(1), "a named op regenerates whatever its status");
    // An unknown name refuses by name and changes nothing after it.
    let out = script::run(&format!("cam.load({}); cam.generate(\"Ghost1\")", json_str(&camp)));
    assert!(!out.ok);
    assert!(out.errors[0].contains("Ghost1"), "{:?}", out.errors);
}

#[test]
fn scripts_accept_display_names_and_report_stable_keys() {
    let camp = tmp("renamed", "camp");
    let world = fixture();
    let mut history = History::default();
    history.execute(&world, RenameOperation::new(&world, "Face1", "Finish face").expect("rename"));
    project::save(&world, &camp).expect("save fixture");
    let out = script::run(&format!(
        "cam.load({}); [cam.operations()[0], cam.generate(\"Finish face\")[0], cam.verify(\"Finish face\").operations[0]]",
        json_str(&camp)
    ));
    let _ = std::fs::remove_file(&camp);
    assert!(out.ok, "{:?}", out.errors);
    let rows = out.result.as_array().expect("three script views");
    assert_eq!(rows.len(), 3);
    for row in rows {
        assert_eq!(row["name"], json!("Finish face"));
        assert_eq!(row["key"], json!("Face1"));
    }
}

#[test]
fn the_bindings_file_is_the_verb_table() {
    // assets/scripts/cam.d.ts is this generated text, committed; the
    // test is the drift alarm the GENERATED header promises.
    // From the manifest, not the CWD: the law must judge the same
    // file wherever the binary is stood up (gate-ladder lore).
    let committed =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/scripts/cam.d.ts"))
            .expect("bindings file");
    let norm = |s: String| s.replace("\r\n", "\n");
    assert_eq!(norm(script::bindings()), norm(committed), "regenerate cam.d.ts from the VERBS table");
}

#[test]
fn scripts_see_no_world_and_no_side_doors() {
    // The sandbox law: the only door is the verb table.
    for probe in ["typeof console", "typeof fetch", "typeof require", "typeof globalThis.__cam_env"] {
        let out = script::run(probe);
        assert!(out.ok, "{probe}: {:?}", out.errors);
        assert_eq!(out.result, json!("undefined"), "{probe} must not exist");
    }
    // __cam_call exists (the shim needs it) but is off the typed
    // surface: calling the raw door with junk refuses, never panics.
    let out = script::run("var e = JSON.parse(String(__cam_call(1, 2))); e.ok ? \"accepted\" : \"refused\"");
    assert!(out.ok, "the door must answer, not crash: {:?}", out.errors);
    assert_eq!(out.result, json!("refused"));
}

#[test]
fn a_post_with_a_named_builtin_shapes_the_program() {
    let camp = tmp("cps", "camp");
    let world = fixture();
    project::save(&world, &camp).expect("save fixture");
    let nc = tmp("cps", "nc");
    let out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {}, cps: \"Haas\" }}).lines",
        json_str(&camp),
        json_str(&nc)
    ));
    assert!(out.ok, "{:?}", out.errors);
    assert!(out.result.as_u64().unwrap_or(0) > 0);
    // A stranger's name is a refusal that lists the registry, never
    // a silent fall to the default.
    let out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {}, cps: \"Acme2000\" }})",
        json_str(&camp),
        json_str(&nc)
    ));
    assert!(!out.ok);
    assert!(out.errors[0].contains("Acme2000") && out.errors[0].contains("Fanuc"), "{:?}", out.errors);
}

#[test]
fn arity_is_a_law_not_a_suggestion() {
    let out = script::run("cam.report(\"extra\")");
    assert!(!out.ok && out.errors[0].contains("takes 0 argument"), "{:?}", out.errors);
    let out = script::run("cam.load()");
    assert!(!out.ok && out.errors[0].contains("takes 1 argument"), "{:?}", out.errors);
}

#[test]
fn a_nonzero_exit_shape_survives_a_late_throw() {
    // Verbs that already ran stay recorded; the uncaught error leads.
    let out = script::run("cam.log(\"one\"); cam.operations(\"x\"); null");
    assert!(!out.ok);
    assert_eq!(out.logs, vec!["one".to_string()]);
    assert!(out.errors.iter().any(|e| e.starts_with("operations:")));
}

#[test]
fn script_tool_library_verbs_roundtrip() {
    // #355: the batch lane's exportTools / importTools are the MCP
    // tools' verbs - one meaning per verb across both doors.
    let world = fixture();
    let camp = tmp("toollib", "camproj");
    let lib = tmp("toollib", "camtools");
    project::save(&world, &camp).expect("save fixture");
    let out = script::run(&format!(
        "cam.load({0}); var e = cam.exportTools({1}); var dup = null; \
         try {{ cam.importTools({1}); }} catch (err) {{ dup = String(err.message); }} [e.tools, dup]",
        json_str(&camp),
        json_str(&lib)
    ));
    let _ = std::fs::remove_file(&camp);
    assert_eq!(out.result[0], json!(1), "{:?}", out.errors);
    assert!(out.result[1].as_str().is_some_and(|m| m.contains("nothing to import")), "{:?}", out.result);
    // A fresh run imports it as one tool.
    let out = script::run(&format!("cam.importTools({})", json_str(&lib)));
    let _ = std::fs::remove_file(&lib);
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["added"], json!(["EM-0400-2FL"]));
}

#[test]
fn cam_post_with_per_op_writes_standalone_files() {
    use camcraft_app::commands::{CreateSetup, SetPostOptions};
    use camcraft_app::schema::PostOptions;

    let world = fixture();
    let mut history = History::default();
    // Add Setup1 (which resolves Face1) and Setup2 with a second face op, then enable per_op.
    history.execute(&world, CreateSetup::new(&world, None, None).expect("Setup1"));
    history.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("Setup2 stock"));
    let params = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let op2 = CreateOperation::new(&world, "Face2", "EM-0400-2FL", SrcRef::Stock, params);
    history.execute(&world, op2.expect("create Face2"));
    history.execute(
        &world,
        SetPostOptions::new(&world, PostOptions { per_op: true, ..Default::default() }).expect("per_op"),
    );

    let dir = std::env::temp_dir().join(format!("camcraft-script-perop-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let camp = dir.join("job.camproj");
    project::save(&world, &camp).expect("save fixture");
    let nc = dir.join("perop.nc");

    let out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {}, cps: \"Fanuc\", program: true }})",
        json_str(&camp),
        json_str(&nc)
    ));
    assert!(out.ok, "{:?}", out.errors);
    assert!(!nc.exists(), "must not write combined file {}", nc.display());
    let files = out.result["files"].as_array().expect("files array");
    assert_eq!(files.len(), 2, "{}", out.result);
    assert_eq!(out.result["ops"], json!(2), "{}", out.result);
    let p1 = PathBuf::from(files[0].as_str().expect("p1"));
    let p2 = PathBuf::from(files[1].as_str().expect("p2"));
    assert!(p1.ends_with("perop_01_Face1.nc") && p2.ends_with("perop_02_Face2.nc"), "{files:?}");
    let t1 = std::fs::read_to_string(&p1).expect("read p1");
    let t2 = std::fs::read_to_string(&p2).expect("read p2");
    assert!(t1.contains("Face1") && !t1.contains("Face2") && t1.contains("M30"), "{t1}");
    assert!(t2.contains("Face2") && !t2.contains("Face1") && t2.contains("M30"), "{t2}");
    assert_eq!(out.result["operations"][0]["program"], json!(t1), "{}", out.result);
    assert_eq!(out.result["operations"][1]["program"], json!(t2), "{}", out.result);

    // Single-op per_op still writes <stem>_01_<op>.nc and populates files + program.
    let single_world = fixture();
    let mut sh = History::default();
    sh.execute(
        &single_world,
        SetPostOptions::new(&single_world, PostOptions { per_op: true, ..Default::default() }).expect("per_op"),
    );
    let single_camp = dir.join("single.camproj");
    project::save(&single_world, &single_camp).expect("save single");
    let single_nc = dir.join("single.nc");
    let single_out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {}, program: true }})",
        json_str(&single_camp),
        json_str(&single_nc)
    ));
    assert!(single_out.ok, "{:?}", single_out.errors);
    assert!(!single_nc.exists(), "single-op per_op writes numbered per-op file");
    let sfiles = single_out.result["files"].as_array().expect("single files");
    assert_eq!(sfiles.len(), 1, "{}", single_out.result);
    let sp1 = PathBuf::from(sfiles[0].as_str().expect("sp1"));
    assert!(sp1.ends_with("single_01_Face1.nc"), "{sfiles:?}");
    assert_eq!(single_out.result["program"], json!(std::fs::read_to_string(&sp1).expect("read sp1")));

    // Directory target and trailing slash write inside that folder; empty or missing parent refuses.
    let folder = dir.join("out_folder");
    std::fs::create_dir_all(&folder).expect("out_folder");
    let dir_out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {} }})",
        json_str(&single_camp),
        json_str(&folder)
    ));
    assert!(dir_out.ok, "{:?}", dir_out.errors);
    assert!(folder.join("program_01_Face1.nc").exists(), "{:?}", dir_out.result);

    let empty_out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: \"\" }})",
        json_str(&single_camp)
    ));
    assert!(!empty_out.ok && empty_out.errors[0].contains("\"to\""), "{:?}", empty_out.errors);

    let missing_out = script::run(&format!(
        "cam.load({}); cam.generate(); cam.post({{ to: {} }})",
        json_str(&single_camp),
        json_str(&dir.join("missing_sub").join("perop.nc"))
    ));
    assert!(!missing_out.ok && missing_out.errors[0].contains("not a folder"), "{:?}", missing_out.errors);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cam_call_builds_headlessly_and_records_caught_refusals() {
    let dir = std::env::temp_dir().join(format!("camcraft-script-call-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let proj = dir.join("built.camproj");
    let nc = dir.join("built.nc");
    let src = format!(
        "cam.call('set_stock', {{ box: {{ dx: 60, dy: 60, dz: 20 }} }});\n\
         cam.call('add_tool', {{ catalog: 'EM-0400-2FL' }});\n\
         cam.call('add_operation', {{ kind: 'Face', tool: 'EM-0400-2FL', source: 'stock', params: {{ stepover: 3, clearance_z: {{ reference: 'top_of_stock', offset: 25 }}, cut_z: {{ reference: 'top_of_stock', offset: -0.5 }}, feed: 1200, plunge_feed: 300, climb: true }} }});\n\
         cam.generate();\n\
         var v = cam.verify();\n\
         cam.save({});\n\
         cam.post({{ to: {} }});\n\
         v",
        json_str(&proj),
        json_str(&nc)
    );
    let out = script::run(&src);
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["ok"], json!(true), "{}", out.result);
    assert!(proj.is_file() && nc.is_file(), "cam.call script writes project and NC");

    let bad = script::run("try { cam.call('no_such_tool', {}); } catch (e) { cam.log(e.message); }");
    assert!(!bad.ok, "caught cam.call refusal must still fail the run");
    assert!(bad.errors.iter().any(|l| l.contains("unknown tool 'no_such_tool'")), "{:?}", bad.errors);
    let _ = std::fs::remove_dir_all(&dir);
}
