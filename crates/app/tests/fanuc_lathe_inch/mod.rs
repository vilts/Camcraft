//! Shared fixtures of the Fanuc lathe inch suite (`lathe_post_inch`,
//! #977): the turning documents of `lathe_post` and `lathe_post_g76`,
//! rebuilt here so their streams can be posted in mm (against the
//! committed mm goldens) and in inch. Not a suite itself.
//!
//! Provenance: our own fixtures (the lathe suites' proven ones).

#![allow(dead_code)]

use flecs_ecs::prelude::World;

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::drilling::{CycleChoice, Hole};
use camcraft_app::geo2d::Point2;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, PostOptions, ThreadingOutput};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

pub const CPS: &str = include_str!("../../../../assets/posts/fanuc-lathe.cps");

/// (inch golden name, the committed mm golden it must match in mm).
pub const FIXTURES: &[(&str, Option<&str>)] = &[
    ("facing", Some("nc/fanuc-lathe/facing.nc")),
    ("profile_rough_finish", Some("nc/fanuc-lathe/profile_rough_finish.nc")),
    ("groove_part", Some("nc/fanuc-lathe/groove_part.nc")),
    ("axial_drill", Some("nc/fanuc-lathe/axial_drill.nc")),
    ("thread_g76", Some("nc/fanuc-lathe/thread_g76.nc")),
    ("thread_g32", None),
];

/// The words of a line, comments dropped.
pub fn words(line: &str) -> Vec<String> {
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in line.chars() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().map(str::to_owned).collect()
}

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
    let v = |j: usize, r: f64, z: f64| {
        let th = std::f64::consts::TAU * j as f64 / segs as f64;
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

fn lathe() -> Machine {
    Machine {
        model: "TestBench lathe".into(),
        vendor: "Camcraft Testbench".into(),
        kind: Some("lathe".into()),
        spindle_min: Some(25.0),
        spindle_max: Some(12000.0),
        coolants: vec![Coolant { name: "Flood".into(), m_code: "M8".into() }],
        ..Default::default()
    }
}

pub fn lathe_mch() -> serde_json::Value {
    camcraft_app::post::machine::mirror(&lathe())
}

fn world(stock: StockShape, z0: f64, bar: Mesh, tools: &[&str]) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, z0), ..Location::default() };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe())).expect("lathe"));
    for t in tools {
        let tool = catalog().into_iter().find(|(n, _)| n == t).map(|(_, t)| t).expect("catalog entry");
        history.execute(&world, AddTool::new(&world, t, tool).expect("tool"));
    }
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    (world, history)
}

/// A bar in stock of its own size, as `lathe_post`'s `bar_world`.
fn bar_world(meridian: &[(f64, f64)], tools: &[&str]) -> (World, History) {
    let r = meridian.iter().map(|p| p.0).fold(0.0f64, f64::max);
    let z0 = meridian.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
    let z1 = meridian.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
    world(StockShape::Cylinder { diameter: 2.0 * r, height: z1 - z0 }, z0, revolve(meridian, 16), tools)
}

fn create(world: &World, history: &mut History, name: &str, tool: &str, params: OpParams) {
    let cmd = CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), params).expect("create");
    history.execute(world, cmd);
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
}

fn h(v: f64) -> Height {
    Height::fixed(v)
}

/// The named fixture's world and the post options its mm golden used.
pub fn fixture(name: &str) -> (World, PostOptions) {
    let plain = PostOptions::default();
    match name {
        "facing" => {
            let bar = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (5.0, 2.0), (5.0, 16.0), (0.0, 16.0)];
            let (w, mut hi) = bar_world(&bar, &["TN-1600-0R8"]);
            let p = OpParams::Facing {
                tolerance: 0.05, clearance_z: h(25.0), top_z: h(16.0), bottom_z: h(14.0),
                stepdown: 0.5, feed: 150.0, plunge_feed: 80.0,
            };
            create(&w, &mut hi, "F1", "TN-1600-0R8", p);
            (w, plain)
        }
        "profile_rough_finish" => {
            let bar = [(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)];
            let (w, mut hi) = bar_world(&bar, &["TN-1600-0R8"]);
            let rough = OpParams::ProfileRough {
                tolerance: 0.05, clearance_z: h(25.0), top_z: h(16.0), bottom_z: h(-16.0),
                stepdown: 0.8, stock_to_leave: 0.25, feed: 150.0, plunge_feed: 80.0,
            };
            let finish = OpParams::ProfileFinish {
                tolerance: 0.05, clearance_z: h(25.0), top_z: h(16.0), bottom_z: h(2.0),
                feed: 120.0, plunge_feed: 60.0,
            };
            create(&w, &mut hi, "R1", "TN-1600-0R8", rough);
            create(&w, &mut hi, "S1", "TN-1600-0R8", finish);
            (w, plain)
        }
        "groove_part" => {
            let bar = [(0.0, -16.0), (6.0, -16.0), (6.0, -6.8), (4.0, -6.8), (4.0, -3.6), (6.0, -3.6), (6.0, 28.0), (0.0, 28.0)];
            let (w, mut hi) = bar_world(&bar, &["GT-0200", "GT-0300"]);
            let groove = OpParams::GrooveFinish {
                tolerance: 0.05, clearance_z: h(20.0), top_z: h(-3.6), bottom_z: h(-6.8),
                feed: 150.0, plunge_feed: 60.0,
            };
            let part = OpParams::Part {
                tolerance: 0.05, clearance_z: h(20.0), top_z: h(28.0), bottom_z: h(-10.0), stepdown: 0.5,
                feed: 150.0, plunge_feed: 60.0, bar_load: true, bar_advance: 30.0, bar_unload: true,
                bar_transfer: false, inspect_tol: 0.0,
            };
            create(&w, &mut hi, "G1", "GT-0200", groove);
            create(&w, &mut hi, "P1", "GT-0300", part);
            (w, plain)
        }
        "axial_drill" => {
            let bar = revolve(&[(0.0, -20.0), (10.0, -20.0), (10.0, 20.0), (2.05, 20.0), (2.05, 10.0), (0.0, 10.0)], 64);
            let (w, mut hi) = world(StockShape::Box { dx: 24.0, dy: 24.0, dz: 40.0 }, -20.0, bar, &["DR-0400"]);
            let hole = Hole { xy: Point2::new(0.0, 0.0), radius: 2.05, entry_z: 20.0, floor_z: 10.0, through: false, stepped: false };
            let p = OpParams::Drill {
                clearance_z: h(30.0), retract_z: None, top_z: h(20.0), bottom_z: None, cycle: CycleChoice::Auto,
                peck: 0.0, dwell: 0.0, feed: 120.0, plunge_feed: 80.0, holes: vec![hole], cross: vec![],
            };
            create(&w, &mut hi, "AX1", "DR-0400", p);
            (w, plain)
        }
        _ => thread(name == "thread_g76"),
    }
}

/// `lathe_post_g76`'s threaded bar: wall r6, a 60-degree V every 2.0
/// from z 8 to z 14, 1.2 deep; G76 or the G32 default.
fn thread(g76: bool) -> (World, PostOptions) {
    let (major, pitch, depth) = (6.0, 2.0, 1.2);
    let half = depth * (30.0_f64).to_radians().tan();
    let mut m = vec![(0.0, -16.0), (major, -16.0), (major, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((major - depth, c));
        m.push((major, c + half));
        if c + pitch <= 14.0 + 1e-9 {
            m.push((major, c + pitch - half));
        }
        c += pitch;
    }
    m.push((major, 14.0 + half));
    m.push((major, 40.0));
    m.push((0.0, 40.0));
    let stock = StockShape::Cylinder { diameter: 12.0, height: 56.0 };
    let (w, mut hi) = world(stock, -16.0, revolve(&m, 16), &["TD-1732-60"]);
    let p = OpParams::Threading {
        tolerance: 0.05, clearance_z: h(24.0), top_z: h(14.3), bottom_z: h(7.7), pitch, depth,
        infeed: 0.4, feed: 120.0, plunge_feed: 40.0, hand: ThreadHand::Right, infeed_plan: Default::default(),
    };
    create(&w, &mut hi, "TH1", "TD-1732-60", p);
    let threading = if g76 { ThreadingOutput::G76Cycle } else { ThreadingOutput::G32Passes };
    (w, PostOptions { threading, ..PostOptions::default() })
}
