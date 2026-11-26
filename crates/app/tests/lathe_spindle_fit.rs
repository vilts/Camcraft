//! The lathe spindle law's measure (issue #62 review): what is bounded
//! is the whole rigid displacement of the part between its fitted frame
//! and the spindle frame - the axis drift over the part's height AND
//! the lean times the part's radius (a lean cones a face by R*a, which
//! a thin wide flange shows long before its axis drifts). And the fit
//! of an honest plumb revolve must sit far below the tolerance even
//! when the mesh is what an STL export hands over: uneven segment
//! spacing, a seam, vertices rounded to f32, far from the world origin.
//!
//! Gate map (M4.4): covers app::operation (spindle_law's reading and
//! collect_job's door), motion::multiaxis::axis (fit, via the reading).
//! Lane shape (M4.4): all tests here run in seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{
    AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock,
};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::operation::{spindle_reading, Height, OpParams, SrcRef, SPINDLE_TOL};
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
/// A thin wide washer: 30 mm across, 0.4 mm thick, centred on Z0.
const WASHER: [(f64, f64); 4] = [(0.0, -0.2), (15.0, -0.2), (15.0, 0.2), (0.0, 0.2)];
/// A stepped shaft with a cone and a shoulder - every zone kind.
const SHAFT: [(f64, f64); 8] =
    [(0.0, -30.0), (12.0, -30.0), (12.0, -5.0), (8.0, 0.0), (8.0, 20.0), (5.0, 20.0), (5.0, 30.0), (0.0, 30.0)];

/// The closed revolve about world Z at the angles `ths` (the last
/// meets the first), each vertex then mapped by `at`.
fn revolve_at(meridian: &[(f64, f64)], ths: &[f64], at: &dyn Fn(Point3) -> Point3) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = ths[j % ths.len()];
        at(Point3::new(r * th.cos(), r * th.sin(), z))
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..ths.len() {
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
    facets
}

fn uniform(segs: usize) -> Vec<f64> {
    (0..segs).map(|j| TAU * j as f64 / segs as f64).collect()
}

/// A rotation about world X by `rad`: the axis leans in Y.
fn lean_y(rad: f64) -> impl Fn(Point3) -> Point3 {
    let (s, c) = rad.sin_cos();
    move |p| Point3::new(p.x, p.y * c - p.z * s, p.y * s + p.z * c)
}

fn lathe_world(part: Vec<Triangle>) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let stock = SetStock::new(
        &world,
        camcraft_app::stock::StockShape::Box { dx: 40.0, dy: 40.0, dz: 40.0 },
        Location::default(),
    );
    history.execute(&world, stock.expect("box stock"));
    let lathe = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    let tool = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t).expect("insert");
    history.execute(&world, AddTool::new(&world, "TN-1600-0R8", tool).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("part", Mesh::from_facets(part)));
    (world, history)
}

/// Generate a Facing on the washer's top; Err is the refusal text.
fn face_washer(part: Vec<Triangle>) -> Result<(), String> {
    let (world, mut history) = lathe_world(part);
    let p = OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(5.0),
        top_z: Height::fixed(0.2),
        bottom_z: Height::fixed(0.1),
        stepdown: 0.05,
        feed: 150.0,
        plunge_feed: 80.0,
        picks: Default::default(),
    };
    let cmd = CreateOperation::new(&world, "F1", "TN-1600-0R8", SrcRef::Entity("part".into()), p).expect("create");
    history.execute(&world, cmd);
    let cmd = GenerateOperation::new(&world, "F1").map_err(|e| e.to_string())?;
    history.execute(&world, cmd);
    Ok(())
}

#[test]
fn a_thin_washer_leaning_under_the_old_axis_budget_is_refused_by_its_coned_face() {
    // 2 mrad: the old law read the axis 0.2 * 0.002 = 0.4 um off at the
    // washer's faces and passed it; its top face posts as a cone 15 *
    // 0.002 = 30 um deep - sixty times the Z word's half digit.
    let part = revolve_at(&WASHER, &uniform(32), &lean_y(0.002));
    let r = spindle_reading(&part, [0.0; 3]).expect("a revolve reads");
    assert!(r.axial > 0.029 && r.axial < 0.031, "the face cone is R*a: {r:?}");
    let e = face_washer(part).expect_err("a coned face never generates");
    for s in ["spindle", "0.115 degrees", "align"] {
        assert!(e.contains(s), "the refusal names '{s}': {e}");
    }
}

#[test]
fn the_washer_boundary_is_the_tolerance_on_the_largest_radius() {
    // R*sin(a) just under and just over the half digit.
    let under = (0.98 * SPINDLE_TOL / 15.0).asin();
    let over = (1.02 * SPINDLE_TOL / 15.0).asin();
    let part = revolve_at(&WASHER, &uniform(32), &lean_y(under));
    let r = spindle_reading(&part, [0.0; 3]).expect("reads");
    assert!(r.worst() <= SPINDLE_TOL, "just under: {r:?}");
    if let Err(e) = face_washer(part) {
        assert!(!e.contains("lathe spindle"), "just under the law passes it: {e}");
    }
    let part = revolve_at(&WASHER, &uniform(32), &lean_y(over));
    let r = spindle_reading(&part, [0.0; 3]).expect("reads");
    assert!(r.worst() > SPINDLE_TOL, "just over: {r:?}");
    let e = face_washer(part).expect_err("just over is refused");
    assert!(e.contains("lathe spindle"), "{e}");
}

/// Uneven angles (a smooth wobble plus a jitter, as a CAD tessellator's
/// chord law gives them), a duplicated seam vertex pair at 0/TAU, and
/// every coordinate rounded to f32 the way an STL file stores it.
fn stl_like(meridian: &[(f64, f64)], segs: usize, at: (f64, f64)) -> Vec<Triangle> {
    let mut ths: Vec<f64> = (0..segs)
        .map(|j| {
            let t = j as f64 / segs as f64;
            let jitter = 0.27 * ((j * 7919) % 13) as f64 / 13.0 - 0.13;
            TAU * (t + 0.04 * (3.0 * TAU * t).sin() / 3.0 + jitter / segs as f64)
        })
        .collect();
    ths[0] = 0.0;
    let f32ed = move |p: Point3| {
        let r = |v: f64| f64::from(v as f32);
        Point3::new(r(p.x + at.0), r(p.y + at.1), r(p.z))
    };
    // The seam: a second vertex column a micron short of the first, so
    // the ring closes on a sliver (CAD seams do this).
    ths.push(TAU - 1e-6);
    revolve_at(meridian, &ths, &f32ed)
}

#[test]
fn an_stl_like_plumb_revolve_fits_far_below_the_tolerance() {
    // Measured, not assumed: the reading of plumb revolves on their own
    // origin, tessellated unevenly and stored in f32, near and far from
    // the world origin. The law's tolerance must not refuse them; the
    // fit's noise is printed (run with --nocapture) and bounded at a
    // tenth of the tolerance.
    let mut worst = 0.0f64;
    for (name, mer) in [("washer", &WASHER[..]), ("shaft", &SHAFT[..])] {
        for segs in [24, 61, 180] {
            for at in [(0.0, 0.0), (123.456, -78.9)] {
                let part = stl_like(mer, segs, at);
                let o = [f64::from(at.0 as f32), f64::from(at.1 as f32), 0.0];
                let r = spindle_reading(&part, o).expect("a plumb revolve reads");
                println!(
                    "{name} segs {segs} at {at:?}: lean {:.2e} deg, offset {:.2e} mm, radial {:.2e}, axial {:.2e}",
                    r.lean_deg, r.offset_mid, r.radial, r.axial
                );
                assert!(r.worst() <= 0.1 * SPINDLE_TOL, "{name} {segs} {at:?}: {r:?}");
                worst = worst.max(r.worst());
            }
        }
    }
    println!("worst fit noise {worst:.2e} mm against SPINDLE_TOL {SPINDLE_TOL}");
}
