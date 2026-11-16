//! Unit tests of the pass-wide evaluation budget (#1599): a shell with
//! many runaway faces stops after a few trips, not after every face.

use super::super::step_guard::{Guarded, Trip};
use super::*;
use std::sync::atomic::Ordering;
use truck_meshalgo::prelude::RobustMeshableShape;

/// The `bspline_discontinuous.step` shell with its jumping face (face 6)
/// copied `extra` more times: a hostile shell of many runaway faces.
fn many_runaways(extra: usize, cap: u64) -> Guarded {
    let text = include_str!("../../../../../testdata/models/bspline_discontinuous.step");
    let exchange = truck_stepio::r#in::ruststep::parser::parse(text).expect("fixture parses");
    let table = truck_stepio::r#in::Table::from_data_section(&exchange.data[0]);
    let shell = table.shell.values().next().expect("one shell");
    let mut cshell = table.to_compressed_shell(shell).expect("converts");
    let jump = cshell.faces[5].clone();
    cshell.faces.extend(std::iter::repeat_n(jump, extra));
    Guarded::new(cshell, cap)
}

fn spent(g: &Guarded) -> u64 {
    g.meters().iter().map(|m| m.used.load(Ordering::Relaxed)).sum()
}

/// #1599: the evaluations of one pass do not grow with the number of
/// runaway faces. Every face in flight when the pass stops ends at once,
/// so the pass spends at most `PASS_TRIPS` caps for the faces that
/// tripped plus less than one cap for each worker thread.
#[test]
fn a_pass_with_many_runaway_faces_stops_after_a_few_trips() {
    let cap = 200_000;
    let threads = rayon::current_num_threads() as u64;
    let extra = (2 * (PASS_TRIPS + threads) + 8) as usize;
    let g = many_runaways(extra, cap);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.shell.robust_triangulation(0.01)));
    let used = spent(&g);
    let bound = (PASS_TRIPS + threads) * (cap + 1);
    assert!(used <= bound, "{} runaway faces spent {used} evaluations, bound {bound}", extra + 1);
    assert!(g.stopped(), "the pass is marked stopped");
    let trip = g.tripped().expect("a stopped pass is refused");
    assert!(!trip.continuous, "a stopped pass never keeps a coarser mesh: {trip:?}");
    assert_eq!(trip.kind, "B-spline surface", "the refusal names a face that ran out, not a stopped one");
    // A new pass starts with the whole budget again.
    g.reset();
    assert!(!g.stopped() && g.tripped().is_none());
}

/// #1599: under `PASS_TRIPS` trips nothing changes - the one runaway face of
/// the fixture trips alone and every other face divides to the end.
#[test]
fn a_pass_with_one_runaway_face_is_not_stopped() {
    let g = many_runaways(0, 200_000);
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| g.shell.robust_triangulation(0.01)));
    assert!(!g.stopped(), "one trip must not stop the pass");
    assert_eq!(g.tripped(), Some(Trip { index: 5, kind: "B-spline surface", continuous: false }));
}

/// #1599: a trip counts once per meter, whatever way the meter was
/// refused, and the pass stops at exactly `PASS_TRIPS`.
#[test]
fn the_pass_stops_at_the_trip_limit_and_a_stopped_meter_is_not_a_trip() {
    let pass = std::sync::Arc::new(Pass::default());
    let meters: Vec<Meter> = (0..PASS_TRIPS + 1).map(|_| Meter::in_pass(3, std::sync::Arc::clone(&pass))).collect();
    for m in meters.iter().take(PASS_TRIPS as usize - 1) {
        m.exhaust();
        m.exhaust();
        assert!(!m.spend_many(5), "a tripped meter spends no more");
    }
    assert!(!pass.stopped(), "{} trips do not stop the pass", PASS_TRIPS - 1);
    let last = &meters[PASS_TRIPS as usize - 1];
    assert!(!last.spend_many(4));
    assert!(pass.stopped(), "the trip that reaches the limit stops the pass");
    let healthy = &meters[PASS_TRIPS as usize];
    assert!(!healthy.spend_many(1) && !healthy.spend(), "a stopped pass evaluates nothing more");
    healthy.exhaust();
    healthy.flag(super::super::step_nearest::Fault::Grid);
    assert!(!healthy.refused(), "a meter stopped by the pass is not reported as a runaway");
    assert_eq!(healthy.used.load(Ordering::Relaxed), 0);
}

/// #1599: proven faces (the fixture's planes) that trip until the pass
/// stops are not a continuous trip: the faces the stop cut short were
/// never judged, so no coarser mesh may be kept for the pass.
#[test]
fn a_stopped_pass_of_proven_faces_is_not_continuous() {
    let g = many_runaways(0, 1_000);
    g.meters()[0].exhaust();
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "elementary surface", continuous: true }), "one plane alone");
    for m in g.meters().iter().take(PASS_TRIPS as usize) {
        m.exhaust();
    }
    assert!(g.stopped());
    assert_eq!(g.tripped(), Some(Trip { index: 0, kind: "elementary surface", continuous: false }));
}

/// #1599: the refusal of a stopped pass says the tessellation stopped and
/// that more of the model may be broken; any other refusal is unchanged.
#[test]
fn the_refusal_of_a_stopped_pass_says_it_stopped() {
    let why = || ImportError::Step("face #9 could not be tessellated".into());
    let e = noted(true, why()).to_string();
    assert!(e.contains("face #9 could not be tessellated"), "{e}");
    assert!(e.contains(&format!("stopped after {PASS_TRIPS} faces or edges")), "{e}");
    assert_eq!(noted(false, why()).to_string(), why().to_string());
}

/// #1599: an edge curve's division spends the same pass budget as the
/// faces (#1545's curve meter): an edge trip counts, and a stopped pass
/// stops the edges too.
#[test]
fn edge_trips_count_against_the_pass_budget() {
    let g = many_runaways(0, 1_000);
    let edges = &g.shell.edges;
    assert!(edges.len() >= PASS_TRIPS as usize, "the fixture has enough edges");
    for e in edges.iter().take(PASS_TRIPS as usize) {
        e.curve.meter.exhaust();
    }
    assert!(g.stopped(), "edge trips stop the pass");
    assert!(g.meters().iter().all(|m| !m.spend_many(1)), "faces of a stopped pass evaluate nothing");
}
