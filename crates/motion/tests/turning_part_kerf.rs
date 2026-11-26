//! #3452: a part-off's kerf stands wholly on the scrap side of the
//! parting plane, which is the finished part's END FACE. The issue's
//! case: a Ø30 part Z-70..0 with a 1 x 45° back chamfer, the bar run on
//! to Z-84, a 3 mm blade - the kerf is Z-73..-70 and the part keeps its
//! 70 mm and its chamfer. The scrap side is the CHUCK side (ISO 841:
//! -Z), never a guess from the stock: a model that carries its grip
//! stub (Z-84..0, plane Z-70) under a stock with a front facing
//! allowance still parts Z-73..-70. A model held from the +Z side
//! cannot be stated by any setup yet (#3469), so a plane with the whole
//! model on the chuck side is refused by name. The kernel refuses a
//! kerf that meets no bar, and names its numbers; the generation
//! verdict refuses a hand-built part-off whose kerf enters the part
//! (the pre-fix output: the blade centred on the plane; and a kerf put
//! on the part side of the plane).
//!
//! Gate map (M4.4): covers camcraft_motion::turning (parting kernel,
//! parting_law waste side and kerf verdict).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    blade_warnings, chuck_side, part_in, part_kerf_refusal, PartParams, StockEnv, WasteSide, CHUCK_NOTE, KERF_TOL,
};

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
/// The issue's blade: GT-0300, 3 mm.
const W: f64 = 3.0;

/// The closed-solid revolve the turning suites share.
fn revolve(n_circ: usize, meridian: &[(f64, f64)]) -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / n_circ as f64;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let mut t = Vec::new();
    for seg in meridian.windows(2) {
        let ((ra, za), (rb, zb)) = (seg[0], seg[1]);
        for j in 0..n_circ {
            let (a1, a2, b1, b2) = (v(j, ra, za), v(j + 1, ra, za), v(j, rb, zb), v(j + 1, rb, zb));
            if ra <= 1e-12 {
                t.push(Triangle::new(a1, b2, b1));
            } else if rb <= 1e-12 {
                t.push(Triangle::new(a1, a2, b2));
            } else {
                t.push(Triangle::new(a1, a2, b2));
                t.push(Triangle::new(a1, b2, b1));
            }
        }
    }
    t
}

/// The issue's pin: Ø30, Z-70..0, a 1 x 45° chamfer at the back face.
fn pin() -> Vec<Triangle> {
    revolve(32, &[(0.0, -70.0), (14.0, -70.0), (15.0, -69.0), (15.0, 0.0), (0.0, 0.0)])
}

/// The pin with its grip stub in the model: Z-84..0, the finished
/// part Z-70..0 (the reader's case of the second read of #3472).
fn stubbed_pin() -> Vec<Triangle> {
    revolve(32, &[(0.0, -84.0), (15.0, -84.0), (15.0, 0.0), (0.0, 0.0)])
}

/// The pin mirrored: Z0..70, its chamfered end face at Z70.
fn mirrored_pin() -> Vec<Triangle> {
    revolve(32, &[(0.0, 0.0), (15.0, 0.0), (15.0, 69.0), (14.0, 70.0), (0.0, 70.0)])
}

/// A Ø34 bar from `z0` to `z1`, read in the part's frame.
fn bar(part: &[Triangle], z0: f64, z1: f64) -> Option<StockEnv> {
    let f = Frame::about(part).expect("the part revolves");
    Some(StockEnv { r_out: 17.0, clearance: 1.0, s_top: f.station_of(z1), s_bot: f.station_of(z0) })
}

fn params(parting_z: f64, clearance_z: f64, top_z: f64) -> PartParams {
    PartParams {
        tolerance: TOL,
        clearance_z,
        top_z,
        parting_z,
        stepdown: 2.0,
        feed: 0.0,
        plunge_feed: 0.05,
        blade_width: W,
        kerf: Default::default(),
    }
}

/// The (lo, hi) world-Z span the blade's band takes on each cutting
/// move (the CL is the middle of the tip edge; the band is W wide).
fn kerf_spans(f: &Frame, t: &Toolpath) -> Vec<(f64, f64)> {
    t.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| {
            let (sa, sb) = (f.sr(m.from).0, f.sr(m.to).0);
            (f.height_of(sa.min(sb) - W * 0.5), f.height_of(sa.max(sb) + W * 0.5))
        })
        .collect()
}

#[test]
fn the_issue_case_kerf_stands_behind_the_end_face() {
    let part = pin();
    let f = Frame::about(&part).expect("upright");
    let t = part_in(&part, &params(-70.0, 10.0, 0.0), bar(&part, -84.0, 0.0)).expect("the bar parts");
    let spans = kerf_spans(&f, &t);
    assert!(!spans.is_empty(), "the part-off cuts");
    for &(lo, hi) in &spans {
        assert!(hi <= -70.0 + KERF_TOL, "the blade's +Z flank at Z{hi} enters the part (end face Z-70)");
        assert!((lo + 73.0).abs() < KERF_TOL, "the kerf's -Z flank at Z{lo}, wanted Z-73");
    }
    let kept = 0.0 - spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    assert!((kept - 70.0).abs() < KERF_TOL, "the part keeps {kept} mm, wanted the model's 70");
    let last_feed = t.moves.iter().rev().find(|m| m.kind != MoveKind::Rapid).expect("a feed");
    assert!(f.sr(last_feed.to).1.abs() < 1e-9, "the last rung severs on the axis");
    assert_eq!(part_kerf_refusal(&part, &t, W, -70.0, TOL), None, "the verdict passes it");
    assert!(!t.notes.iter().any(|n| n == CHUCK_NOTE), "the plane is at the model's end: the model says what is kept: {:?}", t.notes);
    assert!(blade_warnings(&part, &t, W, TOL, 0.0).is_empty(), "the meridian scan stays silent");
}

#[test]
fn a_front_facing_allowance_never_turns_the_kerf_into_the_part() {
    // The model carries its grip stub, the plane is inside it, and the
    // stock stands flush at the back with a facing allowance at the
    // front: the kerf stays on the chuck side, Z-73..-70.
    let part = stubbed_pin();
    let f = Frame::about(&part).expect("upright");
    assert_eq!(chuck_side(), WasteSide::Below);
    for front in [0.0, 0.1, 1.0, 5.0] {
        let stock = bar(&part, -84.0, front);
        let t = part_in(&part, &params(-70.0, 10.0, 0.0), stock).expect("the bar parts");
        let spans = kerf_spans(&f, &t);
        assert!(!spans.is_empty(), "allowance {front}: the part-off cuts");
        for &(lo, hi) in &spans {
            assert!(
                hi <= -70.0 + KERF_TOL && (lo + 73.0).abs() < KERF_TOL,
                "allowance {front}: kerf Z{lo}..Z{hi}, wanted Z-73..-70"
            );
        }
        assert_eq!(part_kerf_refusal(&part, &t, W, -70.0, TOL), None, "allowance {front}");
        assert!(t.notes.iter().any(|n| n == CHUCK_NOTE), "the plane is inside the model: the path states the rule: {:?}", t.notes);
    }
}

#[test]
fn a_model_wholly_on_the_chuck_side_is_refused_by_name() {
    // The mirrored pin (Z0..70) parted at its +Z end: no setup states a
    // bar held from +Z yet, so the part would be the empty side.
    let part = mirrored_pin();
    let f = Frame::about(&part).expect("upright");
    let err = part_in(&part, &params(70.0, 100.0, 84.0), bar(&part, 0.0, 84.0)).expect_err("no part past the plane");
    let msg = format!("{err:?}");
    for want in ["stands wholly on the chuck side (lower Z)", "Z0.000 to Z70.000", "parting plane Z70.000"] {
        assert!(msg.contains(want), "the refusal names {want:?}: {msg}");
    }
    let why = part_kerf_refusal(&part, &plunge_at(&f, 68.5), W, 70.0, TOL).expect("refused");
    assert!(why.contains("wholly on the chuck side"), "{why}");
}

#[test]
fn a_plane_the_model_runs_past_parts_toward_the_chuck() {
    // A model that carries its own bar (Z-40..0, the plane at Z-20):
    // with no bar to read, ISO 841's lathe Z - the chuck at -Z.
    let model = revolve(32, &[(0.0, -40.0), (6.0, -40.0), (6.0, 0.0), (0.0, 0.0)]);
    let f = Frame::about(&model).expect("upright");
    let t = part_in(&model, &params(-20.0, 10.0, 0.0), None).expect("parts");
    for (lo, hi) in kerf_spans(&f, &t) {
        assert!((hi + 20.0).abs() < KERF_TOL && (lo + 23.0).abs() < KERF_TOL, "kerf Z{lo}..Z{hi}, wanted Z-23..-20");
    }
}

#[test]
fn a_kerf_that_meets_no_bar_is_refused_by_its_numbers() {
    // No stock: the bar is the model alone, and Z-73..-70 is air.
    let part = pin();
    let err = part_in(&part, &params(-70.0, 10.0, 0.0), None).expect_err("nothing past the part's end");
    assert!(matches!(err, GenError::Empty(_)), "{err:?}");
    let msg = format!("{err:?}");
    for want in ["does not meet the bar", "3.000 mm kerf", "Z-73.000 to Z-70.000", "Z-70.000 to Z0.000"] {
        assert!(msg.contains(want), "the refusal names {want:?}: {msg}");
    }
    // The bar ends at the part's end face: still nothing to part.
    assert!(part_in(&part, &params(-70.0, 10.0, 0.0), bar(&part, -70.0, 0.0)).is_err());
    // The bar runs on: the same kerf is accepted.
    assert!(part_in(&part, &params(-70.0, 10.0, 0.0), bar(&part, -84.0, 0.0)).is_ok());
}

/// A hand-built part-off: one plunge to the axis at CL `z_cl`.
fn plunge_at(f: &Frame, z_cl: f64) -> Toolpath {
    let s = f.station_of(z_cl);
    let mut b = PathBuilder::new(f.point(20.0, s));
    b.push(MoveKind::Feed, f.point(0.0, s), Some(0.05));
    b.push(MoveKind::Rapid, f.point(20.0, s), None);
    b.finish()
}

#[test]
fn the_verdict_refuses_a_kerf_into_the_part() {
    let part = pin();
    let f = Frame::about(&part).expect("upright");
    // The pre-fix output: the blade centred on the end face.
    let why = part_kerf_refusal(&part, &plunge_at(&f, -70.0), W, -70.0, TOL).expect("a gouge");
    for want in ["cuts into the finished part", "1.500 mm past", "Z-71.500 to Z-68.500", "Z-70.000"] {
        assert!(why.contains(want), "the refusal names {want:?}: {why}");
    }
    // Two micrometres in is a gouge; the blade on the face is not.
    assert!(part_kerf_refusal(&part, &plunge_at(&f, -71.5 + 2e-6), W, -70.0, TOL).is_some());
    assert_eq!(part_kerf_refusal(&part, &plunge_at(&f, -71.5), W, -70.0, TOL), None);
    assert_eq!(part_kerf_refusal(&part, &plunge_at(&f, -75.0), W, -70.0, TOL), None);
    // A kerf put wholly on the part side (the second read's wrong-side
    // output, CL Z-68.5 on the stubbed model) is refused by its numbers.
    let stub = stubbed_pin();
    let fs = Frame::about(&stub).expect("upright");
    let why = part_kerf_refusal(&stub, &plunge_at(&fs, -68.5), W, -70.0, TOL).expect("a gouge");
    for want in ["cuts into the finished part", "Z-70.000 to Z-67.000", "3.000 mm past"] {
        assert!(why.contains(want), "the refusal names {want:?}: {why}");
    }
    // A judge that cannot measure never reports silence.
    assert!(part_kerf_refusal(&part, &plunge_at(&f, -71.5), 0.0, -70.0, TOL).is_some());
    assert!(part_kerf_refusal(&[], &plunge_at(&f, -71.5), W, -70.0, TOL).is_some());
}
