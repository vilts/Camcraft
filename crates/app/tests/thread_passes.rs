//! Thread-milling pass control end to end (#988): decreasing passes of
//! constant chip area with a finishing pass, spring passes and a fit
//! allowance verify clean and post; an M4 spindle with a left-hand-cut
//! mill climbs the other way round, posts M4 through the Fanuc post and
//! is refused by name by a post that cannot reverse the spindle; the
//! door refuses a cutter against its spindle, a negative or oversized
//! allowance and a negative finish or one that leaves nothing to rough when the
//! operation is made or edited; edits undo; the keys round-trip through
//! the project file; an M3 thread after an M4 one posts M3 again.
//!
//! Provenance: ISO 724 thread sizes (M8 x 1.25: minor Ø6.647), Smid's
//! constant-area threading passes; our own fixtures.
//!
//! Gate map (M4.4): covers app::operation (thread_op), app::commands
//! (operation gates), camcraft_motion::machining2d (thread_mill, thread_passes),
//! app::posting, app::project (params DTOs), post::dump.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod thread_support;

use flecs_ecs::prelude::World;

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation};
use camcraft_app::machining2d::{PassSchedule, ThreadPasses};
use camcraft_app::model;
use camcraft_app::operation::{OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::world::new_world;

use thread_support::*;

fn with_passes(passes: ThreadPasses) -> OpParams {
    let mut p = thread(R_M8, None, ThreadHand::Right, true);
    if let OpParams::Thread { passes: q, .. } = &mut p {
        *q = passes;
    }
    p
}

fn post_with(world: &World, script: &str) -> engine::PostOutput {
    let records = posting::stream_records(world).expect("the generated thread dumps to records");
    let machine = serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch")).expect("built-in VMC");
    engine::run(script, &machine, &cnc::serialize(&records)).expect("the post runs")
}

fn refusal(world: &World, params: OpParams) -> String {
    match CreateOperation::new(world, "Thread1", MULTI, SrcRef::Entity("disc".into()), params) {
        Ok(_) => panic!("the door was meant to refuse"),
        Err(e) => e.to_string(),
    }
}

fn passes_of(world: &World) -> ThreadPasses {
    match model::find_by_name(world, "Thread1").and_then(|e| model::operation_of(world, &e)).map(|o| o.params) {
        Some(OpParams::Thread { passes, .. }) => passes,
        other => panic!("a thread op expected, got {other:?}"),
    }
}

#[test]
fn decreasing_passes_with_finish_spring_and_allowance_verify_clean_and_post() {
    let passes =
        ThreadPasses { schedule: PassSchedule::ConstantArea, finish: 0.05, spring: 1, allowance: 0.02, ..ThreadPasses::default() };
    let (world, mut history) = world_with(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    let path = make(&world, &mut history, MULTI, with_passes(passes));
    assert_clean(&world);
    // Helix arcs about the axis: their radii are the passes.
    let mut radii: Vec<f64> = path
        .moves
        .iter()
        .filter(|m| m.arc.is_some_and(|a| a.center.x.abs() < 1e-9 && a.center.y.abs() < 1e-9))
        .map(|m| m.to.x.hypot(m.to.y))
        .collect();
    let last = radii.last().copied().unwrap_or(0.0);
    let h = 0.5413 * 1.25;
    assert!((last + 3.0 - (R_M8 + h - 0.02)).abs() < 2e-6, "the last pass stops the allowance short: {last}");
    radii.dedup_by(|a, b| (*a - *b).abs() < 1e-5);
    // Rough 0.677 - 0.02 - 0.05 = 0.607 from a first pass of 0.25: ceil(5.9) = 6 passes, then the finish
    // (the spring pass repeats it).
    assert_eq!(radii.len(), 7, "{radii:?}");
    let out = post_with(&world, include_str!("../../../assets/posts/fanuc.cps"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.contains("M3") && !out.program.contains("M4"), "an M3 thread stays M3");
}

#[test]
fn a_left_hand_cut_mill_under_m4_climbs_clockwise_and_posts_m4() {
    let passes = ThreadPasses { spindle_reverse: true, cutter_left: true, ..ThreadPasses::default() };
    let (world, mut history) = world_with(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    let path = make(&world, &mut history, MULTI, with_passes(passes));
    assert_clean(&world);
    let helix: Vec<_> =
        path.moves.iter().filter(|m| m.arc.is_some_and(|a| a.center.x.abs() < 1e-9 && a.center.y.abs() < 1e-9)).collect();
    assert!(!helix.is_empty());
    for m in helix {
        assert_eq!(ThreadHand::of_move(m), Some(ThreadHand::Right), "the drawing's hand holds");
        assert_eq!(m.arc.map(|a| a.ccw), Some(false), "climb under M4 orbits a hole clockwise");
    }
    let out = post_with(&world, include_str!("../../../assets/posts/fanuc.cps"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    assert!(out.program.lines().any(|l| l.contains('S') && l.contains("M4")), "{}", out.program);
    // A post that cannot reverse the spindle refuses the section by name.
    let out = post_with(&world, include_str!("../../../assets/posts/haas.cps"));
    assert!(out.warnings.iter().any(|w| w.contains("does not declare millSpindleReverse")), "{:?}", out.warnings);
}

#[test]
fn an_m3_thread_after_an_m4_thread_posts_m3_again() {
    // The Fanuc post's spindle direction is per section: an M4 thread
    // followed by an M3 one on the same mill must turn M3 again, not
    // carry the reverse over (the `spinRev` reset on OperationName).
    let (world, mut history) = world_with(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    let rev = ThreadPasses { spindle_reverse: true, cutter_left: true, ..ThreadPasses::default() };
    make(&world, &mut history, MULTI, with_passes(rev));
    let op = CreateOperation::new(&world, "Thread2", MULTI, SrcRef::Entity("disc".into()), with_passes(ThreadPasses::default()));
    history.execute(&world, op.expect("create the M3 thread"));
    history.execute(&world, GenerateOperation::new(&world, "Thread2").expect("generate the M3 thread"));
    let out = post_with(&world, include_str!("../../../assets/posts/fanuc.cps"));
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    let spindle: Vec<&str> =
        out.program.lines().filter(|l| l.split_whitespace().any(|w| w == "M3" || w == "M4")).collect();
    // (The fixture's drill ahead posts its own M3 first.)
    let has = |l: &str, code: &str| l.split_whitespace().any(|w| w == code);
    let m4 = spindle.iter().position(|l| has(l, "M4"));
    let next = m4.and_then(|i| spindle.get(i + 1));
    assert!(next.is_some_and(|l| has(l, "M3")), "the thread after the M4 one turns M3 again: {spindle:?}\n{}", out.program);
}

#[test]
fn impossible_pass_control_is_refused_when_the_operation_is_made() {
    let (world, _history) = world_with(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    let msg = refusal(&world, with_passes(ThreadPasses { spindle_reverse: true, ..ThreadPasses::default() }));
    assert!(msg.contains("right-hand-cut thread mill under an M4"), "{msg}");
    let msg = refusal(&world, with_passes(ThreadPasses { allowance: -0.05, ..ThreadPasses::default() }));
    assert!(msg.contains("past the thread's standard depth"), "{msg}");
    let msg = refusal(&world, with_passes(ThreadPasses { allowance: 0.3, ..ThreadPasses::default() }));
    assert!(msg.contains("would not gauge"), "{msg}");
    let msg = refusal(&world, with_passes(ThreadPasses { finish: 0.7, ..ThreadPasses::default() }));
    assert!(msg.contains("leaves nothing to rough"), "{msg}");
    let msg = refusal(&world, with_passes(ThreadPasses { finish: -0.05, ..ThreadPasses::default() }));
    assert!(msg.contains("finishing pass depth cannot be negative"), "{msg}");
}

#[test]
fn a_pass_control_edit_is_gated_undoes_and_round_trips_the_file() {
    let (world, mut history) = world_with(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    make(&world, &mut history, MULTI, with_passes(ThreadPasses::default()));
    let wanted = ThreadPasses {
        schedule: PassSchedule::ConstantArea,
        finish: 0.04,
        spring: 2,
        allowance: 0.01,
        spindle_reverse: true,
        cutter_left: true,
    };
    let bad = EditOperationParams::new(&world, "Thread1", with_passes(ThreadPasses { cutter_left: true, ..ThreadPasses::default() }));
    assert!(bad.is_err_and(|e| e.to_string().contains("left-hand-cut thread mill under an M3")));
    let edit = EditOperationParams::new(&world, "Thread1", with_passes(wanted)).expect("a cuttable edit");
    history.execute(&world, edit);
    assert_eq!(passes_of(&world), wanted);
    let path = std::env::temp_dir().join(format!("camcraft_988_passes_{}.camproj", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let loaded = new_world();
    camcraft_app::project::load(&loaded, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    assert_eq!(passes_of(&loaded), wanted, "every key round-trips");
    history.undo(&world);
    assert_eq!(passes_of(&world), ThreadPasses::default(), "Ctrl+Z restores the #894 ride");
}
