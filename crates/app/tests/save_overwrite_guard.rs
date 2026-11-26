//! Pins for #3608: a save over the project file that the document was
//! loaded from or last saved to is refused, by name, when that file on disk
//! changed after the session opened or saved it - from a script (`cam.save`), from
//! the headless MCP tool `save_project`, and from an agent attached to the
//! window (the live seam takes the window's stamp in and hands the new one
//! back). `overwrite: true` replaces the file. A session's own save is the
//! new stamp, so the next save is not refused. A save to another path, and
//! a save over a file deleted since, are not refused.
//!
//! Gate map (M4.4): covers app::project (disk_stamp), app::script (save_guard, api),
//! app::mcp (live, tools).

use camcraft_app::commands::History;
use camcraft_app::mcp::{LiveGate, LiveOptions, Server, answer_live_with};
use camcraft_app::project;
use camcraft_app::project::DiskStamp;
use camcraft_app::script;
use camcraft_app::world::new_world;

use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// What another program writes over the project: a different size, so the
/// stamp differs on every file system, whatever its clock resolution.
const FOREIGN: &str = "{\"written by\": \"another program\"}";

fn dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("camcraft-stamp-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).expect("read the project file")
}

/// The file holds a project again, not the other program's text.
fn is_project(path: &Path) -> bool {
    project::open(&new_world(), &read(path)).is_ok()
}

fn changed_refusal(text: &str) -> bool {
    text.contains("the file on disk changed after this session opened or saved it")
        && text.contains("overwrite: true")
}

/// One headless `tools/call`: the structured answer, or the error text.
fn call(server: &mut Server, name: &str, args: Value) -> Result<Value, String> {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a reply");
    let result = &reply["result"];
    if result["isError"] == json!(true) {
        Err(result["content"][0]["text"]
            .as_str()
            .unwrap_or_default()
            .to_owned())
    } else {
        Ok(result["structuredContent"].clone())
    }
}

#[test]
fn a_stamp_sees_a_foreign_write_but_not_a_missing_file_or_another_path() {
    let d = dir("unit");
    let path = d.join("job.camproj");
    assert!(DiskStamp::of(&path).is_none(), "no file, no stamp");
    std::fs::write(&path, "{}").expect("write");
    let stamp = DiskStamp::of(&path).expect("a stamp");
    assert_eq!(stamp.path(), path.as_path());
    assert!(!stamp.changed(&path), "nothing wrote the file");
    assert!(
        !stamp.changed(&d.join("other.camproj")),
        "another path is not this stamp's file"
    );
    std::fs::write(&path, FOREIGN).expect("write");
    assert!(stamp.changed(&path), "another size");
    std::fs::remove_file(&path).expect("remove");
    assert!(!stamp.changed(&path), "a file that is gone has not changed");
    assert!(
        DiskStamp::of(&d).is_none(),
        "a folder is not a project file"
    );
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_mcp_save_over_a_changed_file_is_refused_until_overwrite() {
    let d = dir("mcp");
    let path = d.join("job.camproj");
    let p = path.display().to_string();
    let mut s = Server::new();
    call(&mut s, "save_project", json!({ "path": p })).expect("a new file");
    std::fs::write(&path, FOREIGN).expect("another program writes the file");

    for args in [
        json!({}),
        json!({ "path": p }),
        json!({ "overwrite": false }),
    ] {
        let e = call(&mut s, "save_project", args.clone()).expect_err("refused");
        assert!(
            e.starts_with("save_project: ") && e.contains(&p) && changed_refusal(&e),
            "{args}: {e}"
        );
        assert_eq!(read(&path), FOREIGN, "{args}: nothing was written");
    }
    call(&mut s, "save_project", json!({ "overwrite": true }))
        .expect("overwrite replaces the file");
    assert!(is_project(&path));
    // The session's own save is the new stamp: no false alarm.
    call(&mut s, "save_project", json!({})).expect("a save after our own save");
    call(&mut s, "save_project", json!({ "path": p })).expect("and again, by path");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_mcp_open_stamps_the_file() {
    let d = dir("open");
    let path = d.join("job.camproj");
    project::save(&new_world(), &path).expect("a project on disk");
    let mut s = Server::new();
    call(
        &mut s,
        "open_project",
        json!({ "path": path.display().to_string() }),
    )
    .expect("open");
    std::fs::write(&path, FOREIGN).expect("another program writes the file");
    let e = call(&mut s, "save_project", json!({})).expect_err("refused");
    assert!(changed_refusal(&e), "{e}");
    assert_eq!(read(&path), FOREIGN, "nothing was written");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_save_to_another_path_or_over_a_deleted_file_is_not_refused() {
    let d = dir("other");
    let path = d.join("job.camproj");
    let copy = d.join("copy.camproj");
    project::save(&new_world(), &path).expect("a project on disk");
    let mut s = Server::new();
    call(
        &mut s,
        "open_project",
        json!({ "path": path.display().to_string() }),
    )
    .expect("open");
    std::fs::write(&path, FOREIGN).expect("another program writes the file");
    call(
        &mut s,
        "save_project",
        json!({ "path": copy.display().to_string() }),
    )
    .expect("another path");
    assert_eq!(read(&path), FOREIGN, "the changed file is kept");
    // The copy is now the document's file. Deleted, it is put back.
    std::fs::remove_file(&copy).expect("remove");
    call(&mut s, "save_project", json!({})).expect("a deleted file is saved again");
    assert!(is_project(&copy));
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_bad_save_option_is_refused_by_name_and_writes_nothing() {
    let d = dir("bad");
    let path = d.join("job.camproj");
    let p = path.display().to_string();
    let mut s = Server::new();
    let e = call(
        &mut s,
        "save_project",
        json!({ "path": p, "overwrite": "yes" }),
    )
    .expect_err("refused");
    assert!(e.contains("\"overwrite\" must be true or false"), "{e}");
    let out = script::run(&format!("cam.save({}, {{ force: true }})", json!(p)));
    assert!(
        out.errors
            .iter()
            .any(|e| e.contains("unknown option \"force\"")),
        "{:?}",
        out.errors
    );
    let out = script::run(&format!("cam.save({}, {{}}, 3)", json!(p)));
    assert!(
        out.errors
            .iter()
            .any(|e| e.contains("save takes a path and one options object, got 3 arguments")),
        "{:?}",
        out.errors
    );
    assert!(!path.exists(), "nothing was written");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn a_script_save_over_a_changed_file_is_refused_until_overwrite() {
    let d = dir("script");
    let path = d.join("job.camproj");
    // A project with a tool, so that the job can write over the file with
    // exportTools: a job writes files only through its verbs, so that verb
    // stands in for another program.
    let mut s = Server::new();
    call(&mut s, "add_tool", json!({ "catalog": "EM-0400-2FL" })).expect("a tool");
    call(
        &mut s,
        "save_project",
        json!({ "path": path.display().to_string() }),
    )
    .expect("save");
    let p = json!(path.display().to_string());
    let out = script::run(&format!(
        "cam.load({p});\n\
         cam.exportTools({p});\n\
         var refused = \"\";\n\
         try {{ cam.save(); }} catch (e) {{ refused = String(e.message); }}\n\
         cam.save(null, {{ overwrite: true }});\n\
         cam.save();\n\
         refused"
    ));
    let refused = out.result.as_str().unwrap_or_default();
    assert!(
        refused.starts_with("save: ") && changed_refusal(refused),
        "{refused:?} / {:?}",
        out.errors
    );
    assert_eq!(
        out.errors.len(),
        1,
        "only the refused save is recorded: {:?}",
        out.errors
    );
    assert!(is_project(&path), "the overwrite put the project back");
    let _ = std::fs::remove_dir_all(&d);
}

#[test]
fn an_attached_agent_save_meets_the_window_stamp_and_hands_back_the_new_one() {
    let d = dir("live");
    let path = d.join("job.camproj");
    let world = new_world();
    let mut history = History::default();
    project::save(&world, &path).expect("the window's file");
    let window = DiskStamp::of(&path);
    std::fs::write(&path, FOREIGN).expect("another program writes the file");

    // One save_project call that the user allowed, as the window's AI chat sends it.
    let live = |history: &mut History, args: Value, stamp: Option<DiskStamp>| {
        let line = json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call",
            "params": { "name": "save_project", "arguments": args.clone() } })
        .to_string();
        let options = LiveOptions {
            gate: LiveGate::allow_once("save_project", args),
            selection: None,
            read_only: false,
            doc_stamp: stamp,
        };
        let answer = answer_live_with(&world, history, Some(&path), &line, &options);
        let reply: Value = serde_json::from_str(&answer.reply.expect("a reply")).expect("json");
        (
            reply["result"]["isError"] == json!(true),
            reply,
            answer.doc_stamp,
        )
    };

    let (refused, reply, back) = live(&mut history, json!({}), window.clone());
    assert!(refused && changed_refusal(&reply.to_string()), "{reply}");
    assert_eq!(back, window, "a refused save hands back the window's stamp");
    assert_eq!(read(&path), FOREIGN, "nothing was written");

    let (refused, reply, back) = live(&mut history, json!({ "overwrite": true }), window);
    assert!(!refused, "{reply}");
    assert!(is_project(&path));
    assert_eq!(
        back,
        DiskStamp::of(&path),
        "the stamp of our own save comes back for the window"
    );

    let (refused, reply, _) = live(&mut history, json!({}), back);
    assert!(!refused, "no false alarm after our own save: {reply}");
    let _ = std::fs::remove_dir_all(&d);
}
