//! #1186: a project whose tool has unusable numbers is refused by the
//! tool's name and the rule it breaks, in plain words - never a debug
//! dump of the whole definition. Driven through the real loader
//! (`project::open`) on a saved file with one field broken.
//!
//! Gate map (#1186): covers app::project (validate: the tool arm),
//! tools::tool_sanity.
//! Lane shape (M4.4): runs in seconds.
//!
//! Provenance: our own fixtures over our own load law.

use serde_json::{Value, json};

use camcraft_app::commands::{AddTool, History};
use camcraft_app::project::{self, ProjectError};
use camcraft_app::tools::{ToolKind, catalog};
use camcraft_app::world::new_world;

/// A saved project with one catalog end mill named "T1", as JSON.
fn saved() -> Value {
    let world = new_world();
    let mut h = History::default();
    let tool = catalog().into_iter().find(|(_, t)| t.kind == ToolKind::EndMill).map(|(_, t)| t).expect("an end mill");
    h.execute(&world, AddTool::new(&world, "T1", tool).expect("add"));
    let path = std::env::temp_dir().join(format!("camcraft-1186-{}.camcraft", std::process::id()));
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    serde_json::from_str(&text).expect("json")
}

/// The loader's refusal of `file` with T1's tool definition edited.
fn refusal(edit: impl Fn(&mut Value)) -> String {
    let mut file = saved();
    let tool = file["entities"]
        .as_array_mut()
        .and_then(|es| es.iter_mut().find(|e| e["name"] == "T1"))
        .map(|e| &mut e["tool"])
        .expect("T1 in the file");
    edit(tool);
    match project::open(&new_world(), &file.to_string()) {
        Err(ProjectError::Corrupt(msg)) => msg,
        other => panic!("expected a Corrupt refusal, got {other:?}"),
    }
}

#[test]
fn a_bad_turning_field_is_refused_by_tool_and_field() {
    let msg = refusal(|t| t["turning"] = json!({ "vc": 0.0, "feed_per_rev": 0.2 }));
    assert!(msg.contains("tool 'T1': turning surface speed 0 m/min is outside 0..2000 m/min"), "{msg}");
    assert!(!msg.contains("ToolDto"), "no debug dump: {msg}");
    let msg = refusal(|t| t["turning"] = json!({ "vc": 150.0, "feed_per_rev": -0.1 }));
    assert!(msg.contains("tool 'T1': turning feed -0.1 mm/rev is outside"), "{msg}");
    let msg = refusal(|t| t["turning"] = json!({ "vc": 150.0, "feed_per_rev": 0.2, "max_rpm": 1.0 }));
    assert!(msg.contains("tool 'T1': spindle speed") && msg.contains("is over the tool's turning max rpm 1"), "{msg}");
}

#[test]
fn a_bad_milling_field_is_refused_by_tool_and_field() {
    let msg = refusal(|t| t["corner_r"] = json!(100.0));
    assert!(msg.contains("tool 'T1': corner radius 100 mm is more than half the"), "{msg}");
    assert!(!msg.contains("ToolDto"), "no debug dump: {msg}");
    let msg = refusal(|t| t["feed"] = json!(0.0));
    assert!(msg.contains("tool 'T1': feed 0 mm/min must be over 0"), "{msg}");
}
