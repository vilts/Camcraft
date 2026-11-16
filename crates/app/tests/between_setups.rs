//! #1532: a program that runs from one setup into the next pauses there
//! by default - the part has to be flipped or re-clamped - or writes one
//! file per setup; running straight on is the user's explicit choice.
//! Pinned on the Fanuc and Tormach PathPilot dialects, the undoable
//! choice, its project round trip, and the refusals (a post that cannot
//! stop, a Go home at the change, one file asked for per setup).
//! Provenance: our own fixtures over our own commands; no vendor material.
//!
//! Gate map (#1532): covers app::posting (op_streams, setup_change),
//! app::posting_mounts (single, post_mounts per setup), app::schema
//! (BetweenSetups), app::project (post_options between_setups),
//! app::setup_post_options (program_problems), app::mcp (the post tool's
//! between_setups), post::engine (SetupChange
//! refusal), assets/posts fanuc.cps and tormach.cps.
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, CreateSetup, EditOperationParams, GenerateOperation, History, SetPostOptions, SetStock,
};
use camcraft_app::operation::{AfterOperation, Height, OpParams, SrcRef};
use camcraft_app::schema::{BetweenSetups, PostOptions};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, project};

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

/// Stock, tool, two setups (G54 then G55), one generated face in each.
fn doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
    h.execute(&world, stock.expect("box stock"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TOOL).map(|(_, t)| t).expect("tool");
    h.execute(&world, AddTool::new(&world, TOOL, tool).expect("add tool"));
    for i in 1..=2 {
        h.execute(&world, CreateSetup::new(&world, None, None).expect("setup"));
        if i > 1 {
            let stock = SetStock::new(&world, StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 }, Default::default());
            h.execute(&world, stock.expect("setup stock"));
        }
        let name = format!("Face{i}");
        h.execute(&world, CreateOperation::new(&world, &name, TOOL, SrcRef::Stock, face()).expect("create"));
        h.execute(&world, GenerateOperation::new(&world, &name).expect("generate"));
    }
    (world, h)
}

fn choose(world: &World, h: &mut History, b: BetweenSetups) {
    let next = PostOptions { between_setups: b, ..posting::post_options(world) };
    h.execute(world, SetPostOptions::new(world, next).expect("between setups"));
}

fn cps(name: &str) -> &'static str {
    posting::BUILTINS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).expect("built-in post")
}

fn tmp(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-1532-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn post_as(world: &World, post: &str, tag: &str) -> Result<String, String> {
    let dir = tmp(tag);
    let out = posting::post_script(world, &dir.join("prog.nc"), cps(post)).map(|p| p.program);
    let _ = std::fs::remove_dir_all(&dir);
    out
}

/// The text between the first section's cut and the second setup's
/// first operation: where the setup change has to stop.
fn between<'a>(program: &'a str, second_op: &str) -> &'a str {
    let end = program.find(second_op).unwrap_or(program.len());
    let start = program[..end].rfind("M5").unwrap_or(0);
    &program[start..end]
}

#[test]
fn the_default_pauses_between_setups_on_fanuc_with_a_comment_naming_the_next() {
    let (world, _h) = doc();
    assert_eq!(posting::post_options(&world).between_setups, BetweenSetups::Pause, "the default pauses");
    let nc = post_as(&world, "Fanuc", "fanuc-pause").expect("posts");
    let gap = between(&nc, "Face2");
    assert!(gap.contains("(SETUP CHANGE - NEXT: Setup2)"), "the comment names the next setup:\n{gap}");
    assert!(gap.contains("M0"), "a program stop before Setup2's first cut:\n{gap}");
    assert!(gap.contains("G28 Z0"), "Z goes up clear of the part first:\n{gap}");
    // One stop: the last setup's end is the program's own end.
    assert_eq!(nc.matches("SETUP CHANGE").count(), 1, "{nc}");
}

#[test]
fn the_default_pauses_between_setups_on_tormach_and_an_optional_stop_writes_m1() {
    let (world, mut h) = doc();
    let nc = post_as(&world, "Tormach PathPilot", "tormach-pause").expect("posts");
    let gap = between(&nc, "Face2");
    assert!(gap.contains("(MSG, SETUP CHANGE - NEXT: Setup2)"), "PathPilot shows the operator the next setup:\n{gap}");
    assert!(gap.contains("G53 G0 Z"), "Z straight up first:\n{gap}");
    assert!(gap.lines().any(|l| l.split_whitespace().any(|w| w == "M0")), "a program stop:\n{gap}");
    choose(&world, &mut h, BetweenSetups::OptionalStop);
    let nc = post_as(&world, "Tormach PathPilot", "tormach-m1").expect("posts");
    let gap = between(&nc, "Face2");
    assert!(gap.lines().any(|l| l.split_whitespace().any(|w| w == "M1")), "an optional stop:\n{gap}");
    assert!(!gap.lines().any(|l| l.split_whitespace().any(|w| w == "M0")), "no program stop:\n{gap}");
    let nc = post_as(&world, "Fanuc", "fanuc-m1").expect("posts");
    assert!(between(&nc, "Face2").contains("M1"), "{nc}");
}

#[test]
fn run_straight_on_is_the_old_program_and_is_undoable() {
    let (world, mut h) = doc();
    let paused = post_as(&world, "Fanuc", "cont-paused").expect("posts");
    choose(&world, &mut h, BetweenSetups::Continue);
    let on = post_as(&world, "Fanuc", "cont-on").expect("posts");
    assert!(!on.contains("SETUP CHANGE"), "{on}");
    assert!(!between(&on, "Face2").contains("M0"), "no stop when the user asks to run on:\n{on}");
    h.undo(&world);
    assert_eq!(posting::post_options(&world).between_setups, BetweenSetups::Pause);
    assert_eq!(post_as(&world, "Fanuc", "cont-undone").expect("posts"), paused);
}

#[test]
fn one_file_per_setup_writes_two_whole_programs() {
    let (world, mut h) = doc();
    choose(&world, &mut h, BetweenSetups::FilePerSetup);
    let dir = tmp("per-setup");
    let files = posting::post_mounts(&world, &dir.join("prog.nc")).expect("posts one file per setup");
    assert_eq!(files.len(), 2, "one file per setup");
    let setups: Vec<Vec<String>> = files.iter().map(|f| f.mount.setups.clone()).collect();
    assert_eq!(setups, vec![vec!["Setup1".to_owned()], vec!["Setup2".to_owned()]]);
    for f in &files {
        assert!(f.path.exists(), "written: {}", f.path.display());
        assert!(f.posted.program.contains("M30"), "each file ends as a program of its own");
        assert!(!f.posted.program.contains("SETUP CHANGE"), "no setup change inside a file");
    }
    assert!(files[0].posted.program.contains("Face1") && !files[0].posted.program.contains("Face2"));
    assert!(files[1].posted.program.contains("Face2") && !files[1].posted.program.contains("Face1"));
    let _ = std::fs::remove_dir_all(&dir);
    // The one-file post cannot hold two setups: refused by name.
    let why = post_as(&world, "Fanuc", "per-setup-single").expect_err("refused");
    assert!(why.contains("One file per setup") && why.contains("Setup1") && why.contains("Setup2"), "{why}");
}

#[test]
fn a_post_that_cannot_stop_is_refused_by_name_at_the_setup_change() {
    let (world, mut h) = doc();
    let why = post_as(&world, "Makera Carvera", "carvera").expect_err("refused");
    assert!(why.contains("changes to setup 'Setup2'") && why.contains("Between setups"), "{why}");
    assert_eq!(why.matches("afterModes").count(), 1, "one refusal, not a second for the stop itself: {why}");
    choose(&world, &mut h, BetweenSetups::Continue);
    post_as(&world, "Makera Carvera", "carvera-on").expect("runs on when asked");
}

#[test]
fn a_go_home_at_the_setup_change_is_refused_by_name() {
    let (world, mut h) = doc();
    h.execute(&world, EditOperationParams::with_after(&world, "Face1", AfterOperation::GoHome).expect("home"));
    let why = post_as(&world, "Fanuc", "home").expect_err("refused");
    assert!(why.contains("Face1") && why.contains("Go home") && why.contains("Setup2"), "{why}");
    // An operator's own stop there is the same stop: one M0.
    h.execute(&world, EditOperationParams::with_after(&world, "Face1", AfterOperation::ProgramStop).expect("stop"));
    let nc = post_as(&world, "Fanuc", "own-stop").expect("posts");
    assert_eq!(between(&nc, "Face2").matches("M0").count(), 1, "{nc}");
}

#[test]
fn the_choice_round_trips_and_the_default_writes_no_key() {
    let (world, mut h) = doc();
    let path = tmp("rt").join("doc.ccam");
    project::save(&world, &path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(!text.contains("between_setups"), "the default writes no key");
    for b in [BetweenSetups::OptionalStop, BetweenSetups::Continue, BetweenSetups::FilePerSetup] {
        choose(&world, &mut h, b);
        project::save(&world, &path).expect("save");
        let text = std::fs::read_to_string(&path).expect("read");
        assert!(text.contains(&format!("\"between_setups\": \"{}\"", b.word())), "{text}");
        let w = new_world();
        project::open(&w, &text).expect("open");
        assert_eq!(posting::post_options(&w).between_setups, b);
    }
    let bad = std::fs::read_to_string(&path).expect("read").replace("file-per-setup", "sideways");
    let why = project::open(&new_world(), &bad).expect_err("unknown word refused");
    assert!(format!("{why}").contains("sideways"), "{why}");
    let _ = std::fs::remove_dir_all(path.parent().expect("dir"));
}

fn mcp(server: &mut camcraft_app::mcp::Server, name: &str, args: serde_json::Value) -> (bool, serde_json::Value) {
    let msg = serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert!(result.is_object(), "tools/call answers with a result: {reply}");
    let refused = result["isError"] == serde_json::json!(true);
    (refused, if refused { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
}

#[test]
fn the_mcp_post_tool_takes_the_choice_through_the_undoable_command() {
    use serde_json::json;
    let (world, _h) = doc();
    let dir = tmp("mcp");
    let file = dir.join("job.camproj");
    project::save(&world, &file).expect("save");
    let mut server = camcraft_app::mcp::Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    server.handle(&init).expect("the server answers");
    let (refused, body) = mcp(&mut server, "open_project", json!({ "path": file.to_string_lossy() }));
    assert!(!refused, "{body}");
    let nc = dir.join("mcp.nc");
    let out = nc.to_string_lossy().into_owned();
    let read = |p: &std::path::Path| std::fs::read_to_string(p).expect("posted");
    let (refused, body) = mcp(&mut server, "post", json!({ "output": out, "post": "Fanuc" }));
    assert!(!refused, "{body}");
    assert_eq!(read(&nc).matches("SETUP CHANGE - NEXT: Setup2").count(), 1, "the default pauses");
    let (refused, body) = mcp(&mut server, "post", json!({ "output": out, "post": "Fanuc", "between_setups": "continue" }));
    assert!(!refused, "{body}");
    assert!(!read(&nc).contains("SETUP CHANGE"), "run straight on, as asked");
    let (refused, why) = mcp(&mut server, "post", json!({ "output": out, "post": "Fanuc", "between_setups": "sideways" }));
    assert!(refused && why.as_str().is_some_and(|w| w.contains("sideways") && w.contains("file-per-setup")), "{why}");
    // The choice is a command: undo brings the pause back.
    let (refused, body) = mcp(&mut server, "undo", json!({}));
    assert!(!refused, "{body}");
    let (refused, body) = mcp(&mut server, "post", json!({ "output": out, "post": "Fanuc" }));
    assert!(!refused, "{body}");
    assert!(read(&nc).contains("SETUP CHANGE - NEXT: Setup2"), "undone");
    let (refused, body) = mcp(&mut server, "post", json!({ "output": out, "post": "Fanuc", "between_setups": "file-per-setup" }));
    assert!(!refused, "{body}");
    assert_eq!(body["files"].as_array().map(Vec::len), Some(2), "one file per setup: {body}");
    let _ = std::fs::remove_dir_all(&dir);
}
