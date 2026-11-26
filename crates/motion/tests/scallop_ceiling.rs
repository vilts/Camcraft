//! M13.9 / #37: the scallop law's reach ceiling at the generator door.
//! A stated stepover (or steep stepover) at or past the ball diameter,
//! or a tolerance at or past half the ball radius, is refused by name:
//! past `s = 2r` the cusp formula `h = r − sqrt(r² − (s/2)²)` describes
//! no cusp, only uncut ground. Just under the ceiling generates as
//! before; nonsense radius and tolerance refuse at the same gate.
//! Provenance: scallop height of adjacent ball-end passes (machining
//! handbooks); our own fixtures, per the Clean-Room Protocol.
//!
//! Gate map (M4.4): covers camcraft_motion::finishing3d (params gate).

use camcraft_motion::finishing3d::{parallel, waterline, Finish3DParams, FinishPattern};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;

const R: f64 = 3.0;

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A 20 x 20 x 10 box centred on the Z axis.
fn box_facets() -> Vec<Triangle> {
    let q = |sx: f64, sy: f64, z: f64| p3(sx * 10.0, sy * 10.0, z);
    let (a, b, c, d) = (q(-1.0, -1.0, 0.0), q(1.0, -1.0, 0.0), q(1.0, 1.0, 0.0), q(-1.0, 1.0, 0.0));
    let (e, f, g, h) = (q(-1.0, -1.0, 10.0), q(1.0, -1.0, 10.0), q(1.0, 1.0, 10.0), q(-1.0, 1.0, 10.0));
    let quad = |p: [Point3; 4]| vec![Triangle::new(p[0], p[1], p[2]), Triangle::new(p[0], p[2], p[3])];
    let mut out = Vec::new();
    for s in [[e, f, g, h], [a, c, b, d], [a, b, f, e], [b, c, g, f], [c, d, h, g], [d, a, e, h]] {
        out.extend(quad(s));
    }
    out
}

fn params(pattern: FinishPattern) -> Finish3DParams {
    Finish3DParams {
        gap_link: camcraft_motion::finishing3d::GapLink::Jump,
        pattern,
        stepover: None,
        tool_diameter: 2.0 * R,
        tolerance: 0.1,
        clearance_z: 25.0,
        retract_z: None,
        top_z: 10.0,
        bottom_z: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: None,
        pencil_reference: None,
        waterlines: true,
    }
}

fn refused_by_ceiling(r: Result<camcraft_motion::cl::Toolpath, GenError>) -> bool {
    matches!(r, Err(GenError::Invalid(m)) if m.contains("scallop ceiling"))
}

#[test]
fn a_stepover_at_or_past_the_ball_diameter_is_refused_by_name() {
    let f = box_facets();
    for s in [2.0 * R, 2.0 * R + 0.01, 50.0] {
        let p = Finish3DParams { stepover: Some(s), ..params(FinishPattern::Parallel) };
        assert!(refused_by_ceiling(parallel(&f, &p)), "stepover {s} passed the ceiling");
    }
    let under = Finish3DParams { stepover: Some(2.0 * R - 0.01), ..params(FinishPattern::Parallel) };
    let t = parallel(&f, &under);
    assert!(matches!(&t, Ok(t) if !t.moves.is_empty()), "just under the ceiling must generate: {:?}", t.err());
}

#[test]
fn a_steep_stepover_at_or_past_the_ball_diameter_is_refused_by_name() {
    let f = box_facets();
    for s in [2.0 * R, 2.0 * R + 0.01] {
        let p = Finish3DParams { steep_stepover: Some(s), ..params(FinishPattern::SteepShallow) };
        assert!(refused_by_ceiling(parallel(&f, &p)), "steep stepover {s} passed the ceiling");
        // The waterline ignores the field, yet a nonsense word still refuses.
        let w = Finish3DParams { steep_stepover: Some(s), ..params(FinishPattern::Waterline) };
        assert!(refused_by_ceiling(waterline(&f, &w)), "steep stepover {s} passed the waterline gate");
    }
    let under = Finish3DParams { steep_stepover: Some(2.0 * R - 0.01), ..params(FinishPattern::SteepShallow) };
    let t = parallel(&f, &under);
    assert!(matches!(&t, Ok(t) if !t.moves.is_empty()), "just under the ceiling must generate: {:?}", t.err());
}

#[test]
fn a_tolerance_at_or_past_half_the_radius_is_refused_by_name() {
    // sqrt(8·r·tol) reaches the ball diameter 2r at tol = r/2.
    let f = box_facets();
    for tol in [R / 2.0, R / 2.0 + 0.01] {
        let p = Finish3DParams { tolerance: tol, ..params(FinishPattern::Waterline) };
        assert!(refused_by_ceiling(waterline(&f, &p)), "tolerance {tol} passed the ceiling");
    }
    let under = Finish3DParams { tolerance: R / 2.0 - 0.01, ..params(FinishPattern::Waterline) };
    let t = waterline(&f, &under);
    assert!(matches!(&t, Ok(t) if !t.moves.is_empty()), "just under the ceiling must generate: {:?}", t.err());
}

#[test]
fn a_degenerate_radius_or_tolerance_is_refused_at_the_gate() {
    let f = box_facets();
    for d in [0.0, -6.0, f64::NAN] {
        let p = Finish3DParams { tool_diameter: d, ..params(FinishPattern::Parallel) };
        assert!(matches!(parallel(&f, &p), Err(GenError::Invalid(m)) if m.contains("tool")), "diameter {d} passed");
    }
    for tol in [0.0, -0.1, f64::INFINITY] {
        let p = Finish3DParams { tolerance: tol, ..params(FinishPattern::Parallel) };
        assert!(matches!(parallel(&f, &p), Err(GenError::Invalid(m)) if m.contains("tolerance")), "tolerance {tol} passed");
    }
    for s in [0.0, -2.0] {
        let p = Finish3DParams { stepover: Some(s), ..params(FinishPattern::Parallel) };
        assert!(matches!(parallel(&f, &p), Err(GenError::Invalid(_))), "stepover {s} passed");
    }
}
