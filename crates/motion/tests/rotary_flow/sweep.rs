//! The swept-ball and approach proofs (review of PR #3630): safe
//! stations do not prove a safe cut. Every emitted move - the flow
//! chords, the plunges, the retracts, the links and the exit - is
//! judged as a swept ball, and a move that no proof clears refuses
//! the program by name.

use camcraft_motion::cl::MoveKind;
use camcraft_motion::geometry::Triangle;
use camcraft_motion::multiaxis::{rotary_flow, AxisControl};

use crate::fixture::*;

#[test]
fn every_move_of_the_rotor_sweeps_the_ball_clear_of_the_part() {
    // The review's own fixture: the 72-station, three-turn rotor. Each
    // endpoint passed the station pin before, yet the chord between
    // two pushed-out stations dipped 0.0065 mm into the floor.
    let facets = groove(N_CIRC, TURNS, 0.0);
    let chains = root_pair(N_CIRC, TURNS, 0.0);
    let tool = ball(2.0);
    let path = rotary_flow(
        &chains,
        &facets,
        &stated_axis(),
        &params(&tool, Some(0.9)),
        &AxisControl::default(),
    )
    .unwrap();
    no_swept_gouge(&path, &facets, 1.0);
    // The pin reaches the cut: the swept moves include the flow
    // chords, not only the links.
    let feeds = path.moves.iter().filter(|m| m.kind == MoveKind::Feed).count();
    assert!(feeds > 1000, "the pin judged only {feeds} feed moves");
}

#[test]
fn a_radial_plunge_through_an_overhang_is_refused_by_name() {
    // The review's reproduction: a facet hangs over the first flow
    // station, between it and the 16 mm clearance cylinder. The radial
    // plunge from (16, 0, 0) toward (11, 0, 0) runs through it.
    let mut facets = groove(N_CIRC, TURNS, 0.0);
    facets.push(Triangle::new(
        p3(13.5, -0.5, -0.5),
        p3(13.5, 0.5, -0.5),
        p3(13.5, 0.0, 0.5),
    ));
    let chains = root_pair(N_CIRC, TURNS, 0.0);
    let tool = ball(2.0);
    let mut q = params(&tool, Some(0.9));
    q.clearance = 16.0;
    let got = rotary_flow(&chains, &facets, &stated_axis(), &q, &AxisControl::default());
    let e = match got {
        Ok(path) => panic!(
            "the kernel posted a path through the overhang: {} moves",
            path.moves.len()
        ),
        Err(e) => e,
    };
    let msg = format!("{e}");
    assert!(
        msg.contains("sweeps the ball") && msg.contains("plunge"),
        "the refusal does not name the unsafe plunge: {msg}"
    );
    let at = e.location().expect("the refusal names its spot");
    assert!(
        at.x > 8.0 && at.x < 13.5 && at.y.abs() < 1.0,
        "the refusal stands at {at:?}, not under the overhang"
    );
}
