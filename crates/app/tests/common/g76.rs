//! The G76 suites' shared document (#1842 split it out of
//! lathe_post_g76_schedule.rs, which stood at the 400-line law): a
//! threaded bar on a lathe, the thread generated with a chosen infeed
//! plan, the Fanuc lathe post chosen with G76 output, and the control's
//! schedule for the posted words.
//!
//! Provenance: our own fixtures. The control model follows the published
//! G76 two-block descriptions (FANUC Series 0i-F lathe operator's manual
//! B-64604EN-1 section 4.2.7; Siemens SINUMERIK ISO Turning programming
//! manual A5E52211627B, "Multiple-thread cutting cycle (G76)"; GSK988TA
//! G76) - behaviour only, see docs/decisions/1842-g76-schedule-sources.md.

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
use camcraft_app::turning::ThreadInfeed;
use camcraft_app::world::new_world;

pub const TAU: f64 = std::f64::consts::TAU;
pub const LATHE_CPS: &str = include_str!("../../../../assets/posts/fanuc-lathe.cps");
pub const MAJOR: f64 = 6.0;
pub const PITCH: f64 = 2.0;
pub const DEPTH: f64 = 1.2;

pub fn revolve(meridian: &[(f64, f64)]) -> Mesh {
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
pub fn threaded() -> Mesh {
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
pub fn g76_world(infeed: f64, plan: ThreadInfeed) -> World {
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
pub fn post(world: &World) -> (String, Vec<String>) {
    let stream = camcraft_app::post::cnc::serialize(&posting::stream_records(world).expect("generated"));
    post_stream(world, &stream, "mm")
}

/// `stream` posted in `unit` ("mm" or "inch") on the world's machine.
pub fn post_stream(world: &World, stream: &str, unit: &str) -> (String, Vec<String>) {
    let mut options = posting::options_value(posting::post_options(world));
    if let Some(m) = options.as_object_mut() {
        m.insert("Units".into(), serde_json::json!(unit));
    }
    let out = engine::run_with(LATHE_CPS, &posting::machine_value(world), stream, &options).expect("the lathe script runs");
    (out.program, out.warnings)
}

/// A word's number from the G76 block whose words include `marker`.
pub fn word(text: &str, marker: &str, letter: char) -> String {
    let line = text.lines().find(|l| l.starts_with("G76") && l.contains(marker)).expect("the G76 block");
    line.split_whitespace()
        .find_map(|w| w.strip_prefix(letter).filter(|_| w != "G76").map(str::to_string))
        .unwrap_or_else(|| panic!("{letter} in {line}"))
}

/// The program's least increment in mm: 0.001 mm under G21, 0.0001 in
/// under G20 (the no-point words count it; the R word is in its unit).
pub fn least_increment(text: &str) -> f64 {
    if text.lines().any(|l| l.split_whitespace().any(|w| w == "G20")) {
        0.00254
    } else {
        0.001
    }
}

/// The posted words as numbers: (m, a, dmin, d, k, dd), lengths in mm.
pub fn g76_words(text: &str) -> (usize, f64, f64, f64, f64, f64) {
    let inc = least_increment(text);
    let unit = if inc < 0.002 { 1.0 } else { 25.4 };
    let p1 = word(text, "R", 'P');
    let (m, a) = (p1[0..2].parse::<usize>().expect("m"), p1[4..6].parse::<f64>().expect("a"));
    let dmin = word(text, "R", 'Q').parse::<f64>().expect("dmin") * inc;
    let d = word(text, "R", 'R').parse::<f64>().expect("d") * unit;
    let k = word(text, "F", 'P').parse::<f64>().expect("k") * inc;
    let dd = word(text, "F", 'Q').parse::<f64>().expect("dd") * inc;
    (m, a, dmin, d, k, dd)
}

/// The control's depths (radius, from the crest, in mm) for the posted
/// words. It works on the words as written - each rounded on its own -
/// never on the record's numbers. The minimum clamps the cut of ONE pass
/// (the depth below the pass before it, as cut): both published
/// descriptions state the clamp on that one-pass cut (#1842).
pub fn control_depths(text: &str) -> (Vec<f64>, f64) {
    let (m, a, dmin, d, k, dd) = g76_words(text);
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
pub fn laid(world: &World) -> (Vec<(f64, f64)>, f64) {
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

/// The kernel's laid pass ends: the z of each pass's last threading arc
/// (the move before its retract), and the record's end z.
pub fn laid_ends(world: &World) -> (Vec<f64>, f64) {
    let e = camcraft_app::model::find_by_name(world, "TH1").expect("op");
    let path = camcraft_app::model::toolpath_of(world, &e).expect("cache").path;
    let ends = path
        .moves
        .windows(2)
        .filter(|w| w[0].arc.is_some() && w[1].kind == camcraft_app::cl::MoveKind::Rapid)
        .map(|w| w[0].to.z)
        .collect();
    (ends, path.threads.first().expect("record").end.z)
}

pub fn same_ladder(world: &World, text: &str) {
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
