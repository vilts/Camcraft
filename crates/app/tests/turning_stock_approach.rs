//! Issue #130: the turning kernels approach from the setup stock, not
//! the part. A groove and a parting cut in a bar wider than the part
//! approach and retract clear of the bar's OD by the retract clearance,
//! their ladders start at the bar's skin, and the program posts clean;
//! with no stock readable the rough is the part-based path it always
//! was, and says so in a note.
//!
//! Gate map (M4.4): covers app::operation (turning_stock, generate's
//! turning arms), app::posting (the verification gate on a lathe
//! document).
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::cl::{Move, MoveKind};
use camcraft_app::commands::{AddTool, CreateOperation, GenerateOperation, History, ImportModel, LoadMachine, SetStock};
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::machine::Machine;
use camcraft_app::model;
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::posting;
use camcraft_app::schema::Location;
use camcraft_app::stock::StockShape;
use camcraft_app::tools::catalog;
use camcraft_app::verify::Warning;

const TAU: f64 = std::f64::consts::TAU;
/// The kernels' radial gap over the bar's OD (turning_stock's).
const CLEAR: f64 = 1.0;

fn revolve(meridian: &[(f64, f64)]) -> Vec<Triangle> {
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
    facets
}

/// turning_blade's grooved bar: wall r 6 from z -16 to 28, a groove
/// floored at r 4 over z -6.8 .. -3.6.
fn grooved() -> Vec<Triangle> {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, -6.8), (4.0, -6.8), (4.0, -3.6), (6.0, -3.6), (6.0, 28.0), (0.0, 28.0)])
}

/// turning_axial_rapids' flanged bar: r 6, neck r 3.5 over z 2 .. 16.
fn flanged() -> Vec<Triangle> {
    revolve(&[(0.0, -16.0), (6.0, -16.0), (6.0, 2.0), (3.5, 2.0), (3.5, 16.0), (0.0, 16.0)])
}

/// A lathe document holding `part`, in a round bar of `bar` mm from
/// z -16 over `len` when one is given, with `tool` on the rack and the
/// op `params` generated.
fn run(part: Vec<Triangle>, bar: Option<(f64, f64)>, tool: &str, params: OpParams) -> (World, Vec<Warning>, Vec<Move>, Vec<String>) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    if let Some((diameter, height)) = bar {
        let at = Location { pos: Point3::new(0.0, 0.0, -16.0), ..Location::default() };
        history.execute(&world, SetStock::new(&world, StockShape::Cylinder { diameter, height }, at).expect("bar stock"));
    }
    let lathe = Machine { model: "TestBench lathe".into(), kind: Some("lathe".into()), spindle_min: Some(25.0), spindle_max: Some(12000.0), ..Default::default() };
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(lathe)).expect("lathe"));
    history.execute(&world, camcraft_app::commands::SelectPost::new(&world, camcraft_app::schema::PostRef::Named("Fanuc Lathe".into())).expect("lathe post"));
    let t = catalog().into_iter().find(|(n, _)| *n == tool).map(|(_, t)| t).expect("catalog");
    history.execute(&world, AddTool::new(&world, tool, t).expect("tool"));
    history.execute(&world, ImportModel::from_mesh("bar", Mesh::from_facets(part)));
    history.execute(&world, CreateOperation::new(&world, "T1", tool, SrcRef::Entity("bar".into()), params).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "T1").expect("generate"));
    let e = model::find_by_name(&world, "T1").expect("op");
    let cache = model::toolpath_of(&world, &e).expect("cache");
    (world, cache.warnings.clone(), cache.path.moves.clone(), cache.path.notes.clone())
}

fn rho(q: Point3) -> f64 {
    q.x.hypot(q.y)
}

/// Every rapid that travels along the axis stands clear of the r 10
/// bar by the clearance; no rapid is named; the post writes.
fn clears_and_posts(world: &World, warns: &[Warning], moves: &[Move], tag: &str) {
    for m in moves.iter().filter(|m| m.kind == MoveKind::Rapid && (m.from.z - m.to.z).abs() > 1e-9) {
        assert!(rho(m.from) >= 10.0 + CLEAR - 1e-9 && rho(m.to) >= 10.0 + CLEAR - 1e-9, "an axial rapid inside the bar: {m:?}");
    }
    assert!(!warns.iter().any(|w| matches!(w, Warning::RapidInStock { .. })), "{warns:?}");
    let out = std::env::temp_dir().join(format!("camcraft-stock-approach-{tag}-{}.nc", std::process::id()));
    let posted = posting::post_setup(world, &out);
    let _ = std::fs::remove_file(&out);
    assert!(posted.is_ok(), "{posted:?}");
}

#[test]
fn a_groove_in_a_wider_bar_approaches_from_the_bar_and_posts_clean() {
    let params = OpParams::GrooveRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(35.0),
        top_z: Height::fixed(-3.6),
        bottom_z: Height::fixed(-6.8),
        stepdown: 1.0,
        stock_to_leave: 0.0,
        feed: 150.0,
        plunge_feed: 60.0,
    };
    let (world, warns, moves, _) = run(grooved(), Some((20.0, 44.0)), "GT-0200", params);
    // The ladder steps from the bar's skin: the first rung one stepdown
    // under r 10, the last on the floor.
    let tips: Vec<f64> = moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| rho(m.to)).collect();
    assert!(tips.first().is_some_and(|&t| (t - 9.0).abs() < 1e-6), "the first rung starts at the bar: {tips:?}");
    assert!(tips.iter().any(|&t| (t - 4.0).abs() < 1e-6), "the floor is reached: {tips:?}");
    clears_and_posts(&world, &warns, &moves, "groove");
}

#[test]
fn a_parting_cut_in_a_wider_bar_approaches_from_the_bar_and_posts_clean() {
    let params = OpParams::Part {
        tolerance: 0.05,
        clearance_z: Height::fixed(35.0),
        top_z: Height::fixed(28.0),
        bottom_z: Height::fixed(20.0),
        stepdown: 0.5,
        feed: 150.0,
        plunge_feed: 60.0,
        bar_load: false,
        bar_advance: 0.0,
        bar_unload: false,
        bar_transfer: false,
        inspect_tol: 0.0,
    };
    let (world, warns, moves, _) = run(grooved(), Some((20.0, 44.0)), "GT-0200", params);
    let tips: Vec<f64> = moves.iter().filter(|m| m.kind == MoveKind::Feed).map(|m| rho(m.to)).collect();
    assert!(tips.first().is_some_and(|&t| (t - 9.5).abs() < 1e-6), "the parting ladder starts at the bar: {tips:?}");
    assert!(tips.last().is_some_and(|&t| t.abs() < 1e-9), "the last rung severs on the axis: {tips:?}");
    clears_and_posts(&world, &warns, &moves, "part");
}

#[test]
fn with_no_stock_the_rough_is_the_part_based_path_and_says_so() {
    let params = OpParams::ProfileRough {
        tolerance: 0.05,
        clearance_z: Height::fixed(25.0),
        top_z: Height::fixed(16.0),
        bottom_z: Height::fixed(-16.0),
        stepdown: 0.8,
        stock_to_leave: 0.25,
        feed: 150.0,
        plunge_feed: 80.0,
    };
    let (_, _, moves, notes) = run(flanged(), None, "TN-1600-0R8", params);
    // The kernel's stock-blind entry point is the pre-#130 path.
    let nose = catalog().into_iter().find(|(n, _)| *n == "TN-1600-0R8").map(|(_, t)| t.nose_radius).expect("catalog");
    let blind = camcraft_app::turning::profile_rough(
        &flanged(),
        &camcraft_app::turning::ProfileRoughParams {
            clearance_z: 25.0,
            top_z: 16.0,
            bottom_z: -16.0,
            stepdown: 0.8,
            stock_to_leave: 0.25,
            feed: 150.0,
            plunge_feed: 80.0,
            nose_radius: nose,
            tolerance: 0.05,
        },
    )
    .expect("the part-based rough");
    assert_eq!(moves.len(), blind.moves.len(), "the move count changed");
    for (a, b) in moves.iter().zip(&blind.moves) {
        assert!(a.kind == b.kind && (a.to - b.to).dot(a.to - b.to) < 1e-18, "the path moved: {a:?} vs {b:?}");
    }
    assert!(notes.iter().any(|n| n.contains("no setup stock")), "the part-based approach is not named: {notes:?}");
}
