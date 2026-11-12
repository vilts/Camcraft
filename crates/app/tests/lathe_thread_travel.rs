//! #631: the built-in Lathe (X 0..320) refused every thread - a thread
//! pass is an arc about the spindle axis whose far side reads negative
//! X (the CL's antipode), and the travel gate judged that raw X. On a
//! lathe, an arc about the spindle is the spindle turning: the slide
//! reaches the radius. A thread now passes the built-in Lathe, and a
//! real over-travel is still refused by name.
//!
//! Gate map (#631): covers app::machine, post::machine (check_document's travel law).
//! Provenance: our own fixtures (the lathe_post_g76 thread bar).

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::{self, parse_mch};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting::MACHINES;
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
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

/// The thread bar on a machine loaded from `mch` text.
fn thread_on(mch: &str) -> World {
    let world = new_world();
    let mut h = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 56.0 }, at);
    h.execute(&world, cmd.expect("bar stock"));
    let m = parse_mch(mch).expect("machine parses");
    h.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(m)).expect("lathe"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "TD-1732-60").map(|(_, t)| t).expect("insert");
    h.execute(&world, AddTool::new(&world, "TD-1732-60", tool).expect("tool"));
    h.execute(&world, ImportModel::from_mesh("bar", threaded()));
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
        hand: camcraft_app::cl::ThreadHand::Right,
    };
    h.execute(&world, CreateOperation::new(&world, "TH1", "TD-1732-60", SrcRef::Entity("bar".into()), params).expect("create"));
    h.execute(&world, GenerateOperation::new(&world, "TH1").expect("generate"));
    world
}

fn travel_problems(world: &World) -> Vec<String> {
    let (_, m) = machine::of(world);
    let m = m.expect("a mounted machine");
    let check = machine::check_document(world, &m).expect("the check runs");
    check.problems.into_iter().filter(|p| p.contains("stroke is")).collect()
}

fn builtin_lathe() -> &'static str {
    MACHINES.iter().find(|(n, _)| *n == "Lathe").map(|(_, t)| *t).expect("the built-in Lathe ships")
}

#[test]
fn a_thread_passes_the_built_in_lathe() {
    let world = thread_on(builtin_lathe());
    let p = travel_problems(&world);
    assert!(p.is_empty(), "the spindle-arc antipode is not a slide position: {p:?}");
}

#[test]
fn a_real_over_travel_on_a_lathe_is_still_refused() {
    // The same lathe with an X stroke short of the thread's radius (3 mm).
    // Only the X stroke reaches 320 (Z is +-615); its min is left as shipped.
    let short = builtin_lathe().replacen("\"max\": 320.0", "\"max\": 2.0", 1);
    assert_ne!(short, builtin_lathe(), "fixture: the X band was rewritten");
    let world = thread_on(&short);
    let p = travel_problems(&world);
    assert!(p.iter().any(|l| l.contains(" in X")), "a radius past the X stroke is refused by name: {p:?}");
}

