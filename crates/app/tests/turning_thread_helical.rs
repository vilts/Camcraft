//! Issue #3023 at the app level: a Threading operation on a turned
//! shaft whose thread is modelled as a HELIX (the issue's part, an
//! M16x1.5 thread on a Ø24 / Ø30 shaft) is refused with a message that
//! names the helical thread - before the slice of the job region, which
//! read the issue's STEP helix as "the mesh is open ... dangling chord
//! ends", and before the axis fit, which read the native model as
//! "every axis fits this model equally". The kernel's own law is pinned
//! one crate down (camcraft-motion's turning_thread_helical suite).
//!
//! Gate map (M4.4): covers camcraft_app::operation (the threading arm
//! of generate).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const MAJOR: f64 = 8.0;
const DEPTH: f64 = 0.92;
const PITCH: f64 = 1.5;
const TURNS: usize = 11;
const N: usize = 96;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn revolve(meridian: &[(f64, f64)], t: &mut Vec<Triangle>) {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / N as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..N {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra > 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            }
            if rb > 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
}

/// The issue's shaft: a right-hand helical V thread from z 0 to 16.5,
/// relief, Ø24 body, Ø30 head to z 70. Every 29th thread facet is
/// left out: the gaps of the issue's STEP tessellation.
fn helical_shaft() -> Mesh {
    let v = |j: usize, i: usize| {
        let th = TAU * j as f64 / N as f64;
        let r = if i.is_multiple_of(2) { MAJOR } else { MAJOR - DEPTH };
        let z = i as f64 * PITCH * 0.5 + PITCH * j as f64 / N as f64;
        p3(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    let mut k = 0usize;
    for i in 0..2 * TURNS {
        for j in 0..N {
            for f in [
                Triangle::new(v(j, i), v(j + 1, i), v(j + 1, i + 1)),
                Triangle::new(v(j, i), v(j + 1, i + 1), v(j, i + 1)),
            ] {
                k += 1;
                if !k.is_multiple_of(29) {
                    t.push(f);
                }
            }
        }
    }
    revolve(&[(0.0, 0.0), (MAJOR, 0.0)], &mut t);
    let top = PITCH * TURNS as f64 + PITCH;
    revolve(
        &[(MAJOR, top), (MAJOR, 17.0), (12.0, 17.0), (12.0, 53.0), (15.0, 53.0), (15.0, 70.0), (0.0, 70.0)],
        &mut t,
    );
    Mesh::from_facets(t)
}

#[test]
fn a_helical_thread_is_refused_by_name_before_the_slice() {
    let world = new_world();
    let mut history = History::default();
    history.execute(
        &world,
        SetStock::new(
            &world,
            camcraft_app::stock::StockShape::Cylinder { diameter: 30.0, height: 70.0 },
            Location { pos: Point3::new(0.0, 0.0, 0.0), ..Location::default() },
        )
        .expect("bar stock"),
    );
    let lathe = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "TD-1732-60").map(|(_, t)| t).expect("catalog insert");
    history.execute(&world, AddTool::new(&world, "TD-1732-60", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("shaft", helical_shaft()));
    let params = OpParams::Threading {
        tolerance: 0.01,
        clearance_z: Height::fixed(80.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(1.0),
        pitch: PITCH,
        depth: DEPTH,
        infeed: 0.2,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: camcraft_app::cl::ThreadHand::Right,
        picks: Default::default(),
    };
    let create = CreateOperation::new(&world, "M16", "TD-1732-60", SrcRef::Entity("shaft".into()), params);
    history.execute(&world, create.expect("create"));
    let Err(err) = GenerateOperation::new(&world, "M16") else {
        panic!("a helical thread must be refused");
    };
    let msg = err.to_string();
    assert!(msg.contains("HELICAL") && msg.contains("right-hand") && msg.contains("revolved V"), "{msg}");
}
