//! #2463: a model imported from a file records the file and its SHA-256
//! (a linked model). The link rides ImportModel, UpdateModel and
//! delete-and-undo; SetModelSource links and unlinks as one undo step;
//! the project file (v72) keeps it, a v71 file opens unlinked, and a
//! malformed source row is refused as corrupt. The MCP tool sets the link
//! and get_job shows it. #2689: the digest is of the one read the import
//! parsed, and the check hashes a file in pieces.
//! Provenance: our own fixtures over our own commands.
//!
//! Gate map (#2463): covers app::model_source, app::commands::import_model,
//! app::commands::update_model, app::commands::delete_entity,
//! app::commands::model_link (SetModelSource), app::project (source_dto,
//! v72), app::mcp::model_link_tools, app::mcp::job (get_job),
//! app::geometry::import::sourced (#2689).
//! Lane shape (M4.4): all tests here run in seconds.

use std::path::{Path, PathBuf};

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use camcraft_app::commands::{DeleteEntity, History, ImportModel, SetModelSource, UpdateModel};
use camcraft_app::mcp::Server;
use camcraft_app::schema::ModelSource;
use camcraft_app::world::new_world;
use camcraft_app::{model, model_source, project};

fn testdata(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/models").join(name)
}

/// A folder of its own for one test, holding a copy of `cube.stl`.
struct Dir(PathBuf);

impl Dir {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("camcraft-2463-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        std::fs::copy(testdata("cube.stl"), dir.join("part.stl")).expect("copy cube");
        Dir(dir)
    }
    fn part(&self) -> PathBuf {
        self.0.join("part.stl")
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn source_of(world: &World, name: &str) -> Option<ModelSource> {
    model::find_by_name(world, name).and_then(|e| model_source::of(world, &e))
}

fn imported(dir: &Dir) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    h.execute(&world, ImportModel::new(&dir.part()).expect("import"));
    (world, h)
}

#[test]
fn an_import_from_a_file_links_the_model_to_that_file() {
    let dir = Dir::new("import");
    let (world, mut h) = imported(&dir);
    let want = model_source::read(&dir.part()).expect("digest");
    assert_eq!(want.sha256.len(), 64);
    assert_eq!(source_of(&world, "part"), Some(want));
    assert!(h.undo(&world));
    assert!(model::find_by_name(&world, "part").is_none());

    let world = new_world();
    let mut h = History::default();
    let bytes = std::fs::read(dir.part()).expect("bytes");
    h.execute(&world, ImportModel::from_bytes("part.stl", &bytes, None).expect("picked"));
    assert_eq!(source_of(&world, "part"), None, "a browser pick has no file to check");
}

#[test]
fn an_update_trades_the_link_and_undo_restores_it() {
    let dir = Dir::new("update");
    let (world, mut h) = imported(&dir);
    let before = source_of(&world, "part").expect("linked");
    let other = ModelSource { path: "/elsewhere/part.stl".into(), sha256: "a".repeat(64) };
    let mesh = ImportModel::new(&testdata("cube.stl")).expect("mesh").mesh;
    h.execute(&world, UpdateModel::new(&world, "part", mesh.clone()).expect("update").with_source(Some(other.clone())));
    assert_eq!(source_of(&world, "part"), Some(other));
    assert!(h.undo(&world));
    assert_eq!(source_of(&world, "part"), Some(before.clone()), "undo restores the link");
    h.execute(&world, UpdateModel::new(&world, "part", mesh.clone()).expect("update"));
    assert_eq!(source_of(&world, "part"), None, "an update from no file unlinks");
    assert!(h.undo(&world));
    h.execute(&world, UpdateModel::new(&world, "part", mesh).expect("repair").keeping_fidelity());
    assert_eq!(source_of(&world, "part"), Some(before), "a repair keeps the link");
}

#[test]
fn delete_and_undo_keep_the_link() {
    let dir = Dir::new("delete");
    let (world, mut h) = imported(&dir);
    let linked = source_of(&world, "part");
    h.execute(&world, DeleteEntity::new(&world, "part").expect("delete"));
    assert!(model::find_by_name(&world, "part").is_none());
    assert!(h.undo(&world));
    assert_eq!(source_of(&world, "part"), linked);
}

#[test]
fn link_and_unlink_are_one_undo_step_each_and_refuse_by_name() {
    let dir = Dir::new("set");
    let (world, mut h) = imported(&dir);
    let linked = source_of(&world, "part");
    h.execute(&world, SetModelSource::unlink(&world, "part").expect("unlink"));
    assert_eq!(source_of(&world, "part"), None);
    let err = SetModelSource::unlink(&world, "part").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(err.contains("'part' is not linked to a file"), "{err}");
    assert!(h.undo(&world));
    assert_eq!(source_of(&world, "part"), linked, "undo links it again");

    let copy = dir.0.join("copy.stl");
    std::fs::copy(dir.part(), &copy).expect("copy");
    h.execute(&world, SetModelSource::link(&world, "part", &copy).expect("link"));
    assert_eq!(source_of(&world, "part").map(|s| s.path), Some(copy.to_string_lossy().into_owned()));
    assert!(h.undo(&world));
    assert_eq!(source_of(&world, "part"), linked);

    for (result, want) in [
        (SetModelSource::link(&world, "nope", &copy).err(), "no entity named 'nope'"),
        (SetModelSource::link(&world, "part", &dir.0.join("gone.stl")).err(), "cannot link 'part': "),
        (SetModelSource::unlink(&world, "nope").err(), "no entity named 'nope'"),
    ] {
        let text = result.map(|e| e.to_string()).unwrap_or_default();
        assert!(text.contains(want), "{want}: {text}");
    }
}

fn saved(world: &World, dir: &Dir) -> String {
    let path = dir.0.join("job.camproj");
    project::save(world, &path).expect("save");
    std::fs::read_to_string(&path).expect("read back")
}

#[test]
fn the_project_file_keeps_the_link_and_a_v71_file_opens_unlinked() {
    let dir = Dir::new("project");
    let (world, _h) = imported(&dir);
    let text = saved(&world, &dir);
    let doc: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(doc["version"], json!(project::PROJECT_VERSION));
    assert!(doc["version"].as_u64().is_some_and(|v| v >= 72), "linked sources require the v72 format barrier");
    let row = doc["entities"].as_array().and_then(|e| e.iter().find(|e| e["name"] == "part")).expect("part row");
    let want = source_of(&world, "part").expect("linked");
    assert_eq!(row["source"], json!({ "path": want.path, "sha256": want.sha256 }));
    let fresh = new_world();
    project::open(&fresh, &text).expect("opens");
    assert_eq!(source_of(&fresh, "part"), Some(want), "round trip");

    let mut old = doc.clone();
    old["version"] = json!(71);
    if let Some(rows) = old["entities"].as_array_mut() {
        for row in rows {
            if let Some(o) = row.as_object_mut() {
                o.remove("source");
            }
        }
    }
    let fresh = new_world();
    project::open(&fresh, &serde_json::to_string(&old).expect("json")).expect("a v71 file opens");
    assert_eq!(source_of(&fresh, "part"), None, "a v71 model is not linked");
    assert!(model::find_by_name(&fresh, "part").is_some());

    let mut newer = doc;
    newer["version"] = json!(project::NEWEST_PROJECT_VERSION + 1);
    let err = project::open(&new_world(), &serde_json::to_string(&newer).expect("json")).expect_err("too new");
    assert!(err.to_string().contains(&format!("unsupported project version {}", project::NEWEST_PROJECT_VERSION + 1)), "{err}");

    let plain = new_world();
    let mut h = History::default();
    let mesh = ImportModel::new(&testdata("cube.stl")).expect("mesh").mesh;
    h.execute(&plain, ImportModel::from_mesh("solid", mesh));
    assert!(!saved(&plain, &dir).contains("\"source\""), "no link, no key");
}

#[test]
fn a_malformed_source_row_is_refused_as_corrupt() {
    let dir = Dir::new("corrupt");
    let (world, _h) = imported(&dir);
    let doc: Value = serde_json::from_str(&saved(&world, &dir)).expect("json");
    let cases = [
        (json!({ "path": "", "sha256": "a".repeat(64) }), "'part' records a source with no file path"),
        (json!({ "path": "p.stl", "sha256": "ABC" }), "'part' records a source whose sha256 is not 64 lowercase hex digits"),
        (json!({ "path": "p.stl", "sha256": "a".repeat(64), "extra": 1 }), "malformed project file"),
    ];
    for (source, want) in cases {
        let mut bad = doc.clone();
        let rows = bad["entities"].as_array_mut().expect("entities");
        let part = rows.iter_mut().find(|e| e["name"] == "part").expect("part");
        part["source"] = source;
        let err = project::open(&new_world(), &serde_json::to_string(&bad).expect("json")).expect_err("corrupt");
        assert!(err.to_string().contains(want), "{want}: {err}");
    }
}

fn call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a response");
    let result = &reply["result"];
    let is_err = result["isError"] == json!(true);
    (is_err, if is_err { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
}

fn job_source(server: &mut Server, id: i64) -> Value {
    let (err, job) = call(server, id, "get_job", json!({}));
    assert!(!err, "{job}");
    job["models"].as_array().and_then(|m| m.iter().find(|m| m["name"] == "part")).map_or(Value::Null, |m| m["source"].clone())
}

#[test]
fn the_mcp_tool_links_and_unlinks_and_get_job_shows_the_source() {
    let dir = Dir::new("mcp");
    let mut s = Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    s.handle(&init).expect("initialize");
    let part = dir.part().to_string_lossy().into_owned();
    let (err, body) = call(&mut s, 1, "import_model", json!({ "path": part }));
    assert!(!err, "{body}");
    let want = model_source::read(&dir.part()).expect("digest");
    assert_eq!(job_source(&mut s, 2), json!({ "path": want.path, "sha256": want.sha256 }), "an import links");

    let (err, body) = call(&mut s, 3, "set_model_link", json!({ "model": "part", "path": null }));
    assert!(!err, "{body}");
    assert_eq!(body["source"], Value::Null);
    assert_eq!(job_source(&mut s, 4), Value::Null, "unlinked");
    let (err, body) = call(&mut s, 5, "set_model_link", json!({ "model": "part", "path": part }));
    assert!(!err, "{body}");
    assert_eq!(body["source"]["sha256"], json!(want.sha256));
    for (id, args, want) in [
        (6, json!({ "model": "part" }), "give \"path\""),
        (7, json!({ "model": "part", "path": 3 }), "must be a string or null"),
        (8, json!({ "model": "nope", "path": null }), "no entity named 'nope'"),
    ] {
        let (err, body) = call(&mut s, id, "set_model_link", args);
        assert!(err && body.as_str().is_some_and(|b| b.contains(want)), "{want}: {body}");
    }
    let (err, body) = call(&mut s, 9, "undo", json!({}));
    assert!(!err, "undo: {body}");
    assert_eq!(job_source(&mut s, 10), Value::Null, "undo reverts one step");
}

/// A closed binary STL tetrahedron, wound outward.
fn binary_tetra() -> Vec<u8> {
    let (o, a, b, c) = ([0f32, 0., 0.], [10f32, 0., 0.], [0f32, 10., 0.], [0f32, 0., 10.]);
    let faces = [([0f32, 0., -1.], [o, b, a]), ([0., -1., 0.], [o, a, c]), ([-1., 0., 0.], [o, c, b]), ([0.577, 0.577, 0.577], [a, b, c])];
    let mut out = vec![0u8; 80];
    out.extend_from_slice(&4u32.to_le_bytes());
    for (n, tri) in faces {
        for v in std::iter::once(n).chain(tri) {
            for x in v {
                out.extend_from_slice(&x.to_le_bytes());
            }
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    out
}

/// #2689: the link records the SHA-256 of the one read the import parsed -
/// an ASCII STL (whose head the streaming parser reads twice), a binary
/// STL, OBJ and STEP - and the link check's streamed digest agrees.
#[test]
fn the_link_digest_is_of_the_one_read_the_import_parsed() {
    let dir = Dir::new("one-read");
    let binary = dir.0.join("tetra.stl");
    std::fs::write(&binary, binary_tetra()).expect("write tetra");
    for path in [dir.part(), binary, testdata("box.obj"), testdata("box.step")] {
        let want = camcraft_geometry::solidcraft::sha256_hex(&std::fs::read(&path).expect("read"));
        let cmd = ImportModel::new(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(cmd.source.map(|s| s.sha256), Some(want.clone()), "import digest of {}", path.display());
        assert_eq!(model_source::read(&path).map(|s| s.sha256), Ok(want), "check digest of {}", path.display());
    }
}

/// #2689: the link check hashes a file in pieces, never whole; a file of
/// several pieces gives the digest of all its bytes.
#[test]
fn the_link_check_hashes_a_file_larger_than_one_piece_as_a_whole() {
    let dir = Dir::new("pieces");
    let big = dir.0.join("big.stl");
    let data: Vec<u8> = (0..(3 << 20) + 17u32).map(|i| (i.wrapping_mul(2_654_435_761) >> 24) as u8).collect();
    std::fs::write(&big, &data).expect("write big");
    assert_eq!(model_source::read(&big).map(|s| s.sha256), Ok(camcraft_geometry::solidcraft::sha256_hex(&data)));
}
