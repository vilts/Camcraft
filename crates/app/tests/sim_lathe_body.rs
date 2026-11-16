//! #1038: the simulated tool of a turning op is its lathe body (#369),
//! drawn as modelled and posed in the frame the holder law judges it in:
//! the tip at the move's point, +x along the path's signed radial, +z
//! along the spindle - so every drawn vertex stands where the law reads
//! it, sqrt((X + x)^2 + y^2) from the axis at station S + z. OD and ID
//! sides both; a lathe steel without a body draws nothing (the axis
//! marker, as before). Our own tests.
//!
//! Gate map (M4.4): covers app::sim_lathe_body, app::sim_tool_body
//! (its lathe arm), operation::turning_holder::body_axes.
//! Lane shape (M4.4): all tests here run in seconds.

mod common;

use flecs_ecs::prelude::World;

use camcraft_app::commands::{CreateOperation, GenerateOperation, History, ImportModel, LatheBodySpec, LoadMachine, SetToolBody};
use camcraft_app::geometry::{Point3, Triangle};
use camcraft_app::operation::{Height, OpParams, SrcRef};
use camcraft_app::sim_lathe_body::{self as lathe, LatheAxes};
use camcraft_app::sim_tool_body::SimToolCache;
use camcraft_app::{model, tool_body};
use camcraft_tools::lathe_holder::LathePlacement;

use common::bore::*;

const INSERT: &str = "TN-1600-0R8";
const TOL: f64 = 1e-6;

/// A closed axis-aligned box over `[lo, hi]`, 12 facets.
fn cuboid(lo: [f64; 3], hi: [f64; 3]) -> Vec<Triangle> {
    let c = |i: usize| Point3::new(if i & 1 == 0 { lo[0] } else { hi[0] }, if i & 2 == 0 { lo[1] } else { hi[1] }, if i & 4 == 0 { lo[2] } else { hi[2] });
    let quads = [[0, 1, 3, 2], [4, 6, 7, 5], [0, 4, 5, 1], [2, 3, 7, 6], [0, 2, 6, 4], [1, 5, 7, 3]];
    quads.iter().flat_map(|q| [Triangle::new(c(q[0]), c(q[1]), c(q[2])), Triangle::new(c(q[0]), c(q[2]), c(q[3]))]).collect()
}

fn body(source: &str, facets: Vec<Triangle>) -> LatheBodySpec {
    let place = LathePlacement { radial: [1.0, 0.0, 0.0], along: [0.0, 0.0, 1.0], tip: [0.0, 0.0, 0.0] };
    LatheBodySpec { source: source.into(), facets, place, zone: None }
}

/// The stepped part on a lathe with the insert racked, and a finish
/// ride down its r 6 neck (turning_holder.rs's OD document).
fn od_world() -> (World, History) {
    let world = camcraft_app::world::new_world();
    let mut history = History::default();
    history.execute(&world, LoadMachine::new(&world, "lathe.mch".into(), Some(machine("lathe"))).expect("lathe"));
    add_tool(&world, &mut history, INSERT);
    let part = revolve(&[(0.0, -16.0), (12.0, -16.0), (12.0, 0.0), (6.0, 0.0), (6.0, 20.0), (0.0, 20.0)]);
    history.execute(&world, ImportModel::from_mesh("step", part));
    let finish = OpParams::ProfileFinish {
        tolerance: 0.05,
        clearance_z: Height::fixed(30.0),
        top_z: Height::fixed(20.0),
        bottom_z: Height::fixed(2.0),
        feed: 150.0,
        plunge_feed: 80.0,
    };
    history.execute(&world, CreateOperation::new(&world, "F1", INSERT, SrcRef::Entity("step".into()), finish).expect("create"));
    history.execute(&world, GenerateOperation::new(&world, "F1").expect("generate"));
    (world, history)
}

fn regen(world: &World, history: &mut History, op: &str) {
    history.execute(world, GenerateOperation::new(world, op).expect("regenerate"));
}

fn p3(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}

/// The tool-frame facets the law judges (the tool's own body).
fn judged_facets(world: &World, tool: &str) -> Vec<Triangle> {
    let te = model::find_by_name(world, tool).expect("tool");
    tool_body::body_of(world, &te).expect("a body").facets.to_vec()
}

/// The path's half plane about the part's axis (world Z through the
/// origin): the first point off the axis, as the law signs radii.
fn path_radial(world: &World, op: &str) -> [f64; 3] {
    let c = cache(world, op).expect("cached");
    let q = c.path.moves.iter().map(|m| m.from).find(|q| rho(*q) > 1e-6).expect("a point off the axis");
    [q.x / rho(q), q.y / rho(q), 0.0]
}

/// Every drawn vertex is its tool-frame vertex posed at `tip`: +x along
/// the path's radial, +z up the spindle, and - the law's own reading -
/// sqrt((X + x)^2 + y^2) from the axis at station S + z.
fn assert_posed_as_judged(world: &World, op: &str, tool: &str, tip: Point3) -> Vec<Triangle> {
    let mut cache = SimToolCache::default();
    let drawn = cache.posed(world, op, p3(tip), [0.0, 0.0, 1.0]).expect("the lathe body is drawn");
    let local = judged_facets(world, tool);
    assert_eq!(drawn.len(), local.len(), "drawn as modelled: one facet per body facet, not revolved");
    let x = path_radial(world, op);
    let signed = if tip.x * x[0] + tip.y * x[1] < 0.0 { -rho(tip) } else { rho(tip) };
    for (d, l) in drawn.iter().zip(&local) {
        for (w, p) in [(d.a, l.a), (d.b, l.b), (d.c, l.c)] {
            let r = [w.x - tip.x, w.y - tip.y, w.z - tip.z];
            assert!((r[0] * x[0] + r[1] * x[1] - p.x).abs() < TOL, "+x runs along the path's radial: {w:?} from {p:?}");
            assert!((r[2] - p.z).abs() < TOL, "+z runs up the spindle: {w:?} from {p:?}");
            let law = ((signed + p.x).powi(2) + p.y * p.y).sqrt();
            assert!((rho(w) - law).abs() < TOL, "the vertex stands {} from the axis, the law reads {law}", rho(w));
        }
    }
    drawn
}

#[test]
fn an_od_holder_is_drawn_at_the_tip_radially_out_up_the_spindle() {
    let (world, mut history) = od_world();
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(body("od.step", cuboid([2.0, -6.0, 0.5], [40.0, 6.0, 10.0])))).expect("a sane holder"));
    regen(&world, &mut history, "F1");
    let path = cache(&world, "F1").expect("cached").path;
    for m in path.moves.iter().step_by((path.moves.len() / 5).max(1)) {
        let drawn = assert_posed_as_judged(&world, "F1", INSERT, m.to);
        // The OD shank stands outward: its far end 40 past the tip.
        let far = drawn.iter().flat_map(|t| [t.a, t.b, t.c]).map(rho).fold(0.0, f64::max);
        assert!(far > rho(m.to) + 39.0, "the shank reaches out from the work: {far} at tip r {}", rho(m.to));
    }
}

#[test]
fn an_id_bar_is_drawn_toward_the_axis_up_the_bore() {
    let (world, mut history) = bore_world();
    pilot(&world, &mut history);
    create(&world, &mut history, "BR1", bore_params());
    history.execute(&world, SetToolBody::lathe(&world, "BR-1000", Some(body("slim.stp", cuboid([-4.0, -1.5, 1.0], [-0.5, 1.5, 80.0])))).expect("a sane bar"));
    regen(&world, &mut history, "BR1");
    let path = cache(&world, "BR1").expect("cached").path;
    let ride = path.moves.iter().filter(|m| rho(m.to) > 4.0).max_by(|a, b| rho(a.to).total_cmp(&rho(b.to))).expect("a move on the wall");
    let drawn = assert_posed_as_judged(&world, "BR1", "BR-1000", ride.to);
    // The bar stands inside the tip's radius, all of it above the tip.
    let pts: Vec<Point3> = drawn.iter().flat_map(|t| [t.a, t.b, t.c]).collect();
    // At its ±1.5 flats the bar's x -0.5 edge reaches
    // hypot(X - 0.5, 1.5), still inside the tip's radius X.
    assert!(pts.iter().all(|p| rho(*p) < rho(ride.to)), "the bar runs toward the axis");
    let near = pts.iter().map(|p| rho(*p)).fold(f64::INFINITY, f64::min);
    let x = rho(ride.to);
    assert!((near - (x - 4.0).hypot(1.5)).abs() < TOL, "its far corners 4 in from the tip: {near} at tip r {x}");
    assert!(pts.iter().all(|p| p.z >= ride.to.z + 1.0 - TOL) && pts.iter().any(|p| (p.z - ride.to.z - 80.0).abs() < TOL), "up the bore");
}

#[test]
fn a_turning_tool_without_a_lathe_body_draws_as_before() {
    let (world, _history) = od_world();
    let mut cache = SimToolCache::default();
    let tip = p3(cache_tip(&world, "F1"));
    assert!(cache.posed(&world, "F1", tip, [0.0, 0.0, 1.0]).is_none(), "the bare insert keeps the axis marker only");
    assert!(cache.is_empty() && cache.refusal().is_none());
    assert!(lathe::lathe_body(&world, INSERT).is_none());
}

fn cache_tip(world: &World, op: &str) -> Point3 {
    cache(world, op).expect("cached").path.moves.first().expect("a move").to
}

#[test]
fn a_new_body_is_drawn_at_once_and_the_frame_freed_with_the_transport() {
    let (world, mut history) = od_world();
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(body("a", cuboid([2.0, -6.0, 0.5], [40.0, 6.0, 10.0])))).expect("a sane holder"));
    regen(&world, &mut history, "F1");
    let mut cache = SimToolCache::default();
    let tip = p3(cache_tip(&world, "F1"));
    let first = cache.posed(&world, "F1", tip, [0.0, 0.0, 1.0]).expect("drawn");
    assert_eq!(cache.posed(&world, "F1", tip, [0.0, 0.0, 1.0]).as_deref(), Some(first.as_slice()), "reused");
    let mut two = cuboid([2.0, -6.0, 0.5], [30.0, 6.0, 10.0]);
    two.extend(cuboid([2.0, -3.0, 10.0], [20.0, 3.0, 30.0]));
    history.execute(&world, SetToolBody::lathe(&world, INSERT, Some(body("b", two))).expect("a sane holder"));
    let next = cache.posed(&world, "F1", tip, [0.0, 0.0, 1.0]).expect("drawn");
    assert_eq!(next.len(), 24, "the new body, not the cached one");
    assert!(!cache.is_empty());
    cache.clear();
    assert!(cache.is_empty());
}

#[test]
fn the_pose_maps_each_tool_axis_onto_its_world_direction() {
    // A tool frame whose +x is world +Y and +z world +Z: +y = z x x is world -X.
    let axes = LatheAxes::new([0.0, 2.0, 0.0], [0.0, 0.0, 3.0]).expect("square");
    assert_eq!(axes.normal(), [-1.0, 0.0, 0.0]);
    let tri = Triangle::new(Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0), Point3::new(1.0, 2.0, 3.0));
    let out = lathe::posed(&[tri], [5.0, 6.0, 7.0], &axes);
    let got: Vec<[f64; 3]> = [out[0].a, out[0].b, out[0].c].into_iter().map(p3).collect();
    assert_eq!(got, vec![[5.0, 7.0, 7.0], [4.0, 6.0, 7.0], [3.0, 7.0, 10.0]]);
    // No frame, no pose: zero, non-finite and skew pairs are refused.
    assert!(LatheAxes::new([0.0; 3], [0.0, 0.0, 1.0]).is_none());
    assert!(LatheAxes::new([f64::NAN, 0.0, 0.0], [0.0, 0.0, 1.0]).is_none());
    assert!(LatheAxes::new([1.0, 0.0, 1.0], [0.0, 0.0, 1.0]).is_none());
}
