//! Pause and home after an operation, suppression, and the post's
//! "skip pauses and homes" switch (#110): the fields, their commands
//! and undo, their file keys, and the words every built-in post writes.
//!
//! Gate map (M4.4): covers app::commands (EditOperationParams::with_after
//! / with_suppressed, SetPostOptions), app::posting (program_ops,
//! stream_records), app::project, app::schema, post::post
//! (AfterOperation, dump), assets/posts/*.cps.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, EditOperationParams, GenerateOperation, History, SetPostOptions, SetStock,
};
use camcraft_app::model;
use camcraft_app::operation::{AfterOperation, Height, OpParams, SrcRef};
use camcraft_app::post::{self, cnc};
use camcraft_app::posting;
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

/// Two generated face ops on two tools, the first with `after` asked.
fn two_faces(after: AfterOperation) -> (World, History) {
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
    if after != AfterOperation::None {
        let cmd = EditOperationParams::with_after(&world, "Face1", after).expect("after edit");
        history.execute(&world, cmd);
    }
    (world, history)
}

fn op(world: &World, name: &str) -> camcraft_app::schema::Operation {
    let e = model::find_by_name(world, name).expect("op");
    model::operation_of(world, &e).expect("operation")
}

fn post_with(world: &World, script: &str) -> post::PostOutput {
    let records = posting::stream_records(world).expect("a fresh world streams");
    let text = cnc::serialize(&records);
    post::post(script, &posting::machine_value(world), &text).expect("the script runs")
}

/// Whether a block carries `code` as a word (Heidenhain ends its
/// blocks with `*`, line numbers lead some dialects' blocks).
fn has_word(line: &str, code: &str) -> bool {
    line.split_whitespace().any(|w| w.trim_end_matches('*') == code)
}

const MILLS: [&str; 6] = ["Fanuc", "Haas", "Siemens 840D", "Heidenhain", "Mazak", "grblHAL"];

fn script(name: &str) -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, s)| *s).expect("a built-in")
}

/// The line index of the first spindle stop (the first section's end)
/// and of the second tool's call.
fn first_end_and_second_tool(lines: &[&str]) -> (usize, usize) {
    let stop = lines
        .iter()
        .position(|l| has_word(l, "M5"))
        .expect("the first section stops its spindle");
    let tool_word = |w: &str| {
        w.starts_with("TOOL") || (w.starts_with('T') && w[1..].starts_with(|c: char| c.is_ascii_digit() || c == '='))
    };
    let second = lines
        .iter()
        .enumerate()
        .skip(stop)
        .find(|(_, l)| l.split_whitespace().any(tool_word))
        .map(|(i, _)| i)
        .expect("the second tool is called");
    (stop, second)
}

#[test]
fn after_defaults_to_none_and_writes_no_stream_word() {
    let (world, _h) = two_faces(AfterOperation::None);
    assert_eq!(op(&world, "Face1").after, AfterOperation::None);
    assert!(!op(&world, "Face1").suppressed);
    let text = cnc::serialize(&posting::stream_records(&world).expect("streams"));
    assert!(!text.contains("AfterOperation"), "carrying on is silent - goldens stay byte-exact");
}

#[test]
fn every_mill_post_writes_its_stop_words_between_the_ops() {
    for (after, word) in [(AfterOperation::OptionalStop, "M1"), (AfterOperation::ProgramStop, "M0")] {
        let (world, _h) = two_faces(after);
        for name in MILLS {
            let out = post_with(&world, script(name));
            assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
            let lines: Vec<&str> = out.program.lines().collect();
            let (stop, second) = first_end_and_second_tool(&lines);
            let at: Vec<usize> = (0..lines.len()).filter(|&i| has_word(lines[i], word)).collect();
            assert_eq!(at.len(), 1, "{name}: one {word}: {}", out.program);
            assert!(stop < at[0] && at[0] < second, "{name}: {word} after op 1's spindle stop, before op 2: {}", out.program);
        }
    }
}

#[test]
fn go_home_retracts_z_then_homes_xy_in_each_dialect() {
    let (world, _h) = two_faces(AfterOperation::GoHome);
    for name in ["Fanuc", "Haas", "Mazak"] {
        let out = post_with(&world, script(name));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        let lines: Vec<&str> = out.program.lines().collect();
        let (stop, second) = first_end_and_second_tool(&lines);
        let between = &lines[stop..second];
        let z = between.iter().position(|l| has_word(l, "G28") && has_word(l, "Z0")).expect("Z home");
        let xy = between.iter().position(|l| has_word(l, "G28") && has_word(l, "X0")).expect("XY home");
        let abs = between.iter().position(|l| has_word(l, "G90")).expect("absolute again");
        assert!(has_word(between[z], "G91"), "{name}: incremental Z0 is straight up: {}", out.program);
        assert!(z < xy && xy < abs, "{name}: Z first, then XY, then G90: {}", out.program);
    }
    let out = post_with(&world, script("Siemens 840D"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let lines: Vec<&str> = out.program.lines().collect();
    let (stop, second) = first_end_and_second_tool(&lines);
    let between = &lines[stop..second];
    let z = between.iter().position(|l| has_word(l, "G53") && has_word(l, "Z0")).expect("Z home");
    let xy = between.iter().position(|l| has_word(l, "G53") && has_word(l, "X0")).expect("XY home");
    assert!(z < xy, "Z first: {}", out.program);
}

#[test]
fn a_post_that_cannot_go_home_refuses_by_name() {
    let (world, _h) = two_faces(AfterOperation::GoHome);
    // #710: grblHAL's G28/G30 positions are controller settings too;
    // TinyG's are set at the machine with G28.1 / G30.1 (its stops are
    // pinned in tinyg_post.rs: a tool change writes an M0 of its own).
    for name in ["Heidenhain", "grblHAL", "TinyG"] {
        let out = post_with(&world, script(name));
        assert!(
            out.warnings.iter().any(|w| w.contains(name) && w.contains("home")),
            "{name}: refused by name, never guessed: {:?}",
            out.warnings
        );
    }
}

#[test]
fn the_lathe_post_homes_x_then_z_and_stops_in_its_own_words() {
    use camcraft_app::cl::{Move, MoveKind, Toolpath};
    use camcraft_app::geometry::Point3;
    let t = Toolpath::new(vec![Move::new(
        MoveKind::Feed,
        Point3 { x: 10.0, y: 0.0, z: 1.0 },
        Point3 { x: 10.0, y: 0.0, z: -5.0 },
        Some(100.0),
    )]);
    let lathe = posting::BUILTINS
        .iter()
        .find(|(n, _)| n.to_lowercase().contains("lathe"))
        .map(|(_, s)| *s)
        .expect("a lathe post");
    for (after, expect) in [
        (AfterOperation::OptionalStop, vec!["M01"]),
        (AfterOperation::ProgramStop, vec!["M00"]),
        (AfterOperation::GoHome, vec!["U0", "W0"]),
    ] {
        let mut records = post::dump(&t, &post::Dump { op_name: "Turn1", tool_number: 1, tool_name: "T", rpm: 800.0, after, ..Default::default() });
        records.extend(post::dump(&t, &post::Dump { op_name: "Turn2", tool_number: 2, tool_name: "U", rpm: 800.0, ..Default::default() }));
        let out = post::post(lathe, &posting::machine_value(&new_world()), &cnc::serialize(&records)).expect("runs");
        let lines: Vec<&str> = out.program.lines().collect();
        let m5 = lines.iter().position(|l| has_word(l, "M05")).expect("spindle stop");
        let m30 = lines.iter().position(|l| has_word(l, "M30")).expect("program end");
        let mut last = m5;
        for w in &expect {
            let at = lines[last..m30].iter().position(|l| has_word(l, w)).map(|i| i + last);
            let at = at.unwrap_or_else(|| panic!("{w} after the first section: {}", out.program));
            assert!(at >= last, "{w} in order: {}", out.program);
            last = at;
        }
        // Homing is X first, then Z, never one diagonal block.
        if after == AfterOperation::GoHome {
            let u = lines.iter().position(|l| has_word(l, "U0")).expect("U0");
            assert!(!has_word(lines[u], "W0"), "two blocks: {}", out.program);
        }
    }
}

#[test]
fn skip_pauses_and_homes_mutes_every_after_action_and_undoes() {
    let (world, mut history) = two_faces(AfterOperation::ProgramStop);
    assert!(cnc::serialize(&posting::stream_records(&world).expect("streams")).contains("AfterOperation"));
    let cmd = SetPostOptions::new(&world, PostOptions { skip_stops: true, ..Default::default() }).expect("switch");
    history.execute(&world, cmd);
    let text = cnc::serialize(&posting::stream_records(&world).expect("streams"));
    assert!(!text.contains("AfterOperation"), "the switch mutes the stream: {text}");
    for name in MILLS {
        let out = post_with(&world, script(name));
        assert!(!out.program.lines().any(|l| has_word(l, "M0")), "{name}: no stop: {}", out.program);
    }
    assert_eq!(op(&world, "Face1").after, AfterOperation::ProgramStop, "the op keeps its choice");
    assert!(SetPostOptions::new(&world, PostOptions { skip_stops: true, ..Default::default() }).is_err(), "a no-op is refused");
    history.undo(&world);
    assert!(!posting::post_options(&world).skip_stops, "undo restores the switch");
    assert!(cnc::serialize(&posting::stream_records(&world).expect("streams")).contains("AfterOperation"));
    history.redo(&world);
    assert!(posting::post_options(&world).skip_stops, "redo reapplies it");
}

#[test]
fn a_suppressed_op_is_absent_from_the_nc_and_still_generated() {
    let (world, mut history) = two_faces(AfterOperation::None);
    let stamp = op(&world, "Face2").stamp;
    let cmd = EditOperationParams::with_suppressed(&world, "Face2", true).expect("suppress");
    history.execute(&world, cmd);
    assert!(op(&world, "Face2").suppressed);
    assert_eq!(op(&world, "Face2").stamp, stamp, "suppression shapes no geometry - still generated");
    let e = model::find_by_name(&world, "Face2").expect("op");
    assert!(model::toolpath_of(&world, &e).is_some_and(|c| !c.path.moves.is_empty()), "the toolpath stays");
    let text = cnc::serialize(&posting::stream_records(&world).expect("streams"));
    assert!(text.contains("Face1") && !text.contains("Face2"), "Face2 left out: {text}");
    let out = post_with(&world, script("Fanuc"));
    assert!(!out.program.contains("T02"), "no second tool call: {}", out.program);
    assert!(!out.program.contains("Face2"), "{}", out.program);
    // Everything suppressed refuses by name - never an empty program.
    let cmd = EditOperationParams::with_suppressed(&world, "Face1", true).expect("suppress");
    history.execute(&world, cmd);
    let err = posting::stream_records(&world).expect_err("nothing to post");
    assert!(err.contains("suppressed"), "{err}");
    history.undo(&world);
    history.undo(&world);
    assert!(!op(&world, "Face2").suppressed, "undo brings it back");
    let text = cnc::serialize(&posting::stream_records(&world).expect("streams"));
    assert!(text.contains("Face2"), "{text}");
}

#[test]
fn a_suppressed_stale_op_does_not_hold_the_post() {
    let (world, mut history) = two_faces(AfterOperation::None);
    // Face2 goes stale (its cut depth moves), then is suppressed.
    let cmd = EditOperationParams::new(&world, "Face2", face_params(18.0)).expect("edit");
    history.execute(&world, cmd);
    assert!(posting::stream_records(&world).is_err(), "stale holds the post");
    let cmd = EditOperationParams::with_suppressed(&world, "Face2", true).expect("suppress");
    history.execute(&world, cmd);
    assert!(posting::stream_records(&world).is_ok(), "a suppressed op is out of the program");
}

#[test]
fn after_and_suppressed_edits_undo_and_redo() {
    let (world, mut history) = two_faces(AfterOperation::None);
    for a in [AfterOperation::OptionalStop, AfterOperation::ProgramStop, AfterOperation::GoHome] {
        let cmd = EditOperationParams::with_after(&world, "Face1", a).expect("edit");
        history.execute(&world, cmd);
        assert_eq!(op(&world, "Face1").after, a);
        history.undo(&world);
        assert_eq!(op(&world, "Face1").after, AfterOperation::None, "undo restores carrying on");
        history.redo(&world);
        assert_eq!(op(&world, "Face1").after, a, "redo reapplies it");
        history.undo(&world);
    }
    let cmd = EditOperationParams::with_suppressed(&world, "Face1", true).expect("edit");
    history.execute(&world, cmd);
    history.undo(&world);
    assert!(!op(&world, "Face1").suppressed);
    history.redo(&world);
    assert!(op(&world, "Face1").suppressed);
    assert!(EditOperationParams::with_after(&world, "Nope", AfterOperation::GoHome).is_err());
    assert!(EditOperationParams::with_suppressed(&world, "Nope", true).is_err());
}

#[test]
fn after_suppressed_and_skip_round_trip_and_old_files_load_defaults() {
    let (world, mut history) = two_faces(AfterOperation::GoHome);
    history.execute(&world, EditOperationParams::with_suppressed(&world, "Face2", true).expect("suppress"));
    history.execute(&world, SetPostOptions::new(&world, PostOptions { skip_stops: true, ..Default::default() }).expect("switch"));
    let path = std::env::temp_dir().join(format!("camcraft-after-op-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("written");
    assert_eq!(text.matches("\"after\"").count(), 1, "only the non-default op writes the key: {text}");
    assert!(text.contains("\"after\": \"go-home\""), "{text}");
    assert_eq!(text.matches("\"suppressed\"").count(), 1, "{text}");
    assert!(text.contains("\"skip_stops\": true"), "{text}");
    let back = new_world();
    camcraft_app::project::load(&back, &path).expect("load");
    assert_eq!(op(&back, "Face1").after, AfterOperation::GoHome);
    assert!(!op(&back, "Face1").suppressed);
    assert!(op(&back, "Face2").suppressed);
    assert!(posting::post_options(&back).skip_stops);

    // A file written before the fields existed loads with the defaults.
    let old: String = text
        .lines()
        .filter(|l| !l.contains("\"after\"") && !l.contains("\"suppressed\"") && !l.contains("\"skip_stops\""))
        .collect::<Vec<_>>()
        .join("\n");
    let old = fix_trailing_commas(&old);
    std::fs::write(&path, old).expect("rewrite");
    let legacy = new_world();
    camcraft_app::project::load(&legacy, &path).expect("an older file loads");
    let _ = std::fs::remove_file(&path);
    assert_eq!(op(&legacy, "Face1").after, AfterOperation::None);
    assert!(!op(&legacy, "Face2").suppressed);
    assert!(!posting::post_options(&legacy).skip_stops);
}

/// Drop a comma left dangling before a closing bracket by the line strip.
fn fix_trailing_commas(s: &str) -> String {
    let lines: Vec<&str> = s.lines().collect();
    let mut out = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        let next = lines.get(i + 1).map(|n| n.trim_start()).unwrap_or("");
        if l.trim_end().ends_with(',') && (next.starts_with('}') || next.starts_with(']')) {
            out.push(l.trim_end().trim_end_matches(',').to_owned());
        } else {
            out.push((*l).to_owned());
        }
    }
    out.join("\n")
}
