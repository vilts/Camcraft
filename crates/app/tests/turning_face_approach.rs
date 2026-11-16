//! #632: lathe facing approaches at RAPID and lifts off each face
//! before it retracts. The posted program's first move after the tool
//! call is a G0 to the clearance point (never a G1 from wherever the
//! turret parked), every pass's retract lifts Z off the face it just
//! made before the radial G0 out, the meridian rapid law (#103) finds
//! no rapid in stock on the facing, and a clearance plane set inside
//! the bar is convicted by that law and blocks the post - the
//! approach is judged, never trusted.
//!
//! Gate map (M4.4): covers motion::turning::face (approach and lift)
//! through app::operation (the meridian rapid law on a facing) and
//! app::posting (the Fanuc lathe program).
//! Lane shape: seconds.

use flecs_ecs::prelude::World;

use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::post::{cnc, engine};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;
use camcraft_app::world::new_world;

const TAU: f64 = std::f64::consts::TAU;
const LATHE_CPS: &str = include_str!("../../../assets/posts/fanuc-lathe.cps");
const TN: &str = "TN-1600-0R8";
/// A Ø12 part, z -16..15, in a Ø12 bar 1 mm longer (to Z16): the
/// facing takes the bar's end down to the part's, Z16 to Z15.
const BAR: [(f64, f64); 4] = [(0.0, -16.0), (6.0, -16.0), (6.0, 15.0), (0.0, 15.0)];

fn revolve(meridian: &[(f64, f64)], segs: usize) -> Mesh {
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

fn bar_world() -> (World, History) {
    let world = new_world();
    let mut history = History::default();
    let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
    let cmd = SetStock::new(&world, camcraft_app::stock::StockShape::Cylinder { diameter: 12.0, height: 32.0 }, at);
    history.execute(&world, cmd.expect("bar stock"));
    let lathe = Machine {
        model: "TestBench lathe".into(),
        kind: Some("lathe".into()),
        spindle_max: Some(12000.0),
        ..Default::default()
    };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    let tool = catalog().into_iter().find(|(n, _)| *n == TN).map(|(_, t)| t).expect("catalog entry");
    history.execute(&world, AddTool::new(&world, TN, tool).expect("insert"));
    history.execute(&world, ImportModel::from_mesh("bar", revolve(&BAR, 16)));
    (world, history)
}

fn facing(clearance: f64) -> OpParams {
    OpParams::Facing {
        tolerance: 0.05,
        clearance_z: Height::fixed(clearance),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(15.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 80.0,
    }
}

fn create(world: &World, history: &mut History, clearance: f64) -> Vec<Warning> {
    let cmd = CreateOperation::new(world, "F1", TN, SrcRef::Entity("bar".into()), facing(clearance)).expect("create");
    history.execute(world, cmd);
    history.execute(world, GenerateOperation::new(world, "F1").expect("generate"));
    let e = model::find_by_name(world, "F1").expect("op");
    model::toolpath_of(world, &e).expect("cache").warnings.clone()
}

/// The posted program's motion lines (G0/G1), in order.
fn motion(world: &World) -> Vec<String> {
    let records = posting::stream_records(world).expect("posts");
    let text = engine::run(LATHE_CPS, &posting::machine_value(world), &cnc::serialize(&records))
        .expect("the lathe script runs")
        .program;
    text.lines().filter(|l| l.starts_with("G00 ") || l.starts_with("G01 ")).map(str::to_owned).collect()
}

/// A motion line's (X, Z) words.
fn xz(line: &str) -> (f64, f64) {
    let word = |c: char| {
        line.split_whitespace()
            .find_map(|w| w.strip_prefix(c).and_then(|v| v.parse::<f64>().ok()))
            .unwrap_or(f64::NAN)
    };
    (word('X'), word('Z'))
}

#[test]
fn the_first_posted_move_is_a_rapid_to_the_clearance_point() {
    let (world, mut history) = bar_world();
    let warns = create(&world, &mut history, 25.0);
    let lines = motion(&world);
    let first = lines.first().expect("a program");
    assert!(first.starts_with("G00 "), "the first move after the tool call is a rapid: {lines:?}");
    let (x, z) = xz(first);
    assert!((z - 25.0).abs() < 1e-9, "on the clearance plane: {first}");
    assert!(x > 12.0, "outside the Ø12 bar: {first}");
    let plunge = &lines[1];
    assert!(plunge.starts_with("G01 ") && (xz(plunge).0 - x).abs() < 1e-9, "then the plunge feeds in at that X: {lines:?}");
    // The approach and every other rapid are judged in the meridian
    // and found in air: nothing named, and the post writes.
    assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{warns:?}");
    let out = std::env::temp_dir().join(format!("camcraft-face-approach-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

#[test]
fn every_facing_retract_lifts_z_before_it_moves_out_in_x() {
    let (world, mut history) = bar_world();
    create(&world, &mut history, 25.0);
    let lines = motion(&world);
    let mut passes = 0;
    for (i, l) in lines.iter().enumerate() {
        // A pass ends on the axis: the most negative X posted (the
        // control point r past the nose centre on the axis).
        if !(l.starts_with("G01 ") && xz(l).0 < 0.0) {
            continue;
        }
        passes += 1;
        let (x, z) = xz(l);
        let (up, out) = (&lines[i + 1], &lines[i + 2]);
        let (ux, uz) = xz(up);
        assert!(up.starts_with("G00 ") && (ux - x).abs() < 1e-9 && uz > z + 1e-9, "the lift off Z{z}: {up}");
        assert!(uz - z <= 0.5 + 1e-9, "a lift no taller than the stepdown: {up}");
        let (ox, oz) = xz(out);
        assert!(out.starts_with("G00 ") && (oz - uz).abs() < 1e-9 && ox > 12.0, "then out in X at the lifted Z: {out}");
    }
    assert_eq!(passes, 3, "three passes, Z16 to Z15 by 0.5: {lines:?}");
}

#[test]
fn a_clearance_plane_inside_the_bar_is_convicted_not_posted() {
    // The approach is a judged rapid: on a clearance plane at Z5 -
    // inside the bar - it would traverse through solid stock, and the
    // meridian law names it and the post refuses.
    let (world, mut history) = bar_world();
    let warns = create(&world, &mut history, 5.0);
    assert!(warns.contains(&Warning::RapidInStock { at: 0 }), "the approach through the bar went unnamed: {warns:?}");
    let out = std::env::temp_dir().join(format!("camcraft-face-inside-{}.nc", std::process::id()));
    let _ = std::fs::remove_file(&out);
    let posted = posting::post_setup(&world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_err(), "an approach through the bar posted");
}
