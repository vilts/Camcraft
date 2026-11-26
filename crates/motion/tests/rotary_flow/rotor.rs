//! The rotor laws: the groove fixture (a rotor's flute) flows
//! multi-turn without gouge, a pair that straddles the azimuth branch
//! aligns by a whole turn, climb reverses each pass, and the fine
//! rotor's envelope stays clear at the scallop law (heavy lane).
//!
//! The groove's width runs AXIALLY - its two root edge helices answer
//! for `v = k*phi` and `v = k*phi + W_v` at the same azimuth - with
//! the steep helical flanks rising radially at the two v-edges. That
//! is the ground a radially posed ball can ride: a groove whose edges
//! were screw-shifted copies of one helix would be one ramp, not a
//! band.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::multiaxis::{rotary_flow, AxisControl};

use crate::fixture::*;

#[test]
fn a_rotor_floor_band_flows_multi_turn_without_gouge() {
    let facets = groove(N_CIRC, TURNS, 0.0);
    let chains = root_pair(N_CIRC, TURNS, 0.0);
    let tool = ball(2.0);
    // 0.9 mm keeps the count clear of 1-ulp ceiling dust at a whole
    // number of spans (4.0 / 0.9 = 4.44 spans: 6 passes).
    let path = rotary_flow(
        &chains,
        &facets,
        &stated_axis(),
        &params(&tool, Some(0.9)),
        &AxisControl::default(),
    )
    .unwrap();
    no_gouge(&path, &facets, 1.0);
    // The developed band is the groove's 4.0 mm axial width: 6
    // passes at 0.9 mm.
    let plunges = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .count();
    assert_eq!(plunges, 6, "the band did not divide into 6 passes: {:?}", path.notes);
    assert!(
        path.notes.iter().any(|n| n.contains("6 flow passes")),
        "the notes do not state the band: {:?}",
        path.notes
    );
    // The passes run MULTI-TURN: the first pass's centers wind the
    // groove's whole three turns about the stated axis.
    let centers = first_pass_centers(&path, 14.0, 1.0);
    assert!(
        centers.len() > 10,
        "the first pass states {} stations",
        centers.len()
    );
    let span = wound(&centers);
    assert!(
        (span - TAU * TURNS as f64).abs() < 0.3,
        "the first pass winds {span:.3} rad, not the groove's {:.3}",
        TAU * TURNS as f64
    );
    // The first pass is the WALL-ADJACENT one: its picked edge lies
    // IN a wall plane, so the radial push-out carries it up the
    // steep flank (the issue's own law). An interior pass rides the
    // floor a ball radius off it, untouched by any wall.
    let (_, rho0) = cyl(centers[0]);
    assert!(
        rho0 > R_ROOT + 0.5,
        "the wall-adjacent pass rides at {rho0:.3}, not pushed out"
    );
    let floors = path
        .moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .filter_map(|m| center_of(m, 1.0))
        .filter(|c| {
            let (_, rho) = cyl(*c);
            rho > R_ROOT + 0.5 && rho < R_ROOT + 1.2
        })
        .count();
    assert!(floors > 10, "no pass rides the floor: {floors} stations");
}

#[test]
fn a_pair_straddling_the_branch_aligns_by_a_whole_turn() {
    // The oblique band, phased so the second chain's raw azimuth
    // straddles the kernel basis' own branch: without the
    // whole-turn alignment the pair would read its width plus a
    // full lap. The pass count is the unshifted band's own.
    let (wb, n) = (0.4f64, 72usize);
    let width = (R_ROOT * wb).hypot(PITCH * wb / TAU + W_V);
    let step = 1.0;
    let want = (width / step).ceil() as usize + 1;
    let straight = {
        let facets = ruled_band(wb, n, 0.0);
        let chains = band_pair(wb, n, 0.0);
        let tool = ball(2.0);
        rotary_flow(&chains, &facets, &stated_axis(), &params(&tool, Some(step)), &AxisControl::default())
            .unwrap()
    };
    let straddle = {
        let phase = 4.5; // B's raw azimuth crosses the kernel basis' own branch
        let facets = ruled_band(wb, n, phase);
        let chains = band_pair(wb, n, phase);
        let tool = ball(2.0);
        let path = rotary_flow(&chains, &facets, &stated_axis(), &params(&tool, Some(step)), &AxisControl::default())
            .unwrap();
        no_gouge(&path, &facets, 1.0);
        no_swept_gouge(&path, &facets, 1.0);
        path
    };
    let count = |p: &Toolpath| p.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert_eq!(count(&straight), want, "the straight band divided wrong");
    assert_eq!(
        count(&straddle),
        want,
        "the straddling pair did not align: {} passes, not {want}",
        count(&straddle)
    );
    assert!(
        straddle.notes.iter().any(|n| n.contains("of whole-turn alignment")),
        "the notes do not state the alignment: {:?}",
        straddle.notes
    );
}

#[test]
fn climb_reverses_each_pass() {
    // The same band walked backwards: the same stations, the same
    // clearance, the travel reversed - a reversed pass opens at the
    // forward pass's end.
    let facets = groove(N_CIRC, TURNS, 0.0);
    let chains = root_pair(N_CIRC, TURNS, 0.0);
    let tool = ball(2.0);
    let mut q = params(&tool, Some(1.0));
    q.climb = false;
    let back = rotary_flow(&chains, &facets, &stated_axis(), &q, &AxisControl::default()).unwrap();
    let fwd = rotary_flow(
        &chains,
        &facets,
        &stated_axis(),
        &params(&tool, Some(1.0)),
        &AxisControl::default(),
    )
    .unwrap();
    no_gouge(&back, &facets, 1.0);
    let f = first_pass_centers(&fwd, 14.0, 1.0);
    let b = first_pass_centers(&back, 14.0, 1.0);
    let (Some(x), Some(y)) = (b.first().copied(), f.last().copied()) else {
        panic!("a pass stated no station");
    };
    assert!(
        (x.x - y.x).abs() < 0.2 && (x.y - y.y).abs() < 0.2,
        "the reversed pass opens at {x:?}, not the forward pass's end {y:?}"
    );
}

/// The full-envelope scan: a finer, longer rotor at the scallop
/// law's own stepover, every station AND every swept move judged
/// against every facet.
#[test]
#[ignore = "heavy lane: release --ignored (the 4-turn fine rotor envelope scanned station by station, minutes in debug)"]
fn the_rotor_envelope_stays_gouge_free_at_the_scallop_law() {
    let (n_circ, turns) = (120usize, 4usize);
    let facets = groove(n_circ, turns, 0.0);
    let chains = root_pair(n_circ, turns, 0.0);
    let tool = ball(2.0);
    let path = rotary_flow(
        &chains,
        &facets,
        &stated_axis(),
        &params(&tool, None),
        &AxisControl::default(),
    )
    .unwrap();
    no_gouge(&path, &facets, 1.0);
    no_swept_gouge(&path, &facets, 1.0);
    // The scallop law's own count: sqrt(8 * r * tol) = 0.63 mm over
    // the 4.0 mm band, ceilinged to whole spans.
    let plunges = path
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Plunge)
        .count();
    let step = (8.0 * 1.0 * 0.05f64).sqrt();
    let want = (W_V / step).ceil() as usize + 1;
    assert_eq!(plunges, want, "the scallop law did not divide the band");
}
