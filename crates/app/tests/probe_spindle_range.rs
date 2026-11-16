//! #565: the machine's spindle-range law judges the speed a program
//! COMMANDS. A touch probe is never spun (the post commands rpm 0 for it,
//! `posting::posted_rpm`), so its catalog rpm is not judged against the
//! machine's spindle window - every probing program on a machine that
//! states a spindle minimum (the Tormach 770MX: 175 rpm) was refused at
//! post. Every cutter is still judged by its own rpm.
//!
//! Gate map (#565): covers app::machine, post::machine (check_document spindle clause),
//! app::posting (posted_rpm).
//! Provenance: our own fixtures; the shipped Tormach 770MX machine file.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, SetStock};
use camcraft_app::machine::{check_document, parse_mch};
use camcraft_app::operation::{Height, HeightReference, OpParams, SrcRef};
use camcraft_app::probing::{ProbeDir, ProbeMode};
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;
use camcraft_app::{posting, tools};
use flecs_ecs::prelude::World;

const PROBE: &str = "PRB-0400";

fn tormach_770mx() -> camcraft_app::machine::Machine {
    let (_, text) = posting::MACHINES.iter().find(|(n, _)| *n == "Tormach 770MX").expect("shipped 770MX");
    let m = parse_mch(text).expect("parses");
    assert!(m.spindle_min.is_some_and(|lo| lo > 1.0), "the 770MX states a spindle minimum: {:?}", m.spindle_min);
    m
}

fn add(world: &World, h: &mut History, name: &str) -> tools::Tool {
    let t = catalog().into_iter().find(|(n, _)| *n == name).map(|(_, t)| t).expect("catalog tool");
    h.execute(world, AddTool::new(world, name, t.clone()).expect("add tool"));
    t
}

/// A 50 x 50 x 25 block and one touch straight down on its top centre.
fn probe_doc() -> (World, History) {
    let world = new_world();
    let mut h = History::default();
    let stock = SetStock::new(&world, StockShape::Box { dx: 50.0, dy: 50.0, dz: 25.0 }, Default::default());
    h.execute(&world, stock.expect("stock"));
    let probe = add(&world, &mut h, PROBE);
    assert!(probe.rpm < 175.0, "the catalog probe states a token rpm the post never commands: {}", probe.rpm);
    let p = OpParams::Probe {
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        mode: ProbeMode::Face,
        dir: ProbeDir::ZNeg,
        dir_y: ProbeDir::YPos,
        target: [0.0, 0.0, 25.0],
        radius: 10.0,
        approach: 5.0,
        overtravel: 5.0,
        backoff: 1.0,
        fast_feed: 300.0,
        slow_feed: 30.0,
        depth: 3.0,
        inset: 5.0,
        set_wcs: Some(0),
        tolerance: 0.5,
    };
    h.execute(&world, CreateOperation::new(&world, "Probe1", PROBE, SrcRef::Stock, p).expect("probe op"));
    h.execute(&world, GenerateOperation::new(&world, "Probe1").expect("generate"));
    (world, h)
}

#[test]
fn a_probe_program_passes_the_spindle_range_of_a_tormach() {
    let (world, _h) = probe_doc();
    let c = check_document(&world, &tormach_770mx()).expect("checks");
    assert!(
        !c.problems.iter().any(|p| p.contains("rpm")),
        "a probe is never spun - its catalog rpm is not a spindle word: {:?}",
        c.problems
    );
}

#[test]
fn a_probe_posts_no_speed_and_a_cutter_its_own() {
    let probe = catalog().into_iter().find(|(n, _)| *n == PROBE).map(|(_, t)| t).expect("probe");
    assert_eq!(posting::posted_rpm(&probe), 0.0);
    let em = catalog().into_iter().find(|(n, _)| *n == "EM-0400-2FL").map(|(_, t)| t).expect("end mill");
    assert_eq!(posting::posted_rpm(&em), em.rpm);
}

#[test]
fn a_cutter_below_the_minimum_is_still_refused_beside_a_probe() {
    let (world, mut h) = probe_doc();
    let mut em = add(&world, &mut h, "EM-0400-2FL");
    em.rpm = 100.0; // under the 770MX's 175
    h.execute(&world, camcraft_app::commands::ChangeTool::new(&world, "EM-0400-2FL", em).expect("slow cutter"));
    let face = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false,
    };
    h.execute(&world, CreateOperation::new(&world, "Face1", "EM-0400-2FL", SrcRef::Stock, face).expect("face"));
    h.execute(&world, GenerateOperation::new(&world, "Face1").expect("generate"));
    let c = check_document(&world, &tormach_770mx()).expect("checks");
    let rpm: Vec<&String> = c.problems.iter().filter(|p| p.contains("rpm")).collect();
    assert_eq!(rpm.len(), 1, "only the cutter is judged: {:?}", c.problems);
    assert!(rpm[0].starts_with("Face1:") && rpm[0].contains("below the machine's 175"), "{rpm:?}");
}
