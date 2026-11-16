//! #931 (slice 2): a setup's work offset is a 1-based index, and the
//! setup's own post bounds it and names it. An index the post cannot
//! express blocks the post by setup name - it is never read as offset 1
//! (G54) anywhere: not in the label, the setup sheet or the stream.
//! Offsets 1-6 keep their legacy words in the project file; 7 and up are
//! the number.
//! Provenance: our own fixtures over RS-274 work-offset semantics; the
//! extended Tormach words are the LinuxCNC G-code reference's G59.1-G59.3.
//!
//! Gate map (#931): covers app::setup_offsets (index, table, check,
//! range_problems, names), app::posting (wcs_context, verification_report,
//! post_setup, post_per_op), app::commands (SetSetupWorkOffset,
//! SetSetupPost), app::project (save, load), app::setup_sheet.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, GenerateOperation, History, SetSetupPost, SetSetupWorkOffset, SetStock,
};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::PostRef;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project, setup, setup_offsets, setup_sheet};

const TOOL: &str = "EM-0400-2FL";
const TORMACH: &str = "Tormach PathPilot";

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

/// A faced 60 x 60 x 20 box in Setup1, which posts with `post`.
fn faced_block(post: Option<&str>) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    h.execute(&world, CreateSetup::new(&world, None, None).expect("materialize Setup1"));
    if let Some(p) = post {
        set_post(&world, &mut h, Some(p));
    }
    let op = CreateOperation::new(&world, "Face1", TOOL, SrcRef::Stock, face_params());
    h.execute(&world, op.expect("create"));
    generate(&world, &mut h);
    (world, h)
}

fn generate(world: &World, h: &mut History) {
    h.execute(world, GenerateOperation::new(world, "Face1").expect("generate"));
}

fn set_post(world: &World, h: &mut History, post: Option<&str>) {
    let next = post.map(|p| PostRef::Named(p.to_owned()));
    h.execute(world, SetSetupPost::new(world, "Setup1", next).expect("setup post"));
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("camcraft-wcsindex-{tag}-{}", std::process::id()));
    p
}

fn post(world: &World, tag: &str) -> (Result<String, String>, bool) {
    let path = tmp(tag).with_extension("nc");
    let _ = std::fs::remove_file(&path);
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let written = path.exists();
    let _ = std::fs::remove_file(&path);
    (out, written)
}

fn has_word(nc: &str, w: &str) -> bool {
    nc.lines().any(|l| l.split_whitespace().any(|t| t == w))
}

/// THE no-fallback pin (triage point 2): a setup at offset 9 switched to
/// a 6-offset post refuses by setup name - it never posts at offset 1,
/// and no label or sheet reads it as G54.
#[test]
fn offset_nine_on_a_six_offset_post_refuses_by_setup_name_and_never_posts_at_offset_one() {
    let (world, mut h) = faced_block(Some(TORMACH));
    h.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", Some(8)).expect("Tormach has offset 9"));
    generate(&world, &mut h);
    let (nc, _) = post(&world, "tormach9");
    let nc = nc.expect("Tormach posts offset 9");
    assert!(has_word(&nc, "G59.3") && !has_word(&nc, "G54"), "{nc}");
    assert_eq!(setup::wcs_label(&world, Some("Setup1")), "WCS (G59.3)");

    // Switch the setup to a post with six offsets: the stored 9 stays 9.
    set_post(&world, &mut h, Some("Fanuc"));
    assert_eq!(setup::work_offset(&world, "Setup1"), Some(8), "the pick is never filtered");
    assert_eq!(setup_offsets::index(&world, Some("Setup1")), 9);
    let problems = posting::verification_report(&world).problems;
    assert!(
        problems.iter().any(|p| p.contains("setup Setup1 uses work offset 9") && p.contains("1-6")),
        "{problems:?}"
    );
    let (out, written) = post(&world, "fanuc9");
    let err = out.expect_err("offset 9 on a 6-offset post is refused");
    assert!(err.contains("setup Setup1") && err.contains("work offset 9"), "{err}");
    assert!(!written, "nothing is written");
    let dir = tmp("perop9");
    let _ = std::fs::create_dir_all(&dir);
    let per_op = posting::post_per_op(&world, &dir, "P");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(per_op.expect_err("per-op refuses too").contains("setup Setup1 uses work offset 9"));

    // Display never falls back to another offset's name.
    let label = setup::wcs_label(&world, Some("Setup1"));
    assert_eq!(label, "WCS (work offset 9)", "{label}");
    let sheet = setup_sheet::render(&world, Some("Setup1")).expect("sheet");
    assert!(sheet.contains("program offset: work offset 9\n") && !sheet.contains("program offset: G54"), "{sheet}");
}

#[test]
fn the_command_refuses_an_offset_the_setups_post_cannot_express() {
    let (world, mut h) = faced_block(None);
    let err = SetSetupWorkOffset::new(&world, "Setup1", Some(6)).err().expect("Fanuc has six offsets");
    let msg = err.to_string();
    assert!(msg.contains("setup Setup1 uses work offset 7") && msg.contains("1-6"), "{msg}");
    set_post(&world, &mut h, Some(TORMACH));
    assert!(SetSetupWorkOffset::new(&world, "Setup1", Some(8)).is_ok(), "Tormach has nine");
    assert!(SetSetupWorkOffset::new(&world, "Setup1", Some(9)).is_err(), "but not ten");
    assert!(SetSetupWorkOffset::new(&world, "Setup1", Some(setup_offsets::MAX_SLOT + 1)).is_err(), "the sanity bound");
}

#[test]
fn the_setups_post_names_its_offsets() {
    let (world, mut h) = faced_block(Some(TORMACH));
    let t = setup::work_offset_names(&world, Some("Setup1"));
    assert_eq!(t.max, 9);
    assert_eq!(setup::offset_name(&world, Some("Setup1"), 1), "G54");
    assert_eq!(setup::offset_name(&world, Some("Setup1"), 7), "G59.1");
    assert_eq!(setup::offset_name(&world, Some("Setup1"), 10), "work offset 10", "never another offset's name");
    set_post(&world, &mut h, None);
    assert_eq!(setup::work_offset_names(&world, Some("Setup1")).max, 6, "the document's Fanuc");
}

#[test]
fn offset_seven_and_up_saves_as_its_number_and_reloads() {
    let (world, mut h) = faced_block(Some(TORMACH));
    h.execute(&world, SetSetupWorkOffset::new(&world, "Setup1", Some(6)).expect("offset 7"));
    let path = tmp("proj").with_extension("ccam");
    project::save(&world, &path).expect("save");
    let first = std::fs::read_to_string(&path).expect("read");
    assert!(first.contains("\"work_offset\": \"7\""), "{first}");
    let world2 = new_world();
    project::load(&world2, &path).expect("load");
    assert_eq!(setup::work_offset(&world2, "Setup1"), Some(6));
    project::save(&world2, &path).expect("resave");
    let second = std::fs::read_to_string(&path).expect("reread");
    let _ = std::fs::remove_file(&path);
    assert_eq!(first, second, "byte-identical round trip");
    // A pick past the sanity bound is a corrupt file, not a guess.
    let bad = first.replace("\"work_offset\": \"7\"", "\"work_offset\": \"256\"");
    let err = project::open(&new_world(), &bad).expect_err("256 is no offset");
    assert!(err.to_string().contains("256"), "{err}");
}
