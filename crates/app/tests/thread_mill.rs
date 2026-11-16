//! Thread milling end to end (#894): a thread mill (multi-form and
//! single-form) threads a modelled hole through the command path, the
//! generator's own verdict and the program simulation find no gouge,
//! the post gate passes and the program carries helical G2/G3 moves
//! with Z; impossible tool and thread pairs are refused by name when
//! the operation is created or edited. Every thread here is cut after a
//! Drill operation that opens the hole (#992; `thread_entry.rs` pins the
//! refusal of an undrilled one).
//!
//! Provenance: ISO 724 thread sizes (M8 x 1.25: minor Ø6.647; M8 x 1:
//! minor Ø6.917) and public handbook thread milling; our own fixtures.
//!
//! Gate map (M4.4): covers app::operation (thread_op, tool_law, tool_gate),
//! app::commands (operation gates), camcraft_motion::machining2d (thread_mill),
//! app::stock_sim (plan_program), app::posting, app::project (tool and
//! params DTOs), camcraft_tools::tools.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;
mod thread_support;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{ThreadHand, Toolpath};
use camcraft_app::commands::{CreateOperation, EditOperationParams, GenerateOperation, History};
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::tools::{Tool, ToolKind};
use camcraft_app::world::new_world;

use thread_support::*;

fn refusal(world: &World, tool: &str, params: OpParams) -> String {
    match CreateOperation::new(world, "Thread1", tool, SrcRef::Entity("disc".into()), params) {
        Ok(_) => panic!("the door was meant to refuse"),
        Err(e) => e.to_string(),
    }
}

/// The document's program through the Fanuc post on the built-in VMC.
fn fanuc(world: &World) -> String {
    let records = posting::stream_records(world).expect("the generated thread dumps to records");
    let machine = serde_json::from_str(include_str!("../../../assets/machines/3axis-vmc.mch")).expect("built-in VMC");
    let out = engine::run(include_str!("../../../assets/posts/fanuc.cps"), &machine, &cnc::serialize(&records))
        .expect("the thread posts");
    assert!(out.warnings.is_empty(), "{:?}", out.warnings);
    out.program
}

/// The helix's half-turns: XY arcs about the hole's axis.
fn helix(path: &Toolpath) -> Vec<camcraft_app::cl::Move> {
    path.moves.iter().filter(|m| m.arc.is_some_and(|a| a.center.x.abs() < 1e-9 && a.center.y.abs() < 1e-9)).copied().collect()
}

#[test]
fn a_multi_form_mill_threads_m8_clean_and_posts_helical_moves() {
    // M8 x 1.25, 12 mm deep, with the catalog's Ø6 multi-form mill (ten rows): one helical revolution per
    // radial pass, right-hand climb - counter-clockwise, rising from the floor.
    let (world, mut history) = world_with(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    let path = make(&world, &mut history, MULTI, thread(R_M8, None, ThreadHand::Right, true));
    assert_clean(&world);
    let turns = helix(&path);
    // Thread depth 0.5413 x 1.25 = 0.677 mm at 0.25 per pass: three passes, two half-turns each.
    assert_eq!(turns.len(), 6);
    assert!(turns.iter().all(|m| ThreadHand::of_move(m) == Some(ThreadHand::Right) && m.to.z > m.from.z));
    let reach = turns.iter().map(|m| m.to.x.hypot(m.to.y)).fold(0.0, f64::max);
    assert!((reach + 3.0 - (R_M8 + 0.5413 * 1.25)).abs() < 2e-6, "the crests land on the major circle: {reach}");
    assert!(path.moves.iter().all(|m| m.to.z >= FLOOR - 1e-9), "the tip keeps the blind floor");
    common::write_golden("thread_mill_m8_multi_form.cl", &common::gen_to_cltext(&path));
    // The post: counter-clockwise helical arcs are G03 blocks that carry a Z word.
    let nc = fanuc(&world);
    let helical = nc.lines().filter(|l| l.contains("G03") && l.contains('Z') && l.contains('I')).count();
    assert!(helical >= 6, "every half-turn posts as a G03 with Z:\n{nc}");
    assert!(!nc.contains("G84") && !nc.contains("G33"), "thread milling is plain interpolation, no tapping or sync cycle");
    common::write_golden("nc/thread_mill_m8_multi_form.nc", &nc);
}

#[test]
fn a_single_form_mill_winds_one_turn_per_thread_at_the_stated_pitch() {
    // M8 x 1 fine with the catalog's single-form mill (0.5 to 1.5 mm): the operation states the pitch.
    let (world, mut history) = world_with(R_M8_FINE, &[(SINGLE, catalog_tool(SINGLE))]);
    let path = make(&world, &mut history, SINGLE, thread(R_M8_FINE, Some(1.0), ThreadHand::Right, true));
    assert_clean(&world);
    let turns = helix(&path);
    // 12 mm less the quarter-pitch lead-in: twelve turns; depth 0.5413 at 0.25 per pass: three passes.
    assert_eq!(turns.len(), 3 * 2 * 12);
    assert!(turns.iter().all(|m| ((m.to.z - m.from.z) - 0.5).abs() < 1e-9), "half a pitch per half-turn, rising");
    common::write_golden("thread_mill_m8_fine_single_form.cl", &common::gen_to_cltext(&path));
}

#[test]
fn a_left_hand_conventional_thread_verifies_clean_too() {
    for (hand, climb) in [(ThreadHand::Left, true), (ThreadHand::Left, false), (ThreadHand::Right, false)] {
        let (world, mut history) = world_with(R_M8_FINE, &[(SINGLE, catalog_tool(SINGLE))]);
        let path = make(&world, &mut history, SINGLE, thread(R_M8_FINE, Some(1.0), hand, climb));
        assert_clean(&world);
        for m in helix(&path) {
            assert_eq!(ThreadHand::of_move(&m), Some(hand));
            assert_eq!(m.arc.map(|a| a.ccw), Some(climb), "climb orbits a hole counter-clockwise");
        }
        let nc = fanuc(&world);
        let word = if climb { "G03" } else { "G02" };
        assert!(nc.lines().any(|l| l.contains(word) && l.contains('Z')), "{hand:?} {climb}:\n{nc}");
    }
}

#[test]
fn a_thread_mill_whose_roots_meet_the_part_is_a_gouge() {
    // The root law: the hole list says M8's minor wall, the model's hole is Ø6.2 - the body under the
    // teeth would cut the wall, so the generator convicts it and the post gate stays shut.
    let (world, mut history) = world_with(3.1, &[(MULTI, catalog_tool(MULTI))]);
    make(&world, &mut history, MULTI, thread(R_M8, None, ThreadHand::Right, true));
    assert!(!blocking(&world).is_empty(), "the tooth roots enter the wall");
    assert!(!posting::verification_gate(&world).is_empty());
}

#[test]
fn a_bottom_below_the_blind_floor_is_a_gouge() {
    // The floor law keeps no thread-depth allowance at the tip: one millimetre into the modelled floor is
    // convicted. #992: the drill ahead goes deeper still (gouging the floor itself), so the thread mill's
    // way in is open and the floor law is what judges it.
    let (world, mut history) = bare_world(R_M8, &[(MULTI, catalog_tool(MULTI))]);
    drill_ahead(&world, &mut history, tap_drill_for(R_M8), DRILL_FLOOR - 3.5);
    let mut params = thread(R_M8, None, ThreadHand::Right, true);
    if let OpParams::Thread { bottom_z, .. } = &mut params {
        *bottom_z = Some(Height::fixed(DRILL_FLOOR - 1.0));
    }
    make(&world, &mut history, MULTI, params);
    assert!(!blocking(&world).is_empty(), "the tip enters the blind floor");
}

#[test]
fn impossible_tool_and_thread_pairs_are_refused_when_the_operation_is_made() {
    let whitworth = Tool { tip_angle: 55.0, ..catalog_tool(MULTI) };
    let tools = [
        (MULTI, catalog_tool(MULTI)),
        (SINGLE, catalog_tool(SINGLE)),
        ("TAP-M6", catalog_tool("TAP-M6")),
        ("EM-0600-4FL", catalog_tool("EM-0600-4FL")),
        ("TM-55", whitworth),
    ];
    let (world, _history) = world_with(R_M8, &tools);
    let rh = |r: f64, pitch: Option<f64>| thread(r, pitch, ThreadHand::Right, true);
    // A tap is not a thread mill: one helical orbit of it is a crash.
    for other in ["TAP-M6", "EM-0600-4FL"] {
        let msg = refusal(&world, other, rh(R_M8, None));
        assert!(msg.contains("a Thread operation cuts with a thread mill or single-form thread mill"), "{msg}");
    }
    let msg = refusal(&world, MULTI, rh(R_M8, Some(1.0)));
    assert!(msg.contains("only its own 1.25 mm pitch"), "{msg}");
    // An unstated pitch on a single-form mill is an unfinished operation: it may be made (a new Thread
    // operation starts that way) and is refused at generate, by name.
    let mut history = History::default();
    let unstated = CreateOperation::new(&world, "Unstated", SINGLE, SrcRef::Entity("disc".into()), rh(R_M8, None));
    history.execute(&world, unstated.expect("an unfinished thread operation may be made"));
    let msg = GenerateOperation::new(&world, "Unstated").err().map(|e| e.to_string()).unwrap_or_default();
    assert!(msg.contains("state the thread's pitch"), "{msg}");
    let msg = refusal(&world, SINGLE, rh(R_M8, Some(2.0)));
    assert!(msg.contains("outside this single-form thread mill's 0.5 to 1.5 mm range"), "{msg}");
    let msg = refusal(&world, MULTI, rh(2.9, None));
    assert!(msg.contains("has no air to enter the 5.8 mm drill hole"), "{msg}");
    let msg = refusal(&world, "TM-55", rh(R_M8, None));
    assert!(msg.contains("ground to 55 degrees"), "{msg}");
    let mut deep = rh(R_M8, None);
    if let OpParams::Thread { infeed, .. } = &mut deep {
        *infeed = 1.0;
    }
    assert!(refusal(&world, MULTI, deep).contains("more than the"), "an infeed past the thread depth");
    for bad in [0.0, -1.0, f64::NAN] {
        assert!(CreateOperation::new(&world, "T", SINGLE, SrcRef::Entity("disc".into()), rh(R_M8, Some(bad))).is_err());
    }
}

#[test]
fn a_pitch_edit_is_gated_and_undoes() {
    let (world, mut history) = world_with(R_M8_FINE, &[(SINGLE, catalog_tool(SINGLE))]);
    make(&world, &mut history, SINGLE, thread(R_M8_FINE, Some(1.0), ThreadHand::Right, true));
    let pitch_of = |world: &World| {
        let e = model::find_by_name(world, "Thread1").expect("op");
        match model::operation_of(world, &e).expect("operation").params {
            OpParams::Thread { pitch, hand, .. } => (pitch, hand),
            _ => panic!("a thread op"),
        }
    };
    // Outside the tool's range: the edit never enters the history.
    let wide = EditOperationParams::new(&world, "Thread1", thread(R_M8_FINE, Some(3.0), ThreadHand::Right, true));
    assert!(wide.is_err_and(|e| e.to_string().contains("outside")));
    let edit = EditOperationParams::new(&world, "Thread1", thread(R_M8_FINE, Some(1.25), ThreadHand::Left, true));
    history.execute(&world, edit.expect("a pitch inside the range"));
    assert_eq!(pitch_of(&world), (Some(1.25), ThreadHand::Left));
    assert!(history.undo(&world));
    assert_eq!(pitch_of(&world), (Some(1.0), ThreadHand::Right));
    assert!(history.redo(&world));
    assert_eq!(pitch_of(&world), (Some(1.25), ThreadHand::Left));
}

#[test]
fn thread_mills_round_trip_through_the_tool_json_and_the_project_file() {
    // The tool JSON is the shape the MCP server's tool calls and the library file carry.
    for name in [MULTI, SINGLE] {
        let tool = catalog_tool(name);
        let json = camcraft_app::project::tool_to_json(&tool);
        let word = if tool.kind == ToolKind::ThreadMill { "thread_mill" } else { "thread_mill_single" };
        assert_eq!(json["kind"], word);
        assert_eq!(json.get("pitch_min").is_some(), tool.kind == ToolKind::ThreadMillSingle, "written only when set");
        assert_eq!(camcraft_app::project::tool_from_json(&json).expect("reads back"), tool);
    }
    let (world, mut history) = world_with(R_M8_FINE, &[(SINGLE, catalog_tool(SINGLE))]);
    let params = thread(R_M8_FINE, Some(1.0), ThreadHand::Left, false);
    make(&world, &mut history, SINGLE, params.clone());
    let path = std::env::temp_dir().join(format!("camcraft_894_thread_{}.camproj", std::process::id()));
    camcraft_app::project::save(&world, &path).expect("save");
    let loaded = new_world();
    camcraft_app::project::load(&loaded, &path).expect("load");
    let _ = std::fs::remove_file(&path);
    let tool = model::find_by_name(&loaded, SINGLE).and_then(|e| model::tool_of(&loaded, &e)).expect("tool");
    assert_eq!(tool, catalog_tool(SINGLE));
    let e = model::find_by_name(&loaded, "Thread1").expect("op");
    assert_eq!(model::operation_of(&loaded, &e).expect("operation").params, params);
}
