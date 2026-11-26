//! #1531 regression with an analytic R2 channel and synthetic rails.
//! Provenance: elementary circle geometry and synthetic cutter profiles.
//! Gate map (#1531): covers camcraft_sim::verify (finish, types).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::Point2;
use camcraft_sim::geometry::{Point3, Triangle};
use camcraft_sim::verify::{verify, Cutter, Mode, ToolGeom, VerifyContext, Warning};

use super::{region, tool, TOL};

// #1531: extrude a flat floor with R2 inside fillets and vertical walls.
// The facet sagitta is less than 0.00004 mm at this angular resolution.
fn filleted_channel() -> Vec<Triangle> {
    const SEGMENTS: usize = 128;
    let arc = |i: usize| {
        let a = std::f64::consts::FRAC_PI_2 * i as f64 / SEGMENTS as f64;
        (6.0 + 2.0 * a.sin(), 2.0 - 2.0 * a.cos())
    };
    let profile: Vec<_> = std::iter::once((-8.0, 8.0))
        .chain((0..=SEGMENTS).rev().map(|i| {
            let (y, z) = arc(i);
            (-y, z)
        }))
        .chain((0..=SEGMENTS).map(arc))
        .chain(std::iter::once((8.0, 8.0)))
        .collect();
    let mut facets = Vec::new();
    for pair in profile.windows(2) {
        let [(y0, z0), (y1, z1)] = [pair[0], pair[1]];
        let a = Point3::new(-12.0, y0, z0);
        let b = Point3::new(12.0, y0, z0);
        let c = Point3::new(12.0, y1, z1);
        let d = Point3::new(-12.0, y1, z1);
        facets.extend([Triangle::new(a, b, c), Triangle::new(a, c, d)]);
    }
    facets
}

fn rail_warnings(y: f64, z: f64, corner_r: Option<f64>) -> Vec<Warning> {
    let mut path = Toolpath::default();
    let rail = Move::new(MoveKind::Feed, Point3::new(-5.0, y, z), Point3::new(5.0, y, z), Some(600.0));
    path.moves.push(rail.with_axis([0.0, 0.0, 1.0]));
    let tool = ToolGeom { radius: 4.0, shank_radius: 4.0, holder_radius: 4.0, ..tool() };
    let cutter = corner_r.map(|corner_r| Cutter { corner_r, ..Cutter::flat(&tool) });
    let wall = region(vec![Point2::new(-12.0, -8.0), Point2::new(12.0, -8.0), Point2::new(12.0, 8.0), Point2::new(-12.0, 8.0)]);
    let facets = filleted_channel();
    let cx = VerifyContext {
        region: &wall,
        model: Some(&facets),
        tool: &tool,
        cutter,
        mode: Mode::Envelope,
        stock_top_z: 8.0,
        clearance_z: 20.0,
        gouge_tol: TOL,
        finish_tol: Some(TOL),
        ..Default::default()
    };
    verify(&path, &cx)
}

fn has_finish_gouge(warnings: &[Warning]) -> bool {
    warnings.iter().any(|w| match w {
        Warning::Gouge { at: 0 } => true,
        Warning::Envelope { gouge_mm, .. } => *gouge_mm > TOL,
        _ => false,
    })
}

#[test]
fn bullnose_envelope_catches_the_r1_gouge_that_the_ball_misses() {
    let t = 1.0 - std::f64::consts::FRAC_1_SQRT_2;
    let y = -4.0 + t;
    let z = t - 0.1;
    // Analytic vertical clearance: R1 = -0.100 mm, Ø8 ball = +0.193 mm.
    let ball = rail_warnings(y, z, Some(4.0));
    assert!(!has_finish_gouge(&ball), "Ø8 ball: {ball:?}");
    assert_eq!(ball, rail_warnings(y, z, None), "explicit and implicit balls agree");
    let tangent = rail_warnings(y, t, Some(1.0));
    assert!(!has_finish_gouge(&tangent), "tangent Ø8 R1: {tangent:?}");
    let gouging = rail_warnings(y, z, Some(1.0));
    assert!(has_finish_gouge(&gouging), "lowered Ø8 R1 missed its 0.100 mm gouge: {gouging:?}");
}

#[test]
fn bullnose_envelope_distinguishes_r1_from_r3_on_the_same_rail() {
    let r3 = rail_warnings(-4.0, 0.0, Some(3.0));
    assert!(!has_finish_gouge(&r3), "Ø8 R3 on the floor and wall: {r3:?}");
    let r1 = rail_warnings(-4.0, 0.0, Some(1.0));
    assert!(has_finish_gouge(&r1), "Ø8 R1 on the same rail: {r1:?}");
}

#[test]
fn bullnose_envelope_marks_the_r3_rail_rest_as_unmeasured() {
    // The analytic surface leaves 0.633322528 mm vertically at
    // (x, y) = (0, -6 - sqrt(2)). The native cutter sweep only judges gouge.
    let r3 = rail_warnings(-4.0, 0.0, Some(3.0));
    assert!(r3.iter().any(|w| matches!(w, Warning::RestUnjudged { at: 0, .. })), "Ø8 R3 rest: {r3:?}");
    assert!(!r3.iter().any(|w| matches!(w, Warning::Envelope { .. })), "no ball-proxy stock readings: {r3:?}");
}
