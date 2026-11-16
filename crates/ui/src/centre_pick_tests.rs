//! #692 review: the UI point takers read a round edge as its centre - the
//! three-points draft (`points_pick::take`) refuses a straight edge with
//! nothing drafted, drafts a round one as a point and refuses a fourth;
//! an axis by two points drafts a round edge too; and the `picks_text`
//! readout shows a centre pick's coordinates, not an edge's.
//!
//! Provenance: our own fixture (a square plate with a round through hole)
//! and assertions; no vendor material.

use camcraft_app::commands::ImportModel;
use camcraft_app::geometry::{Mesh, Point3, Triangle};
use camcraft_app::operation::{FeatureSel, GeoSel};
use camcraft_app::pick::{self, InPlaneAxis};
use camcraft_app::units::Unit;

use crate::axis_pick::AxisBy;
use crate::state::State;

const TOP: f64 = 10.0;
const HALF: f64 = 20.0;
const SEGS: usize = 32;
const CX: f64 = 5.0;
const CY: f64 = -3.0;
const R: f64 = 4.0;

/// A `2*HALF` square plate, z 0..TOP, centred on (CX, CY), with a round
/// through hole of radius `R` in [`SEGS`] chords.
fn holed_plate() -> Mesh {
    let ang = |k: usize| std::f64::consts::TAU * (k % SEGS) as f64 / SEGS as f64;
    let outer = |k: usize, z: f64| {
        let (s, c) = ang(k).sin_cos();
        let m = HALF / c.abs().max(s.abs());
        Point3::new(CX + m * c, CY + m * s, z)
    };
    let inner = |k: usize, z: f64| {
        let (s, c) = ang(k).sin_cos();
        Point3::new(CX + R * c, CY + R * s, z)
    };
    let mut f = Vec::new();
    let mut quad = |a: Point3, b: Point3, c: Point3, d: Point3| {
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    };
    for k in 0..SEGS {
        let n = k + 1;
        quad(outer(k, TOP), outer(n, TOP), inner(n, TOP), inner(k, TOP));
        quad(inner(k, 0.0), inner(n, 0.0), outer(n, 0.0), outer(k, 0.0));
        quad(outer(k, 0.0), outer(n, 0.0), outer(n, TOP), outer(k, TOP));
        quad(inner(k, TOP), inner(n, TOP), inner(n, 0.0), inner(k, 0.0));
    }
    Mesh::from_facets(f)
}

fn plate_state() -> State {
    let mut state = State::new();
    state.history.execute(&state.world, ImportModel::from_mesh("plate", holed_plate()));
    state
}

/// The run of the plate whose every vertex satisfies `on`, as an edge pick.
fn run_where(state: &State, on: impl Fn([f64; 3]) -> bool) -> GeoSel {
    let g = pick::graph_of(&state.world, "plate").expect("graph");
    let run = g
        .runs()
        .iter()
        .find(|r| {
            r.edges.iter().all(|&e| g.edge_ends(e).is_some_and(|(a, b)| on(a) && on(b)))
                && r.edges.iter().all(|e| g.edges.binary_search(e).is_ok())
        })
        .map(|r| r.id)
        .expect("a run there");
    GeoSel { entity: "plate".into(), feature: FeatureSel::Chain(run) }
}

fn rim_at(z: f64) -> impl Fn([f64; 3]) -> bool {
    move |p| (p[2] - z).abs() < 1e-9 && ((p[0] - CX).hypot(p[1] - CY) - R).abs() < 1e-6
}

/// The plate's straight top side at y = CY - HALF.
fn top_front_side(p: [f64; 3]) -> bool {
    (p[2] - TOP).abs() < 1e-9 && (p[1] - (CY - HALF)).abs() < 1e-6
}

fn corner(state: &State, at: [f64; 3]) -> GeoSel {
    let g = pick::graph_of(&state.world, "plate").expect("graph");
    let p = g
        .points
        .iter()
        .find(|p| p.at.iter().zip(at).all(|(a, b)| (a - b).abs() < 1e-6))
        .expect("a corner there");
    GeoSel { entity: "plate".into(), feature: FeatureSel::Vertex(p.id) }
}

fn drafted(state: &State) -> Vec<GeoSel> {
    state.points_pick.as_ref().map(|d| d.picks.clone()).unwrap_or_default()
}

#[test]
fn three_points_draft_a_round_edge_as_its_centre_and_refuse_a_straight_one() {
    let mut state = plate_state();
    crate::points_pick::arm(&mut state, "Setup1");
    // A straight edge gives no centre: refused by name, nothing drafted.
    let side = run_where(&state, top_front_side);
    crate::points_pick::take(&mut state, side);
    assert!(state.status.contains("is an edge with no centre") && state.status.contains("straight"), "{}", state.status);
    assert!(drafted(&state).is_empty(), "a straight edge is not drafted");
    // A round edge is drafted as a point (its centre).
    let rim = run_where(&state, rim_at(TOP));
    crate::points_pick::take(&mut state, rim.clone());
    assert_eq!(drafted(&state), vec![rim.clone()], "{}", state.status);
    assert!(state.status.contains("point 1 picked"), "{}", state.status);
    for p in [[CX + HALF, CY + HALF, TOP], [CX - HALF, CY + HALF, TOP]] {
        let c = corner(&state, p);
        crate::points_pick::take(&mut state, c);
    }
    assert_eq!(drafted(&state).len(), 3, "{}", state.status);
    // A fourth point - here another round edge's centre - is refused.
    let bottom = run_where(&state, rim_at(0.0));
    crate::points_pick::take(&mut state, bottom);
    assert_eq!(drafted(&state).len(), 3, "a fourth point is refused");
    assert_eq!(drafted(&state)[0], rim, "the draft is unchanged");
    assert!(state.status.contains("Remove last or Clear"), "{}", state.status);
}

#[test]
fn an_axis_by_two_points_drafts_a_round_edge_and_reads_out_its_centre() {
    let mut state = plate_state();
    crate::axis_pick::set_by(&mut state, "Setup1", InPlaneAxis::Y, AxisBy::Points);
    let side = run_where(&state, top_front_side);
    crate::axis_pick::take(&mut state, side);
    let picks = |s: &mut State| s.axis_pick.as_mut().map(|d| d.slot(InPlaneAxis::Y).picks.clone()).unwrap_or_default();
    assert!(picks(&mut state).is_empty(), "a straight edge is no point: {}", state.status);
    let rim = run_where(&state, rim_at(TOP));
    crate::axis_pick::take(&mut state, rim.clone());
    assert_eq!(picks(&mut state), vec![rim.clone()], "{}", state.status);
    // The readout names the centre's coordinates, as a point - not an edge.
    let text = crate::axis_pick::picks_text(&state.world, &[rim], Unit::Millimeters);
    assert_eq!(text, "1 of 2 points on 'plate' (5.000, -3.000, 10.000) mm");
}
