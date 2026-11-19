//! #196, #342: a setup has one WCS, and the work offset (G54..G59) is
//! only the word its post writes for it. The label is an undoable
//! command, saved with the project, and posting reads it for every
//! operation of the setup: relabelling changes the G-word and nothing
//! else; placing the WCS moves every coordinate by exactly its
//! placement. A height measured from the work offset reads the setup's
//! WCS (#329), so a relabel moves no height. Two setups of one program
//! posting one word from different placements refuse by name.
//! Provenance: our own fixtures over RS-274 work-offset semantics.
//!
//! Gate map (#196): covers app::posting (wcs_context, verification_report,
//! post_setup, post_per_op), app::commands (SetSetupWorkOffset,
//! CreateSetup), app::project (save, load, open), app::setup_sheet,
//! app::setup_origin.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History,
    SetSetupWorkOffset, SetStock, SetWcsOrigin,
};
use camcraft_app::geometry::{IDENTITY_Q, Point3};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project, setup, setup_sheet};

const TOOL: &str = "EM-0400-2FL";

fn face_params() -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(19.5),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// A 60 x 60 x 20 box faced at Z 19.5, inside a materialized Setup1.
fn faced_block() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    let op = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face_params());
    h.execute(&world, op.expect("create"));
    generate(&world, &mut h, "Face1");
    (world, h)
}

fn generate(world: &World, h: &mut History, name: &str) {
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// Place the active setup's one WCS.
fn place(world: &World, h: &mut History, x: f64, y: f64, z: f64) {
    let loc = Location { pos: Point3::new(x, y, z), rot: IDENTITY_Q };
    h.execute(world, SetWcsOrigin::new(world, loc).expect("place the WCS"));
}

fn pick(world: &World, h: &mut History, setup: &str, slot: Option<usize>) {
    h.execute(world, SetSetupWorkOffset::new(world, setup, slot).expect("pick"));
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-setupwcs-{tag}-{}", std::process::id()));
    p
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let path = tmp(tag).with_extension("nc");
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let _ = std::fs::remove_file(&path);
    out
}

/// Every `X`/`Y`/`Z` word of a program's work-offset moves, in order, as
/// (axis, value). `G28` homes are machine-reference words, never shifted
/// by a work offset, so their lines are left out.
fn axis_words(nc: &str) -> Vec<(char, f64)> {
    nc.lines()
        .filter(|l| !l.contains("G28"))
        .flat_map(str::split_whitespace)
        .filter_map(|t| {
            let mut c = t.chars();
            let a = c.next()?;
            if !matches!(a, 'X' | 'Y' | 'Z') {
                return None;
            }
            c.as_str().parse::<f64>().ok().map(|v| (a, v))
        })
        .collect()
}

/// The program with every axis value blanked: what must not change.
fn shape(nc: &str) -> String {
    nc.split_whitespace()
        .map(|t| match t.chars().next() {
            Some(a @ ('X' | 'Y' | 'Z')) if t[1..].parse::<f64>().is_ok() => a.to_string(),
            _ => t.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn a_relabelled_wcs_posts_its_word_and_placing_it_moves_coordinates_by_exactly_the_offset() {
    let (world, mut h) = faced_block();
    let g54 = post(&world, "g54").expect("the unlabelled setup posts at G54");
    assert!(g54.contains("G54") && !g54.contains("G55"), "{g54}");

    // #342: the label is only the word - the same origin, the same numbers.
    pick(&world, &mut h, "Setup1", Some(1));
    let relabelled = post(&world, "g55-label").expect("the G55 setup posts");
    assert!(relabelled.contains("G55") && !relabelled.contains("G54"), "{relabelled}");
    assert_eq!(g54.replace("G54", "G55"), relabelled, "only the word moved");

    // Placing the setup's WCS moves every coordinate by exactly it.
    place(&world, &mut h, 10.0, 20.0, 5.0);
    // A milling path is world geometry: the placement moves no input, so
    // the cached path stays fresh and posts through the new origin.
    let g55 = post(&world, "g55").expect("the placed G55 setup posts");
    assert_eq!(shape(&g54).replace("G54", "G55"), shape(&g55), "only the word and the numbers move");
    let (a, b) = (axis_words(&g54), axis_words(&g55));
    assert_eq!(a.len(), b.len());
    assert!(!a.is_empty());
    for ((ax, va), (bx, vb)) in a.iter().zip(&b) {
        assert_eq!(ax, bx);
        let shift = match ax {
            'X' => 10.0,
            'Y' => 20.0,
            _ => 5.0,
        };
        assert!((va - vb - shift).abs() < 1e-3, "{ax}{va} vs {bx}{vb}");
    }
}

#[test]
fn the_pick_undoes_and_redoes_exactly() {
    let (world, mut h) = faced_block();
    assert_eq!(setup::work_offset(&world, "Setup1"), None, "a new setup picks nothing");
    pick(&world, &mut h, "Setup1", Some(3));
    assert_eq!(setup::work_offset(&world, "Setup1"), Some(3));
    assert!(h.undo(&world));
    assert_eq!(setup::work_offset(&world, "Setup1"), None);
    assert!(h.redo(&world));
    assert_eq!(setup::work_offset(&world, "Setup1"), Some(3));
    // Validated at construction: no slot 6, no repeat, no non-setup.
    assert!(SetSetupWorkOffset::new(&world, "Setup1", Some(6)).is_err());
    assert!(SetSetupWorkOffset::new(&world, "Setup1", Some(3)).is_err());
    assert!(SetSetupWorkOffset::new(&world, "Face1", Some(1)).is_err());
    assert!(SetSetupWorkOffset::new(&world, "Nope", Some(1)).is_err());
}

#[test]
fn a_datum_transfer_carries_the_pick() {
    let (world, mut h) = faced_block();
    pick(&world, &mut h, "Setup1", Some(2));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("transfer"));
    assert_eq!(setup::work_offset(&world, "Setup2"), Some(2));
}

#[test]
fn the_pick_round_trips_and_old_files_read_the_inferred_law() {
    let (world, mut h) = faced_block();
    pick(&world, &mut h, "Setup1", Some(1));
    let path = tmp("proj").with_extension("ccam");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"work_offset\": \"G55\""), "{first}");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(setup::work_offset(&world2, "Setup1"), Some(1));
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "byte-identical round trip");

    // An unpicked setup writes no key.
    assert!(h.undo(&world));
    project::save(&world, &path).expect("save unpicked");
    let unpicked = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    assert!(!unpicked.contains("work_offset"), "{unpicked}");

    // A v50 file (no key) reads as no pick; an unspellable word is corrupt.
    let old = r#"{"version": 50, "entities": [
        {"name": "Setup1", "translation": [0.0, 0.0, 0.0], "kind": "setup",
         "setup": {"stock": "Stock", "wcs": ["G54", "G55"]}}]}"#;
    let w = new_world();
    project::open(&w, old).expect("a v50 setup reads");
    assert_eq!(setup::work_offset(&w, "Setup1"), None);
    let bad = old
        .replace("\"version\": 50", &format!("\"version\": {}", project::PROJECT_VERSION))
        .replace("\"wcs\": [\"G54\", \"G55\"]", "\"wcs\": \"G54\", \"work_offset\": \"G60\"");
    let err = project::open(&new_world(), &bad).expect_err("G60 is no slot");
    assert!(err.to_string().contains("G60"), "{err}");
}

#[test]
fn a_relabel_moves_no_height_and_undoes_to_the_old_word() {
    // #342: a height names no slot (#329) and reads the setup's one WCS.
    // Labelling it G56 changes the word the post writes, never the plane
    // a height reads: the op stays fresh, and undo restores the word.
    let (world, mut h) = faced_block();
    let mut p = face_params();
    if let OpParams::Face { clearance_z, cut_z, .. } = &mut p {
        *clearance_z = Height::of(HeightReference::WorkOffset, 25.0);
        *cut_z = Height::of(HeightReference::WorkOffset, 19.5);
    }
    place(&world, &mut h, 0.0, 0.0, -3.0);
    h.execute(&world, EditOperationParams::new(&world, "Face1", p).expect("edit"));
    generate(&world, &mut h, "Face1");
    let ctx = Some("Setup1");
    let cut = Height::of(HeightReference::WorkOffset, 19.5);
    assert_eq!(cut.z_for(&world, ctx), Some(16.5), "the setup's WCS plane");
    let g54 = post(&world, "wo-g54").expect("posts");
    assert!(g54.contains("G54") && !g54.contains("G56"), "{g54}");

    pick(&world, &mut h, "Setup1", Some(2));
    assert_eq!(cut.z_for(&world, ctx), Some(16.5), "the label moved no height");
    // The stamp digests the word a work-offset height posts under (kept
    // from #329 so migrated stamps stay byte-identical): the op asks for
    // one regenerate, which lands the very same path.
    generate(&world, &mut h, "Face1");
    let g56 = post(&world, "wo-g56").expect("posts at the new word");
    assert_eq!(g54.replace("G54", "G56"), g56, "only the word changed");

    assert!(h.undo(&world)); // the regenerate
    assert!(h.undo(&world));
    assert_eq!(setup::work_offset(&world, "Setup1"), None);
    assert_eq!(post(&world, "wo-undo").expect("posts"), g54, "undo restores the old word");
    assert!(h.redo(&world));
    assert_eq!(setup::work_offset(&world, "Setup1"), Some(2));
}

#[test]
fn two_setups_on_one_word_from_different_placements_refuse_one_file() {
    let (world, mut h) = faced_block();
    h.execute(&world, CreateSetup::new(&world, None, None).expect("Setup2"));
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("Setup2 stock"));
    place(&world, &mut h, 100.0, 0.0, 0.0); // Setup2/WCS, the active setup's
    let op = CreateOperation::new(&world, "Face2", TOOL, SrcRef::Stock, face_params());
    h.execute(&world, op.expect("create"));
    generate(&world, &mut h, "Face2");
    let err = post(&world, "shared").expect_err("one G54 register, two zeros");
    assert!(
        err.contains("setups Setup1 and Setup2 both post at G54 from different placements")
            && err.contains("pick another work offset for Setup2"),
        "{err}"
    );
    // Per-operation files are each one setup's: exempt.
    let dir = tmp("perop");
    let _ = std::fs::create_dir_all(&dir);
    let files = posting::post_per_op(&world, &dir, "P").expect("per-op files post");
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(files.len(), 2);
    // Labelling Setup2's WCS G55 resolves it - its origin stays put.
    pick(&world, &mut h, "Setup2", Some(1));
    let nc = post(&world, "resolved").expect("distinct words post");
    assert!(nc.contains("G54") && nc.contains("G55"), "{nc}");
}

#[test]
fn the_setup_sheet_names_the_program_offset() {
    let (world, mut h) = faced_block();
    let sheet = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet.contains("program offset: G54\n"), "{sheet}");
    pick(&world, &mut h, "Setup1", Some(4));
    let sheet = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet.contains("program offset: G58\n"), "{sheet}");
}

/// #870: the sheet says where the origin is in words - what the
/// operator touches off - beside its numbers.
#[test]
fn the_setup_sheet_says_where_the_origin_is_in_words() {
    let (world, mut h) = faced_block();
    let b = camcraft_app::model::stock_world_aabb_for(&world, Some("Setup1")).expect("stock box");
    place(&world, &mut h, b.min.x, b.min.y, b.max.z);
    let sheet = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet.contains("  origin: top of stock, front-left corner (X0 Y0 Z0)\n"), "{sheet}");
    place(&world, &mut h, (b.min.x + b.max.x) * 0.5, (b.min.y + b.max.y) * 0.5, b.max.z);
    let sheet = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet.contains("  origin: top of stock, centre (X0 Y0 Z0)\n"), "{sheet}");
    // Off every anchor point: distances, never a corner it is not on.
    place(&world, &mut h, b.min.x + 12.0, b.min.y, b.max.z + 1.5);
    let sheet = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(
        sheet.contains("  origin: off the stock's corners: X +12.000 mm from its left side, Y 0.000 mm from its front, Z +1.500 mm from its top (X0 Y0 Z0)\n"),
        "{sheet}"
    );
}
