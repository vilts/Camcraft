//! #300: the native tool library file. Export writes the document's tools
//! as `camcraft-tools/1` JSON; import reads it back as one undo unit. The
//! laws pinned: the round trip is exact; a duplicate name is never
//! overwritten (identical tool skipped, different tool renamed); a tool
//! `is_sane` refuses, a corrupt file or malformed JSON refuse the whole
//! import by name with nothing applied; undo removes exactly the imported
//! tools and redo brings them back. #3904: a drawn cutter rides the file
//! (version 5), is judged by name, and lands and undoes with its tool.
//! #3662: the converted cutters (`converted_3662`).
//! Provenance: our own tests over our own format and commands.
//!
//! Gate map (M4.4): covers app::tool_library, app::commands (tool_import, tool).

use camcraft_app::commands::{AddTool, History, ImportMode, ImportTools, SetToolCutter};
use camcraft_app::spun::{SpunPoint, SpunProfile};
use camcraft_app::tool_cutter;
use camcraft_app::model;
use camcraft_app::project;
use camcraft_app::tool_library::{
    LibraryTool, export_tool_library, parse_tool_library, with_library_extension,
};
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;
use serde_json::{Value, json};

#[path = "tool_library/converted_3662.rs"]
mod converted_3662;

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

/// #3069: the router starter set imports through the normal command.
#[test]
fn router_starter_library_imports_without_refusals_and_undoes_as_one_unit() {
    let text = include_str!("../../../assets/tools/catalog.json");
    let lib = parse_tool_library(text).expect("the starter library parses");
    let router: Vec<_> = lib.iter().filter(|t| t.name.starts_with("Router ")).collect();
    assert_eq!(router.len(), 22, "the router set is bundled");
    let (w, mut h) = world_with(&[]);
    let report = import(&w, &mut h, text).expect("all starter tools import");
    assert_eq!(report.added.len(), lib.len());
    assert!(report.skipped.is_empty() && report.renamed.is_empty());
    for t in &lib {
        assert_eq!(tool_named(&w, &t.name).as_ref(), Some(&t.tool), "{}", t.name);
    }
    assert!(h.undo(&w));
    assert!(tool_names(&w).is_empty());
    assert!(!h.can_undo(), "one import is one undo unit");
    assert!(h.redo(&w));
    for t in router {
        assert_eq!(tool_named(&w, &t.name).as_ref(), Some(&t.tool), "{}", t.name);
    }
}

/// #1152: a body electrode's outline rides the library file both ways,
/// and the shipped-catalog reader of the tools crate reads the same row.
#[test]
fn a_body_electrodes_outline_roundtrips_through_the_library() {
    let ell = vec![[-5.0, -4.0], [5.0, -4.0], [5.0, 0.0], [-1.0, 0.0], [-1.0, 4.0], [-5.0, 4.0]];
    let e = camcraft_tools::electrode::electrode_outline_tool(&ell, 16.0, 15.0);
    let (src, _h) = world_with(&[("E-L", e.clone())]);
    let text = export_tool_library(&src).expect("export");
    assert!(text.contains("\"outline\""), "{text}");
    let lib = parse_tool_library(&text).expect("the library reads back");
    assert_eq!(lib[0].tool, e);
    // A row whose sizes leave its outline refuses the whole file by name.
    let mut v: Value = serde_json::from_str(&text).expect("json");
    v["tools"][0]["definition"]["diameter"] = json!(12.0);
    let why = parse_tool_library(&v.to_string()).expect_err("sizes off the outline");
    assert!(why.contains("tool 'E-L' is refused") && why.contains("the outline's X and Y extents"), "{why}");
}

/// #3368: a flat or ball taper tip rides the library file, and such a file
/// says `TIP_FORM_VERSION`, so an older build refuses it instead of reading
/// the tip as sharp. A library of sharp tools keeps its older number.
#[test]
fn a_tip_form_round_trips_and_steps_the_version() {
    use camcraft_app::tool_library::{CUTTER_VERSION, HOLDER_REF_VERSION, TIP_FORM_VERSION};
    use camcraft_app::tools::TaperTip;
    let sharp = cat("CM-0600-90");
    let (w, _h) = world_with(&[("CM", sharp.clone())]);
    let text = export_tool_library(&w).expect("export");
    let doc: Value = serde_json::from_str(&text).expect("json");
    assert!(doc["version"].as_u64().is_some_and(|v| v < TIP_FORM_VERSION), "no tip form, no step: {doc}");
    assert!(!text.contains("tip_form"), "{text}");
    let flat = Tool { tip_form: TaperTip::Flat, tip_diameter: 1.0, ..sharp.clone() };
    let ball = Tool { tip_form: TaperTip::Ball, corner_r: 0.5, ..sharp };
    let (w, _h) = world_with(&[("FLAT", flat.clone()), ("BALL", ball.clone())]);
    let text = export_tool_library(&w).expect("export");
    let doc: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(doc["version"], json!(TIP_FORM_VERSION), "a tip form steps the version");
    let back = parse_tool_library(&text).expect("this build reads its own file");
    let got = |n: &str| back.iter().find(|t| t.name == n).map(|t| (t.tool.tip_form, t.tool.tip_diameter, t.tool.corner_r));
    assert_eq!(got("FLAT"), Some((TaperTip::Flat, 1.0, flat.corner_r)));
    assert_eq!(got("BALL"), Some((TaperTip::Ball, 0.0, 0.5)));
    // The step is no number an older reader knows: past holder refs and
    // past the drawn cutter (version 5, PR #3906).
    const { assert!(TIP_FORM_VERSION > HOLDER_REF_VERSION && TIP_FORM_VERSION > CUTTER_VERSION) };
    let newer = text.replacen(&format!("\"version\": {TIP_FORM_VERSION}"), &format!("\"version\": {}", TIP_FORM_VERSION + 1), 1);
    assert!(parse_tool_library(&newer).is_err_and(|e| e.contains("not supported")), "a newer number is refused");
}

/// #3904 + #3368: a drawn cutter on a flat-tip taper writes both keys, and
/// the file says the higher number (`TIP_FORM_VERSION`).
#[test]
fn a_drawn_cutter_and_a_tip_form_together_say_the_tip_form_version() {
    use camcraft_app::tool_library::TIP_FORM_VERSION;
    use camcraft_app::tools::TaperTip;
    let (w, _h) = necked_world();
    let flat = Tool { tip_form: TaperTip::Flat, tip_diameter: 1.0, ..cat("CM-0600-90") };
    let mut h = History::default();
    h.execute(&w, AddTool::new(&w, "FLAT", flat.clone()).expect("add tool"));
    let text = export_tool_library(&w).expect("export");
    let v: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["version"], json!(TIP_FORM_VERSION), "{text}");
    let lib = parse_tool_library(&text).expect("this build reads its own file");
    let by = |n: &str| lib.iter().find(|t| t.name == n).expect("tool");
    assert_eq!(by(NECKED).cutter, cutter_named(&w, NECKED), "the cutter survives");
    assert_eq!(by("FLAT").tool.tip_form, TaperTip::Flat, "the tip form survives");
}

const NECKED: &str = "EM-1000-4FL";

/// EM-1000-4FL (Ø10 x 22 flutes, Ø10 shank) with a Ø8 neck from 22 to 30.
fn necked_points() -> Vec<SpunPoint> {
    [(0.0, 0.0), (0.0, 5.0), (22.0, 5.0), (22.0, 4.0), (30.0, 4.0), (30.0, 5.0), (62.0, 5.0), (62.0, 0.0)]
        .into_iter()
        .map(|(z, r)| SpunPoint::line(z, r))
        .collect()
}

fn necked_world() -> (World, History) {
    let (w, mut h) = world_with(&[(NECKED, cat(NECKED))]);
    h.execute(&w, SetToolCutter::new(&w, NECKED, Some(necked_points())).expect("drawn cutter"));
    (w, h)
}

fn cutter_named(w: &World, n: &str) -> Option<SpunProfile> {
    model::find_by_name(w, n).and_then(|e| tool_cutter::cutter_of(w, &e))
}

#[test]
fn a_drawn_cutter_round_trips_through_the_library_file() {
    let (src, _) = necked_world();
    let drawn = cutter_named(&src, NECKED).expect("a drawn cutter");
    let text = export_tool_library(&src).expect("export");
    let v: Value = serde_json::from_str(&text).expect("json");
    assert_eq!(v["version"], 5, "a file with a cutter says version 5");
    assert!(v["tools"][0]["definition"]["cutter"].is_array(), "the cutter is written: {text}");
    let lib = parse_tool_library(&text).expect("parse");
    assert_eq!(lib[0].cutter.as_ref(), Some(&drawn));
    let (dst, mut h) = world_with(&[]);
    import(&dst, &mut h, &text).expect("import");
    assert_eq!(cutter_named(&dst, NECKED), Some(drawn), "the same drawn cutter lands");
    assert!(h.undo(&dst));
    assert!(tool_named(&dst, NECKED).is_none(), "undo removes the tool and its cutter");
}

#[test]
fn a_cutter_that_disagrees_refuses_the_file_by_tool_name() {
    let (src, _) = necked_world();
    let mut v: Value = serde_json::from_str(&export_tool_library(&src).expect("export")).expect("json");
    // The flutes drawn at Ø12 on a Ø10 tool.
    v["tools"][0]["definition"]["cutter"][1][1] = json!(6.0);
    v["tools"][0]["definition"]["cutter"][2][1] = json!(6.0);
    let err = parse_tool_library(&v.to_string()).expect_err("a disagreeing cutter refuses");
    assert!(err.contains(&format!("tool '{NECKED}'")) && err.contains("cutter"), "{err}");
    v["tools"][0]["definition"]["cutter"] = json!("not points");
    let err = parse_tool_library(&v.to_string()).expect_err("a malformed cutter refuses");
    assert!(err.starts_with(&format!("tool '{NECKED}'")), "{err}");
}

#[test]
fn update_replaces_the_cutter_in_one_undo_step() {
    let (src, _) = necked_world();
    let text = export_tool_library(&src).expect("export");
    let (w, mut h) = world_with(&[(NECKED, cat(NECKED))]);
    assert!(cutter_named(&w, NECKED).is_none());
    let lib = parse_tool_library(&text).expect("parse");
    let cmd = ImportTools::with_mode(&w, lib, ImportMode::Update).expect("an update with only a new cutter is a change");
    assert_eq!(cmd.report().updated, vec![NECKED.to_owned()]);
    h.execute(&w, cmd);
    assert_eq!(cutter_named(&w, NECKED), cutter_named(&src, NECKED));
    assert!(h.undo(&w));
    assert!(cutter_named(&w, NECKED).is_none(), "one undo restores the tool without a cutter");
    assert_eq!(tool_named(&w, NECKED), Some(cat(NECKED)));
    assert!(h.redo(&w));
    assert_eq!(cutter_named(&w, NECKED), cutter_named(&src, NECKED));
}

#[test]
fn a_library_without_a_cutter_keeps_version_4_or_lower() {
    let (src, _) = world_with(&first_two());
    let text = export_tool_library(&src).expect("export");
    let v: Value = serde_json::from_str(&text).expect("json");
    assert!(v["version"].as_u64().is_some_and(|n| n <= 4), "{text}");
    assert!(v["tools"][0]["definition"].get("cutter").is_none(), "{text}");
    // A file newer than this build refuses by version; nothing is dropped.
    // (#3368 took version 6 for the taper tip form, so 7 is the newer one.)
    let err = parse_tool_library(r#"{"format":"camcraft-tools","version":7,"tools":[]}"#).expect_err("version 7");
    assert!(err.contains("version 7") && err.contains("1 to 6"), "{err}");
}

/// #3369: a form mill rides the library file and the project file with
/// its outline, pilot and preset - the `form_mill` word, which an older
/// build refuses by name, so no version step - and a number edit
/// (`ChangeTool`) keeps the outline. The preset alone (a catalogue entry
/// without points) reads as the points the preset writes.
#[test]
fn a_form_mill_round_trips_with_its_outline() {
    use camcraft_app::commands::ChangeTool;
    use camcraft_app::tool_library::TIP_FORM_VERSION;
    let ro = cat("FM-RO-R6");
    let form = ro.form.clone().expect("a catalogue form mill states its form");
    assert!(form.points.len() >= 4 && form.pilot_len > 0.0, "{form:?}");
    let (w, mut h) = world_with(&[("RO", ro.clone())]);
    let text = export_tool_library(&w).expect("export");
    assert!(text.contains("\"form_mill\"") && text.contains("\"roundover\"") && text.contains("\"pilot_len\""), "{text}");
    let doc: Value = serde_json::from_str(&text).expect("json");
    assert!(doc["version"].as_u64().is_some_and(|v| v < TIP_FORM_VERSION), "a form mill steps no version: {doc}");
    let lib = parse_tool_library(&text).expect("the library reads back");
    assert_eq!(lib[0].tool, ro, "the library file keeps the form mill exactly");
    // The project file keeps it too, byte for byte.
    let saved = project::save_text(&w).expect("saved");
    let opened = new_world();
    project::open(&opened, &saved).expect("this build reads its own form mill");
    assert_eq!(tool_named(&opened, "RO"), Some(ro.clone()));
    assert_eq!(project::save_text(&opened).expect("saved again"), saved, "byte for byte");
    // A number edit keeps the drawn outline; one undo restores the tool.
    let edited = Tool { shank_len: ro.shank_len + 5.0, rpm: 12000.0, ..ro.clone() };
    h.execute(&w, ChangeTool::new(&w, "RO", edited.clone()).expect("a number edit"));
    assert_eq!(tool_named(&w, "RO").and_then(|t| t.form), Some(form.clone()), "the outline stays");
    assert!(h.undo(&w));
    assert_eq!(tool_named(&w, "RO"), Some(ro.clone()));
    // A diameter that leaves the outline is refused by name.
    let off = Tool { diameter: 20.0, ..ro.clone() };
    let why = ChangeTool::new(&w, "RO", off).err().map(|e| e.to_string()).unwrap_or_default();
    assert!(why.contains("diameter and flute length are its outline's"), "{why}");
    // The preset without points reads as the points the preset writes.
    let mut v: Value = serde_json::from_str(&text).expect("json");
    v["tools"][0]["definition"]["form"]["points"] = json!([]);
    let lib = parse_tool_library(&v.to_string()).expect("a preset without points reads");
    assert_eq!(lib[0].tool.form, Some(form));
}
