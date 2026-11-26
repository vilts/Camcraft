//! Chip-break dips on the Fanuc LATHE post (issue #419): a Threading
//! op whose roughing passes carry in-pass dips posts each dip as
//! `G32 X Z F` segments inside the pass's one unbroken G32 chain - the
//! continuous thread cutting FANUC B-64304EN-1 §3.2 / §3.4 states.
//! Pinned here: the G32 golden; the posted chain's Z words and lead the
//! plain thread's (the phase law in the program); G76 refuses dips by
//! name; a post without `continuousThreading` refuses them by name; the
//! lathe script refuses a bent dip (steeper than 45 degrees, deeper
//! than its pass, not continuing a chain) by name; the plan through the
//! command door, the stream text and the project file.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_thread_dips`
//!
//! Gate map (M4.4): covers app::posting (stream_records, post_setup),
//! camcraft_post::post (engine continuousThreading gate, the ChipBreakDips PARAM,
//! THREAD DIP word, the lathe dialect's dip segments and G76 refusal),
//! app::commands (the threading gate on Create), app::project (the
//! infeed plan's dips key, the record's chip_break_dips key).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{ArcPlane, MoveKind};
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetPostOptions,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::cnc::{self, MoveRec, Record};
use camcraft_app::post::engine;
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostOptions, PostRef, ThreadingOutput};
use camcraft_app::tools::catalog;
use camcraft_app::turning::{ThreadDips, ThreadInfeed};
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const MAJOR: f64 = 6.0;
const PITCH: f64 = 2.0;
const DEPTH: f64 = 1.2;
const END: f64 = 24.0;

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

/// Wall r 6, a 60-degree V every 2.0 from z 8 to z 24 (eight leads),
/// 1.2 deep, long plain walls either side so the axis fit is sure.
fn threaded() -> Mesh {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let root = MAJOR - DEPTH;
    let mut m = vec![(0.0, -60.0), (MAJOR, -60.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= END + 1e-9 {
        m.push((root, c));
        m.push((MAJOR, c + half));
        if c + PITCH <= END + 1e-9 {
            m.push((MAJOR, c + PITCH - half));
        }
        c += PITCH;
    }
    m.extend([(MAJOR, END + half), (MAJOR, END + 60.0), (0.0, END + 60.0)]);
    revolve(&m)
}

/// Dips every two leads, 0.3 high - under the 0.4 passes.
fn dips() -> ThreadInfeed {
    ThreadInfeed { dips: ThreadDips { interval: 4.0, height: 0.3, length: 0.0 }, ..ThreadInfeed::default() }
}

fn params(plan: ThreadInfeed) -> OpParams {
    OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(END + 10.0),
        top_z: Height::fixed(END + 0.3),
        bottom_z: Height::fixed(7.7),
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: plan,
        hand: camcraft_app::cl::ThreadHand::Right,
        picks: Default::default(),
    }
}

/// The threaded bar on a lathe with an insert mounted, no op yet.
fn bench() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -60.0), ..Location::default() };
    let shape = camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: END + 120.0 };
    history.execute(&world, SetStock::new(&world, shape, at).expect("bar stock"));
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
    (world, history)
}

/// The thread generated with `plan`, the lathe post chosen (G32).
fn thread_world(plan: ThreadInfeed) -> (World, History) {
    let (world, mut history) = bench();
    let cmd = CreateOperation::new(&world, "TH1", "TD-1732-60", SrcRef::Entity("bar".into()), params(plan));
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("generate"));
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    (world, history)
}

/// A stream through a script with the given options, gate-free;
/// (program, every warning).
fn post_with(cps: &str, world: &World, records: &[Record], options: PostOptions) -> (String, Vec<String>) {
    let out = engine::run_with(
        cps,
        &posting::machine_value(world),
        &cnc::serialize(records),
        &posting::options_value(options),
    )
    .expect("the script runs");
    (out.program, out.warnings)
}

fn post(world: &World, records: &[Record]) -> (String, Vec<String>) {
    post_with(LATHE_CPS, world, records, PostOptions::default())
}

/// The G32 blocks of the program as (has an X word, Z word, F word).
fn g32s(text: &str) -> Vec<(bool, String, String)> {
    text.lines()
        .filter(|l| l.starts_with("G32"))
        .map(|l| {
            let w = |c: char| l.split_whitespace().find_map(|w| w.strip_prefix(c)).unwrap_or("").to_string();
            (l.contains(" X"), w('Z'), w('F'))
        })
        .collect()
}

#[test]
fn a_dipped_thread_posts_g32_segments_to_the_golden() {
    let (world, _history) = thread_world(dips());
    let records = posting::stream_records(&world).expect("generated");
    let stream = cnc::serialize(&records);
    assert!(stream.contains("PARAM ChipBreakDips 1.000000\n"), "the section says it carries dips");
    assert!(stream.contains(" DIP 1"), "the record marks the dips");
    let (text, warnings) = post(&world, &records);
    assert!(warnings.is_empty(), "{warnings:?}");
    // Two roughing passes, three dips each, an out- and an in-segment
    // per dip: twelve G32 X Z F blocks, the X words between the pass's
    // own diameter and 0.6 of diameter above it.
    let dipped: Vec<&str> = text.lines().filter(|l| l.starts_with("G32 X")).collect();
    assert_eq!(dipped.len(), 12, "{text}");
    for l in &dipped {
        let x: f64 = l.split_whitespace().find_map(|w| w.strip_prefix('X')).and_then(|v| v.parse().ok()).expect("X");
        let ok = [11.2, 10.4].iter().any(|d| x >= d - 1e-9 && x <= d + 0.6 + 1e-9);
        assert!(ok, "a dip X word {x} off its pass: {l}");
    }
    assert!(!text.contains("G76"), "{text}");
    // The in-app post (the verdict, the gate) writes the same program.
    let path = std::env::temp_dir().join(format!("camcraft-dips-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    posting::post_setup(&world, &path).expect("a clean dipped thread posts through the gate");
    let gated = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    assert_eq!(gated, text, "the gated program is the stream's own translation");
    common::write_golden("nc/fanuc-lathe/thread_g32_dips.nc", &text);
}

#[test]
fn the_posted_chain_keeps_the_plain_threads_z_words_and_lead() {
    let (plain_world, _h1) = thread_world(ThreadInfeed::default());
    let (dip_world, _h2) = thread_world(dips());
    let plain = g32s(&post(&plain_world, &posting::stream_records(&plain_world).expect("generated")).0);
    let dipped = g32s(&post(&dip_world, &posting::stream_records(&dip_world).expect("generated")).0);
    assert_eq!(plain.len(), dipped.len(), "a dip adds no block and drops none");
    for ((_, za, fa), (_, zb, fb)) in plain.iter().zip(&dipped) {
        assert_eq!(za, zb, "a dip moved a Z word: the helix phase");
        assert_eq!(fa, fb, "a dip changed the lead");
        assert_eq!(fb, "2.", "every G32 states the 2.0 lead as F, with its decimal point (#1836)");
    }
    assert!(plain.iter().all(|(x, _, _)| !x), "the plain thread's G32s carry no X");
}

#[test]
fn g76_refuses_dips_by_name() {
    let (world, mut history) = thread_world(dips());
    let g76 = PostOptions { threading: ThreadingOutput::G76Cycle, ..PostOptions::default() };
    history.execute(&world, SetPostOptions::new(&world, g76).expect("G76"));
    let records = posting::stream_records(&world).expect("generated");
    let (text, warnings) = post_with(LATHE_CPS, &world, &records, g76);
    assert!(
        warnings.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains("chip-break dips")),
        "{warnings:?}"
    );
    assert!(!text.contains("G76 X"), "a refused cycle was written: {text}");
}

#[test]
fn a_post_without_continuous_threading_refuses_dips_by_name() {
    let (world, _history) = thread_world(dips());
    let records = posting::stream_records(&world).expect("generated");
    let undeclared = LATHE_CPS.replace("continuousThreading = true;", "continuousThreading = false;");
    assert_ne!(undeclared, LATHE_CPS, "the lathe post declares the flag");
    let mill = include_str!("../../../assets/posts/fanuc.cps");
    for cps in [undeclared.as_str(), mill] {
        let (_, warnings) = post_with(cps, &world, &records, PostOptions::default());
        assert!(
            warnings.iter().any(|w| w.contains("operation TH1") && w.contains("does not declare continuousThreading")),
            "{warnings:?}"
        );
    }
    // The plain thread asks nothing of the flag.
    let (plain, _h) = thread_world(ThreadInfeed::default());
    let records = posting::stream_records(&plain).expect("generated");
    assert!(!cnc::serialize(&records).contains("ChipBreakDips"));
    let (_, warnings) = post_with(&undeclared, &plain, &records, PostOptions::default());
    assert!(warnings.is_empty(), "{warnings:?}");
}

/// The index of the first dip record (an XY arc whose end radius about
/// its centre differs from its start's) and its start point.
fn first_dip(records: &[Record]) -> (usize, (f64, f64, f64)) {
    let mut pos = (0.0, 0.0, 0.0);
    for (i, r) in records.iter().enumerate() {
        if let Record::Move(m) = r {
            if let (Some((cx, cy)), ArcPlane::Xy) = (m.center, m.plane) {
                let r0 = (pos.0 - cx).hypot(pos.1 - cy);
                let r1 = (m.x - cx).hypot(m.y - cy);
                if (r1 - r0).abs() > 0.01 {
                    return (i, pos);
                }
            }
            pos = (m.x, m.y, m.z);
        }
    }
    panic!("no dip in the stream")
}

/// The stream with the first dip's end set to `radius` about its centre.
fn bent(records: &[Record], radius: impl Fn(f64) -> f64) -> Vec<Record> {
    let (i, start) = first_dip(records);
    let mut out = records.to_vec();
    if let Some(Record::Move(m)) = out.get_mut(i) {
        let (cx, cy) = m.center.expect("an arc");
        let r0 = (start.0 - cx).hypot(start.1 - cy);
        let r1 = (m.x - cx).hypot(m.y - cy);
        let k = radius(r0) / r1;
        m.x = cx + (m.x - cx) * k;
        m.y = cy + (m.y - cy) * k;
    }
    out
}

#[test]
fn the_lathe_script_refuses_a_bent_dip_by_name() {
    let (world, _history) = thread_world(dips());
    let records = posting::stream_records(&world).expect("generated");
    // Steeper than 45 degrees: 0.6 of radius over 1.0 of Z.
    let (_, w) = post(&world, &bent(&records, |r0| r0 + 0.6));
    assert!(w.iter().any(|w| w.contains("steeper than 45 degrees")), "{w:?}");
    // Deeper than the pass it rides.
    let (_, w) = post(&world, &bent(&records, |r0| r0 - 0.2));
    assert!(w.iter().any(|w| w.contains("deeper than the pass")), "{w:?}");
    // Not continuing a chain: a straight feed before the dip ends the
    // G32 chain, so the dip would be a fresh, index-waiting G32.
    let (i, (x, y, z)) = first_dip(&records);
    let mut broken = records.clone();
    let feed = MoveRec {
        kind: MoveKind::Feed,
        x,
        y,
        z,
        feed: Some(40.0),
        center: None,
        ccw: false,
        plane: ArcPlane::Xy,
        abc: None,
    };
    broken.insert(i, Record::Move(feed));
    let (_, w) = post(&world, &broken);
    assert!(w.iter().any(|w| w.contains("does not continue a G32 chain")), "{w:?}");
}

#[test]
fn dips_ride_the_command_door_the_stream_and_the_project_file() {
    // Higher than the first pass's depth: refused by name at Create.
    let (world, _history) = bench();
    let high = ThreadInfeed { dips: ThreadDips { interval: 4.0, height: 0.5, length: 0.0 }, ..ThreadInfeed::default() };
    let e = CreateOperation::new(&world, "TH1", "TD-1732-60", SrcRef::Entity("bar".into()), params(high));
    assert!(e.is_err_and(|e| format!("{e:?}").contains("chip-break dip")), "a dip above the first pass was created");
    // The stream text round trip keeps the DIP word.
    let (world, _history) = thread_world(dips());
    let records = posting::stream_records(&world).expect("generated");
    let text = cnc::serialize(&records);
    let back = cnc::parse(&text).expect("parses");
    assert_eq!(cnc::serialize(&back), text);
    // The project file keeps the plan and the cached record's flag.
    let file = std::env::temp_dir().join(format!("camcraft-dips-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &file).expect("saved");
    let saved = std::fs::read_to_string(&file).expect("read back");
    let other = new_world();
    camcraft_app::project::load(&other, &file).expect("loaded");
    let _ = std::fs::remove_file(&file);
    assert!(saved.contains("\"dips\"") && saved.contains("\"chip_break_dips\""), "both keys written when set");
    let e = camcraft_app::model::find_by_name(&other, "TH1").expect("op");
    let loaded = camcraft_app::model::toolpath_of(&other, &e).expect("cache").path;
    assert!(loaded.threads.iter().all(|t| t.chip_break_dips) && !loaded.threads.is_empty());
    let op = camcraft_app::model::operation_of(&other, &e).expect("an operation");
    match op.params {
        OpParams::Threading { infeed_plan, .. } => assert_eq!(infeed_plan, dips()),
        other => panic!("not a thread: {other:?}"),
    }
    // A plain thread writes neither key.
    let (plain, _h) = thread_world(ThreadInfeed::default());
    let file = std::env::temp_dir().join(format!("camcraft-nodips-{}.json", std::process::id()));
    camcraft_app::project::save(&plain, &file).expect("saved");
    let saved = std::fs::read_to_string(&file).expect("read back");
    let _ = std::fs::remove_file(&file);
    assert!(!saved.contains("\"dips\"") && !saved.contains("chip_break_dips"), "no dips key on a plain thread");
}

#[test]
fn every_dip_dimension_changes_the_operation_stamp() {
    let (world, _history) = thread_world(dips());
    let e = camcraft_app::model::find_by_name(&world, "TH1").expect("op");
    let op = camcraft_app::model::operation_of(&world, &e).expect("an operation");
    let stamp = camcraft_app::operation::op_stamp(&world, &op, "TH1");
    for (dimension, changed) in [
        ("interval", ThreadDips { interval: 6.0, ..dips().dips }),
        ("height", ThreadDips { height: 0.2, ..dips().dips }),
        ("length", ThreadDips { length: 0.8, ..dips().dips }),
    ] {
        let mut edited = op.clone();
        if let OpParams::Threading { infeed_plan, .. } = &mut edited.params {
            infeed_plan.dips = changed;
        }
        assert_ne!(
            camcraft_app::operation::op_stamp(&world, &edited, "TH1"),
            stamp,
            "changing dip {dimension} must invalidate the cached path"
        );
    }
}
