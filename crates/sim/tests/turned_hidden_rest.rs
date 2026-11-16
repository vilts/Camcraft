//! #1408: the envelope scan's hidden-rest half on a turning nose path,
//! read as the turning work sees it (`stock_sim::envelope_scan_turned`).
//! What "right" means: a groove floor out of the band the noses travel
//! is no rest of the pass; given the spindle, a hidden floor the nose
//! CAN reach inside its band still convicts (leave or none), the gouge
//! half is untouched, and without a spindle the scan is bit for bit
//! the old one.
//!
//! Provenance: our own fixtures - the issue's 64-segment revolves -
//! specced from the Prime Directive (a false clean is a gouge-class
//! bug); no vendor material.
//!
//! Gate map (M4.4): covers camcraft_sim::stock_sim (scan, scan_hidden,
//! scan_turned), camcraft_sim::verify (finish, law (f)).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::Region;
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::stock_sim::{self, LatheAxis};
use camcraft_sim::verify::{self, Mode, VerifyContext, Warning};

const TAU: f64 = std::f64::consts::TAU;
const NOSE: f64 = 0.8;
const TOL: f64 = 0.05;

/// The issue's groove bar: r 10, z 0..48, a groove r 5.5 over z 34..37.
const GROOVE: &[(f64, f64)] = &[
    (0.0, 0.0), (10.0, 0.0), (10.0, 34.0), (5.5, 34.0), (5.5, 37.0), (10.0, 37.0), (10.0, 48.0), (0.0, 48.0),
];

fn p(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

fn feed(from: Point3, to: Point3) -> Move {
    Move { kind: MoveKind::Feed, from, to, feed: Some(150.0), arc: None, axis: None, rotary: None }
}

fn revolve(meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 64.0;
        p(r * th.cos(), r * th.sin(), z)
    };
    let mut facets = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..64 {
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

/// The Facing's last station: the nose centre across the end face at
/// z 48 + r, inward to the axis, in the y = 0 meridian.
fn facing_pass() -> Vec<Move> {
    vec![feed(p(10.8, 0.0, 48.8), p(0.0, 0.0, 48.8))]
}

/// A nose that rides the OD down past the groove without entering it:
/// its band (z 30..48.8) claims the groove floor at z 34, which the
/// nose could reach (the groove is 3 mm wide) and did not.
fn bridging_pass() -> Vec<Move> {
    vec![feed(p(0.0, 0.0, 48.8), p(10.8, 0.0, 48.8)), feed(p(10.8, 0.0, 48.8), p(10.8, 0.0, 30.0))]
}

#[test]
fn a_groove_floor_under_the_facing_band_is_not_its_rest() {
    let f = revolve(GROOVE);
    let old = stock_sim::envelope_scan_centers(&f, &facing_pass(), NOSE, TOL).expect("scan");
    assert!(old.rest_mm > 15.0, "the unturned reading convicts the far rim: {}", old.rest_mm);
    let turned = stock_sim::envelope_scan_turned(&f, &facing_pass(), NOSE, TOL, (Some(LatheAxis::Z), 0.0)).expect("scan");
    let gate = TOL + turned.cell * turned.cell / (2.0 * NOSE);
    assert!(turned.rest_mm <= gate, "out of the band, out of the verdict: {}", turned.rest_mm);
    assert_eq!(turned.gouge_mm.to_bits(), old.gouge_mm.to_bits(), "the gouge half never moves");
}

#[test]
fn a_hidden_floor_the_nose_could_reach_inside_its_band_still_convicts() {
    // The floor at r 6.3..10 stands 0.8 + ~3.7 off the OD ride: real
    // owed rest in the pass's own band, with the spindle given, with
    // the rough's leave given too (the leave forgives 0.25, not 3.7).
    let f = revolve(GROOVE);
    for leave in [0.0, 0.25] {
        let rep = stock_sim::envelope_scan_turned(&f, &bridging_pass(), NOSE, TOL, (Some(LatheAxis::Z), leave)).expect("scan");
        assert!(rep.rest_mm > 3.0, "leave {leave}: the bridged groove floor is the pass's rest: {}", rep.rest_mm);
    }
}

#[test]
fn without_a_spindle_the_scan_is_the_old_one_bit_for_bit() {
    let f = revolve(GROOVE);
    for moves in [facing_pass(), bridging_pass()] {
        let old = stock_sim::envelope_scan_centers(&f, &moves, NOSE, TOL).expect("scan");
        let none = stock_sim::envelope_scan_turned(&f, &moves, NOSE, TOL, (None, 0.25)).expect("scan");
        assert_eq!(old.rest_mm.to_bits(), none.rest_mm.to_bits());
        assert_eq!(old.gouge_mm.to_bits(), none.gouge_mm.to_bits());
        assert_eq!((old.at.x.to_bits(), old.at.y.to_bits()), (none.at.x.to_bits(), none.at.y.to_bits()));
    }
}

/// A nose that rides the OD down from the end face and then goes into
/// the groove at centre station `z`, in to centre radius `r` - the
/// review's probe on PR #1429.
fn into_the_groove(z: f64, r: f64) -> Vec<Move> {
    vec![feed(p(10.8, 0.0, 48.8), p(10.8, 0.0, z)), feed(p(10.8, 0.0, z), p(r, 0.0, z))]
}

#[test]
fn a_pass_that_stops_short_of_the_groove_floor_it_entered_convicts() {
    // The review's table (PR #1429): the band clause read the three
    // shortfall rows as 0.000 - the floor 0.7 mm or more under the
    // nose, inside the very groove the pass entered, posted clean. A
    // floor the pass stood over is its rest; only a floor whose recess
    // the pass never stood level with is another op's.
    let f = revolve(GROOVE);
    let read = |z: f64, r: f64| {
        stock_sim::envelope_scan_turned(&f, &into_the_groove(z, r), NOSE, TOL, (Some(LatheAxis::Z), 0.0))
            .expect("scan")
    };
    let cut = read(34.8, 6.3);
    let gate = TOL + cut.cell * cut.cell / (2.0 * NOSE);
    assert!(cut.rest_mm <= gate, "z 34.8 r 6.3: the floor is cut: {}", cut.rest_mm);
    assert!(read(34.8, 8.0).rest_mm > 1.0, "z 34.8 r 8.0: the radial shortfall convicts");
    for (z, r, least) in [(35.5, 6.3, 0.5), (35.5, 8.0, 0.5), (36.0, 6.3, 1.0)] {
        let rep = read(z, r);
        assert!(rep.rest_mm > least, "z {z} r {r}: the floor left under the nose is this pass's rest: {}", rep.rest_mm);
    }
}

/// Law (f) through `verify`: a turning nose path on `facets` with the
/// spindle given, as the app hands it.
fn verdict(facets: &[Triangle], moves: Vec<Move>) -> Vec<Warning> {
    let tool = verify::ToolGeom {
        radius: NOSE,
        nose_radius: NOSE,
        flute_len: 5.0,
        shank_radius: 0.0,
        shank_len: 0.0,
        holder_radius: 0.0,
        holder_len: 0.0,
    };
    let region = Region::default();
    let cx = VerifyContext {
        region: &region,
        model: Some(facets),
        tool: &tool,
        mode: Mode::Envelope,
        stock_top_z: 50.0,
        clearance_z: 55.0,
        gouge_tol: 0.01,
        z_tol: 1e-6,
        finish_tol: Some(TOL),
        lathe_axis: Some(LatheAxis::Z),
        ..Default::default()
    };
    let path = Toolpath { moves, ..Default::default() };
    verify::verify(&path, &cx).into_iter().filter(Warning::blocks_post).collect()
}

#[test]
fn law_f_still_blocks_a_real_rest_and_a_real_gouge_by_name() {
    let f = revolve(GROOVE);
    let rest = verdict(&f, bridging_pass());
    assert!(
        rest.iter().any(|w| matches!(w, Warning::Envelope { rest_mm, .. } if *rest_mm > 3.0)),
        "the bridged floor blocks: {rest:?}"
    );
    // The last station 0.8 too deep: the nose centre on the end face.
    let gouge = verdict(&f, vec![feed(p(10.8, 0.0, 48.0), p(0.0, 0.0, 48.0))]);
    assert!(
        gouge.iter().any(|w| matches!(w, Warning::Envelope { gouge_mm, .. } if *gouge_mm > 0.5)),
        "the gouge blocks: {gouge:?}"
    );
    assert!(gouge.iter().any(|w| w.to_string().starts_with("finish envelope")), "by name: {gouge:?}");
    // And the honest station posts.
    assert!(verdict(&f, facing_pass()).iter().all(|w| !matches!(w, Warning::Envelope { .. })), "the facing station is clean");
}
