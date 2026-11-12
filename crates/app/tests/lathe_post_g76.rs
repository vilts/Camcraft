//! The G76 threading cycle on the Fanuc LATHE post (issue #34): the
//! thread kernel's passes carry a thread-cycle record in the stream,
//! and with the document's post option "Threading: G76 cycle" the
//! lathe post writes the two-block G76 from it instead of the G32
//! passes. Pinned here: the G76 golden, the G32 default unchanged by
//! the record (the stream without THREAD records posts byte-
//! identically), the record through the stream text and the project
//! file, the refusals by name (no record, a record out of range), and
//! the option as an undoable command that the in-app post follows.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_g76`
//!
//! Gate map (M4.4): covers app::posting (post options, stream_records,
//! post_script), post::post (cnc THREAD record, engine run_with, the
//! lathe dialect's G76), app::commands (SetPostOptions undo),
//! app::project (thread record + post options round trip).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;
use serde_json::json;

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetPostOptions,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, cnc_thread, engine};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostOptions, PostRef, ThreadingOutput};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const MAJOR: f64 = 6.0;
const PITCH: f64 = 2.0;
const DEPTH: f64 = 1.2;

fn revolve(meridian: &[(f64, f64)]) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..16 {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                facets.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                facets.push(Triangle::new(a1, a2, b2));
            } else {
                facets.push(Triangle::new(a1, a2, b2));
                facets.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    Mesh::from_facets(facets)
}

/// The app threading suite's bar: wall r 6, a 60-degree V every 2.0
/// from z 8 to z 14, 1.2 deep.
fn threaded() -> Mesh {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let root = MAJOR - DEPTH;
    let mut m = vec![(0.0, -16.0), (MAJOR, -16.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((root, c));
        m.push((MAJOR, c + half));
        if c + PITCH <= 14.0 + 1e-9 {
            m.push((MAJOR, c + PITCH - half));
        }
        c += PITCH;
    }
    m.push((MAJOR, 14.0 + half));
    m.push((MAJOR, 40.0));
    m.push((0.0, 40.0));
    revolve(&m)
}

/// The threaded bar in bar-sized stock on a lathe that states its
/// ceiling and coolant, the thread generated, the lathe post chosen.
fn thread_world() -> (World, History) {
    thread_world_hand(ThreadHand::Right)
}

fn thread_world_hand(hand: ThreadHand) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 56.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "TD-1732-60").map(|(_, t)| t).expect("insert");
    history.execute(&world, AddTool::new(&world, "TD-1732-60", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", threaded()));
    let params = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(14.3),
        bottom_z: Height::fixed(7.7),
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        hand,
    };
    let cmd = CreateOperation::new(&world, "TH1", "TD-1732-60", SrcRef::Entity("bar".into()), params);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("generate"));
    let cmd = SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into()));
    history.execute(&world, cmd.expect("lathe post"));
    (world, history)
}

fn g76() -> PostOptions {
    PostOptions { threading: ThreadingOutput::G76Cycle, ..PostOptions::default() }
}

/// A stream through the lathe script with the given options, gate-free;
/// (program, the warnings the SCRIPT added beyond the stream's WARNs).
fn post(world: &World, records: &[cnc::Record], options: PostOptions) -> (String, Vec<String>) {
    let out = engine::run_with(
        LATHE_CPS,
        &posting::machine_value(world),
        &cnc::serialize(records),
        &posting::options_value(options),
    )
    .expect("the lathe script runs");
    let stream_warns = records.iter().filter(|r| matches!(r, cnc::Record::Warn { .. })).count();
    (out.program, out.warnings[stream_warns.min(out.warnings.len())..].to_vec())
}

fn thread_rec(records: &[cnc::Record]) -> cnc_thread::ThreadRec {
    records
        .iter()
        .find_map(|r| match r {
            cnc::Record::Thread(Some(t)) => Some(t.clone()),
            _ => None,
        })
        .expect("the stream carries a thread record")
}

fn with_rec(records: &[cnc::Record], f: impl Fn(&mut cnc_thread::ThreadRec)) -> Vec<cnc::Record> {
    records
        .iter()
        .cloned()
        .map(|r| match r {
            cnc::Record::Thread(Some(mut t)) => {
                f(&mut t);
                cnc::Record::Thread(Some(t))
            }
            other => other,
        })
        .collect()
}

#[test]
fn a_g76_thread_posts_to_the_lathe_golden() {
    let (world, mut history) = thread_world();
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76 chosen"));
    let records = posting::stream_records(&world).expect("generated");
    let (text, refused) = post(&world, &records, g76());
    assert!(refused.is_empty(), "the script refused the record: {refused:?}");
    // Root Ø9.6 at Z8, from the start point at the clearance radius on
    // the TOP centre (Z14) - #629: a right-hand thread cut toward the
    // chuck under M03; one finishing pass, square pull-out, radial
    // infeed; 1.2 high in 0.4 rungs; lead 2.
    assert!(text.contains("M03\n") && !text.contains("M04"), "{text}");
    assert!(text.contains("G00 X12.1 Z14\nG76 P010000 Q400 R0\n"), "{text}");
    assert!(text.contains("G76 X9.6 Z8 P1200 Q400 F2\n"), "{text}");
    assert_eq!(text.matches("G76").count(), 2, "one two-block cycle: {text}");
    assert!(!text.contains("G32"), "the passes are the control's now: {text}");
    // The in-app post (gate and all) follows the document's option.
    let path = std::env::temp_dir().join(format!("camcraft-g76-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    posting::post_setup(&world, &path).expect("a clean thread posts through the gate");
    let gated = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert_eq!(gated, text, "the gated program is the stream's own G76 translation");
    common::write_golden("nc/fanuc-lathe/thread_g76.nc", &text);
}

#[test]
fn the_g32_default_is_unchanged_by_the_record() {
    let (world, _history) = thread_world();
    assert_eq!(posting::post_options(&world), PostOptions::default());
    let records = posting::stream_records(&world).expect("generated");
    let bare: Vec<cnc::Record> = records.iter().filter(|r| !matches!(r, cnc::Record::Thread(_))).cloned().collect();
    assert!(bare.len() + 2 == records.len(), "one THREAD / THREAD END pair");
    let (text, refused) = post(&world, &records, PostOptions::default());
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G32 Z") && !text.contains("G76"), "{text}");
    // Byte-identical to the stream without its records, and to the
    // option-less engine (every pre-#34 caller).
    assert_eq!(text, post(&world, &bare, PostOptions::default()).0);
    let plain = engine::run(LATHE_CPS, &posting::machine_value(&world), &cnc::serialize(&records)).expect("runs");
    assert_eq!(text, plain.program);
    // A mill post has no cycle hook: the record changes nothing there.
    let fanuc = include_str!("../../../assets/posts/fanuc.cps");
    let mch = posting::machine_value(&world);
    let a = engine::run_with(fanuc, &mch, &cnc::serialize(&records), &posting::options_value(g76())).expect("runs");
    let b = engine::run(fanuc, &mch, &cnc::serialize(&bare)).expect("runs");
    assert_eq!(a.program, b.program);
}

#[test]
fn the_thread_record_survives_the_round_trip() {
    let (world, mut history) = thread_world();
    let e = camcraft_app::model::find_by_name(&world, "TH1").expect("op");
    let path = camcraft_app::model::toolpath_of(&world, &e).expect("cache").path;
    assert_eq!(path.threads.len(), 1, "one record over the passes");
    let t = path.threads[0];
    assert!((t.major_diameter - 12.0).abs() < 1e-6 && (t.minor_diameter - 9.6).abs() < 1e-6, "{t:?}");
    assert!((t.lead - PITCH).abs() < 1e-12 && (t.height - DEPTH).abs() < 1e-6, "{t:?}");
    assert!((t.first_depth - 0.4).abs() < 1e-6 && (t.min_depth - 0.4).abs() < 1e-6, "{t:?}");
    assert!((t.start.z - 14.0).abs() < 1e-6 && (t.end.z - 8.0).abs() < 1e-6, "{t:?}");
    assert_eq!(t.hand, ThreadHand::Right);
    assert_eq!(path.moves[t.first].to, t.start);
    assert!(t.last + 1 < path.moves.len(), "the out-rapid stays outside the span");
    // The stream text: write, read, write again - identical, and the
    // record read back is the record written (on the µm lattice).
    let records = posting::stream_records(&world).expect("generated");
    let text = cnc::serialize(&records);
    let back = cnc::parse(&text).expect("parses");
    assert_eq!(cnc::serialize(&back), text);
    let (a, b) = (thread_rec(&records), thread_rec(&back));
    assert!((a.lead - b.lead).abs() < 1e-9 && (a.height - b.height).abs() < 1e-6, "{a:?} vs {b:?}");
    assert!((a.end[2] - b.end[2]).abs() < 1e-6 && a.spring_passes == b.spring_passes);
    assert!(text.contains(" HAND right\n"), "the stream states the hand: {text}");
    assert_eq!((a.hand, b.hand), (ThreadHand::Right, ThreadHand::Right));
    // The project file: the cached record and the option both survive.
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));
    let file = std::env::temp_dir().join(format!("camcraft-g76-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &file).expect("saved");
    let other = new_world();
    camcraft_app::project::load(&other, &file).expect("loaded");
    let _ = std::fs::remove_file(&file);
    let e2 = camcraft_app::model::find_by_name(&other, "TH1").expect("op");
    let loaded = camcraft_app::model::toolpath_of(&other, &e2).expect("cache").path;
    assert_eq!(loaded.threads.len(), 1, "the record survives the file");
    let l = loaded.threads[0];
    assert_eq!((l.first, l.last, l.spring_passes, l.hand), (t.first, t.last, t.spring_passes, t.hand));
    assert_eq!(loaded.moves[l.first].to, l.start, "still opens on its move");
    for (x, y) in [(l.lead, t.lead), (l.height, t.height), (l.minor_diameter, t.minor_diameter), (l.end.z, t.end.z)] {
        assert!((x - y).abs() < 1e-6, "{l:?} vs {t:?}");
    }
    assert_eq!(posting::post_options(&other), g76());
}

#[test]
fn g76_refuses_by_name_without_a_record_or_out_of_range() {
    let (world, _history) = thread_world();
    let records = posting::stream_records(&world).expect("generated");
    let bare: Vec<cnc::Record> = records.iter().filter(|r| !matches!(r, cnc::Record::Thread(_))).cloned().collect();
    let (_, refused) = post(&world, &bare, g76());
    assert!(
        refused.iter().any(|w| w.contains("G76") && w.contains("no thread cycle record")),
        "passes without a record posted under G76: {refused:?}"
    );
    type Bend = fn(&mut cnc_thread::ThreadRec);
    let cases: [(&str, Bend); 6] = [
        ("infeed angle", |t| t.infeed_angle = 45.0),
        ("spring passes", |t| t.spring_passes = 120),
        ("pull-out", |t| t.pull_out = 0.13),
        ("not an external thread", |t| t.minor_diameter = 13.0),
        ("first-pass depth", |t| t.first_depth = 5.0),
        ("no lead", |t| t.lead = 0.0),
    ];
    for (why, bend) in cases {
        let (text, refused) = post(&world, &with_rec(&records, bend), g76());
        assert!(
            refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains(why)),
            "{why}: not refused by name: {refused:?}"
        );
        assert!(!text.contains("G76 X"), "{why}: a refused cycle was written: {text}");
    }
}

#[test]
fn the_threading_option_rides_the_history_both_ways() {
    let (world, mut history) = thread_world();
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));
    assert_eq!(posting::post_options(&world), g76());
    assert!(SetPostOptions::new(&world, g76()).is_err(), "the same choice again is no step");
    assert!(history.undo(&world));
    assert_eq!(posting::post_options(&world), PostOptions::default());
    let path = std::env::temp_dir().join(format!("camcraft-g76-undo-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    posting::post_setup(&world, &path).expect("posts");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert!(text.contains("G32") && !text.contains("G76"), "undone: back to G32 passes: {text}");
    assert!(history.redo(&world));
    assert_eq!(posting::post_options(&world), g76());
    assert_eq!(posting::options_value(g76()), json!({ "Threading": "G76" }));
}

/// The program's G32 blocks' Z words, in order.
fn g32_zs(text: &str) -> Vec<f64> {
    text.lines()
        .filter(|l| l.starts_with("G32 "))
        .filter_map(|l| l.split_whitespace().find_map(|w| w.strip_prefix('Z')).and_then(|z| z.parse().ok()))
        .collect()
}

#[test]
fn a_right_hand_thread_posts_toward_the_chuck_under_m03() {
    // #629: the standard right-hand external thread - M03, every G32
    // pass falls in Z (toward the chuck), G76 ends below its start.
    // Before the fix the passes rose under M03: a left-hand thread.
    let (world, _history) = thread_world();
    let records = posting::stream_records(&world).expect("generated");
    assert!(
        !records.iter().any(|r| matches!(r, cnc::Record::Param { name, .. } if name == "SpindleDirection")),
        "forward is the stream's silent default"
    );
    let (text, refused) = post(&world, &records, PostOptions::default());
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G97 S800 M03\n") && !text.contains("M04"), "{text}");
    let zs = g32_zs(&text);
    assert_eq!(zs.len(), 18, "three passes of six half-turns: {text}");
    for pass in zs.chunks(6) {
        assert!(pass.windows(2).all(|w| w[1] < w[0]), "every G32 pass descends: {pass:?}");
        assert_eq!((pass[0], pass[5]), (13.0, 8.0), "{pass:?}");
    }
    let (g76, refused) = post(&world, &records, g76());
    assert!(refused.is_empty(), "{refused:?}");
    assert!(g76.contains("M03") && g76.contains("G00 X12.1 Z14\n") && g76.contains("G76 X9.6 Z8 "), "{g76}");
}

#[test]
fn a_left_hand_thread_posts_toward_the_chuck_under_m04() {
    // #629: the passes still run toward the chuck; the helix turns the
    // other way, which the tool does about the part only under M04 -
    // stated before the section opens and voiced with an operator note.
    let (world, _history) = thread_world_hand(ThreadHand::Left);
    let records = posting::stream_records(&world).expect("generated");
    assert!(
        records.iter().any(|r| matches!(r, cnc::Record::Param { name, value } if name == "SpindleDirection" && value == "reverse")),
        "the stream says the spindle runs reverse"
    );
    assert_eq!(thread_rec(&records).hand, ThreadHand::Left);
    let (text, refused) = post(&world, &records, PostOptions::default());
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G97 S800 M04\n") && !text.contains("M03"), "{text}");
    assert!(text.contains("MOUNT THE INSERT FOR M04"), "{text}");
    let zs = g32_zs(&text);
    assert_eq!(zs.len(), 18, "{text}");
    assert!(zs.chunks(6).all(|p| p.windows(2).all(|w| w[1] < w[0])), "toward the chuck: {zs:?}");
    let (g76, refused) = post(&world, &records, g76());
    assert!(refused.is_empty(), "{refused:?}");
    assert!(g76.contains("M04") && g76.contains("G76 X9.6 Z8 "), "{g76}");
    // The gated in-app post agrees.
    let path = std::env::temp_dir().join(format!("camcraft-lh-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    posting::post_setup(&world, &path).expect("a left-hand thread posts through the gate");
    let gated = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert_eq!(gated, text);
}

#[test]
fn the_hand_is_never_dropped() {
    // #629: every way the hand could go missing or disagree is refused
    // by name - never posted the other hand.
    let (world, _history) = thread_world();
    let records = posting::stream_records(&world).expect("generated");
    // A G76 record claiming the other hand than the section's M03.
    let (text, refused) = post(&world, &with_rec(&records, |t| t.hand = ThreadHand::Left), g76());
    assert!(
        refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains("left-hand thread cut toward the chuck needs M04")),
        "{refused:?}"
    );
    assert!(!text.contains("G76 X"), "{text}");
    // The left-hand passes posted under M03 (the reverse PARAM lost):
    // every half-turn refused, no G32 written.
    let (lw, _h) = thread_world_hand(ThreadHand::Left);
    let lh = posting::stream_records(&lw).expect("generated");
    let lost: Vec<cnc::Record> = lh
        .iter()
        .filter(|r| !matches!(r, cnc::Record::Param { name, .. } if name == "SpindleDirection"))
        .cloned()
        .collect();
    let (text, refused) = post(&lw, &lost, PostOptions::default());
    assert!(refused.iter().any(|w| w.contains("would cut the other hand")), "{refused:?}");
    assert!(!text.contains("G32"), "{text}");
    // A post that cannot voice the hand refuses the reverse section.
    let fanuc = include_str!("../../../assets/posts/fanuc.cps");
    let out = engine::run(fanuc, &posting::machine_value(&lw), &cnc::serialize(&lh)).expect("runs");
    assert!(out.warnings.iter().any(|w| w.contains("does not declare threadHands")), "{:?}", out.warnings);
    // The stream text states the hand, and a record without it is refused.
    let line = cnc::serialize(&records);
    assert!(line.contains(" HAND right\n"), "{line}");
    let bare = line.replace(" HAND right", "");
    assert!(cnc::parse(&bare).is_err_and(|e| e.to_string().contains("HAND")), "a THREAD record without its hand parses");
}
