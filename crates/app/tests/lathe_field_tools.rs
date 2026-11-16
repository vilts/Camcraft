//! The lathe field eye against each generated blade kind (#21): what
//! the document's own field walk (`stock_view::field_view_with`) does
//! with a real Threading and a real FormTurning program, not a
//! synthetic pass. A form gauge's crown is cut - air in the formed
//! valley at every azimuth, steel left beside it. A Threading op's
//! passes are half-turn helical arcs about the spindle, which no
//! revolved reading cuts honestly (a helix revolved is a full ring,
//! crests gone): the plan refuses them by name before the walk
//! (`LATHE_THREAD_REFUSAL`), never as a "skewed chord" from inside it.
//!
//! Provenance: our own engines and the M5.2 meridian law; nothing from
//! vendor binaries.
//!
//! Gate map (M4.4): covers app::stock_sim, sim::stock_sim (plan_program's lathe clause,
//! stock_sim::lathe's helix gate), app::stock_view (field_view_with's
//! lathe arm).
//! Lane shape (M4.4): all tests here run in seconds in debug.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, OpStatus, SrcRef};
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::stock_sim::{LATHE_HELIX_REFUSAL, LATHE_THREAD_REFUSAL};
use camcraft_app::tools::catalog;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
/// The display budget the scans run at - seconds in debug.
const TEXELS: f64 = 512_000.0;

/// A closed solid of revolution about world Z, 16 facets round.
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

/// A lathe document: bar stock `diameter` across over z
/// `z_lo..z_hi` (bar-sized, #103), the bar model, the catalog `tool`
/// on the rack.
fn lathe_world(diameter: f64, (z_lo, z_hi): (f64, f64), tool: &str, bar: Mesh) -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let shape = StockShape::Cylinder { diameter, height: z_hi - z_lo };
    let at = Location { pos: Point3::new(0.0, 0.0, z_lo), ..Location::default() };
    history.execute(&world, SetStock::new(&world, shape, at).expect("bar stock"));
    let mch = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(mch)).expect("lathe"));
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, tool, t).expect("catalog tool"));
    history.execute(&world, ImportModel::from_mesh("bar", bar));
    (world, history)
}

fn generate(world: &World, history: &mut History, name: &str, tool: &str, params: OpParams) {
    let op = CreateOperation::new(world, name, tool, SrcRef::Entity("bar".into()), params).expect("create");
    history.execute(world, op);
    history.execute(world, GenerateOperation::new(world, name).expect("generate"));
    let e = model::find_by_name(world, name).expect("op exists");
    assert_eq!(camcraft_app::operation::status_of(world, &e), OpStatus::Ok, "{name} generated");
}

/// The threading suite's bar: wall r 6, a 60-degree V 1.2 deep every
/// 2.0 from z 8 to z 14, long past the thread so the axis fit reads a
/// revolve.
fn threaded() -> Mesh {
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
    m.extend([(major, 14.0 + half), (major, 40.0), (0.0, 40.0)]);
    revolve(&m)
}

#[test]
fn a_generated_threading_op_refuses_the_field_by_name() {
    let (world, mut history) = lathe_world(12.0, (-16.0, 40.0), "TD-1732-60", threaded());
    let p = OpParams::Threading {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(14.3),
        bottom_z: Height::fixed(7.7),
        pitch: 2.0,
        depth: 1.2,
        infeed: 0.4,
        feed: 120.0,
        plunge_feed: 40.0,
        infeed_plan: Default::default(),
        hand: camcraft_app::cl::ThreadHand::Right,
    };
    generate(&world, &mut history, "TH1", "TD-1732-60", p);
    let e = model::find_by_name(&world, "TH1").expect("op exists");
    let arcs = model::toolpath_of(&world, &e).map_or(0, |c| c.path.arc_count());
    assert!(arcs > 0, "fixture: the threading kernel writes its passes as helical arcs");

    let Err(err) = camcraft_app::stock_view::field_view_with(&world, TEXELS) else {
        panic!("a helical threading program must not be revolved into rings of cut crests");
    };
    assert!(err.contains("TH1") && err.contains(LATHE_THREAD_REFUSAL), "named by op and law: {err}");
    assert!(!err.contains("skewed"), "the plan names the program kind, not a chord: {err}");
    assert!(!err.contains(LATHE_HELIX_REFUSAL), "a thread tool gets the thread refusal: {err}");
}

#[test]
fn a_generated_form_op_cuts_its_crown_on_the_field() {
    // The form suite's bar: blank r 10, a crown of r 6 sunk to r 8 at
    // z 10 across z 8..12.
    let s = |d: f64| 6.0 - (36.0 - d * d).sqrt();
    let formed = revolve(&[
        (0.0, -50.0),
        (10.0, -50.0),
        (10.0, 8.0),
        (8.0 + s(2.0), 8.0),
        (8.0 + s(1.0), 9.0),
        (8.0, 10.0),
        (8.0 + s(1.0), 11.0),
        (8.0 + s(2.0), 12.0),
        (10.0, 12.0),
        (10.0, 50.0),
        (0.0, 50.0),
    ]);
    let (world, mut history) = lathe_world(20.0, (-50.0, 50.0), "FG-0600-04", formed);
    let p = OpParams::FormTurning {
        tolerance: 0.05,
        clearance_z: Height::fixed(24.0),
        top_z: Height::fixed(13.0),
        bottom_z: Height::fixed(7.0),
        feed: 40.0,
        plunge_feed: 20.0,
    };
    generate(&world, &mut history, "FM1", "FG-0600-04", p);
    let view = camcraft_app::stock_view::field_view_with(&world, TEXELS).expect("the field eye cuts the form program");
    // Air in the valley at every azimuth, steel beside the band and
    // below the crown's floor.
    for k in 0..8 {
        let th = TAU * f64::from(k) / 8.0;
        let at = |r: f64, z: f64| Point3::new(r * th.cos(), r * th.sin(), z);
        assert!(view.stock.distance(at(9.2, 10.0)) > 0.0, "the valley is cut at azimuth {th:.2}");
        assert!(view.stock.distance(at(9.2, 0.0)) < 0.0, "steel off the band at azimuth {th:.2}");
        assert!(view.stock.distance(at(7.0, 10.0)) < 0.0, "steel under the crown at azimuth {th:.2}");
    }
}
