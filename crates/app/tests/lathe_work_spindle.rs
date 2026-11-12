//! A milling operation is never posted on a lathe's work spindle
//! (#1190). A lathe post opens every section that states no C angle
//! with the WORK spindle (`G97 S M03`): right for turning and for a
//! drill on centre (the work turns, the drill stands still), wrong for
//! a milling cutter - and the stream names no operation family, so a
//! face pass that stays on Y = 0 reads to the post like a facing pass.
//! The program gate knows the kind: through a turning dialect it lets
//! the turning family, live-tool (C) sections and on-centre Drill
//! holes pass, and refuses every other operation by name - at every
//! post entry point, before a line is written. A mill post is not
//! touched: there `M03` is the tool's own spindle.
//!
//! The two reproductions of the issue are the first two tests; the law
//! over every operation kind is `app::lathe_voice`'s own unit table.
//!
//! Provenance: our own tests; that a lathe drills on centre with the
//! work turning is general handbook practice. No vendor material.
//!
//! Gate map (M4.4): covers app::lathe_voice, app::posting (post_script,
//! post_setup, post_mounts, post_per_op and their script forms),
//! app::script (the post verb), app::mcp (the post tool), post::post
//! (dialect), assets/posts/fanuc-lathe.cps and tormach-lathe.cps
//! (section open).
//! Lane shape (M4.4): all tests here run in seconds.

mod tormach_lathe_support;

use std::path::PathBuf;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::Point3;
use camcraft_app::lathe_voice::{self, LatheVoice};
use camcraft_app::machine::{AxisLimits, Coolant, LiveToolCodes, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{model, posting, project};
use tormach_lathe_support::revolve;

const FANUC_LATHE: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const TORMACH_LATHE: &str = include_str!("../../../assets/posts/tormach-lathe.cps");
const FANUC_MILL: &str = include_str!("../../../assets/posts/fanuc.cps");
const REFUSAL: &str = "F1: a Face operation would be posted on the work spindle - a lathe post turns the work at the \
                       tool's speed while the cutter stands still, so nothing is milled; cut it as C-axis live tooling \
                       (a rotary operation), or move it to a setup that posts through a mill post, or suppress the \
                       operation";

/// A mill-turn bench: the spindle is a C rotary, live-tool words are
/// stated (placeholders, no builder's codes), and the spindle maximum
/// is above both tools' rpm - so nothing but the law under test stands
/// between a milling section and the work spindle.
fn mill_turn() -> Machine {
    Machine {
        model: "TestBench mill-turn".into(),
        kind: Some("mill-turn".into()),
        spindle_max: Some(15000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        axes: vec![
            AxisLimits { name: "X".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Y".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits { name: "Z".into(), feed: Some(4000.0), ..Default::default() },
            AxisLimits {
                name: "C".into(),
                rotary: true,
                axis: Some([0.0, 0.0, 1.0]),
                feed: Some(6000.0),
                rapid: Some(12000.0),
                ..Default::default()
            },
        ],
        live_tool: Some(LiveToolCodes {
            c_axis_on: "M1001".into(),
            c_axis_off: "M1002".into(),
            spindle_on: "M1003".into(),
            spindle_off: "M1004".into(),
        }),
        ..Default::default()
    }
}

/// The bench with `stock` standing on the spindle axis (base at z
/// `z0`), the two catalog tools, and - when `bar` - the Ø20 bar with a
/// Ø4.1 centre hole as the model.
fn world_with(stock: StockShape, z0: f64, bar: bool) -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, z0), ..Location::default() };
    h.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(mill_turn())).expect("mill-turn"));
    for t in ["DR-0400", "EM-0400-2FL"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
        h.execute(&world, AddTool::new(&world, t, tool).expect("tool"));
    }
    if bar {
        let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)], 64);
        h.execute(&world, ImportModel::from_mesh("bar", bar));
    }
    (world, h)
}

fn create(world: &World, h: &mut History, name: &str, tool: &str, src: SrcRef, params: OpParams) {
    h.execute(world, CreateOperation::new(world, name, tool, src, params).expect("create"));
    h.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op");
    assert_eq!(camcraft_app::operation::status_of(world, &e), camcraft_app::operation::OpStatus::Ok, "{name} generates");
}

fn drill(x: f64, y: f64) -> OpParams {
    OpParams::Drill {
        clearance_z: Height::fixed(30.0),
        retract_z: None,
        top_z: Height::fixed(20.0),
        bottom_z: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 120.0,
        plunge_feed: 80.0,
        holes: vec![Hole { xy: Point2::new(x, y), radius: 2.05, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false }],
        cross: vec![],
    }
}

/// Reproduction (a): a mill-family Drill, one hole at X0 Y0.
fn drilled() -> (World, History) {
    let (world, mut h) = world_with(StockShape::Box { dx: 24.0, dy: 24.0, dz: 40.0 }, -20.0, true);
    create(&world, &mut h, "D1", "DR-0400", SrcRef::Entity("bar".into()), drill(0.0, 0.0));
    (world, h)
}

/// Reproduction (b): a Face pass with a flat end mill over a sliver of
/// stock lying along Y = 0 - one pass, on the meridian.
fn faced() -> (World, History) {
    let (world, mut h) = world_with(StockShape::Box { dx: 20.0, dy: 2.0, dz: 8.0 }, -4.0, false);
    let params = OpParams::Face {
        holes: Default::default(),
        blend_turns: false,
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::fixed(30.0),
        cut_z: Height::fixed(4.0),
        top_z: None,
        stepdown: None,
        feed: 300.0,
        plunge_feed: 100.0,
        climb: true,
    };
    create(&world, &mut h, "F1", "EM-0400-2FL", SrcRef::Stock, params);
    (world, h)
}

/// What the script itself writes for the document's stream, with no
/// program gate in front of it; the stream's own WARN records (the
/// verifier's notes) are taken out of the warnings.
fn gate_free(cps: &str, world: &World) -> engine::PostOutput {
    let records = posting::stream_records(world).expect("every op generated");
    let mut out = engine::run(cps, &posting::machine_value(world), &cnc::serialize(&records)).expect("the script runs");
    let warns: Vec<String> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Warn { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    out.warnings.retain(|w| !warns.contains(w));
    out
}

/// A scratch folder of this test's own, empty.
fn scratch(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-1190-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch folder");
    dir
}

fn files_in(dir: &PathBuf) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("scratch folder")
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    names.sort();
    names
}

fn voice(world: &World, name: &str) -> Option<LatheVoice> {
    lathe_voice::voice_of(world, &model::find_by_name(world, name).expect("op"))
}

#[test]
fn a_drill_on_centre_posts_on_the_work_spindle_as_a_lathe_drills() {
    let (world, _h) = drilled();
    assert_eq!(voice(&world, "D1"), Some(LatheVoice::WorkSpindle));
    let dir = scratch("drill");
    let posted = posting::post_script(&world, &dir.join("d.nc"), FANUC_LATHE).expect("an on-centre drill posts");
    let lines: Vec<&str> = posted.program.lines().collect();
    let at = |word: &str| lines.iter().position(|l| *l == word).unwrap_or_else(|| panic!("{word} in:\n{}", posted.program));
    // The work turns at the drill's rpm, the drill stands still on X0.
    assert!(at("G97 S10000 M03") < at("G00 X0 Z20"), "{}", posted.program);
    assert!(lines.iter().any(|l| l.starts_with("G83 Z10 ")), "{}", posted.program);
    assert!(!posted.program.contains("M1003"), "no live-tool word in a work-spindle section");
    // The same hole through a mill post is the mill's own drilling.
    posting::post_script(&world, &dir.join("m.nc"), FANUC_MILL).expect("a mill post drills it as a mill does");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_face_pass_on_the_meridian_is_refused_by_name_never_posted_on_the_work_spindle() {
    let (world, _h) = faced();
    // What the lathe scripts write for it on their own: the work
    // spindle at the end mill's 12000 rpm, feed moves, no refusal -
    // the pass stays on the meridian, so nothing in the stream tells
    // it from a facing pass. This is the wrong output of the issue.
    let bare = gate_free(FANUC_LATHE, &world);
    assert!(bare.warnings.is_empty(), "the script alone cannot refuse it: {:?}", bare.warnings);
    for line in ["G97 S12000 M03", "G01 X-24 Z4 F0.0083", "G01 X24 Z4 F0.025"] {
        assert!(bare.program.lines().any(|l| l == line), "{line} in:\n{}", bare.program);
    }
    let bare = gate_free(TORMACH_LATHE, &world);
    assert!(bare.warnings.is_empty(), "{:?}", bare.warnings);
    assert!(bare.program.lines().any(|l| l.contains("S12000") && l.contains("M3")), "{}", bare.program);

    // The program gate refuses it by name, through both lathe dialects,
    // and writes nothing.
    assert!(matches!(voice(&world, "F1"), Some(LatheVoice::Refused(_))));
    let dir = scratch("face");
    for cps in [FANUC_LATHE, TORMACH_LATHE] {
        let why = posting::post_script(&world, &dir.join("f.nc"), cps).expect_err("a milling pass on a lathe post");
        assert_eq!(why, REFUSAL);
    }
    assert!(files_in(&dir).is_empty(), "nothing written: {:?}", files_in(&dir));
    // A mill post is left alone: its M03 is the cutter's own spindle.
    let milled = posting::post_script(&world, &dir.join("m.nc"), FANUC_MILL).expect("a mill post mills it");
    assert!(milled.program.contains("S12000 M3") || milled.program.contains("S12000 M03"), "{}", milled.program);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn every_post_entry_point_refuses_it() {
    let (world, mut h) = faced();
    let dir = scratch("entries");
    let out = dir.join("p.nc");
    // A caller's own script: one file, one file per mount, one per op.
    assert_eq!(posting::post_script(&world, &out, FANUC_LATHE).err().as_deref(), Some(REFUSAL));
    assert_eq!(posting::post_mounts_script(&world, &out, FANUC_LATHE).err().as_deref(), Some(REFUSAL));
    assert_eq!(posting::post_per_op_script(&world, &dir, "p", FANUC_LATHE).err().as_deref(), Some(REFUSAL));
    // The document's own post (what the GUI's Post and Post per
    // operation, the script verb and the MCP tool run without a script).
    h.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("a registry name"));
    assert_eq!(posting::post_setup(&world, &out).err().as_deref(), Some(REFUSAL));
    assert_eq!(posting::post_mounts(&world, &out).err().as_deref(), Some(REFUSAL));
    assert_eq!(posting::post_per_op(&world, &dir, "p").err().as_deref(), Some(REFUSAL));
    assert!(files_in(&dir).is_empty(), "nothing written: {:?}", files_in(&dir));

    // The MCP post tool (and the script verb under it), on the saved
    // document: with the document's lathe post, and with one named.
    let file = dir.join("job.camproj");
    project::save(&world, &file).expect("the document saves");
    let mut server = camcraft_app::mcp::Server::new();
    let init = json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } });
    server.handle(&init).expect("the server answers");
    for (tool, args) in [("open_project", json!({ "path": file.to_string_lossy() })), ("generate", json!({}))] {
        let (refused, body) = mcp(&mut server, tool, args);
        assert!(!refused, "{tool}: {body}");
    }
    let nc = dir.join("mcp.nc").to_string_lossy().into_owned();
    for args in [json!({ "output": nc }), json!({ "output": nc, "post": "Fanuc Lathe" }), json!({ "output": nc, "post": "Tormach PathPilot Lathe" })] {
        let (refused, why) = mcp(&mut server, "post", args);
        assert!(refused && why.as_str().is_some_and(|w| w.contains(REFUSAL)), "{why}");
    }
    assert_eq!(files_in(&dir), vec!["job.camproj".to_owned()], "nothing posted");
    // Named a mill post, the same document posts.
    let (refused, body) = mcp(&mut server, "post", json!({ "output": nc, "post": "Fanuc" }));
    assert!(!refused, "{body}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_drill_off_centre_on_the_meridian_is_refused_by_name() {
    // A hole at X 6 Y 0: on the meridian, so the post's own "milling
    // work" check does not see it - a drill standing still 6 mm off a
    // turning work's axis would cut a ring, not a hole.
    let (world, mut h) = world_with(StockShape::Box { dx: 24.0, dy: 24.0, dz: 40.0 }, -20.0, true);
    let cmd = CreateOperation::new(&world, "D2", "DR-0400", SrcRef::Entity("bar".into()), drill(6.0, 0.0));
    h.execute(&world, cmd.expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "D2").expect("generate"));
    let dir = scratch("offcentre");
    let why = posting::post_script(&world, &dir.join("d.nc"), FANUC_LATHE).expect_err("an off-centre hole on a lathe post");
    assert!(why.starts_with("D2: a Drill operation would be posted on the work spindle with a hole off the spindle axis (X6.000 Y0.000 from the work offset)"), "{why}");
    assert!(why.contains("only right for a hole on the spindle axis"), "{why}");
    assert!(files_in(&dir).is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The admitted sections of a mill-turn program pass the door: a C-axis
/// rotary contour is voiced by the live tool (no work-spindle M03), a
/// turning-family facing pass by the work spindle.
#[test]
fn live_tool_and_turning_sections_pass_the_lathe_door() {
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let stock = StockShape::Box { dx: 14.0, dy: 14.0, dz: 34.0 };
    h.execute(&world, SetStock::new(&world, stock, Location::default()).expect("stock"));
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(mill_turn())).expect("mill-turn"));
    for t in ["BN-0600", "TN-1600-0R8"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == t).map(|(_, t)| t).expect("catalog entry");
        h.execute(&world, AddTool::new(&world, t, tool).expect("tool"));
    }
    let bar = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)], 16);
    h.execute(&world, ImportModel::from_mesh("bar", bar));
    let wrap = OpParams::RotaryContour {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        retract_z: None,
        top_z: Height::fixed(6.0),
        bottom_z: Height::fixed(-8.0),
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        lead_deg: 0.0,
        lag_deg: 0.0,
        max_tilt_deg: 60.0,
    };
    create(&world, &mut h, "C1", "BN-0600", SrcRef::Entity("bar".into()), wrap);
    assert_eq!(voice(&world, "C1"), Some(LatheVoice::LiveTool));
    // The door lets it through; what the lathe script writes for it is
    // the live tool's voicing, never the work spindle's.
    let live = gate_free(FANUC_LATHE, &world);
    assert!(live.warnings.is_empty(), "{:?}", live.warnings);
    assert!(live.program.contains("M1003 S") && !live.program.contains("M03"), "{}", live.program);
    let dir = scratch("admitted-live");
    // At the full gate this bench's own path is held by the verifier's
    // rapid-in-stock warnings (as on main, where its suite posts it
    // gate-free); whatever holds it, it is not the lathe door.
    let held = posting::post_script(&world, &dir.join("c.nc"), FANUC_LATHE).err().unwrap_or_default();
    assert!(!held.contains("work spindle"), "the lathe door lets a C section through: {held}");
    let _ = std::fs::remove_dir_all(&dir);

    // A turning-family facing pass on the same mill-turn: the work spindle.
    let world = camcraft_app::world::new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let stock = StockShape::Cylinder { diameter: 12.0, height: 32.0 };
    h.execute(&world, SetStock::new(&world, stock, at).expect("bar stock"));
    h.execute(&world, LoadMachine::new(&world, "millturn.mch".into(), Some(mill_turn())).expect("mill-turn"));
    let insert = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t).expect("catalog insert");
    h.execute(&world, AddTool::new(&world, "TN", insert).expect("insert"));
    let bar = revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)], 16);
    h.execute(&world, ImportModel::from_mesh("bar", bar));
    let facing = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(14.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    create(&world, &mut h, "T1", "TN", SrcRef::Entity("bar".into()), facing);
    let dir = scratch("admitted-turn");
    assert_eq!(voice(&world, "T1"), Some(LatheVoice::WorkSpindle));
    let turned = gate_free(FANUC_LATHE, &world);
    assert!(turned.program.lines().any(|l| l.starts_with("G97 S") && l.ends_with(" M03")), "{}", turned.program);
    // Held at the full gate by this bench's finish-envelope warning (as
    // on main), never by the lathe door.
    let held = posting::post_script(&world, &dir.join("t.nc"), FANUC_LATHE).err().unwrap_or_default();
    assert!(!held.contains("work spindle"), "the lathe door lets a turning pass through: {held}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// One MCP tool call on the in-process server: whether the tool
/// refused, and its error text or structured answer.
fn mcp(server: &mut camcraft_app::mcp::Server, name: &str, args: Value) -> (bool, Value) {
    let msg = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": name, "arguments": args } });
    let reply = server.handle(&msg).expect("a request gets a response");
    let result = &reply["result"];
    assert!(result.is_object(), "tools/call answers with a result: {reply}");
    let refused = result["isError"] == json!(true);
    (refused, if refused { result["content"][0]["text"].clone() } else { result["structuredContent"].clone() })
}
