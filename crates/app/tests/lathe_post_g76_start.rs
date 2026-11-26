//! Fanuc lathe G76 free-air start law (issue #2878): a Fanuc-type G76
//! cycle moves the insert from the start point to each pass's depth at
//! rapid traverse, so the start point must stand in free air - in front
//! of the end face, or in a relief groove wider than the insert, clear
//! of both part and remaining stock. When the start point stands above
//! material, the post refuses the G76 cycle by name and writes no G76;
//! G32 passes (which plunge at G01 feed) still post cleanly.
//!
//! Provenance: our own fixtures; Fanuc G76 rapid-infeed behaviour from
//! public Fanuc lathe programming manuals (behaviour only).
//!
//! Gate map (M4.4): covers camcraft_motion::turning (thread, thread_cycle,
//! revolve), camcraft_cl::cl::ThreadCycle, camcraft_post::post (cnc_thread,
//! assets/posts/fanuc-lathe.cps), app::operation (generate, g76_start),
//! app::posting (post_setup).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::cl::ThreadHand;
use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost, SetPostOptions,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{Coolant, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::{Location, PostOptions, PostRef, ThreadingOutput};
use camcraft_app::stock::StockShape;
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

/// Ø12 bar with 60-degree V grooves at Z=8, 10, 12, 14, continuing solid
/// to `end_z` (when `end_z == 40.0`, the pre-#2878 solid-bar fixture;
/// when `end_z == 14.0 + half`, the free end face stands right after the
/// last groove).
fn bar_to(end_z: f64) -> Mesh {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let root = MAJOR - DEPTH;
    let mut m = vec![(0.0, -26.0), (MAJOR, -26.0), (MAJOR, 8.0 - half)];
    let mut c = 8.0;
    while c <= 14.0 + 1e-9 {
        m.push((root, c));
        m.push((MAJOR, c + half));
        if c + PITCH <= 14.0 + 1e-9 {
            m.push((MAJOR, c + PITCH - half));
        }
        c += PITCH;
    }
    if end_z > 14.0 + half + 1e-6 {
        m.push((MAJOR, end_z));
    }
    m.push((0.0, end_z.max(14.0 + half)));
    revolve(&m)
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

fn g76() -> PostOptions {
    PostOptions { threading: ThreadingOutput::G76Cycle, ..PostOptions::default() }
}

fn run_cps(world: &World, records: &[cnc::Record], options: PostOptions) -> (String, Vec<String>) {
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

fn build_world(bar: Mesh, stock_dia: f64, stock_z0: f64, stock_z1: f64, top_z: f64) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, stock_z0), ..Location::default() };
    let stock = StockShape::Cylinder { diameter: stock_dia, height: stock_z1 - stock_z0 };
    history.execute(&world, SetStock::new(&world, stock, at).expect("bar stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe())).expect("lathe"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "TD-1732-60").map(|(_, t)| t).expect("insert");
    history.execute(&world, AddTool::new(&world, "TD-1732-60", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    let params = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(top_z),
        bottom_z: Height::fixed(7.7),
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: ThreadHand::Right,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "TH1", "TD-1732-60", SrcRef::Entity("bar".into()), params).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("generate"));
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    (world, history)
}

#[test]
fn g76_refuses_when_start_point_is_inside_a_solid_bar() {
    // Bar continues at Ø12 to Z=40; the thread starts at Z=14 inside
    // solid material. G76 would rapid into the bar on every pass.
    let (world, mut history) = build_world(bar_to(40.0), 12.0, -26.0, 40.0, 14.3);
    // G32 plunges at G01 feed and still posts cleanly.
    let path_g32 = std::env::temp_dir().join(format!("camcraft-2878-g32-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path_g32);
    posting::post_setup(&world, &path_g32).expect("G32 passes post cleanly");
    let _ = std::fs::remove_file(&path_g32);

    // G76 refuses by name and writes no cycle.
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));
    let records = posting::stream_records(&world).expect("generated");
    let (text, refused) = run_cps(&world, &records, g76());
    assert!(
        refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains("above material")),
        "expected G76 above-material refusal, got: {refused:?}\n{text}"
    );
    assert!(!text.contains("G76 X"), "a refused G76 cycle was written:\n{text}");
    let path_g76 = std::env::temp_dir().join(format!("camcraft-2878-g76-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path_g76);
    let err = posting::post_setup(&world, &path_g76).expect_err("gated post must block unsafe G76");
    assert!(err.contains("G76") && err.contains("above material"), "unexpected gate error: {err}");
    assert!(!path_g76.exists(), "a refused post must write no file");
}

#[test]
fn g76_refuses_on_cone_thread_start_inside_end_face() {
    // Issue #2878 example (from #2394 turning/cone-thread): end face at
    // Z=78, Ø10 thread from Z=63 to Z=78 (pitch 1.5, depth 0.92, 1.2 mm
    // wide 60-degree insert), last groove at Z=77.4 so the insert band
    // [76.8, 78.0] stands above solid material.
    let pitch = 1.5;
    let depth = 0.92;
    let major = 5.0;
    let root = major - depth;
    let half_v = depth * (30.0_f64).to_radians().tan();
    let mut m = vec![(0.0, 0.0), (15.0, 0.0), (15.0, 23.0), (10.0, 23.0), (5.0, 63.0), (major, 63.9 - half_v)];
    let mut c = 63.9;
    while c <= 77.4 + 1e-9 {
        m.push((root, c));
        m.push((major, c + half_v));
        if c + pitch <= 77.4 + 1e-9 {
            m.push((major, c + pitch - half_v));
        }
        c += pitch;
    }
    m.push((major, 78.0));
    m.push((0.0, 78.0));

    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, 0.0), ..Location::default() };
    let stock = StockShape::Cylinder { diameter: 30.0, height: 78.0 };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe())).expect("lathe"));
    let mut insert = catalog().into_iter().find(|(n, _)| *n == "TD-1732-60").map(|(_, t)| t).expect("insert");
    insert.width = 1.2;
    insert.tip_angle = 60.0;
    insert.rpm = 800.0;
    history.execute(&world, AddTool::new(&world, "TD-1200-60", insert).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("cone", revolve(&m)));
    let params = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(82.0),
        top_z: Height::fixed(78.0),
        bottom_z: Height::fixed(63.4),
        pitch,
        depth,
        infeed: 0.25,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: ThreadHand::Right,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "TH1", "TD-1200-60", SrcRef::Entity("cone".into()), params).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("generate"));
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));

    let records = posting::stream_records(&world).expect("generated");
    let (text, refused) = run_cps(&world, &records, g76());
    assert!(
        refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains("above material")),
        "expected G76 above-material refusal on cone-thread, got: {refused:?}\n{text}"
    );
    assert!(!text.contains("G76 X"), "a refused G76 cycle was written:\n{text}");
}

#[test]
fn g76_posts_when_start_point_is_in_free_air_in_front_of_end_face() {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let end_z = 14.0 + half;
    // Start lattice at Z=16.0: insert band [15.134, 16.866] stands
    // completely past the end face (14.693) in free air.
    let (world, mut history) = build_world(bar_to(end_z), 12.0, -26.0, end_z, 16.3);
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));
    let path = std::env::temp_dir().join(format!("camcraft-2878-air-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let posted = posting::post_setup(&world, &path).expect("free-air G76 posts cleanly");
    let _ = std::fs::remove_file(&path);
    assert!(posted.program.contains("G00 X12.1 Z16.\nG76 P010000 Q400 R0.4\n"), "{}", posted.program);
    assert!(posted.program.contains("G76 X9.6 Z8. P1200 Q400 F2.\n"), "{}", posted.program);
}

#[test]
fn g76_refuses_uncut_stock_in_front_of_end_face_and_posts_after_facing() {
    let end_z = 15.0;
    // Part ends at 15.0 (past last groove flank 14.693, clear of insert
    // band [15.134, 16.866] at Z=16.0), but raw stock extends to Z=17.0:
    // at Z=16.0 the insert stands above uncut stock, so G76 must refuse.
    let (world, mut history) = build_world(bar_to(end_z), 12.0, -26.0, 17.0, 16.3);
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));
    let records = posting::stream_records(&world).expect("generated");
    let (text, refused) = run_cps(&world, &records, g76());
    assert!(
        refused.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains("above material")),
        "uncut stock at start Z must refuse G76: {refused:?}\n{text}"
    );
    assert!(!text.contains("G76 X"), "{text}");

    // Add a Facing op ahead of TH1 that faces the stock down to the
    // part's end face: now Z=16.0 is in free air and G76 posts cleanly.
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -26.0), ..Location::default() };
    let stock = StockShape::Cylinder { diameter: 12.0, height: 17.0 - (-26.0) };
    history.execute(&world, SetStock::new(&world, stock, at).expect("stock"));
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe())).expect("lathe"));
    for name in ["TN-1600-0R8", "TD-1732-60"] {
        let tool = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("tool");
        history.execute(&world, AddTool::new(&world, name, tool).expect("add tool"));
    }
    history.execute(&world, ImportModel::from_mesh("bar", bar_to(end_z)));
    let face = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(17.0),
        bottom_z: Height::fixed(end_z),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "F1", "TN-1600-0R8", SrcRef::Entity("bar".into()), face).expect("face"));
    history.execute(&world, GenerateOperation::new(&world, "F1").expect("gen face"));
    let th = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(16.3),
        bottom_z: Height::fixed(7.7),
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: ThreadHand::Right,
        picks: Default::default(),
    };
    history.execute(&world, CreateOperation::new(&world, "TH1", "TD-1732-60", SrcRef::Entity("bar".into()), th).expect("thread"));
    history.execute(&world, GenerateOperation::new(&world, "TH1").expect("gen thread"));
    history.execute(&world, SelectPost::new(&world, PostRef::Named("Fanuc Lathe".into())).expect("post"));
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));
    let records = posting::stream_records(&world).expect("generated");
    let (text, refused) = run_cps(&world, &records, g76());
    assert!(refused.is_empty(), "faced free-air G76 must not refuse: {refused:?}\n{text}");
    assert!(text.contains("G00 X12.1 Z16.\nG76 P010000 Q400 R0.4\n"), "{text}");
    let th_e = camcraft_app::model::find_by_name(&world, "TH1").expect("TH1");
    let th_tp = camcraft_app::model::toolpath_of(&world, &th_e).expect("TH1 cache");
    assert!(th_tp.warnings.is_empty(), "TH1 warnings: {:?}", th_tp.warnings);
    assert!(th_tp.path.threads[0].start_in_air, "TH1 start_in_air after facing");
}

#[test]
fn g76_refuses_legacy_stream_without_air_flag() {
    let half = DEPTH * (30.0_f64).to_radians().tan();
    let end_z = 14.0 + half;
    let (world, mut history) = build_world(bar_to(end_z), 12.0, -26.0, end_z, 16.3);
    history.execute(&world, SetPostOptions::new(&world, g76()).expect("G76"));
    let records = posting::stream_records(&world).expect("generated");
    let stream = cnc::serialize(&records);
    assert!(stream.contains(" AIR 1\n"), "free-air stream carries AIR 1:\n{stream}");
    let legacy = stream.replace(" AIR 1\n", "\n");
    let out = engine::run_with(LATHE_CPS, &posting::machine_value(&world), &legacy, &posting::options_value(g76()))
        .expect("runs");
    assert!(
        out.warnings.iter().any(|w| w.contains("G76 threading cycle refused") && w.contains("above material")),
        "legacy stream without AIR 1 must refuse G76: {:?}",
        out.warnings
    );
    assert!(!out.program.contains("G76 X"), "{}", out.program);
}
