//! Standalone Pause and Home entries in the operation list (#155): add,
//! reorder, delete and undo; the words the post writes for them (the
//! per-op after-action's own); Suppress and "Skip pauses and homes";
//! the project file; and the program walks that must not see them.
//!
//! Gate map (M4.4): covers app::program_entry, app::commands
//! (AddProgramEntry, EditProgramEntry, ReorderOperation, DeleteEntity,
//! CreateOperation, DeleteSetup), app::posting (op_streams,
//! stream_records), app::project (program_entry_dto), app::model
//! (is_model), assets/posts/fanuc.cps, haas.cps, mazak.cps, sinumerik.cps,
//! fanuc-lathe.cps, tormach.cps.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddProgramEntry, AddTool, CreateOperation, DeleteEntity, EditOperationParams, EditProgramEntry,
    GenerateOperation, History, ReorderOperation, SetPostOptions, SetStock,
};
use camcraft_app::model;
use camcraft_app::operation::{self, AfterOperation, Height, OpParams, SrcRef};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting;
use camcraft_app::program_entry;
use camcraft_app::schema::PostOptions;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

fn face_params(cut_z: f64) -> OpParams {
    OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(cut_z),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    }
}

/// Two generated face ops on two tools, nothing between them.
fn two_faces() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    history.execute(&world, stock.expect("box stock"));
    for t in ["EM-0400-2FL", "EM-0600-4FL"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, t, tool).expect("catalog tool"));
    }
    for (name, tool, z) in [("Face1", "EM-0400-2FL", 19.5), ("Face2", "EM-0600-4FL", 19.0)] {
        let cmd = CreateOperation::new(&world, name, tool, SrcRef::Stock, face_params(z));
        history.execute(&world, cmd.expect("create"));
        history.execute(&world, GenerateOperation::new(&world, name).expect("generate"));
    }
    (world, history)
}

fn add(world: &World, history: &mut History, action: AfterOperation, after: Option<&str>) -> String {
    let cmd = AddProgramEntry::new(world, action, after).expect("add entry");
    let name = cmd.name().to_owned();
    history.execute(world, cmd);
    name
}

fn list(world: &World) -> Vec<String> {
    program_entry::sequence(world).into_iter().map(|(n, _)| n).collect()
}

fn ops(world: &World) -> Vec<String> {
    camcraft_app::setup::operation_order(world).into_iter().map(|(n, _)| n).collect()
}

fn stream(world: &World) -> String {
    cnc::serialize(&posting::stream_records(world).expect("a fresh world streams"))
}

fn script(name: &str) -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("a built-in")
}

fn post_with(world: &World, name: &str) -> post::PostOutput {
    post::post(script(name), &posting::machine_value(world), &stream(world)).expect("the script runs")
}

fn has_word(line: &str, code: &str) -> bool {
    line.split_whitespace().any(|w| w.trim_end_matches('*') == code)
}

#[test]
fn an_entry_is_added_reordered_deleted_and_undone() {
    let (world, mut h) = two_faces();
    let pause = add(&world, &mut h, AfterOperation::ProgramStop, Some("Face1"));
    assert_eq!(pause, "Pause1");
    assert_eq!(list(&world), ["Face1", "Pause1", "Face2"], "inserted after the row it was asked of");
    assert_eq!(ops(&world), ["Face1", "Face2"], "the op walk never sees an entry");
    let home = add(&world, &mut h, AfterOperation::GoHome, None);
    assert_eq!(home, "Home1");
    assert_eq!(list(&world), ["Face1", "Pause1", "Face2", "Home1"], "no row: the setup's end");

    // Reordered like an op, by the op's own command.
    h.execute(&world, ReorderOperation::new(&world, "Pause1", "Home1", true).expect("reorder"));
    assert_eq!(list(&world), ["Face1", "Face2", "Home1", "Pause1"]);
    h.undo(&world);
    assert_eq!(list(&world), ["Face1", "Pause1", "Face2", "Home1"], "undo puts it back");
    // An op dragged past an entry keeps every other row in place.
    h.execute(&world, ReorderOperation::new(&world, "Face2", "Face1", false).expect("reorder"));
    assert_eq!(list(&world), ["Face2", "Face1", "Pause1", "Home1"]);
    h.undo(&world);

    // Deleted by the generic delete (Del and the menu), undone whole.
    h.execute(&world, DeleteEntity::new(&world, "Pause1").expect("delete"));
    assert_eq!(list(&world), ["Face1", "Face2", "Home1"]);
    h.undo(&world);
    assert_eq!(list(&world), ["Face1", "Pause1", "Face2", "Home1"], "undo restores the entry at its place");
    let e = model::find_by_name(&world, "Pause1").expect("restored");
    assert_eq!(program_entry::entry_of(&world, &e).map(|x| x.action), Some(AfterOperation::ProgramStop));

    // Undoing the adds removes them and restores the old positions.
    h.undo(&world);
    h.undo(&world);
    assert_eq!(list(&world), ["Face1", "Face2"]);
    h.redo(&world);
    assert_eq!(list(&world), ["Face1", "Pause1", "Face2"], "redo reapplies");

    // A new op lands after the entries, never on one's position.
    let cmd = CreateOperation::new(&world, "Face3", "EM-0400-2FL", SrcRef::Stock, face_params(18.5));
    h.execute(&world, cmd.expect("create"));
    assert_eq!(list(&world), ["Face1", "Pause1", "Face2", "Face3"]);

    // Refusals by name.
    assert!(AddProgramEntry::new(&world, AfterOperation::None, None).is_err(), "continue is no entry");
    assert!(AddProgramEntry::new(&world, AfterOperation::GoHome, Some("Nope")).is_err());
    assert!(!model::model_entities(&world).iter().any(|(n, _)| n == "Pause1"), "an entry is not a model");
}

#[test]
fn an_entry_posts_the_per_op_words_and_changes_no_op() {
    for action in [AfterOperation::OptionalStop, AfterOperation::ProgramStop, AfterOperation::GoHome] {
        let (per_op, mut h1) = two_faces();
        h1.execute(&per_op, EditOperationParams::with_after(&per_op, "Face1", action).expect("after"));
        let (entry, mut h2) = two_faces();
        add(&entry, &mut h2, action, Some("Face1"));
        assert_eq!(stream(&entry), stream(&per_op), "{action:?}: an entry is the per-op word, byte for byte");
        for name in ["Fanuc", "Haas"] {
            assert_eq!(post_with(&entry, name).program, post_with(&per_op, name).program, "{name}");
        }
        for op in ["Face1", "Face2"] {
            let e = model::find_by_name(&entry, op).expect("op");
            assert_eq!(operation::status_of(&entry, &e), operation::OpStatus::Ok, "{op}: an entry stales nothing");
            assert_eq!(model::operation_of(&entry, &e).map(|o| o.after), Some(AfterOperation::None), "{op} keeps its choice");
        }
    }
}

#[test]
fn pause_and_home_entries_post_goldens_and_tormach_refuses_by_name() {
    let (world, mut h) = two_faces();
    add(&world, &mut h, AfterOperation::ProgramStop, Some("Face1"));
    add(&world, &mut h, AfterOperation::GoHome, None);
    for (name, file) in [("Fanuc", "fanuc"), ("Haas", "haas")] {
        let out = post_with(&world, name);
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        let lines: Vec<&str> = out.program.lines().collect();
        let m0 = lines.iter().position(|l| has_word(l, "M0")).expect("the pause");
        let t2 = lines.iter().position(|l| l.split_whitespace().any(|w| w == "T2" || w == "T02")).expect("second tool");
        assert!(m0 < t2, "{name}: the pause stands between the ops: {}", out.program);
        let home = lines.iter().rposition(|l| has_word(l, "G28") && has_word(l, "X0")).expect("home XY");
        let end = lines.iter().position(|l| has_word(l, "M30")).expect("program end");
        assert!(t2 < home && home < end, "{name}: home after op 2, before the end: {}", out.program);
        common::write_golden(&format!("nc/program_entry/{file}_pause_home.nc"), &out.program);
    }
    // Tormach's post writes no trip home (#1532: it reads the stops, the
    // G30 spot is the operator's preset): the entry refuses the way the
    // per-op choice does - by name, never a silent drop.
    let out = post_with(&world, "Tormach PathPilot");
    assert!(
        out.warnings.iter().any(|w| w.contains("trip home") && w.contains("in operation ")),
        "refused by name: {:?}",
        out.warnings
    );
}

/// The Z reference returns a program writes (`G28 Z0`, `G53 Z0`, the
/// lathe's `G28 ... W0`): one per trip home, the program end's included.
fn z_homes(program: &str) -> usize {
    let z = |l: &&str| has_word(l, "Z0") || has_word(l, "W0");
    program.lines().filter(|l| (has_word(l, "G28") || has_word(l, "G53")) && z(l)).count()
}

/// Every built-in post that homes at program end and reads a trip home.
const HOMING: [&str; 4] = ["Fanuc", "Haas", "Mazak", "Siemens 840D"];

#[test]
fn a_trailing_home_entry_merges_with_the_program_end_home() {
    // #504: a Go home last in the program IS the program-end home -
    // written once, the close's own pair not repeated after it.
    let (plain, _h0) = two_faces();
    let (world, mut h) = two_faces();
    let home = add(&world, &mut h, AfterOperation::GoHome, None);
    for name in HOMING {
        let out = post_with(&world, name);
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        assert_eq!(z_homes(&out.program), 1, "{name}: one home before the end: {}", out.program);
        let lines: Vec<&str> = out.program.lines().collect();
        let end = lines.iter().position(|l| has_word(l, "M30") || has_word(l, "M2")).expect("program end");
        let xy = lines.iter().position(|l| has_word(l, "X0") && has_word(l, "Y0")).expect("the asked XY home");
        assert!(xy < end, "{name}: the trip home stands before the end: {}", out.program);
        assert_eq!(z_homes(&post_with(&plain, name).program), 1, "{name}: the plain close homes once");
    }
    // Suppressed, or muted by "Skip pauses and homes": the program is
    // the plain one, its own close home written as always.
    h.execute(&world, EditProgramEntry::with_suppressed(&world, &home, true).expect("suppress"));
    for name in HOMING {
        assert_eq!(post_with(&world, name).program, post_with(&plain, name).program, "{name}: suppressed");
    }
    h.undo(&world);
    h.execute(&world, SetPostOptions::new(&world, PostOptions { skip_stops: true, ..Default::default() }).expect("skip"));
    for name in HOMING {
        assert_eq!(post_with(&world, name).program, post_with(&plain, name).program, "{name}: skipped");
    }
    // Mid-program the trip home stays, and the close still homes.
    let (world, mut h) = two_faces();
    add(&world, &mut h, AfterOperation::GoHome, Some("Face1"));
    for name in HOMING {
        let out = post_with(&world, name);
        assert_eq!(z_homes(&out.program), 2, "{name}: the mid home and the close's: {}", out.program);
    }
}

#[test]
fn the_lathe_post_writes_a_trailing_home_once() {
    use camcraft_app::cl::{Move, MoveKind, Toolpath};
    use camcraft_app::geometry::Point3;
    let t = Toolpath::new(vec![Move::new(
        MoveKind::Feed,
        Point3 { x: 10.0, y: 0.0, z: 1.0 },
        Point3 { x: 10.0, y: 0.0, z: -5.0 },
        Some(100.0),
    )]);
    let posted = |first: AfterOperation, last: AfterOperation| {
        let dump = |op_name, tool_number, tool_name, after| post::Dump {
            op_name,
            tool_number,
            tool_name,
            rpm: 800.0,
            after,
            ..Default::default()
        };
        let mut records = post::dump(&t, &dump("Turn1", 1, "T", first));
        records.extend(post::dump(&t, &dump("Turn2", 2, "U", last)));
        let out = post::post(script("Fanuc Lathe"), &posting::machine_value(&new_world()), &cnc::serialize(&records));
        out.expect("runs").program
    };
    let trailing = posted(AfterOperation::None, AfterOperation::GoHome);
    assert_eq!(z_homes(&trailing), 1, "X then Z, once, no close home after it: {trailing}");
    let mid = posted(AfterOperation::GoHome, AfterOperation::None);
    assert_eq!(z_homes(&mid), 2, "the mid home and the close's: {mid}");
}

#[test]
fn suppress_and_skip_mute_entries_and_undo() {
    let (world, mut h) = two_faces();
    add(&world, &mut h, AfterOperation::ProgramStop, Some("Face1"));
    assert!(stream(&world).contains("AfterOperation"));
    h.execute(&world, EditProgramEntry::with_suppressed(&world, "Pause1", true).expect("suppress"));
    assert!(!stream(&world).contains("AfterOperation"), "a suppressed entry is out of the program");
    assert!(EditProgramEntry::with_suppressed(&world, "Pause1", true).is_err(), "a no-op is refused");
    h.undo(&world);
    assert!(stream(&world).contains("AfterOperation"), "undo brings it back");

    let cmd = SetPostOptions::new(&world, PostOptions { skip_stops: true, ..Default::default() }).expect("switch");
    h.execute(&world, cmd);
    assert!(!stream(&world).contains("AfterOperation"), "Skip pauses and homes mutes entries");
    h.undo(&world);

    h.execute(&world, EditProgramEntry::with_action(&world, "Pause1", AfterOperation::GoHome).expect("action"));
    assert!(stream(&world).contains("AfterOperation home"), "{}", stream(&world));
    assert!(EditProgramEntry::with_action(&world, "Pause1", AfterOperation::None).is_err());
    h.undo(&world);
    assert!(stream(&world).contains("AfterOperation stop"));
}

#[test]
fn the_gap_law_refuses_by_name_and_merges_only_what_is_one_word() {
    // Before every posted op: nothing to stop after - refused, by name.
    let (world, mut h) = two_faces();
    let first = add(&world, &mut h, AfterOperation::ProgramStop, None);
    h.execute(&world, ReorderOperation::new(&world, &first, "Face1", false).expect("to the top"));
    assert_eq!(list(&world)[0], first);
    let err = posting::stream_records(&world).expect_err("nothing to follow");
    assert!(err.contains(&first) && err.contains("before the first"), "{err}");
    // Suppressed, it no longer holds the post; skipped, neither.
    h.execute(&world, EditProgramEntry::with_suppressed(&world, &first, true).expect("suppress"));
    assert!(posting::stream_records(&world).is_ok());
    h.undo(&world);
    h.execute(&world, SetPostOptions::new(&world, PostOptions { skip_stops: true, ..Default::default() }).expect("skip"));
    assert!(posting::stream_records(&world).is_ok(), "skipped entries never refuse");

    // A stop and a trip home in one gap: two motions, one word - refused.
    let (world, mut h) = two_faces();
    h.execute(&world, EditOperationParams::with_after(&world, "Face1", AfterOperation::GoHome).expect("after"));
    add(&world, &mut h, AfterOperation::ProgramStop, Some("Face1"));
    let err = posting::stream_records(&world).expect_err("two asks");
    assert!(err.contains("Pause1") && err.contains("Face1"), "{err}");
    // An optional stop beside a program stop is the program stop.
    h.undo(&world);
    h.undo(&world);
    add(&world, &mut h, AfterOperation::OptionalStop, Some("Face1"));
    add(&world, &mut h, AfterOperation::ProgramStop, Some("Face1"));
    let text = stream(&world);
    assert_eq!(text.matches("AfterOperation").count(), 1, "{text}");
    assert!(text.contains("AfterOperation stop"), "{text}");

    // After a suppressed op the entry joins the last POSTED op's gap.
    let (world, mut h) = two_faces();
    add(&world, &mut h, AfterOperation::ProgramStop, None);
    h.execute(&world, EditOperationParams::with_suppressed(&world, "Face2", true).expect("suppress"));
    let text = stream(&world);
    assert!(text.contains("AfterOperation stop") && !text.contains("Face2"), "{text}");
}

#[test]
fn entries_round_trip_and_documents_without_them_write_no_key() {
    let (world, mut h) = two_faces();
    let path = std::env::temp_dir().join(format!("camcraft-entry-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let plain = std::fs::read_to_string(&path).expect("written");
    assert!(!plain.contains("\"entry\""), "no entries, no key - byte-shape unchanged");

    add(&world, &mut h, AfterOperation::ProgramStop, Some("Face1"));
    add(&world, &mut h, AfterOperation::GoHome, None);
    h.execute(&world, EditProgramEntry::with_suppressed(&world, "Home1", true).expect("suppress"));
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("written");
    assert!(text.contains("\"kind\": \"entry\"") && text.contains("\"action\": \"program-stop\""), "{text}");
    let back = new_world();
    camcraft_app::project::load(&back, &path).expect("load");
    assert_eq!(list(&back), list(&world));
    for name in ["Pause1", "Home1"] {
        let a = model::find_by_name(&world, name).and_then(|e| program_entry::entry_of(&world, &e));
        let b = model::find_by_name(&back, name).and_then(|e| program_entry::entry_of(&back, &e));
        assert_eq!(a, b, "{name}");
    }
    assert_eq!(stream(&back), stream(&world), "the reopened document posts the same");
    camcraft_app::project::save(&back, &path).expect("save again");
    assert_eq!(std::fs::read_to_string(&path).expect("written"), text, "save-load-save is byte-stable");

    // A corrupt entry row refuses the file by name.
    let bad = text.replace("\"action\": \"program-stop\"", "\"action\": \"none\"");
    std::fs::write(&path, bad).expect("rewrite");
    let err = camcraft_app::project::load(&new_world(), &path).expect_err("asks nothing");
    let _ = std::fs::remove_file(&path);
    assert!(err.to_string().contains("Pause1"), "{err}");
}

#[test]
fn the_program_walks_never_see_an_entry() {
    let (plain, _h0) = two_faces();
    let (world, mut h) = two_faces();
    add(&world, &mut h, AfterOperation::ProgramStop, Some("Face1"));
    add(&world, &mut h, AfterOperation::GoHome, None);
    let names = |w: &World| posting::program_ops(w).expect("posts").into_iter().map(|(n, _)| n).collect::<Vec<_>>();
    assert_eq!(names(&world), names(&plain), "the post's tool and freshness gate");
    assert_eq!(camcraft_app::tool_list::program(&world), camcraft_app::tool_list::program(&plain), "tool list");
    assert_eq!(
        camcraft_app::setup_sheet::render(&world, None),
        camcraft_app::setup_sheet::render(&plain, None),
        "setup sheet"
    );
    assert_eq!(
        camcraft_app::setup_sheet::simulate_verdict(&world),
        camcraft_app::setup_sheet::simulate_verdict(&plain),
        "program sim"
    );
    assert_eq!(
        camcraft_app::rest::prior_digest(&world, "Face2", None),
        camcraft_app::rest::prior_digest(&plain, "Face2", None),
        "rest material ahead"
    );
    assert_eq!(posting::verification_gate(&world), posting::verification_gate(&plain), "verify");

    // A setup whose list holds an entry is not deletable, like one with an op.
    let (world, mut h) = two_faces();
    for name in ["Setup1", "Setup2"] {
        let cmd = camcraft_app::commands::CreateSetup::new(&world, Some(name), None).expect("setup");
        h.execute(&world, cmd);
    }
    h.execute(&world, camcraft_app::commands::ActivateSetup::new(&world, Some("Setup2")).expect("activate"));
    add(&world, &mut h, AfterOperation::ProgramStop, None);
    let err = camcraft_app::commands::DeleteSetup::new(&world, "Setup2").err().expect("held");
    assert!(err.to_string().contains("Pause1"), "{err}");
}
