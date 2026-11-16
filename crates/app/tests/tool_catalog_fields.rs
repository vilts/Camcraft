//! #898: a tool's catalog facts (part number, vendor, coating, helix,
//! notes) round-trip through the project file and the tool library, edit
//! through the undoable tool commands (one undo step), are refused by
//! name when unusable, are absent from files of tools without them, and
//! change no toolpath input.
//! Provenance: our own tests over our own command model.
//!
//! Gate map (M4.4): covers app::commands (tool), app::project (ToolDto), app::tool_library.

use camcraft_app::catalog_fields::ToolCatalog;
use camcraft_app::commands::{AddTool, ChangeTool, History};
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::tool_library::{export_tool_library, parse_tool_library};
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn base() -> Tool {
    catalog().remove(0).1
}

fn facts() -> ToolCatalog {
    ToolCatalog {
        part_number: Some("XY-1234".into()),
        vendor: Some("Acme Tooling".into()),
        coating: Some("AlTiN".into()),
        helix_deg: Some(35.0),
        notes: Some("roughing\nonly".into()),
    }
}

fn stated() -> Tool {
    Tool { catalog: facts(), ..base() }
}

fn read(w: &World, n: &str) -> Tool {
    model::find_by_name(w, n).and_then(|e| model::tool_of(w, &e)).expect("tool")
}

fn tmp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("cc898-{}-{name}", std::process::id()))
}

#[test]
fn catalog_facts_round_trip_the_project_file() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T1", stated()).expect("add"));
    let path = tmp("p.camproj");
    project::save(&w, &path).expect("save");
    let w2 = new_world();
    project::load(&w2, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(read(&w2, "T1").catalog, facts());
}

#[test]
fn unstated_facts_write_no_keys_and_old_json_loads() {
    let json = project::tool_to_json(&base()).to_string();
    for key in ["part_number", "vendor", "coating", "helix_deg", "notes"] {
        assert!(!json.contains(key), "{key} must not be written when empty");
    }
    let v: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(project::tool_from_json(&v).expect("old file loads").catalog, ToolCatalog::default());
    let on = project::tool_to_json(&stated());
    assert_eq!(on["part_number"], "XY-1234");
    assert_eq!(on["helix_deg"], 35.0);
}

#[test]
fn catalog_facts_round_trip_the_tool_library() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T1", stated()).expect("add"));
    let text = export_tool_library(&w).expect("export");
    let back = parse_tool_library(&text).expect("parse");
    assert_eq!(back[0].tool.catalog, facts());
}

#[test]
fn editing_the_facts_is_one_undo_step_and_never_moves_geometry() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T1", base()).expect("add"));
    h.execute(&w, ChangeTool::new(&w, "T1", stated()).expect("change"));
    let t = read(&w, "T1");
    assert_eq!(t.catalog, facts());
    assert_eq!(Tool { catalog: ToolCatalog::default(), ..t.clone() }, base(), "only the facts changed");
    assert_eq!(t.geom(), base().geom());
    assert!(h.undo(&w));
    assert_eq!(read(&w, "T1").catalog, ToolCatalog::default());
    assert!(h.redo(&w));
    assert_eq!(read(&w, "T1").catalog, facts());
}

#[test]
fn facts_are_stored_trimmed_and_blank_is_not_stated() {
    let w = new_world();
    let padded = ToolCatalog {
        part_number: Some("  P-9  ".into()),
        vendor: Some("   ".into()),
        ..ToolCatalog::default()
    };
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T1", Tool { catalog: padded, ..base() }).expect("add"));
    let c = read(&w, "T1").catalog;
    assert_eq!(c.part_number.as_deref(), Some("P-9"));
    assert_eq!(c.vendor, None);
}

#[test]
fn unusable_facts_are_refused_by_tool_name_at_creation() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T1", base()).expect("add"));
    let bad = [
        ToolCatalog { helix_deg: Some(91.0), ..ToolCatalog::default() },
        ToolCatalog { helix_deg: Some(-0.5), ..ToolCatalog::default() },
        ToolCatalog { helix_deg: Some(f64::NAN), ..ToolCatalog::default() },
        ToolCatalog { vendor: Some("v".repeat(500)), ..ToolCatalog::default() },
        ToolCatalog { coating: Some("a\u{7}b".into()), ..ToolCatalog::default() },
    ];
    for c in bad {
        let t = Tool { catalog: c, ..base() };
        for e in [
            AddTool::new(&w, "T2", t.clone()).err().expect("add refused").to_string(),
            ChangeTool::new(&w, "T1", t).err().expect("change refused").to_string(),
        ] {
            assert!(e.contains("tool 'T"), "{e}");
        }
    }
    assert_eq!(read(&w, "T1").catalog, ToolCatalog::default());
}

#[test]
fn a_library_with_a_bad_helix_refuses_naming_the_tool() {
    let w = new_world();
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "T1", stated()).expect("add"));
    let text = export_tool_library(&w).expect("export").replace("35.0", "135.0");
    let e = parse_tool_library(&text).expect_err("refused");
    assert!(e.contains("T1"), "{e}");
}
