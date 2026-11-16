//! The G76 words state the verified passes (issue #417): with the
//! document's "Threading: G76 cycle" option the control cuts the
//! passes itself, so the words posted must describe exactly the passes
//! the kernel laid and the verdict judged. The control's schedule for
//! the words `G76 P(m)(r)(a) Q(dmin) R(d)` / `G76 X Z P(k) Q(dd) F` is
//! modelled here as the public G76 references define it (GSK988TA
//! manual G76, Fanuc-compatible; behaviour only): pass n reaches
//! max(dd*sqrt(n), previous + dmin) up to k - d, the first finishing
//! pass cuts d to k and m - 1 more repeat k, and with a non-zero tool
//! angle each pass starts tan(a/2) x its depth along Z from the start
//! point. Pinned: a constant-area flank thread's golden whose words
//! reproduce its laid depths and stations; the default thread's words
//! reproducing its three passes; a refusal by name for each plan the
//! cycle cannot state - the repeat of each roughing depth (issue #418)
//! among them, which posts as G32 passes.
//!
//! Regenerate after an intentional change:
//! `WRITE_GOLDEN=1 cargo test -p camcraft-app --test lathe_post_g76_schedule`
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread_cycle's
//! record), app::posting + the lathe dialect's G76 words and refusals.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetPostOptions,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::engine;
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostOptions, PostRef, ThreadingOutput};
use camcraft_app::tools::catalog;
use camcraft_app::turning::{DepthSchedule, InfeedMethod, ThreadInfeed};
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

/// Wall r 6, a 60-degree V every 2.0 from z 8 to z 14, 1.2 deep.
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
    m.extend([(MAJOR, 14.0 + half), (MAJOR, 40.0), (0.0, 40.0)]);
    revolve(&m)
}

/// The threaded bar on a lathe, the thread generated with `plan` and
/// first depth `infeed`, the lathe post chosen with G76 output.
fn g76_world(infeed: f64, plan: ThreadInfeed) -> World {
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
        infeed,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: plan,
        hand: camcraft_app::cl::ThreadHand::Right,
    };
    let cmd = CreateOperation::new(&world, "TH1", "TD-1732-60", SrcRef::Entity("bar".into()), params);
    history.execute(&world, cmd.expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("generate"));
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    let g76 = PostOptions { threading: ThreadingOutput::G76Cycle, ..PostOptions::default() };
    history.execute(&world, SetPostOptions::new(&world, g76).expect("G76"));
    world
}

/// The program and the script's own refusals.
fn post(world: &World) -> (String, Vec<String>) {
    let stream = camcraft_app::post::cnc::serialize(&posting::stream_records(world).expect("generated"));
    post_stream(world, &stream, "mm")
}

/// `stream` posted in `unit` ("mm" or "inch") on the world's machine.
fn post_stream(world: &World, stream: &str, unit: &str) -> (String, Vec<String>) {
    let mut options = posting::options_value(posting::post_options(world));
    if let Some(m) = options.as_object_mut() {
        m.insert("Units".into(), serde_json::json!(unit));
    }
    let out = engine::run_with(LATHE_CPS, &posting::machine_value(world), stream, &options).expect("the lathe script runs");
    (out.program, out.warnings)
}

/// A word's number from the G76 block whose words include `marker`.
fn word(text: &str, marker: &str, letter: char) -> String {
    let line = text.lines().find(|l| l.starts_with("G76") && l.contains(marker)).expect("the G76 block");
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(letter).filter(|_| w != "G76").map(str::to_string))
        .unwrap_or_else(|| panic!("{letter} in {line}"))
}

/// The program's least increment in mm: 0.001 mm under G21, 0.0001 in
/// under G20 (the no-point words count it; the R word is in its unit).
fn least_increment(text: &str) -> f64 {
    if text.lines().any(|l| l.split_whitespace().any(|w| w == "G20")) {
        0.00254
    } else {
        0.001
    }
}

/// The control's depths (radius, from the crest, in mm) for the posted
/// words. It works on the words as written - each rounded on its own -
/// never on the record's numbers.
fn control_depths(text: &str) -> (Vec<f64>, f64) {
    let inc = least_increment(text);
    let unit = if inc < 0.002 { 1.0 } else { 25.4 };
    let p1 = word(text, "R", 'P');
    let (m, a) = (p1[0..2].parse::<usize>().expect("m"), p1[4..6].parse::<f64>().expect("a"));
    let dmin = word(text, "R", 'Q').parse::<f64>().expect("dmin") * inc;
    let d = word(text, "R", 'R').parse::<f64>().expect("d") * unit;
    let k = word(text, "F", 'P').parse::<f64>().expect("k") * inc;
    let dd = word(text, "F", 'Q').parse::<f64>().expect("dd") * inc;
    let mut cum = Vec::new();
    let mut prev = 0.0_f64;
    let mut n = 1.0_f64;
    loop {
        let next = (dd * n.sqrt()).max(prev + dmin);
        if next >= k - d - 1e-9 {
            cum.push(k - d);
            break;
        }
        cum.push(next);
        prev = next;
        n += 1.0;
    }
    cum.extend(std::iter::repeat_n(k, m));
    (cum, a)
}

/// The kernel's laid passes: (station z, depth from the crest) at each
/// plunge, and the record's start z.
fn laid(world: &World) -> (Vec<(f64, f64)>, f64) {
    let e = camcraft_app::model::find_by_name(world, "TH1").expect("op");
    let path = camcraft_app::model::toolpath_of(world, &e).expect("cache").path;
    let rho = |q: Point3| (q.x * q.x + q.y * q.y).sqrt();
    let passes = path
        .moves
        .iter()
        .filter(|m| m.kind == camcraft_app::cl::MoveKind::Feed && m.arc.is_none())
        .map(|m| (m.to.z, MAJOR - rho(m.to)))
        .collect();
    (passes, path.threads.first().expect("record").start.z)
}

fn same_ladder(world: &World, text: &str) {
    let (want, a) = control_depths(text);
    let (got, z0) = laid(world);
    assert_eq!(got.len(), want.len(), "pass count: laid {got:?} vs the control's {want:?}\n{text}");
    let tan = (a * 0.5).to_radians().tan();
    // mm words are exact on the micron lattice; an inch word is one
    // ten-thousandth from its mm value at most 1.5 increments down a rung.
    let tol = if least_increment(text) < 0.002 { 1e-3 } else { 1.5 * 0.00254 };
    for ((z, depth), w) in got.iter().zip(&want) {
        assert!((depth - w).abs() < tol, "depth: laid {got:?} vs the control's {want:?}");
        // #629's ladder runs toward the chuck (-Z): the record starts a
        // full flank shift toward the free end, each pass `w·tan` back
        // toward the chuck from it (the merge of #416 onto #629).
        assert!((z - (z0 - w * tan)).abs() < tol, "station: laid {got:?}, start {z0}, a {a}");
    }
}

#[test]
fn a_constant_area_flank_g76_states_its_verified_passes() {
    let plan = ThreadInfeed {
        method: InfeedMethod::Flank,
        flank_angle: 27.5,
        schedule: DepthSchedule::ConstantArea,
        min_depth: 0.1,
        finish_allowance: 0.05,
        spring_passes: 1,
        repeat_each_depth: false,
    };
    let world = g76_world(0.4, plan);
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G76 P020055 Q100 R0.05\n"), "{text}");
    assert!(text.contains(" P1200 Q400 F2.\n"), "{text}");
    same_ladder(&world, &text);
    common::write_golden("nc/fanuc-lathe/thread_g76_area.nc", &text);
}

#[test]
fn the_default_thread_g76_states_its_three_passes() {
    let world = g76_world(0.4, ThreadInfeed::default());
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    same_ladder(&world, &text);
    assert!(text.contains("G76 P010000 Q400 R0.4\n"), "{text}");
    // A remainder rung: 0.5, 1.0 and the root 1.2 - R is the last 0.2,
    // not the rung.
    let world = g76_world(0.5, ThreadInfeed::default());
    let (text, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G76 P010000 Q500 R0.2\n"), "{text}");
    same_ladder(&world, &text);
}

#[test]
fn the_default_thread_g76_states_its_three_passes_in_inch() {
    // PR #517 review: each inch word rounds on its own (0.4 mm = Q157,
    // 1.2 mm = P472), so R0.0157 left k - d = 0.0315 one ten-thousandth
    // past the second rung 0.0314 and the control cut a fourth pass
    // nobody verified. R comes from the rounded words' own ladder.
    let world = g76_world(0.4, ThreadInfeed::default());
    let stream = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    let (text, refused) = post_stream(&world, &stream, "inch");
    assert!(refused.is_empty(), "{refused:?}");
    assert!(text.contains("G76 P010000 Q157 R0.0158\n"), "{text}");
    assert!(text.contains(" P472 Q157 F0.07874\n"), "{text}");
    let (want, _) = control_depths(&text);
    assert_eq!(want.len(), 3, "the control cuts {want:?} from\n{text}");
    same_ladder(&world, &text);
}

/// `stream` with the THREAD record's `key` numbers (whole microns) replaced.
fn with_thread_words(stream: &str, words: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for line in stream.lines() {
        if line.starts_with("THREAD X") {
            let mut w: Vec<String> = line.split_whitespace().map(str::to_string).collect();
            for (key, value) in words {
                let at = w.iter().position(|x| x == key).expect("the THREAD word");
                w[at + 1] = (*value).to_string();
            }
            out.push_str(&w.join(" "));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

#[test]
fn a_ladder_the_rounded_words_cannot_state_is_refused_by_name() {
    // Inch ladders whose rounded first depth drifts the control's rungs
    // off the record's: 0.1 mm = Q39 (0.0039 in, 0.9906 mm) over 13
    // sqrt rungs leaves the allowance two increments from the record's
    // R0.033 (0.839 mm) - one past what the words may round by.
    let world = g76_world(0.4, ThreadInfeed::default());
    let stream = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    let cases: [(&str, &str, &str, &str); 2] = [
        ("100", "10", "839", "Q39 (first depth), Q4 (minimum depth) and P472 (thread height) make the control cut 13 rough passes"),
        ("146", "10", "288", "make the control cut 39 rough passes"),
    ];
    for (first, min, finish, why) in cases {
        let s = with_thread_words(&stream, &[("FIRST", first), ("MIN", min), ("FINISH", finish)]);
        let (text, refused) = post_stream(&world, &s, "inch");
        assert!(
            refused.iter().any(|w| w.contains("G76 threading cycle refused - the rounded words cannot state the verified passes") && w.contains(why)),
            "{first}/{finish}: not refused by name: {refused:?}\n{text}"
        );
        assert!(!text.contains("G76 X"), "{first}: a refused cycle was written: {text}");
    }
    // The same record in mm: the micron words state 13 rungs, R one
    // micron over the record (rung 13 is 0.36056, not 0.361).
    let s = with_thread_words(&stream, &[("FIRST", "100"), ("MIN", "10"), ("FINISH", "839")]);
    let (text, refused) = post_stream(&world, &s, "mm");
    assert!(refused.is_empty() && text.contains("G76 P010000 Q10 R0.84\n"), "{refused:?}\n{text}");
}

#[test]
fn a_plan_the_cycle_cannot_state_is_refused_by_name() {
    let alt = ThreadInfeed { method: InfeedMethod::Alternating, flank_angle: 30.0, ..ThreadInfeed::default() };
    let modified = ThreadInfeed { method: InfeedMethod::Flank, flank_angle: 29.5, ..ThreadInfeed::default() };
    let repeat = ThreadInfeed { repeat_each_depth: true, ..ThreadInfeed::default() };
    let cases: [(f64, ThreadInfeed, &str); 4] = [
        (0.4, alt, "alternating flank infeed"),
        (0.4, modified, "not a G76 tool angle"),
        (1.5, ThreadInfeed::default(), "one-pass thread"),
        (0.4, repeat, "repeat each roughing depth"),
    ];
    for (infeed, plan, why) in cases {
        let world = g76_world(infeed, plan);
        let (text, refused) = post(&world);
        assert!(
            refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains(why)),
            "{why}: not refused by name: {refused:?}"
        );
        assert!(!text.contains("G76 X"), "{why}: a refused cycle was written: {text}");
    }
}

#[test]
fn the_alternating_flag_rides_the_stream_and_the_file() {
    let alt = ThreadInfeed { method: InfeedMethod::Alternating, flank_angle: 30.0, ..ThreadInfeed::default() };
    let world = g76_world(0.4, alt);
    let records = posting::stream_records(&world).expect("generated");
    let text = camcraft_app::post::cnc::serialize(&records);
    assert!(text.lines().any(|l| l.starts_with("THREAD X") && l.ends_with(" ALT 1")), "{text}");
    let back = camcraft_app::post::cnc::parse(&text).expect("parses");
    assert_eq!(camcraft_app::post::cnc::serialize(&back), text, "write-parse-write is a fixed point");
    let file = std::env::temp_dir().join(format!("camcraft-g76-alt-{}.json", std::process::id()));
    camcraft_app::project::save(&world, &file).expect("saved");
    let other = new_world();
    camcraft_app::project::load(&other, &file).expect("loaded");
    let _ = std::fs::remove_file(&file);
    let e = camcraft_app::model::find_by_name(&other, "TH1").expect("op");
    let t = camcraft_app::model::toolpath_of(&other, &e).expect("cache").path.threads;
    assert!(t.first().is_some_and(|t| t.alternating), "{t:?}");
    // A single-flank record writes no ALT word: every earlier stream's bytes.
    let world = g76_world(0.4, ThreadInfeed::default());
    let text = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    assert!(!text.contains(" ALT "), "{text}");
}

#[test]
fn a_repeated_depth_posts_as_g32_and_its_flag_rides_the_stream() {
    let repeat = ThreadInfeed { repeat_each_depth: true, ..ThreadInfeed::default() };
    let world = g76_world(0.4, repeat);
    let records = posting::stream_records(&world).expect("generated");
    let text = camcraft_app::post::cnc::serialize(&records);
    assert!(text.lines().any(|l| l.starts_with("THREAD X") && l.ends_with(" REP 1")), "{text}");
    let back = camcraft_app::post::cnc::parse(&text).expect("parses");
    assert_eq!(camcraft_app::post::cnc::serialize(&back), text, "write-parse-write is a fixed point");
    // With G32 output the same passes post: five plunges, each a G32 run.
    let mut history = History::default();
    let g32 = PostOptions { threading: ThreadingOutput::G32Passes, ..PostOptions::default() };
    history.execute(&world, SetPostOptions::new(&world, g32).expect("G32"));
    let (nc, refused) = post(&world);
    assert!(refused.is_empty(), "{refused:?}");
    assert!(nc.contains("G32") && !nc.contains("G76"), "{nc}");
    let (laid, _) = laid(&world);
    let depths: Vec<f64> = laid.iter().map(|&(_, d)| d).collect();
    assert_eq!(depths.len(), 5, "{depths:?}");
    for (d, w) in depths.iter().zip([0.4, 0.4, 0.8, 0.8, 1.2]) {
        assert!((d - w).abs() < 1e-9, "{depths:?}");
    }
    // Off writes no REP word: every earlier stream's bytes.
    let world = g76_world(0.4, ThreadInfeed::default());
    let text = camcraft_app::post::cnc::serialize(&posting::stream_records(&world).expect("generated"));
    assert!(!text.contains(" REP "), "{text}");
}
