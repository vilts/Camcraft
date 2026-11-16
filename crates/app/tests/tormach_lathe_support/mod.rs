//! Shared fixtures of the Tormach PathPilot lathe post suites
//! (`lathe_post_tormach`, `lathe_post_tormach_thread`, #612): the
//! revolved bars, the PathPilot-style lathe, the post runs and the
//! LinuxCNC syntax check every posted program passes. Not a suite
//! itself.
//!
//! Provenance: our own fixtures; the syntax sets are the G and M words
//! the public LinuxCNC G-code reference and Tormach's lathe programming
//! pages list (behaviour only).

#![allow(dead_code)]

use flecs_ecs::prelude::World;

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetStock, SetUnits};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostOptions, PostRef, ThreadingOutput};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

pub const TAU: f64 = std::f64::consts::TAU;
pub const CPS: &str = include_str!("../../../../assets/posts/tormach-lathe.cps");
pub const NAME: &str = "Tormach PathPilot Lathe";

pub fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / segs as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..segs {
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

/// A PathPilot-style lathe: a stated ceiling (G96's D cap), flood M8,
/// in the unit asked.
pub fn lathe(inch: bool) -> Machine {
    Machine {
        model: "TestBench PathPilot lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        units: inch.then(|| "inch".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    }
}

/// The stock a hand-built stream states before its section (#1604: the
/// post orders the moves around G30 by it): a round bar of this radius
/// about the axis, its face at `top`, and the section's ClearancePlane
/// `clear`, all world mm - the PARAMs `post::dump` writes.
pub fn stock_params(radius: f64, top: f64, clear: f64) -> Vec<cnc::Record> {
    let p = |name: &str, v: f64| cnc::Record::Param { name: name.into(), value: serde_json::json!(v) };
    vec![
        p("StockXYLeft", -radius),
        p("StockXYRight", radius),
        p("StockXYFront", -radius),
        p("StockXYBack", radius),
        p("StockZTop", top),
        p("ClearancePlane", clear),
    ]
}

pub fn lathe_mch(inch: bool) -> serde_json::Value {
    camcraft_app::post::machine::mirror(&lathe(inch))
}

pub fn world_with(
    bar: Mesh,
    stock: camcraft_app::stock::StockShape,
    z0: f64,
    tools: &[&str],
    inch: bool,
) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, z0), ..Location::default() };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe(inch))).expect("lathe"));
    // #902: the program's unit is the document's - an inch run is an
    // inch document (on the inch machine file it always ran on).
    if inch {
        history.execute(&world, SetUnits::new(&world, camcraft_app::units::Unit::Inches).expect("inch document"));
    }
    for t in tools {
        let tool = catalog().into_iter().find(|(n, _)| n == t).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, t, tool).expect("tool"));
    }
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    history.execute(&world, SelectPost::new(&world, PostRef::Named(NAME.into())).expect("a registry name"));
    (world, history)
}

pub fn create(world: &World, history: &mut History, name: &str, tool: &str, params: OpParams) {
    let cmd = CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), params);
    history.execute(world, cmd.expect("create"));
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = camcraft_app::model::find_by_name(world, name).expect("op");
    assert_eq!(
        camcraft_app::operation::status_of(world, &e),
        camcraft_app::operation::OpStatus::Ok,
        "{name} generates"
    );
}

pub const MAJOR: f64 = 6.0;
pub const PITCH: f64 = 2.0;
pub const DEPTH: f64 = 1.2;

/// The threading suite's bar: wall r 6, a 60-degree V every `pitch`
/// from z 8 to z 14, `depth` deep.
pub fn threaded(pitch: f64, depth: f64) -> Mesh {
    let half = depth * (30.0_f64).to_radians().tan();
    let root = MAJOR - depth;
    let mut m = vec![(0.0, -16.0), (MAJOR, -16.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((root, c));
        m.push((MAJOR, c + half));
        if c + pitch <= 14.0 + 1e-9 {
            m.push((MAJOR, c + pitch - half));
        }
        c += pitch;
    }
    m.push((MAJOR, 14.0 + half));
    m.push((MAJOR, 40.0));
    m.push((0.0, 40.0));
    revolve(&m, 16)
}

/// The threading world: an external thread on the Ø12 bar from z 14 to
/// z 8 (top -> toward the chuck), this hand, lead and depth.
pub fn thread_world_with(inch: bool, hand: ThreadHand, pitch: f64, depth: f64) -> (World, History) {
    let stock = camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 56.0 };
    let (world, mut history) = world_with(threaded(pitch, depth), stock, -16.0, &["TD-1732-60"], inch);
    let params = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(14.3),
        bottom_z: Height::fixed(7.7),
        pitch,
        depth,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand,
    };
    create(&world, &mut history, "TH1", "TD-1732-60", params);
    (world, history)
}

pub fn thread_world(inch: bool) -> (World, History) {
    thread_world_with(inch, ThreadHand::Right, PITCH, DEPTH)
}

pub fn g76() -> PostOptions {
    PostOptions { threading: ThreadingOutput::G76Cycle, ..PostOptions::default() }
}

/// Records through the script with these options: (program, warnings).
pub fn post(world: &World, records: &[cnc::Record], options: PostOptions) -> (String, Vec<String>) {
    let mut opts = posting::options_value(options);
    opts["Units"] = serde_json::json!(posting::unit_word(camcraft_app::units::of(world)));
    let out = engine::run_with(CPS, &posting::machine_value(world), &cnc::serialize(records), &opts)
        .expect("the script runs");
    (out.program, out.warnings)
}

/// The document's stream through the script with these options; the
/// script must add no warning beyond the stream's own WARN records.
pub fn program_with(world: &World, options: PostOptions) -> String {
    let records = posting::stream_records(world).expect("every op generated");
    let (program, warnings) = post(world, &records, options);
    let warns: Vec<String> = records
        .iter()
        .filter_map(|r| match r {
            cnc::Record::Warn { text } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(warnings, warns, "the PathPilot lathe post refused a turning stream by name");
    linuxcnc_sane(&program);
    program
}

/// The document's program through the in-app gate (the post chosen by
/// name, the gate run, the file written).
pub fn gated(world: &World, tag: &str) -> String {
    let path = std::env::temp_dir().join(format!("camcraft-tormach-lathe-{tag}-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    posting::post_setup(world, &path).expect("a clean lathe program posts through the gate");
    let text = std::fs::read_to_string(&path).expect("written");
    let _ = std::fs::remove_file(&path);
    text
}

const GCODES: &[&str] = &[
    "G0", "G1", "G2", "G3", "G4", "G7", "G18", "G20", "G21", "G30", "G33", "G40", "G54", "G55", "G56", "G57",
    "G58", "G59", "G64", "G76", "G90", "G91.1", "G95", "G96", "G97",
];
const MCODES: &[&str] = &["M0", "M1", "M3", "M4", "M5", "M7", "M8", "M9", "M30"];

/// A LinuxCNC / PathPilot lathe syntax check over a posted program:
/// G7 stated before any motion, every comment one un-nested
/// parenthesis pair, every word a letter the lathe tables list with a
/// number, every G and M from the documented sets above (G32, G80-G83,
/// M6, G43 and friends are not among them), no Y, no O, no %.
pub fn linuxcnc_sane(text: &str) {
    let first_motion = text.lines().position(|l| l.starts_with("G0 ") || l.starts_with("G1 "));
    let g7 = text.lines().position(|l| l.split(' ').any(|w| w == "G7"));
    assert!(g7.is_some() && g7 < first_motion.or(Some(usize::MAX)), "G7 before any motion: {text}");
    assert!(!text.contains('%'), "no % fence: {text}");
    for line in text.lines() {
        if line.starts_with('(') {
            assert!(line.ends_with(')'), "a comment is one line: {line}");
            let inner = &line[1..line.len() - 1];
            assert!(!inner.contains('(') && !inner.contains(')'), "no nested comment: {line}");
            continue;
        }
        assert!(!line.contains('(') && !line.contains(';'), "no trailing comment on a motion line: {line}");
        for w in line.split(' ') {
            let mut ch = w.chars();
            let letter = ch.next().unwrap_or(' ');
            let num = ch.as_str();
            assert!("GMTSFXZIJKPRQHD".contains(letter), "a word LinuxCNC's lathe lists: {w} in {line}");
            assert!(num.parse::<f64>().is_ok(), "a word with a number: {w} in {line}");
            if letter == 'G' {
                assert!(GCODES.contains(&w), "G code outside the PathPilot lathe set: {w} in {line}");
            }
            if letter == 'M' {
                assert!(MCODES.contains(&w), "M code outside the PathPilot lathe set: {w} in {line}");
            }
            if letter == 'T' {
                assert!(num.len() == 4, "T word is tool + offset, T0101: {w}");
            }
        }
    }
}

/// The G33 blocks of a program: (Z, K) as written.
pub fn g33s(text: &str) -> Vec<(String, String)> {
    let word = |l: &str, c: char| l.split(' ').find_map(|w| w.strip_prefix(c)).unwrap_or("").to_string();
    text.lines().filter(|l| l.starts_with("G33 ")).map(|l| (word(l, 'Z'), word(l, 'K'))).collect()
}

/// The program's one G76 block, parsed word by word (letter -> number).
pub fn g76_words(text: &str) -> Vec<(char, f64)> {
    let line = text.lines().find(|l| l.starts_with("G76 ")).unwrap_or("");
    line.split(' ')
        .skip(1)
        .filter_map(|w| {
            let mut c = w.chars();
            let l = c.next()?;
            c.as_str().parse().ok().map(|v| (l, v))
        })
        .collect()
}
