//! Document-open regressions for #978, through the real MCP/script doors.
//! Provenance: the documented replacement contract and our own box/Face fixture.

use super::{Client, tmp_dir, write_box_stl};
use serde_json::{Value, json};
use std::path::Path;

fn face_job(dir: &Path) -> Client {
    let stl = dir.join("block.stl");
    write_box_stl(&stl);
    let mut c = Client::new();
    let imported = c.ok("import_model", json!({"path": stl}));
    assert_eq!(imported["facets"], json!(12));
    c.ok("set_stock", json!({"box": {"dx": 60.0, "dy": 60.0, "dz": 20.0}}));
    c.ok("add_tool", json!({"catalog": "EM-0400-2FL"}));
    c.ok("add_operation", json!({"kind": "Face", "tool": "EM-0400-2FL"}));
    let generated = c.ok("generate", json!({}));
    assert_eq!(generated["generated"].as_array().expect("generated list").len(), 1);
    assert_eq!(generated["generated"][0]["status"], json!("ok"));
    let job = c.ok("get_job", json!({}));
    assert_eq!(job["operations"].as_array().expect("operations").len(), 1);
    assert_eq!(job["operations"][0]["key"], json!("Face1"));
    assert_eq!(job["operations"][0]["status"], json!("ok"));
    assert_eq!(job["can_undo"], json!(true));
    c
}

fn post(c: &mut Client, path: &Path) -> String {
    let posted = c.ok("post", json!({"output": path, "post": "Fanuc"}));
    assert_eq!(posted["ops"], json!(1));
    let text = std::fs::read_to_string(path).expect("posted NC");
    // Fanuc's onSection emits this preamble exactly once per section.
    assert_eq!(text.matches("G90 G54 G40 G49 G80").count(), 1, "{text}");
    text
}

fn read_project(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).expect("project file")).expect("project JSON")
}

#[test]
fn opening_a_saved_job_replaces_operations_and_posts_each_section_once() {
    let dir = tmp_dir("open-replaces");
    let mut c = face_job(&dir);
    let path = dir.join("job.camproj");
    let nc = dir.join("job.nc");
    let before = post(&mut c, &nc);
    c.ok("save_project", json!({"path": path}));
    let saved = read_project(&path);
    let file_ops: Vec<_> = saved["entities"].as_array().expect("entities").iter()
        .filter(|e| e["kind"] == json!("operation")).collect();
    assert_eq!(file_ops.len(), 1, "the file itself has only one operation");
    assert!(file_ops[0]["operation"]["cache"].is_object(), "a generated Face was saved");

    for _ in 0..2 {
        assert_eq!(c.ok("get_job", json!({}))["operations"].as_array().expect("live operations").len(), 1,
            "open is exercised over a non-empty document, including a repeated open");
        let opened = c.ok("open_project", json!({"path": path}));
        assert_eq!(opened["operations"].as_array().expect("opened operations").len(), 1, "{opened}");
        let job = c.ok("get_job", json!({}));
        assert_eq!(job["operations"].as_array().expect("job operations").len(), 1, "{job}");
        assert_eq!(job["operations"][0]["key"], json!("Face1"));
        assert_eq!(job["operations"][0]["status"], json!("ok"));
        assert_eq!(job["can_undo"], json!(false));
        assert_eq!(job["can_redo"], json!(false));
        c.refused("undo", json!({}));
        c.refused("redo", json!({}));
        assert_eq!(post(&mut c, &nc), before, "the program is not cut twice");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn opening_an_empty_project_removes_the_old_document_and_redo() {
    let dir = tmp_dir("open-empty");
    let path = dir.join("empty.camproj");
    let mut empty = Client::new();
    empty.ok("save_project", json!({"path": path}));
    let expected = empty.ok("get_job", json!({}));
    let saved = read_project(&path);
    let mut c = face_job(&dir);
    c.ok("edit_operation", json!({"name": "Face1", "params": {"feed": 900.0}}));
    c.ok("undo", json!({}));
    let old = c.ok("get_job", json!({}));
    assert_eq!(old["can_undo"], json!(true));
    assert_eq!(old["can_redo"], json!(true));
    assert!(!old["models"].as_array().expect("old models").is_empty());
    assert!(!old["tools"].as_array().expect("old tools").is_empty());

    c.ok("open_project", json!({"path": path}));
    assert_eq!(c.ok("get_job", json!({})), expected);
    c.refused("undo", json!({}));
    c.refused("redo", json!({}));
    c.ok("save_project", json!({}));
    assert_eq!(read_project(&path), saved, "no old entities or document settings survive");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_failed_open_preserves_the_document_history_and_save_destination() {
    let dir = tmp_dir("open-failed");
    let mut c = face_job(&dir);
    let path = dir.join("current.camproj");
    let nc = dir.join("current.nc");
    let before_nc = post(&mut c, &nc);
    c.ok("edit_operation", json!({"name": "Face1", "params": {"feed": 900.0}}));
    c.ok("undo", json!({}));
    c.ok("save_project", json!({"path": path}));
    let before = c.ok("get_job", json!({}));
    assert_eq!(before["can_undo"], json!(true));
    assert_eq!(before["can_redo"], json!(true));
    let saved = read_project(&path);
    let malformed = dir.join("malformed.camproj");
    std::fs::write(&malformed, "{not JSON").expect("bad JSON fixture");
    let future = dir.join("future.camproj");
    let mut bad = saved.clone();
    bad["version"] = json!(999999);
    std::fs::write(&future, bad.to_string()).expect("future fixture");
    let invalid = dir.join("invalid.camproj");
    bad = saved.clone();
    bad["units"] = json!("nonsense");
    std::fs::write(&invalid, bad.to_string()).expect("invalid fixture");

    for bad_path in [dir.join("missing.camproj"), malformed, future, invalid] {
        let reason = c.refused("open_project", json!({"path": bad_path}));
        assert!(reason.starts_with("open_project:"), "{reason}");
        assert_eq!(c.ok("get_job", json!({})), before, "failed open: {}", bad_path.display());
        assert_eq!(post(&mut c, &nc), before_nc);
        c.ok("save_project", json!({}));
        assert_eq!(read_project(&path), saved, "bare save still targets the old document");
    }
    c.ok("redo", json!({}));
    assert_eq!(c.ok("get_job", json!({}))["operations"][0]["params"]["face"]["feed"], json!(900.0));
    c.ok("undo", json!({}));
    assert_eq!(c.ok("get_job", json!({})), before);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn script_load_replaces_the_document_and_forgets_the_previous_post() {
    let dir = tmp_dir("script-open");
    let path = dir.join("job.camproj");
    let nc = dir.join("job.nc");
    face_job(&dir).ok("save_project", json!({"path": path}));
    let out = camcraft_app::script::run(&format!(
        "cam.load({path}); cam.post({{to: {nc}, cps: 'Fanuc'}}); \
         var previous = cam.report().posted; cam.load({path}); \
         ({{previous: previous, report: cam.report()}})",
        path = json!(path), nc = json!(nc),
    ));
    assert!(out.ok, "{:?}", out.errors);
    assert_eq!(out.result["previous"], json!(nc));
    assert_eq!(out.result["report"]["posted"], Value::Null);
    assert_eq!(out.result["report"]["document"], json!(path));
    assert_eq!(out.result["report"]["operations"].as_array().expect("script operations").len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}
