//! #3469: the part-off's kept piece and its back stock. The default
//! (`PartKerf::default()`: the piece cut off is the part, no back
//! stock) parts the #3452 case as before: end face Z-70, a 3 mm blade,
//! the kerf Z-73..-70. A back stock moves the whole kerf that far into
//! the bar, and the verdict keeps that stock as part material. "The
//! piece in the chuck" turns the kerf to the free-end side of the
//! plane: a stub cut off the free end of a part held end for end. Each
//! setting that puts the blade into what the part keeps is refused by
//! name, and a back stock that is not a size is refused.
//!
//! Gate map (M4.4): covers camcraft_motion::turning (parting kernel,
//! parting_law kept piece, back stock and kerf verdict).
//! Lane shape (M4.4): all tests here run in seconds.

use camcraft_motion::cl::{MoveKind, PathBuilder, Toolpath};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::turning::profile::Frame;
use camcraft_motion::turning::{
    part_in, part_kerf_refusal_for, PartKept, PartKerf, PartParams, StockEnv, CHUCK_NOTE, KERF_TOL,
};

const TAU: f64 = std::f64::consts::TAU;
const TOL: f64 = 0.05;
/// The #3452 blade: GT-0300, 3 mm.
const W: f64 = 3.0;

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

/// The #3452 pin: Ø30, Z-70..0, a 1 x 45° chamfer at the back face.
fn pin() -> Vec<Triangle> {
    revolve(32, &[(0.0, -70.0), (14.0, -70.0), (15.0, -69.0), (15.0, 0.0), (0.0, 0.0)])
}

/// A part held end for end: the finished part Z0..70 is in the chuck,
/// and a stub Z70..84 stands at the free end.
fn chucked_with_stub() -> Vec<Triangle> {
    revolve(32, &[(0.0, 0.0), (15.0, 0.0), (15.0, 84.0), (0.0, 84.0)])
}

fn bar(part: &[Triangle], z0: f64, z1: f64) -> Option<StockEnv> {
    let f = Frame::about(part).expect("the part revolves");
    Some(StockEnv { r_out: 17.0, clearance: 1.0, s_top: f.station_of(z1), s_bot: f.station_of(z0) })
}

fn params(parting_z: f64, top_z: f64, kerf: PartKerf) -> PartParams {
    PartParams {
        tolerance: TOL,
        clearance_z: top_z + 10.0,
        top_z,
        parting_z,
        stepdown: 2.0,
        feed: 0.0,
        plunge_feed: 0.05,
        blade_width: W,
        kerf,
    }
}

fn kerf(kept: PartKept, back_stock: f64) -> PartKerf {
    PartKerf { kept, back_stock }
}

/// The (lo, hi) world-Z span of the blade band on each cutting move.
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

fn assert_kerf(f: &Frame, t: &Toolpath, lo: f64, hi: f64, what: &str) {
    let spans = kerf_spans(f, t);
    assert!(!spans.is_empty(), "{what}: the part-off cuts");
    for &(a, b) in &spans {
        assert!((a - lo).abs() < KERF_TOL && (b - hi).abs() < KERF_TOL, "{what}: kerf Z{a}..Z{b}, wanted Z{lo}..Z{hi}");
    }
}

/// One radial plunge of the blade, CL at `z`, from the bar's OD to the
/// axis.
fn plunge_at(f: &Frame, z: f64) -> Toolpath {
    let s = f.station_of(z);
    let mut b = PathBuilder::new(f.point(20.0, s));
    b.push(MoveKind::Feed, f.point(0.0, s), Some(0.05));
    b.push(MoveKind::Rapid, f.point(20.0, s), None);
    b.finish()
}

#[test]
fn the_default_setting_parts_the_3452_case_as_before() {
    assert_eq!(PartKerf::default(), kerf(PartKept::CutOff, 0.0), "the default is the #3452 rule");
    let part = pin();
    let f = Frame::about(&part).expect("upright");
    let t = part_in(&part, &params(-70.0, 0.0, PartKerf::default()), bar(&part, -84.0, 0.0)).expect("parts");
    assert_kerf(&f, &t, -73.0, -70.0, "default");
    assert_eq!(part_kerf_refusal_for(&part, &t, W, -70.0, TOL, PartKerf::default()), None);
}

#[test]
fn a_back_stock_moves_the_whole_kerf_into_the_bar() {
    let part = pin();
    let f = Frame::about(&part).expect("upright");
    let k = kerf(PartKept::CutOff, 0.5);
    let t = part_in(&part, &params(-70.0, 0.0, k), bar(&part, -84.0, 0.0)).expect("parts");
    assert_kerf(&f, &t, -73.5, -70.5, "0.5 mm back stock");
    assert_eq!(part_kerf_refusal_for(&part, &t, W, -70.0, TOL, k), None, "the verdict passes its own kerf");
    // The default kerf (Z-73..-70) takes the 0.5 mm that the part keeps.
    let why = part_kerf_refusal_for(&part, &plunge_at(&f, -71.5), W, -70.0, TOL, k).expect("refused");
    for want in ["cuts into the finished part", "0.500 mm back stock", "Z-70.000"] {
        assert!(why.contains(want), "the refusal names {want:?}: {why}");
    }
}

#[test]
fn the_chuck_piece_turns_the_kerf_to_the_free_end() {
    let part = chucked_with_stub();
    let f = Frame::about(&part).expect("upright");
    let k = kerf(PartKept::Chuck, 0.0);
    let t = part_in(&part, &params(70.0, 84.0, k), bar(&part, 0.0, 84.0)).expect("the stub comes off");
    assert_kerf(&f, &t, 70.0, 73.0, "the chuck piece");
    assert!(!t.notes.iter().any(|n| n == CHUCK_NOTE), "the user stated the side: no rule note: {:?}", t.notes);
    assert_eq!(part_kerf_refusal_for(&part, &t, W, 70.0, TOL, k), None, "the verdict passes it");
    // The default rule on the same model cuts Z67..70, into the piece
    // in the chuck: the chuck-piece verdict refuses it by name.
    let why = part_kerf_refusal_for(&part, &plunge_at(&f, 68.5), W, 70.0, TOL, k).expect("a gouge");
    for want in ["cuts into the finished part", "3.000 mm past", "free-end side (higher Z)"] {
        assert!(why.contains(want), "the refusal names {want:?}: {why}");
    }
    // With a back stock the stub's kerf moves up the bar.
    let k = kerf(PartKept::Chuck, 1.0);
    let t = part_in(&part, &params(70.0, 84.0, k), bar(&part, 0.0, 84.0)).expect("parts");
    assert_kerf(&f, &t, 71.0, 74.0, "the chuck piece, 1 mm back stock");
    assert_eq!(part_kerf_refusal_for(&part, &t, W, 70.0, TOL, k), None);
}

#[test]
fn the_chuck_piece_with_no_model_below_the_plane_is_refused_by_name() {
    // The #3452 pin stands wholly above its end face: no piece stays in
    // the chuck, so "the piece in the chuck" parts nothing.
    let part = pin();
    let k = kerf(PartKept::Chuck, 0.0);
    let err = part_in(&part, &params(-70.0, 0.0, k), bar(&part, -84.0, 0.0)).expect_err("refused");
    let why = format!("{err:?}");
    for want in ["nothing to part", "free-end side (higher Z)", "Z-70.000 to Z0.000"] {
        assert!(why.contains(want), "the kernel names {want:?}: {why}");
    }
    let f = Frame::about(&part).expect("upright");
    let why = part_kerf_refusal_for(&part, &plunge_at(&f, -68.5), W, -70.0, TOL, k).expect("refused");
    assert!(why.contains("nothing to part"), "{why}");
}

#[test]
fn a_back_stock_that_is_not_a_size_is_refused() {
    let part = pin();
    let f = Frame::about(&part).expect("upright");
    for bad in [-0.1, f64::NAN, f64::INFINITY] {
        let k = kerf(PartKept::CutOff, bad);
        let err = part_in(&part, &params(-70.0, 0.0, k), bar(&part, -84.0, 0.0)).expect_err("refused");
        assert!(format!("{err:?}").contains("back stock"), "{bad}: {err:?}");
        let why = part_kerf_refusal_for(&part, &plunge_at(&f, -71.5), W, -70.0, TOL, k).expect("refused");
        assert!(why.contains("back stock"), "{bad}: {why}");
    }
}
