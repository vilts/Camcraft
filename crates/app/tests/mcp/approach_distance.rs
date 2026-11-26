//! #3745: the approach distance outside the stock - its default, the
//! MCP set and get through `edit_operation` (the undoable
//! `EditOperationParams`), undo and redo, the refusal of a negative value
//! when the command is made, and the save / load round trip.
//! Provenance: our own operation parameter and its MCP patch contract.
//!
//! Gate map: covers app::mcp (edit_operation), app::project::approach,
//! app::commands::operation (EditOperationParams).

use super::{Client, tmp_dir};
use serde_json::{Value, json};

fn pocket_client() -> Client {
    let mut c = Client::new();
    c.ok("set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    c.ok("add_tool", json!({ "catalog": "EM-0600-4FL" }));
    c.ok(
        "add_operation",
        json!({ "kind": "2D Pocket", "tool": "EM-0600-4FL", "name": "P1", "params": { "pass_start": "outside" } }),
    );
    c
}

fn distance(c: &mut Client) -> Value {
    let job = c.ok("get_job", json!({}));
    job["operations"][0]["params"]["pocket"]["links"]["approach"]["approach_distance"].clone()
}

fn set(d: Value) -> Value {
    json!({ "name": "P1", "params": { "links": { "approach": { "approach_distance": d } } } })
}

#[test]
fn approach_distance_default_is_two_mm_and_writes_no_key() {
    assert_eq!(camcraft_app::links::Approach::DEFAULT.approach_distance, 2.0);
    assert_eq!(camcraft_app::links::Approach::default().approach_distance, 2.0);
    let mut c = pocket_client();
    assert_eq!(distance(&mut c), Value::Null, "the default writes no key");
}

#[test]
fn mcp_sets_approach_distance_and_refuses_a_negative_value() {
    let mut c = pocket_client();
    let edit = c.ok("edit_operation", set(json!(5.0)));
    assert_eq!(edit["params"]["pocket"]["links"]["approach"]["approach_distance"], json!(5.0), "{edit}");
    assert_eq!(distance(&mut c), json!(5.0));
    let before = c.ok("get_job", json!({}));
    for bad in [json!(-1.0), json!(-0.001)] {
        let msg = c.refused("edit_operation", set(bad.clone()));
        assert!(msg.contains("approach distance"), "{bad}: {msg}");
        assert_eq!(c.ok("get_job", json!({})), before, "{bad}: a refused patch changes nothing");
    }
    let msg = c.refused(
        "add_operation",
        json!({ "kind": "2D Pocket", "tool": "EM-0600-4FL", "name": "P2",
                "params": { "links": { "approach": { "approach_distance": -2.0 } } } }),
    );
    assert!(msg.contains("approach distance"), "{msg}");
    assert_eq!(c.ok("get_job", json!({})), before, "the refused operation is not made");
    // Zero is legal: the tool's edge on the stock edge.
    c.ok("edit_operation", set(json!(0.0)));
    assert_eq!(distance(&mut c), json!(0.0));
}

#[test]
fn approach_distance_edit_undoes_and_redoes() {
    let mut c = pocket_client();
    c.ok("edit_operation", set(json!(4.5)));
    c.ok("edit_operation", set(json!(7.0)));
    assert_eq!(distance(&mut c), json!(7.0));
    c.ok("undo", json!({}));
    assert_eq!(distance(&mut c), json!(4.5), "undo restores the previous value");
    c.ok("undo", json!({}));
    assert_eq!(distance(&mut c), Value::Null, "undo restores the default");
    c.ok("redo", json!({}));
    assert_eq!(distance(&mut c), json!(4.5), "redo applies the edit again");
}

#[test]
fn approach_distance_round_trips_through_save_and_load() {
    let dir = tmp_dir("approach-distance");
    let mut c = pocket_client();
    c.ok("edit_operation", set(json!(3.25)));
    let path = dir.join("job.camproj");
    c.ok("save_project", json!({ "path": path.display().to_string() }));
    let text = std::fs::read_to_string(&path).expect("saved");
    assert!(text.contains("\"approach_distance\": 3.25"), "{text}");
    let mut d = Client::new();
    d.ok("open_project", json!({ "path": path.display().to_string() }));
    assert_eq!(distance(&mut d), json!(3.25), "the value loads back");
    // A file without the field (every earlier file) loads the default.
    let old = dir.join("old.camproj");
    std::fs::write(&old, text.replace("\"approach_distance\": 3.25", "\"plunge_height\": 1.0")).expect("write old");
    let mut e = Client::new();
    e.ok("open_project", json!({ "path": old.display().to_string() }));
    assert_eq!(distance(&mut e), Value::Null, "absent loads as the default");
    let _ = std::fs::remove_dir_all(&dir);
}
