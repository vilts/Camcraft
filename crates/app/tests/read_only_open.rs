//! Pins for #3609: a project open read-only refuses a save over its own
//! file through the MCP tool `save_project`, the script call `cam.save` and
//! the live seam of a window started with `--read-only`. The file stays
//! byte for byte. A save to a different file works and ends read-only.
//!
//! Gate map: covers app::project (read_only), app::mcp (tools, live),
//! app::script (api).
//! Provenance: our own tests.

use camcraft_app::commands::History;
use camcraft_app::mcp::{answer_live_with, LiveGate, LiveOptions, Server};
use camcraft_app::project::refuse_read_only_save as refuse_save;
use camcraft_app::world::new_world;

use serde_json::{json, Value};
use std::path::{Path, PathBuf};

fn tmp_dir(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("camcraft-ro-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("temp dir");
    p
}

/// One `tools/call`: (isError, structuredContent or the refusal text).
fn call(server: &mut Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    let body = if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() };
    (is_err, body)
}

fn ok(server: &mut Server, name: &str, args: Value) -> Value {
    let (err, body) = call(server, name, args);
    assert!(!err, "{name} refused: {body}");
    body
}

fn refused(server: &mut Server, name: &str, args: Value) -> String {
    let (err, body) = call(server, name, args);
    assert!(err, "{name} should have refused, got {body}");
    body.as_str().unwrap_or_default().to_owned()
}

/// An empty project saved at `dir/job.camproj`, and its bytes.
fn saved_project(dir: &Path) -> (PathBuf, Vec<u8>) {
    let path = dir.join("job.camproj");
    ok(&mut Server::new(), "save_project", json!({ "path": path }));
    let bytes = std::fs::read(&path).expect("project file");
    (path, bytes)
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).expect("project file")
}

#[test]
fn the_rule_refuses_only_the_opened_file_of_a_read_only_project() {
    let dir = tmp_dir("rule");
    let (path, _) = saved_project(&dir);
    let other = dir.join("other.camproj");
    let why = refuse_save(Some(path.as_path()), true, &path).expect_err("the opened file is refused");
    assert!(why.contains("is open read-only"), "{why}");
    assert!(refuse_save(Some(path.as_path()), false, &path).is_ok(), "not read-only");
    assert!(refuse_save(Some(path.as_path()), true, &other).is_ok(), "another file: Save As");
    assert!(refuse_save(None, true, &path).is_ok(), "the document has no file");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_read_only_project_refuses_a_save_over_its_file_and_keeps_it() {
    let dir = tmp_dir("refuse");
    let (path, before) = saved_project(&dir);
    let mut s = Server::new();
    let opened = ok(&mut s, "open_project", json!({ "path": path, "read_only": true }));
    assert_eq!(opened["read_only"], json!(true), "{opened}");
    // A change is allowed in memory.
    ok(&mut s, "set_units", json!({ "unit": "inch" }));
    // The bare save, the same path, and another spelling of that path.
    std::fs::create_dir_all(dir.join("sub")).expect("sub folder");
    let other_spelling = dir.join("sub").join("..").join("job.camproj");
    for args in [json!({}), json!({ "path": path }), json!({ "path": other_spelling })] {
        let why = refused(&mut s, "save_project", args.clone());
        assert!(why.starts_with("save_project: "), "{args}: {why}");
        assert!(why.contains("is open read-only"), "{args}: {why}");
        assert_eq!(read(&path), before, "{args}: the file is unchanged");
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_save_to_another_file_works_and_ends_read_only() {
    let dir = tmp_dir("save-as");
    let (path, before) = saved_project(&dir);
    let copy = dir.join("copy.camproj");
    let mut s = Server::new();
    ok(&mut s, "open_project", json!({ "path": path, "read_only": true }));
    ok(&mut s, "set_units", json!({ "unit": "inch" }));
    ok(&mut s, "save_project", json!({ "path": copy }));
    let inch = read(&copy);
    // The bare save now writes the copy, which is not read-only.
    ok(&mut s, "set_units", json!({ "unit": "mm" }));
    ok(&mut s, "save_project", json!({}));
    assert_ne!(read(&copy), inch, "the bare save wrote the copy");
    assert_eq!(read(&path), before, "the opened file stays as it was");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_plain_open_ends_read_only_and_a_bad_flag_changes_nothing() {
    let dir = tmp_dir("plain");
    let (path, _) = saved_project(&dir);
    let mut s = Server::new();
    ok(&mut s, "open_project", json!({ "path": path, "read_only": true }));
    let why = refused(&mut s, "open_project", json!({ "path": path, "read_only": "yes" }));
    assert!(why.contains("\"read_only\" must be true or false"), "{why}");
    refused(&mut s, "save_project", json!({}));
    let opened = ok(&mut s, "open_project", json!({ "path": path }));
    assert!(opened.get("read_only").is_none(), "{opened}");
    ok(&mut s, "set_units", json!({ "unit": "inch" }));
    ok(&mut s, "save_project", json!({}));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_script_save_after_a_read_only_open_is_refused() {
    let dir = tmp_dir("script");
    let (path, before) = saved_project(&dir);
    let out = camcraft_app::script::run(&format!(
        "cam.call('open_project', {{path: {p}, read_only: true}}); \
         var why = ''; try {{ cam.save(); }} catch (e) {{ why = String(e); }} why",
        p = json!(path),
    ));
    let why = out.result.as_str().unwrap_or_default().to_owned();
    let errors = out.errors;
    let after = read(&path);
    let _ = std::fs::remove_dir_all(dir);
    assert!(why.contains("is open read-only"), "{why} / {errors:?}");
    assert!(errors.iter().any(|e| e.starts_with("save: ") && e.contains("is open read-only")), "{errors:?}");
    assert_eq!(after, before, "the file is unchanged");
}

#[test]
fn the_live_seam_refuses_an_approved_save_over_a_read_only_window_project() {
    let dir = tmp_dir("live");
    let (path, before) = saved_project(&dir);
    let world = new_world();
    let mut history = History::default();
    // One live call on the window's document, approved by the user.
    let mut live = |name: &str, args: Value, read_only: bool| -> Value {
        let line = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "arguments": args.clone() } })
        .to_string();
        let options = LiveOptions { gate: LiveGate::allow_once(name, args), selection: None, read_only, doc_stamp: None };
        let answer = answer_live_with(&world, &mut history, Some(path.as_path()), &line, &options);
        serde_json::from_str(&answer.reply.expect("a reply")).expect("json")
    };
    let changed = live("set_units", json!({ "unit": "inch" }), true);
    let refused = live("save_project", json!({}), true);
    let kept = read(&path);
    // The same approved call saves when the window is not read-only, so
    // the refusal is the read-only rule, not the approval gate.
    let saved = live("save_project", json!({}), false);
    let after = read(&path);
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(changed["result"]["isError"], json!(false), "{changed}");
    assert_eq!(refused["result"]["isError"], json!(true), "{refused}");
    let text = refused["result"]["content"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("is open read-only"), "{refused}");
    assert_eq!(kept, before, "the refused save wrote nothing");
    assert_eq!(saved["result"]["isError"], json!(false), "{saved}");
    assert_ne!(after, before, "the save that was not refused wrote the change");
}
