//! #899: the tool library import modes. Add keeps the duplicate law (see
//! tool_library.rs); Update puts the file's definition into a same-named
//! tool in place (same entity, same controller number, operations go
//! Stale, undo restores exactly); Replace is Update plus removing the
//! document tools the file lacks, refused by name when an operation uses
//! one. Every mode is one undo unit.
//! Provenance: our own tests over our own command model.
//!
//! Gate map (M4.4): covers app::commands (tool_import), app::script (importTools).

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportMode, ImportTools, SetStock, SetToolNumber,
};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tool_library::LibraryTool;
use camcraft_app::tools::{Tool, catalog};
use camcraft_app::world::new_world;
use flecs_ecs::prelude::World;

fn cat(name: &str) -> Tool {
    catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool")
}

fn lib(items: &[(&str, Tool)]) -> Vec<LibraryTool> {
    items.iter().map(|(n, t)| LibraryTool::bare(*n, t.clone())).collect()
}

fn tool_named(w: &World, n: &str) -> Option<Tool> {
    model::find_by_name(w, n).and_then(|e| model::tool_of(w, &e))
}

fn number_of(w: &World, n: &str) -> Option<u32> {
    model::find_by_name(w, n).and_then(|e| model::tool_number_of(w, &e))
}

fn do_import(w: &World, h: &mut History, items: &[(&str, Tool)], mode: ImportMode) -> Result<String, String> {
    let cmd = ImportTools::with_mode(w, lib(items), mode).map_err(|e| e.to_string())?;
    let s = cmd.report().summary();
    h.execute(w, cmd);
    Ok(s)
}

fn fatter(t: &Tool) -> Tool {
    Tool { diameter: t.diameter + 1.0, ..t.clone() }
}

/// Stock, tools `T1` (end mill, number 7) and `T2`, one generated Face op on `T1`.
fn world_with_op() -> (World, History) {
    let w = new_world();
    let mut h = History::default();
    let stock = StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 };
    h.execute(&w, SetStock::new(&w, stock, Location::default()).expect("stock"));
    h.execute(&w, AddTool::new(&w, "T1", cat("EM-0400-2FL")).expect("T1"));
    h.execute(&w, AddTool::new(&w, "T2", cat("EM-0600-4FL")).expect("T2"));
    h.execute(&w, SetToolNumber::new(&w, "T1", Some(7)).expect("number"));
    let params = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    h.execute(&w, CreateOperation::new(&w, "Face1", "T1", SrcRef::Stock, params).expect("op"));
    h.execute(&w, GenerateOperation::new(&w, "Face1").expect("generate"));
    (w, h)
}

fn status(w: &World, n: &str) -> OpStatus {
    let e = model::find_by_name(w, n).expect("entity");
    camcraft_app::operation::status_of(w, &e)
}

#[test]
fn update_changes_in_place_keeps_identity_and_number_and_stales_ops() {
    let (w, mut h) = world_with_op();
    let before = model::find_by_name(&w, "T1").expect("T1");
    let old = tool_named(&w, "T1").expect("tool");
    assert_eq!(status(&w, "Face1"), OpStatus::Ok);
    let next = fatter(&old);
    let s = do_import(&w, &mut h, &[("T1", next.clone())], ImportMode::Update).expect("update");
    assert!(s.contains("1 updated (T1)") && !s.contains("imported"), "{s}");
    assert_eq!(model::find_by_name(&w, "T1"), Some(before), "same entity, not a rename");
    assert_eq!(tool_named(&w, "T1"), Some(next));
    assert_eq!(number_of(&w, "T1"), Some(7), "the controller number stays");
    assert_eq!(status(&w, "Face1"), OpStatus::Stale, "the op reads the new numbers");
    assert!(model::find_by_name(&w, "T1 (2)").is_none(), "Update never renames");
    h.undo(&w);
    assert_eq!(tool_named(&w, "T1"), Some(old), "undo restores the definition exactly");
    assert_eq!(number_of(&w, "T1"), Some(7));
    assert_eq!(status(&w, "Face1"), OpStatus::Ok, "and the op is fresh again");
    h.redo(&w);
    assert_eq!(number_of(&w, "T1"), Some(7));
    assert_ne!(status(&w, "Face1"), OpStatus::Ok);
}

#[test]
fn update_adds_unmatched_and_skips_identical_in_one_undo() {
    let (w, mut h) = world_with_op();
    let same = tool_named(&w, "T2").expect("T2");
    let items = [("T2", same), ("New", cat("EM-0800-4FL")), ("T1", fatter(&cat("EM-0400-2FL")))];
    let s = do_import(&w, &mut h, &items, ImportMode::Update).expect("update");
    assert!(s.contains("imported 1 tool") && s.contains("1 updated (T1)") && s.contains("already in the document (T2)"), "{s}");
    assert!(tool_named(&w, "New").is_some());
    assert_eq!(number_of(&w, "New"), None);
    h.undo(&w);
    assert!(tool_named(&w, "New").is_none(), "one undo removes the add");
    assert_eq!(tool_named(&w, "T1"), Some(cat("EM-0400-2FL")), "and reverts the update");
}

#[test]
fn update_of_an_identical_library_has_nothing_to_do() {
    let (w, _h) = world_with_op();
    let same = tool_named(&w, "T1").expect("T1");
    let err = ImportTools::with_mode(&w, lib(&[("T1", same)]), ImportMode::Update).err().expect("nothing").to_string();
    assert!(err.contains("nothing to import"), "{err}");
}

#[test]
fn update_refuses_a_name_that_is_not_a_tool() {
    let (w, _h) = world_with_op();
    let err = ImportTools::with_mode(&w, lib(&[("Face1", cat("EM-0400-2FL"))]), ImportMode::Update)
        .err()
        .expect("refused")
        .to_string();
    assert!(err.contains("Face1") && err.contains("not a tool"), "{err}");
}

#[test]
fn update_refuses_a_retype_that_strands_an_operation() {
    let (w, _h) = world_with_op();
    let drill = catalog().into_iter().map(|(_, t)| t).find(|t| t.kind != cat("EM-0400-2FL").kind).expect("other kind");
    let e = ImportTools::with_mode(&w, lib(&[("T1", drill)]), ImportMode::Update).err().expect("stranded").to_string();
    assert!(e.contains("T1") && e.contains("Face1"), "{e}");
}

#[test]
fn replace_removes_unmatched_unused_tools_and_undo_restores_them_exactly() {
    let (w, mut h) = world_with_op();
    let t2 = tool_named(&w, "T2").expect("T2");
    h.execute(&w, SetToolNumber::new(&w, "T2", Some(9)).expect("number"));
    let items = [("T1", fatter(&cat("EM-0400-2FL"))), ("Fresh", cat("EM-0800-4FL"))];
    let s = do_import(&w, &mut h, &items, ImportMode::Replace).expect("replace");
    assert!(s.contains("1 replaced (T1)") && s.contains("1 removed (T2)") && s.contains("imported 1 tool"), "{s}");
    assert!(tool_named(&w, "T2").is_none(), "the library lacked T2");
    assert!(tool_named(&w, "Fresh").is_some());
    assert_eq!(number_of(&w, "T1"), Some(7));
    h.undo(&w);
    assert_eq!(tool_named(&w, "T2"), Some(t2), "T2 is back, exactly");
    assert_eq!(number_of(&w, "T2"), Some(9), "with its number");
    assert!(tool_named(&w, "Fresh").is_none());
    assert_eq!(tool_named(&w, "T1"), Some(cat("EM-0400-2FL")));
    h.redo(&w);
    assert!(tool_named(&w, "T2").is_none());
}

#[test]
fn replace_refuses_to_remove_a_tool_an_operation_uses() {
    let (w, h) = world_with_op();
    let before = tool_named(&w, "T2");
    // The library lacks T1, which Face1 cuts with.
    let err = ImportTools::with_mode(&w, lib(&[("T2", fatter(&cat("EM-0600-4FL")))]), ImportMode::Replace)
        .err()
        .expect("refused")
        .to_string();
    assert!(err.contains("T1") && err.contains("Face1") && err.contains("nothing imported"), "{err}");
    assert_eq!(tool_named(&w, "T2"), before, "nothing applied");
    assert!(tool_named(&w, "T1").is_some());
    drop(h);
}

#[test]
fn add_stays_the_default_and_renames_a_different_duplicate() {
    assert_eq!(ImportMode::default(), ImportMode::Add);
    let (w, mut h) = world_with_op();
    let next = fatter(&cat("EM-0400-2FL"));
    let s = do_import(&w, &mut h, &[("T1", next.clone())], ImportMode::Add).expect("add");
    assert!(s.contains("T1 -> T1 (2)"), "{s}");
    assert_eq!(tool_named(&w, "T1"), Some(cat("EM-0400-2FL")), "the original is untouched");
    assert_eq!(tool_named(&w, "T1 (2)"), Some(next));
    assert_eq!(status(&w, "Face1"), OpStatus::Ok);
}

#[test]
fn modes_parse_by_name() {
    assert_eq!(ImportMode::parse("Update"), Ok(ImportMode::Update));
    assert_eq!(ImportMode::parse(" replace "), Ok(ImportMode::Replace));
    assert_eq!(ImportMode::parse("add"), Ok(ImportMode::Add));
    assert!(ImportMode::parse("merge").expect_err("unknown").contains("merge"));
}
