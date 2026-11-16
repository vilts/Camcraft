//! Continuous C-axis live tooling, slice 3 (issue #33): machine C
//! rotary sense (`"axis": [0, 0, -1]`) across rotary gate, machine
//! check, setup sheet and Fanuc lathe post, plus the in-app world
//! posting gate test on a mill-turn document.
//!
//! Provenance: our own integration tests; RS-274 / ISO 6983 C-about-Z
//! convention. No vendor material consulted.
//!
//! Gate map (M4.4): covers camcraft_app::machine,
//! camcraft_app::operation, camcraft_app::posting,
//! camcraft_app::setup_sheet, camcraft_post::machine,
//! assets/posts/fanuc-lathe.cps.
//! Lane shape (M4.4): all tests here run in seconds.

use std::path::PathBuf;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SelectPost,
    SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle, IDENTITY_Q};
use camcraft_app::machine::{self, AxisLimits, Coolant, LiveToolCodes, Machine};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::{Location, PostRef};
use camcraft_app::setup_sheet;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::{posting, world};

const TAU: f64 = std::f64::consts::TAU;
const C_RATE: f64 = 6000.0;

fn codes() -> LiveToolCodes {
    LiveToolCodes {
        c_axis_on: "M1001".into(),
        c_axis_off: "M1002".into(),
        spindle_on: "M1003".into(),
        spindle_off: "M1004".into(),
    }
}

fn mill_turn(axis: [f64; 3], travel: Option<(f64, f64)>) -> Machine {
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
                axis: Some(axis),
                travel,
                feed: Some(C_RATE),
                rapid: Some(2.0 * C_RATE),
                ..Default::default()
            },
        ],
        live_tool: Some(codes()),
        ..Default::default()
    }
}

fn bar() -> Vec<Triangle> {
    const MERIDIAN: [(f64, f64); 6] = [
        (0.0, -16.0),
        (6.0, -16.0),
        (6.0, -2.0),
        (5.0, 2.0),
        (5.0, 16.0),
        (0.0, 16.0),
    ];
    let pt = |rho: f64, s: f64, j: usize| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(rho * th.cos(), rho * th.sin(), s)
    };
    let mut f = Vec::new();
    for w in MERIDIAN.windows(2) {
        let ((ra, za), (rb, zb)) = (w[0], w[1]);
        for j in 0..16 {
            let k = (j + 1) % 16;
            let (a1, a2, b1, b2) = (pt(ra, za, j), pt(ra, za, k), pt(rb, zb, j), pt(rb, zb, k));
            if ra <= 1e-12 {
                f.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                f.push(Triangle::new(a1, a2, b2));
            } else {
                f.push(Triangle::new(a1, a2, b2));
                f.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    f
}

fn contour_params() -> OpParams {
    OpParams::RotaryContour {
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
    }
}

fn world_doc(machine: Machine) -> (World, History) {
    let w = world::new_world();
    let mut h = History::default();
    let tool = catalog()
        .into_iter()
        .find(|(n, _)| *n == "BN-0600")
        .map(|(_, t)| t)
        .expect("catalog ball");
    h.execute(&w, AddTool::new(&w, "BN-0600", tool).expect("tool"));
    let stock = SetStock::new(
        &w,
        StockShape::Cylinder { diameter: 12.4, height: 32.0 },
        Location { pos: Point3::new(0.0, 0.0, -16.0), rot: IDENTITY_Q },
    );
    h.execute(&w, stock.expect("stock"));
    h.execute(&w, ImportModel::from_mesh("bar", Mesh::from_facets(bar())));
    h.execute(&w, LoadMachine::new(&w, "millturn.mch".into(), Some(machine)).expect("machine"));
    h.execute(&w, SelectPost::new(&w, PostRef::Named("Fanuc Lathe".into())).expect("post"));
    h.execute(
        &w,
        CreateOperation::new(&w, "CWrap", "BN-0600", SrcRef::Entity("bar".into()), contour_params())
            .expect("create op"),
    );
    h.execute(&w, GenerateOperation::new(&w, "CWrap").expect("generate op"));
    (w, h)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("camcraft-33-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

#[test]
fn a_minus_z_c_axis_flips_travel_gate_post_and_setup_sheet() {
    // Machine counts C clockwise about +Z (axis [0, 0, -1]) over 0..720 deg:
    // canonical C (about +Z) therefore unwinds into -720..0 deg while the
    // machine check and post map back to controller C in 0..720 deg.
    let (w, _h) = world_doc(mill_turn([0.0, 0.0, -1.0], Some((0.0, 720.0))));
    let op = camcraft_app::model::find_by_name(&w, "CWrap").expect("op");
    let cached = camcraft_app::model::toolpath_of(&w, &op).expect("toolpath");
    assert!(
        cached.path.moves.iter().filter_map(|m| m.rotary).all(|c| (-720.0 - 1e-9..=1e-9).contains(&c)),
        "flipped program travel keeps canonical C in -720..0"
    );
    let check = machine::check_document(&w, &machine::of(&w).1.expect("machine")).expect("check");
    assert!(check.is_ok(), "machine check problems: {:?}", check.problems);
    let gate = posting::verification_gate(&w);
    assert!(gate.is_empty(), "verification gate problems: {gate:?}");

    let dir = scratch("minus-z");
    let out = dir.join("prog.nc");
    let posted = posting::post_setup(&w, &out).expect("in-app post_setup passes");
    assert!(posted.program.contains("G98") && posted.program.contains("M1001"), "{}", posted.program);
    let has_c = posted.program.lines().any(|l| {
        l.split(' ')
            .find_map(|tok| tok.strip_prefix('C').and_then(|v| v.parse::<f64>().ok()))
            .is_some_and(|c| c > 0.0)
    });
    assert!(has_c, "clock-wise -Z C writes positive controller C angles:\n{}", posted.program);
    assert!(
        !posted.program.lines().any(|l| l.contains(" C-")),
        "travel 0..720 never posts negative C:\n{}",
        posted.program
    );

    let sheet = setup_sheet::render(&w, None).expect("setup sheet");
    assert!(sheet.contains("MACHINE AXES"), "reversed C appears under MACHINE AXES:\n{sheet}");
    assert!(sheet.contains("C rotary") && sheet.contains("-Z"), "names C sense:\n{sheet}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn an_in_app_world_c_axis_program_passes_gate_posts_and_renders_setup_sheet() {
    let (w, _h) = world_doc(mill_turn([0.0, 0.0, 1.0], Some((-360.0, 360.0))));
    let report = posting::verification_report(&w);
    assert!(report.problems.is_empty(), "gate problems: {:?}", report.problems);
    let dir = scratch("world-gate");
    let out = dir.join("world.nc");
    let posted = posting::post_setup(&w, &out).expect("post_setup");
    assert!(posted.program.lines().any(|l| l.starts_with("G01 X") && l.contains(" C")));
    let sheet = setup_sheet::render(&w, None).expect("setup sheet");
    assert!(sheet.contains("CWrap"), "{sheet}");
    assert!(!sheet.contains("MACHINE AXES"), "standard +Z C stays silent:\n{sheet}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tilted_c_axis_is_refused_by_name_in_machine_check() {
    // Construct a C-stamped path in a world with a machine claiming C about X.
    let mut bad = mill_turn([0.0, 0.0, 1.0], Some((-360.0, 360.0)));
    let (w, _h) = world_doc(bad.clone());
    for a in &mut bad.axes {
        if a.rotary && a.name == "C" {
            a.axis = Some([1.0, 0.0, 0.0]);
        }
    }
    let check = machine::check_document(&w, &bad).expect("check");
    assert!(!check.is_ok(), "tilted C rotary must fail machine check");
    assert!(
        check.problems.iter().any(|p| p.contains("C rotary") && p.contains("Z")),
        "named refusal expected: {:?}",
        check.problems
    );
}
