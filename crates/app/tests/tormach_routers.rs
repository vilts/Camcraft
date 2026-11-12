//! #620: the Tormach PathPilot routers' machine files - each states its
//! own manual's figures (travel, spindle window, power, feeds, ATC) and
//! nothing the documents leave unstated - and the machine check holding
//! the router spindle's floor: a mill-speed job on the 24R is refused.
//! Provenance: our own tests over our own machine files; the figures are
//! Tormach's published documents - 24R: operator's manual UM10564 0826A,
//! § 2.2 "Machine Specifications"; xsTECH Pro: spec sheet
//! (xstech_pro_spec_sheet_final, 2026) and the CE operator's manual
//! UM10859, § 2 (the smaller of the two documents' Y and Z travels);
//! xsTECH (no manual exists): Tormach's spec sheet xstech_spec_sheet_0225,
//! its product and package pages, and knowledge-base articles (the
//! narrowest spindle window and smallest power where they disagree).
//! Gate map (M4.4): covers app::posting (MACHINES registry),
//! app::machine, post::machine (check_document spindle window), post::machine.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, SetStock};
use camcraft_app::geometry::{IDENTITY_Q, Point3, cube};
use camcraft_app::machine::{Machine, check_document, parse_mch};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{engine, machine};
use camcraft_app::posting::MACHINES;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use std::path::Path;

const IN: f64 = 25.4;

fn shipped(name: &str) -> Machine {
    let text = MACHINES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, t)| *t)
        .unwrap_or_else(|| panic!("{name} is not a shipped machine"));
    parse_mch(text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// One router's documented figures. `ipm: None` is a feed the documents
/// do not state - the file must state none either.
struct Spec {
    name: &'static str,
    travel_in: [f64; 3],
    rpm: (Option<f64>, f64),
    kw: f64,
    ipm: Option<f64>,
    tools: Option<usize>,
}

const SPECS: [Spec; 3] = [
    // UM10564 § 2.2: 24.75 x 55.75 x 6.7 in, 10000-24000 rpm 2 hp
    // (1.5 kW) electrospindle, 200 IPM on every axis, ATC optional with
    // no pocket count.
    Spec {
        name: "Tormach 24R",
        travel_in: [24.75, 55.75, 6.7],
        rpm: (Some(10000.0), 24000.0),
        kw: 1.5,
        ipm: Some(200.0),
        tools: None,
    },
    // Spec sheet: X 10.2 in with the tool changer fitted, 350 W to
    // 20000 rpm with no minimum stated, 4-station ATC, no feed rate.
    // Y 7.9 in / Z 6.3 in: the smaller figure where the CE manual and the
    // US sheet disagree (9.3 / 6.5 in the other document).
    Spec {
        name: "Tormach xsTECH Pro",
        travel_in: [10.2, 7.9, 6.3],
        rpm: (None, 20000.0),
        kw: 0.35,
        ipm: None,
        tools: Some(4),
    },
    // Spec sheet 02/25 + product page: 8.6 x 6.3 x 2.4 in. Spindle: the
    // narrowest window and smallest power the Tormach sources state -
    // 5000 rpm floor (package page; the KB says 3000), 15000 rpm ceiling
    // (KB; the spec sheet says 20000), 80 W (KB; 120 / 125 W elsewhere).
    // 120 IPM "maximum speed" (package page). Manual ER11 collet, no ATC.
    Spec {
        name: "Tormach xsTECH",
        travel_in: [8.6, 6.3, 2.4],
        rpm: (Some(5000.0), 15000.0),
        kw: 0.08,
        ipm: Some(120.0),
        tools: None,
    },
];

#[test]
fn every_tormach_router_states_its_documents_figures() {
    for s in &SPECS {
        let m = shipped(s.name);
        let n = s.name;
        assert_eq!(m.model, n);
        assert_eq!(m.units.as_deref(), Some("inch"), "{n} is programmed in inches");
        assert_eq!(m.axes.len(), 3, "{n}: X, Y, Z");
        for (a, want) in m.axes.iter().zip(s.travel_in) {
            let (lo, hi) = a.travel.unwrap_or_else(|| panic!("{n} {} travel", a.name));
            assert!(close(hi, want * IN) && close(lo, -want * IN), "{n} {}: {lo}..{hi}", a.name);
        }
        assert_eq!((m.spindle_min, m.spindle_max), (s.rpm.0, Some(s.rpm.1)), "{n} rpm");
        assert_eq!(m.spindle_power, Some(s.kw), "{n} kW");
        for a in &m.axes {
            match s.ipm {
                Some(ipm) => assert!(a.feed.is_some_and(|f| close(f, ipm * IN)), "{n} {} feed {:?}", a.name, a.feed),
                None => assert_eq!(a.feed, None, "{n} {}: no feed is documented", a.name),
            }
        }
        assert!(m.spindle_ranges.is_empty(), "{n}: a direct-drive spindle, no belt");
        assert_eq!(m.rigid_tapping, Some(false), "{n}: no rigid tapping documented");
        assert!(m.coolants.is_empty(), "{n}: no coolant documented");
        assert_eq!(m.tool_count, s.tools, "{n} ATC stations");
        assert_eq!(m.safe_retract_z, Some(0.0), "{n}: PathPilot homes Z0 at the top");
    }
}

/// An unstated feed stays silent in the script: no fake ceiling.
#[test]
fn the_xstech_pro_states_no_feed_ceiling() {
    let p = machine::script_props(&machine::mirror(&shipped("Tormach xsTECH Pro")));
    assert_eq!(p["feedRateMax"], 0.0, "{p}");
    assert_eq!(p["spindleMax"], 20000.0, "{p}");
}

/// Every router posts through the PathPilot post, clean: the library
/// offers it for them (they are mills by kind, as every Tormach file is).
#[test]
fn the_routers_post_through_pathpilot() {
    let list = camcraft_app::machine::library::builtins();
    for name in SPECS.iter().map(|s| s.name) {
        let e = list.iter().find(|e| e.name == name).unwrap_or_else(|| panic!("{name} is in the library"));
        assert!(e.is_valid(), "{name} refuses: {:?}", e.refusals);
        assert!(e.posts.contains(&"Tormach PathPilot"), "{name}: {:?}", e.posts);
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/posts/tormach.cps");
    let cps = std::fs::read_to_string(&p).expect("tormach.cps");
    let stream = "PARAM SpindleSpeed 12000\nTOOL 1 T1\nSECTION\nRapid X 0 Y 0 Z 5000\nFeed X 1000 Y 0 Z -1000 F 500000\nENDSECTION\n";
    for name in SPECS.iter().map(|s| s.name) {
        let out = engine::run(&cps, &machine::mirror(&shipped(name)), stream).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(out.warnings.is_empty(), "{name}: {:?}", out.warnings);
        assert!(out.program.contains("S12000"), "{name}:\n{}", out.program);
        assert!(out.program.contains("G20"), "{name}: inch\n{}", out.program);
    }
}

/// A face job whose tool runs `rpm`, inside every router's envelope.
fn face_job(rpm: f64) -> (flecs_ecs::prelude::World, History) {
    let world = new_world();
    let mut h = History::default();
    let loc = Location { pos: Point3::new(0.0, 0.0, 0.0), rot: IDENTITY_Q };
    let stock = StockShape::Box { dx: 60.0, dy: 60.0, dz: 20.0 };
    h.execute(&world, SetStock::new(&world, stock, loc).expect("box stock"));
    let mut tool = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("catalog tool");
    tool.rpm = rpm;
    h.execute(&world, AddTool::new(&world, "EM-0400-2FL", tool).expect("add tool"));
    h.execute(
        &world,
        ImportModel::from_mesh("part", cube(Point3::new(-30.0, -30.0, 0.0), Point3::new(30.0, 30.0, 18.0))),
    );
    let face = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        blend_turns: false,
        stepover: 3.0,
        clearance_z: Height::fixed(25.0),
        cut_z: Height::fixed(18.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
    };
    let op = CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face).expect("create face");
    h.execute(&world, op);
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate face"));
    (world, h)
}

/// The 24R's electrospindle starts at 10000 rpm: a mill-speed job
/// (5000 rpm) is refused by the machine check naming the op, the tool
/// and the floor; the same job at 18000 rpm passes.
#[test]
fn a_mill_speed_job_is_refused_on_the_24r() {
    let m = shipped("Tormach 24R");
    let (world, _h) = face_job(5000.0);
    let c = check_document(&world, &m).expect("checks");
    assert!(
        c.problems.iter().any(|p| p.starts_with("Face1:")
            && p.contains("EM-0400-2FL")
            && p.contains("5000 rpm, below the machine's 10000")),
        "{:?}",
        c.problems
    );
    let (world, _h) = face_job(18000.0);
    let c = check_document(&world, &m).expect("checks");
    assert!(c.is_ok(), "{:?}", c.problems);
}

/// The xsTECH's window is the narrowest its sources state: an 18000 rpm
/// job (inside the spec sheet's 20000) is refused against the knowledge
/// base's 15000, a 4000 rpm job (inside the KB's 3000 floor) against the
/// package page's 5000, and 12000 rpm passes.
#[test]
fn the_xstech_spindle_window_is_the_narrowest_stated() {
    let m = shipped("Tormach xsTECH");
    for (rpm, want) in [(18000.0, "18000 rpm, above the machine's 15000"), (4000.0, "4000 rpm, below the machine's 5000")] {
        let (world, _h) = face_job(rpm);
        let c = check_document(&world, &m).expect("checks");
        assert!(c.problems.iter().any(|p| p.starts_with("Face1:") && p.contains(want)), "{rpm}: {:?}", c.problems);
    }
    let (world, _h) = face_job(12000.0);
    let c = check_document(&world, &m).expect("checks");
    assert!(c.is_ok(), "{:?}", c.problems);
}
