//! `list_features` face rows when the outside cannot be told (#2186, #956):
//! `facing` and `normal` are null and `outside_unknown` names the reason; a
//! closed box keeps its outward values. `lay_part_on_face` refuses the same face.
//!
//! Gate map (M4.4): covers app::mcp::features (#2186), app::pick::align.

use camcraft_app::mcp::Server;
use serde_json::{Value, json};
use std::path::PathBuf;

/// A 20 x 20 x 10 box as ASCII STL, outward winding; `skip` drops one facet
/// (index 0..12) to open the shell.
fn box_stl(skip: Option<usize>) -> String {
    let v = |i: usize| -> [f64; 3] {
        [if i & 1 == 0 { -10.0 } else { 10.0 }, if i & 2 == 0 { -10.0 } else { 10.0 }, if i & 4 == 0 { 0.0 } else { 10.0 }]
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    let mut s = String::from("solid box\n");
    let mut k = 0;
    for q in quads {
        for tri in [[q[0], q[1], q[2]], [q[0], q[2], q[3]]] {
            k += 1;
            if skip == Some(k - 1) {
                continue;
            }
            s.push_str(" facet normal 0 0 0\n  outer loop\n");
            for i in tri {
                let p = v(i);
                s.push_str(&format!("   vertex {} {} {}\n", p[0], p[1], p[2]));
            }
            s.push_str("  endloop\n endfacet\n");
        }
    }
    s.push_str("endsolid box\n");
    s
}

fn call(server: &mut Server, id: i64, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a response");
    let r = &reply["result"];
    let err = r["isError"] == json!(true);
    (err, if err { r["content"][0]["text"].clone() } else { r["structuredContent"].clone() })
}

fn listing(tag: &str, skip: Option<usize>) -> (Server, Value) {
    let mut dir: PathBuf = std::env::temp_dir();
    dir.push(format!("camcraft-mcp-outside-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("box.stl");
    std::fs::write(&path, box_stl(skip)).expect("write stl");
    let mut s = Server::new();
    s.handle(&json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }));
    let (e, b) = call(&mut s, 2, "import_model", json!({ "path": path.display().to_string() }));
    assert!(!e, "{b}");
    let (e, f) = call(&mut s, 3, "list_features", json!({ "model": "box" }));
    assert!(!e, "list_features must not fail on one face: {f}");
    (s, f)
}

#[test]
fn an_open_shell_face_says_its_outside_is_unknown() {
    // Facet 2 and 3 are the top; dropping facet 0 opens the bottom.
    let (mut s, f) = listing("open", Some(0));
    let faces = f["faces"].as_array().expect("faces");
    assert!(!faces.is_empty(), "{f}");
    for fa in faces {
        assert!(fa["normal"].is_null() && fa["facing"].is_null(), "no guessed side: {fa}");
        let why = fa["outside_unknown"].as_str().unwrap_or_default();
        assert!(why.contains("open body"), "the reason is named: {fa}");
    }
    // The tool that acts on the face refuses it with the same reason.
    call(&mut s, 4, "add_setup", json!({}));
    let id = faces[0]["id"].clone();
    let (err, body) = call(&mut s, 5, "lay_part_on_face", json!({ "face": { "model": "box", "id": id } }));
    assert!(err && body.as_str().unwrap_or_default().contains("open body"), "{body}");
}

#[test]
fn a_closed_box_keeps_its_outward_normals() {
    let (_s, f) = listing("closed", None);
    let faces = f["faces"].as_array().expect("faces");
    let bottom = faces.iter().find(|fa| fa["bounds"]["max"][2] == json!(0.0)).expect("bottom");
    assert_eq!(bottom["facing"], json!("down"), "{bottom}");
    assert_eq!(bottom["normal"], json!([0.0, 0.0, -1.0]), "{bottom}");
    assert!(bottom["outside_unknown"].is_null(), "{bottom}");
    let top = faces.iter().find(|fa| fa["bounds"]["min"][2] == json!(10.0)).expect("top");
    assert_eq!(top["facing"], json!("up"), "{top}");
}
