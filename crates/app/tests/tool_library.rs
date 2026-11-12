//! #300: the native tool library file. Export writes the document's tools
//! as `camcraft-tools/1` JSON; import reads it back as one undo unit. The
//! laws pinned: the round trip is exact; a duplicate name is never
//! overwritten (identical tool skipped, different tool renamed); a tool
//! `is_sane` refuses, a corrupt file or malformed JSON refuse the whole
//! import by name with nothing applied; undo removes exactly the imported
//! tools and redo brings them back.
//! Provenance: our own tests over our own format and commands.
//!
//! Gate map (M4.4): covers app::tool_library, app::commands (tool_import, tool).

use camcraft_app::commands::{AddTool, History, ImportTools};
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::tool_library::{
    LibraryTool, export_tool_library, parse_tool_library, with_library_extension,
};
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

fn cat(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

fn first_two() -> Vec<(&'static str, Tool)> {
    catalog().into_iter().take(2).collect()
}

fn world_with(tools: &[(&str, Tool)]) -> (World, History) {
    let w = new_world();
    let mut h = History::default();
    for (n, t) in tools {
        h.execute(&w, AddTool::new(&w, n, t.clone()).expect("add tool"));
    }
    (w, h)
}

fn tool_named(w: &World, n: &str) -> Option<Tool> {
    model::find_by_name(w, n).and_then(|e| model::tool_of(w, &e))
}

fn tool_names(w: &World) -> Vec<String> {
    model::named_entities(w).into_iter().filter(|(_, e)| model::tool_of(w, e).is_some()).map(|(n, _)| n).collect()
}

fn import(w: &World, h: &mut History, text: &str) -> Result<camcraft_app::commands::ImportReport, String> {
    let lib = parse_tool_library(text)?;
    let cmd = ImportTools::new(w, lib).map_err(|e| e.to_string())?;
    let report = cmd.report().clone();
    h.execute(w, cmd);
    Ok(report)
}

#[test]
fn export_then_import_roundtrips_exactly() {
    let tools = first_two();
    let (src, _) = world_with(&tools);
    let text = export_tool_library(&src).expect("export");
    let v: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["format"], "camcraft-tools");
    assert_eq!(v["version"], 1);
    assert_eq!(v["tools"].as_array().map(Vec::len), Some(2));
    // The MCP definition shape, verbatim.
    assert_eq!(v["tools"][0]["definition"], project::tool_to_json(&tools[0].1));

    let (dst, mut h) = world_with(&[]);
    let report = import(&dst, &mut h, &text).expect("import");
    assert_eq!(report.added.len(), 2);
    for (n, t) in &tools {
        assert_eq!(tool_named(&dst, n).as_ref(), Some(t), "{n} round-trips exactly");
    }
    // And the second export of the imported document is the same file.
    assert_eq!(export_tool_library(&dst).expect("re-export"), text);
}

#[test]
fn bare_mcp_array_imports_too() {
    let (n, t) = first_two().remove(0);
    let text = json!([{ "name": n, "definition": project::tool_to_json(&t) }]).to_string();
    let (w, mut h) = world_with(&[]);
    import(&w, &mut h, &text).expect("bare array import");
    assert_eq!(tool_named(&w, n), Some(t));
}

#[test]
fn empty_document_refuses_export() {
    let (w, _) = world_with(&[]);
    let err = export_tool_library(&w).expect_err("nothing to export");
    assert!(err.contains("no tools"), "{err}");
}

#[test]
fn duplicate_identical_is_skipped_and_different_is_renamed_never_overwritten() {
    let tools = first_two();
    let (src, _) = world_with(&tools);
    let text = export_tool_library(&src).expect("export");

    // The document already holds tool 0 identically, and a DIFFERENT
    // tool under tool 1's name.
    let other = cat("EM-0600-4FL");
    assert_ne!(other, tools[1].1);
    let (w, mut h) = world_with(&[(tools[0].0, tools[0].1.clone()), (tools[1].0, other.clone())]);
    let report = import(&w, &mut h, &text).expect("import");
    assert_eq!(report.skipped, vec![tools[0].0.to_owned()]);
    let renamed = format!("{} (2)", tools[1].0);
    assert_eq!(report.renamed, vec![(tools[1].0.to_owned(), renamed.clone())]);
    assert!(report.added.is_empty());
    // The existing tool is untouched; the library's copy sits beside it.
    assert_eq!(tool_named(&w, tools[1].0), Some(other));
    assert_eq!(tool_named(&w, &renamed), Some(tools[1].1.clone()));
    assert!(report.summary().contains("1 renamed"), "{}", report.summary());

    // Importing the same file again renames once more (` (3)`), never overwrites.
    let report2 = import(&w, &mut h, &text).expect("second import");
    assert_eq!(report2.renamed[0].1, format!("{} (3)", tools[1].0));
}

#[test]
fn library_already_in_document_refuses_with_nothing_applied() {
    let tools = first_two();
    let (w, mut h) = world_with(&tools);
    let text = export_tool_library(&w).expect("export");
    let before = tool_names(&w);
    let err = import(&w, &mut h, &text).expect_err("all present");
    assert!(err.contains("already in the document"), "{err}");
    assert_eq!(tool_names(&w), before);
}

#[test]
fn insane_tool_refuses_whole_file_by_name() {
    let tools = first_two();
    let (src, _) = world_with(&tools);
    let mut v: Value = serde_json::from_str(&export_tool_library(&src).expect("export")).expect("json");
    v["tools"][1]["definition"]["diameter"] = json!(-3.0);
    let (w, mut h) = world_with(&[]);
    let err = import(&w, &mut h, &v.to_string()).expect_err("insane tool");
    assert!(err.contains(tools[1].0), "names the tool: {err}");
    assert!(tool_names(&w).is_empty(), "nothing applied");
    assert!(!h.can_undo());
}

#[test]
fn missing_required_field_refuses_by_name() {
    let tools = first_two();
    let (src, _) = world_with(&tools);
    let mut v: Value = serde_json::from_str(&export_tool_library(&src).expect("export")).expect("json");
    if let Some(d) = v["tools"][0]["definition"].as_object_mut() {
        d.remove("diameter");
    }
    let err = parse_tool_library(&v.to_string()).expect_err("missing diameter");
    assert!(err.contains(tools[0].0) && err.contains("diameter"), "{err}");
}

#[test]
fn malformed_and_foreign_files_refuse_cleanly() {
    for (text, want) in [
        ("", "bad JSON"),
        ("{not json", "bad JSON"),
        ("42", "expected an object or an array"),
        (r#"{"format":"something-else","version":1,"tools":[]}"#, "something-else"),
        (r#"{"format":"camcraft-tools","version":99,"tools":[]}"#, "version 99"),
        (r#"{"format":"camcraft-tools","version":1}"#, "no \"tools\""),
        (r#"{"format":"camcraft-tools","version":1,"units":"inch","tools":[]}"#, "units"),
        (r#"{"format":"camcraft-tools","version":1,"tools":[]}"#, "no tools"),
        (r#"[{"definition":{}}]"#, "tool #1 has no name"),
        (r#"[{"name":"A"}]"#, "'A' has no definition"),
        (r#"[{"name":"A","definition":"x"}]"#, "tool 'A'"),
    ] {
        let err = parse_tool_library(text).expect_err(text);
        assert!(err.contains(want), "{text:?} -> {err}");
    }
}

#[test]
fn name_twice_in_file_refuses() {
    let (n, t) = first_two().remove(0);
    let d = project::tool_to_json(&t);
    let text = json!([{ "name": n, "definition": d }, { "name": n, "definition": d }]).to_string();
    let err = parse_tool_library(&text).expect_err("twice");
    assert!(err.contains("twice") && err.contains(n), "{err}");
}

#[test]
fn reserved_name_refuses_with_nothing_applied() {
    let (_, t) = first_two().remove(0);
    let (w, _) = world_with(&[]);
    let lib = vec![
        LibraryTool::bare("Mine", t.clone()),
        LibraryTool::bare("Stock", t),
    ];
    let err = ImportTools::new(&w, lib).err().expect("reserved").to_string();
    assert!(err.contains("Stock"), "{err}");
    assert!(tool_names(&w).is_empty());
}

#[test]
fn import_is_one_undo_unit_and_redo_restores_it() {
    let tools = first_two();
    let (src, _) = world_with(&tools);
    let text = export_tool_library(&src).expect("export");
    let keep = cat("EM-0600-4FL");
    let (w, mut h) = world_with(&[("Keep", keep.clone())]);
    import(&w, &mut h, &text).expect("import");
    assert_eq!(tool_names(&w).len(), 3);

    assert!(h.undo(&w));
    assert_eq!(tool_names(&w), vec!["Keep".to_owned()], "undo removes exactly the imported tools");
    assert_eq!(tool_named(&w, "Keep"), Some(keep));

    assert!(h.redo(&w));
    for (n, t) in &tools {
        assert_eq!(tool_named(&w, n).as_ref(), Some(t));
    }
}

/// A name typed without a suffix still writes a `.camtools` file
/// (macOS's save panel keeps the typed name as is); a name that already
/// has an extension is kept as typed. Hands-on finding on PR #348.
#[test]
fn an_export_name_without_a_suffix_gets_the_library_extension() {
    use std::path::Path;
    assert_eq!(with_library_extension(Path::new("/tmp/shop")), Path::new("/tmp/shop.camtools"));
    assert_eq!(with_library_extension(Path::new("/tmp/shop.camtools")), Path::new("/tmp/shop.camtools"));
    assert_eq!(with_library_extension(Path::new("/tmp/shop.json")), Path::new("/tmp/shop.json"));
    assert_eq!(with_library_extension(Path::new("/tmp/my.tools/shop")), Path::new("/tmp/my.tools/shop.camtools"));
}

/// #741: the starter catalog is the asset `assets/tools/catalog.json`, in
/// this very `camcraft-tools/1` format. The strict app reader and the
/// tools crate's own reader (which `catalog()` uses) must agree on it,
/// tool for tool.
#[test]
fn the_shipped_catalog_asset_reads_the_same_through_the_library_reader() {
    let text = include_str!("../../../assets/tools/catalog.json");
    let lib = parse_tool_library(text).expect("shipped catalog is a valid tool library");
    let cat = catalog();
    assert_eq!(lib.len(), cat.len());
    for (l, (name, t)) in lib.iter().zip(cat.iter()) {
        assert_eq!(l.name, *name);
        assert_eq!(l.tool, *t, "{name}");
    }
}
