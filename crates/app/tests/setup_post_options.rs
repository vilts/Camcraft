//! #389: each setup's post options and the setup's own tool-list export.
//! A setup follows the document's post options until it is given its own
//! (`SetSetupPostOptions`, undoable); the skip switch is read per
//! operation from its setup, the Output and thread output are the active
//! setup's and a program whose setups disagree on them is refused by name.
//! A document that never sets per-setup options saves and posts byte for
//! byte as before. The tool-list export is a pure text read.
//! Provenance: our own fixtures over our own commands; no vendor material.
//!
//! Gate map (#389): covers app::setup_post_options, app::commands
//! (SetSetupPostOptions, SetPostOptions, CreateSetup datum transfer),
//! app::posting (post_options, op_streams skip switch, post_setup),
//! app::project (SetupDto post_options), app::tool_list_export.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    ActivateSetup, AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation,
    History, SetPostOptions, SetSetupPostOptions, SetStock,
};
use camcraft_app::operation::{AfterOperation, Height, OpParams, SrcRef};
use camcraft_app::post::cnc;
use camcraft_app::schema::{BetweenSetups, PostOptions, ThreadingOutput};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{model, posting, project, setup_post_options, tool_list_export};

const TOOL: &str = "EM-0400-2FL";

fn face() -> OpParams {
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

/// Stock, tool, `setups` setups (0 = none), one generated face per setup,
/// each face asking for a program stop after it.
fn doc(setups: usize) -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    if setups == 0 {
        face_op(&world, &mut h, "Face1");
    }
    for i in 1..=setups {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        if i > 1 {
            let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
            h.execute(&world, stock.expect("setup stock"));
        }
        face_op(&world, &mut h, &format!("Face{i}"));
    }
    (world, h)
}

fn face_op(world: &World, h: &mut History, name: &str) {
    let op = CreateOperation::new(world, name, TOOL, SrcRef::Stock, face());
    h.execute(world, op.expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let stop = EditOperationParams::with_after(world, name, AfterOperation::ProgramStop).expect("after edit");
    h.execute(world, stop);
}

fn tmp(tag: &str, ext: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!("camcraft-389-{tag}-{}.{ext}", std::process::id()));
    path
}

fn saved(world: &World, tag: &str) -> String {
    let path = tmp(tag, "ccam");
    project::save(world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    let _ = std::fs::remove_file(&path);
    text
}

fn post(world: &World, tag: &str) -> Result<String, String> {
    let path = tmp(tag, "nc");
    let out = posting::post_setup(world, &path).map(|p| p.program);
    let _ = std::fs::remove_file(&path);
    out
}

/// How many operations post their after-action (the stream's own record).
fn stops(world: &World) -> usize {
    cnc::serialize(&posting::stream_records(world).expect("streams")).matches("AfterOperation").count()
}

fn skip() -> PostOptions {
    PostOptions { skip_stops: true, ..PostOptions::default() }
}

#[test]
fn a_setups_own_post_options_do_undo_redo() {
    let (world, mut h) = doc(2);
    assert_eq!(setup_post_options::own(&world, Some("Setup2")), None, "a new setup follows the document");
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", Some(skip())).expect("own options"));
    assert_eq!(setup_post_options::own(&world, Some("Setup2")), Some(skip()));
    assert_eq!(posting::post_options(&world), skip(), "Setup2 is active: its options are in force");
    assert_eq!(setup_post_options::document(&world), PostOptions::default(), "the document keeps its own");
    h.undo(&world);
    assert_eq!(setup_post_options::own(&world, Some("Setup2")), None);
    assert_eq!(posting::post_options(&world), PostOptions::default());
    h.redo(&world);
    assert_eq!(setup_post_options::own(&world, Some("Setup2")), Some(skip()));
    // A repeat is refused by name, never an empty undo step.
    let again = SetSetupPostOptions::new(&world, "Setup2", Some(skip()));
    assert!(again.is_err_and(|e| e.to_string().contains("already has those post options")));
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", None).expect("follow again"));
    let again = SetSetupPostOptions::new(&world, "Setup2", None);
    assert!(again.is_err_and(|e| e.to_string().contains("already follows the document's post options")));
    let ghost = SetSetupPostOptions::new(&world, "Nope", None);
    assert!(ghost.is_err());
    // The document's own command still edits only the document.
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(PostOptions::default())).expect("own"));
    h.execute(&world, SetPostOptions::new(&world, skip()).expect("document skips"));
    assert_eq!(setup_post_options::own(&world, Some("Setup1")), Some(PostOptions::default()));
    assert_eq!(setup_post_options::of(&world, Some("Setup2")), skip(), "Setup2 follows the document");
}

#[test]
fn a_setup_that_skips_posts_without_its_stops_while_the_other_follows_the_document() {
    let (world, mut h) = doc(2);
    // #1532: the program runs straight on between the setups, so the
    // count reads the operations' own stops only (a setup change stops
    // whatever the skip switch says - `between_setups.rs`).
    let cont = |o: PostOptions| PostOptions { between_setups: BetweenSetups::Continue, ..o };
    h.execute(&world, SetPostOptions::new(&world, cont(PostOptions::default())).expect("run straight on"));
    assert_eq!(stops(&world), 2, "both faces stop");
    let both = post(&world, "skip-before").expect("posts");
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(cont(skip()))).expect("Setup1 skips"));
    assert_eq!(stops(&world), 1, "only Setup2's face keeps its stop");
    let one = post(&world, "skip-one").expect("posts: the skip switch may differ per setup");
    assert_ne!(one, both);
    // The document skipping too reaches Setup2 (which follows it) only.
    h.execute(&world, SetPostOptions::new(&world, cont(skip())).expect("document skips"));
    assert_eq!(stops(&world), 0);
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(cont(PostOptions::default()))).expect("own: keep stops"));
    assert_eq!(stops(&world), 1, "Setup1's own options keep its stop while the document skips");
}

#[test]
fn setups_disagreeing_on_the_output_are_refused_by_name() {
    let (world, mut h) = doc(2);
    let both = post(&world, "out-same").expect("posts");
    let g76 = PostOptions { threading: ThreadingOutput::G76Cycle, ..PostOptions::default() };
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(g76)).expect("own threading"));
    let why = post(&world, "out-diff").expect_err("refused");
    assert!(why.contains("setup 'Setup1' posts with other post options") && why.contains("'Setup2'"), "{why}");
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    assert!(posting::verification_gate(&world).iter().any(|p| p.contains("setup 'Setup2' posts with other post options")));
    // The same options on every setup post like the document.
    h.execute(&world, ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(PostOptions::default())).expect("own default"));
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", Some(PostOptions::default())).expect("own default"));
    assert_eq!(post(&world, "out-back").expect("posts"), both);
}

/// #1540: the End units are one word for the whole program. Setups that
/// differ in them are refused by name, whichever setup is active - never
/// posted with the active setup's word. Setups that agree post exactly
/// what the document's own choice posts.
#[test]
fn setups_disagreeing_on_the_end_units_are_refused_by_name() {
    use camcraft_app::schema::EndUnits;
    let (world, mut h) = doc(2);
    let inch = PostOptions { end_units: EndUnits::Inch, ..PostOptions::default() };
    let mm = PostOptions { end_units: EndUnits::Millimeters, ..PostOptions::default() };
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(inch)).expect("Setup1 ends in inch"));
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", Some(mm)).expect("Setup2 ends in mm"));
    let why = post(&world, "end-diff-2").expect_err("refused with Setup2 active");
    assert!(why.contains("setup 'Setup1' posts with other post options (end units) than the active setup 'Setup2'"), "{why}");
    h.execute(&world, ActivateSetup::new(&world, Some("Setup1")).expect("activate"));
    let why = post(&world, "end-diff-1").expect_err("refused with Setup1 active");
    assert!(why.contains("setup 'Setup2' posts with other post options (end units) than the active setup 'Setup1'"), "{why}");
    // A setup that follows the document is judged by the document's word.
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", None).expect("Setup2 follows"));
    assert!(post(&world, "end-follow").is_err(), "the document leaves the unit modal; Setup1 ends in inch");
    // Agreement posts the program the document's own choice posts.
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", None).expect("Setup1 follows"));
    h.execute(&world, SetPostOptions::new(&world, inch).expect("document ends in inch"));
    let document = post(&world, "end-doc").expect("one word: posts");
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", Some(inch)).expect("Setup2 owns inch"));
    assert_eq!(post(&world, "end-same").expect("posts"), document);
    // Two differences are both named.
    let both = PostOptions { per_op: true, end_units: EndUnits::Millimeters, ..PostOptions::default() };
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", Some(both)).expect("Setup2 differs twice"));
    let why = post(&world, "end-two").expect_err("refused");
    assert!(why.contains("(output, end units)"), "{why}");
}

#[test]
fn a_single_setup_document_posts_byte_identically() {
    for setups in [0, 1] {
        let (world, mut h) = doc(setups);
        let expected = post(&world, &format!("single-{setups}")).expect("posts");
        let file = saved(&world, &format!("single-{setups}"));
        assert!(!file.contains("skip_stops") && !file.contains("post_options"), "no option key written: {file}");
        if setups == 1 {
            let own = SetSetupPostOptions::new(&world, "Setup1", Some(PostOptions::default()));
            h.execute(&world, own.expect("own options equal to the document's"));
            assert_eq!(post(&world, "single-own").expect("posts"), expected, "the same options post the same program");
        }
    }
}

#[test]
fn own_post_options_round_trip_and_old_files_follow_the_document() {
    let (world, mut h) = doc(2);
    let untouched = saved(&world, "rt-untouched");
    let w = new_world();
    project::open(&w, &untouched).expect("a file without the key opens");
    assert_eq!(setup_post_options::own(&w, Some("Setup1")), None);
    assert_eq!(saved(&w, "rt-untouched-resave"), untouched, "saves as it was");

    // #596's end unit rides a setup's own options too (merge with main).
    let own = PostOptions {
        threading: ThreadingOutput::G76Cycle,
        skip_stops: true,
        per_op: true,
        end_units: camcraft_app::schema::EndUnits::Inch,
        smoothing: true,
        explain: false,
        between_setups: Default::default(),
    };
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", Some(own)).expect("own options"));
    let text = saved(&world, "rt");
    assert!(text.contains("\"per_op_files\": true") && text.contains("\"threading\": \"g76\""), "{text}");
    assert!(text.contains("\"end_units\": \"inch\""), "{text}");
    let w = new_world();
    project::open(&w, &text).expect("own options open");
    assert_eq!(setup_post_options::own(&w, Some("Setup2")), Some(own));
    assert_eq!(setup_post_options::own(&w, Some("Setup1")), None);
    assert_eq!(saved(&w, "rt-resave"), text);
    // An unspellable word is a corrupt file, refused by the setup's name.
    let bad = text.replace("\"threading\": \"g76\"", "\"threading\": \"g99\"");
    let refused = project::open(&new_world(), &bad);
    assert!(refused.is_err_and(|e| e.to_string().contains("setup 'Setup2'")));
}

#[test]
fn a_datum_transfer_keeps_the_source_setups_post_options() {
    let (world, mut h) = doc(1);
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(skip())).expect("own"));
    h.execute(&world, CreateSetup::new(&world, None, Some("Setup1")).expect("datum transfer"));
    assert_eq!(setup_post_options::own(&world, Some("Setup2")), Some(skip()));
}

#[test]
fn the_tool_list_exports_as_csv_and_text() {
    let (world, mut h) = doc(2);
    let second = "EM-0600-4FL";
    let tool = catalog().into_iter().find(|(n, _)| *n == second).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, second, tool.clone()).expect("add tool"));
    face_op_with(&world, &mut h, "Face3", second);
    let first = model::find_by_name(&world, TOOL).and_then(|e| model::tool_of(&world, &e)).expect("first tool");

    let csv = tool_list_export::csv(&world, Some("Setup2"));
    let lines: Vec<&str> = csv.split("\r\n").collect();
    assert_eq!(lines[0], "T,Tool,Diameter mm,Kind,Holder,Operations");
    let holder = |d: f64, l: f64| if d > 0.0 && l > 0.0 { format!("{d:.3} x {l:.3}") } else { "none".to_owned() };
    assert_eq!(
        lines[1],
        format!("1,{TOOL},{:.3},{},{},Face2", first.diameter, first.kind, holder(first.holder_d, first.holder_len))
    );
    assert_eq!(
        lines[2],
        format!("2,{second},{:.3},{},{},Face3", tool.diameter, tool.kind, holder(tool.holder_d, tool.holder_len))
    );
    assert_eq!(lines.len(), 4, "header, two tools, trailing line end: {csv}");
    assert!(!csv.contains("Face1"), "Setup1's operation is not on Setup2's list");

    let text = tool_list_export::text(&world, Some("Setup2"));
    assert!(text.starts_with("TOOL LIST - Setup2\n"), "{text}");
    assert!(text.contains(TOOL) && text.contains(second) && text.contains("2 tool(s)"), "{text}");
    let one = tool_list_export::text(&world, Some("Setup1"));
    assert!(one.contains("Face1") && !one.contains(second), "{one}");
    // The path's extension picks the format.
    assert_eq!(tool_list_export::for_path(&world, Some("Setup2"), std::path::Path::new("a.CSV")), csv);
    assert_eq!(tool_list_export::for_path(&world, Some("Setup2"), std::path::Path::new("a.txt")), text);
}

#[test]
fn a_tool_list_field_with_a_comma_is_quoted() {
    let (world, mut h) = doc(0);
    let odd = "Mill, \"long\"";
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, odd, tool).expect("add tool"));
    face_op_with(&world, &mut h, "Face2", odd);
    let csv = tool_list_export::csv(&world, None);
    assert!(csv.contains("\"Mill, \"\"long\"\"\""), "{csv}");
    let empty = new_world();
    assert_eq!(tool_list_export::csv(&empty, None), "T,Tool,Diameter mm,Kind,Holder,Operations\r\n");
    assert!(tool_list_export::text(&empty, None).contains("no operation"));
}

fn face_op_with(world: &World, h: &mut History, name: &str, tool: &str) {
    let op = CreateOperation::new(world, name, tool, SrcRef::Stock, face());
    h.execute(world, op.expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

/// #389 x #155: a Pause / Home entry folds into the gap of the op before
/// it, so the skip switch of THAT op's setup mutes it - an entry after a
/// skipping setup's op neither posts nor refuses, while the other setup
/// keeps its stop.
#[test]
fn an_entry_in_a_skipping_setups_gap_is_muted_with_its_op() {
    use camcraft_app::commands::AddProgramEntry;
    let (world, mut h) = doc(2);
    h.execute(&world, AddProgramEntry::new(&world, AfterOperation::GoHome, Some("Face2")).expect("entry"));
    let err = posting::op_streams(&world).expect_err("a stop and a trip home in one gap");
    assert!(err.contains("already ends with"), "{err}");
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup2", Some(skip())).expect("Setup2 skips"));
    posting::op_streams(&world).unwrap_or_else(|e| panic!("the muted gap posts: {e}"));
    assert_eq!(stops(&world), 1, "Setup1's face keeps its stop; Setup2's op and its entry are muted");
    h.undo(&world);
    assert!(posting::op_streams(&world).is_err(), "undo brings the clash back");
}

/// How many operations carry a smoothing ask in the stream.
fn smoothed(world: &World) -> usize {
    cnc::serialize(&posting::stream_records(world).expect("streams"))
        .lines()
        .filter(|l| l.starts_with("PARAM Smoothing "))
        .count()
}

/// #616 + #389 (review of #635): smoothing is read per op, from the op's
/// OWN setup, like "Skip pauses and homes" - never from whichever setup
/// happens to be active. Setup1 turns it on, Setup2 follows the document
/// (off): exactly Setup1's pocket asks, whichever setup is active. (A
/// Face asks no smoothing, so each setup also gets a Pocket over its
/// stock - a kind that does.)
#[test]
fn smoothing_follows_each_ops_own_setup_not_the_active_one() {
    let (world, mut h) = doc(2);
    let mut pocket = camcraft_app::operation::retype_params(&face(), camcraft_app::operation::OpType::Pocket.ordinal());
    // No model here: the heights are stated, on the 20 mm stock.
    if let OpParams::Pocket { clearance_z, retract_z, top_z, bottom_z, .. } = &mut pocket {
        *clearance_z = Height::fixed(25.0);
        *retract_z = None;
        *top_z = Height::fixed(20.0);
        *bottom_z = Height::fixed(19.0);
    }
    for (i, setup) in ["Setup1", "Setup2"].iter().enumerate() {
        h.execute(&world, ActivateSetup::new(&world, Some(setup)).expect("activate"));
        let name = format!("Pocket{}", i + 1);
        h.execute(&world, CreateOperation::new(&world, &name, TOOL, SrcRef::Stock, pocket.clone()).expect("pocket"));
        h.execute(&world, GenerateOperation::new(&world, &name).expect("generate"));
    }
    assert_eq!(smoothed(&world), 0, "off everywhere by default");
    let on = PostOptions { smoothing: true, ..PostOptions::default() };
    h.execute(&world, SetSetupPostOptions::new(&world, "Setup1", Some(on)).expect("Setup1 smooths"));
    for active in ["Setup1", "Setup2"] {
        h.execute(&world, ActivateSetup::new(&world, Some(active)).expect("activate"));
        assert_eq!(smoothed(&world), 1, "only Setup1's pocket smooths, with {active} active");
    }
    // The document turning it on reaches Setup2 (which follows it) too.
    h.execute(&world, SetPostOptions::new(&world, on).expect("document smooths"));
    assert_eq!(smoothed(&world), 2);
}
