//! The corner minimum over polygonal profiles (#3208): coordinates
//! alone do not prove that a profile came from an arc or a mesh.
//! Coarse facets and authored corners keep the strict cover tolerance.
//!
//! The wire and gap below are SYNTHETIC: the issue's 0.25 mm wire and a
//! plausible gap, from no machine's manual.

use super::*;

const WIRE: f64 = 0.25;
const GAP: f64 = 0.037;

fn params() -> WireParams {
    WireParams {
        wire_diameter: WIRE,
        gap: GAP,
        condition: "C110".into(),
        side: WireSide::Die,
        start: Some(Point2::new(0.0, 0.0)),
        plane_z: -12.0,
        tabs: None,
        stock: None,
        clearance: 1.0,
        sharp_corners: false,
    }
}

/// A `w` x `w` square centred on the origin with corners of radius `r`,
/// each quarter round drawn in `n` facets.
fn square(w: f64, r: f64, n: u32) -> Loop {
    let h = w * 0.5 - r;
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(h, h, 0.0_f64), (-h, h, 90.0), (-h, -h, 180.0), (h, -h, 270.0)] {
        for s in 0..=n {
            let t = (a0 + 90.0 * f64::from(s) / f64::from(n)).to_radians();
            pts.push(Point2::new(cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    Loop::new(pts).unwrap()
}

fn cut(r: f64, n: u32) -> Result<Toolpath, GenError> {
    wire_contour(&[square(10.0, r, n)], &[], &params())
}

/// A fine polygonal fillet fits within the existing cover tolerance.
#[test]
fn a_fine_fillet_within_the_cover_tolerance_is_cut() {
    assert!((params().kerf_offset() - 0.162).abs() < 1e-12);
    if let Err(e) = cut(0.25, 30) {
        panic!("r0.25 in 30 facets per quarter is refused: {e}");
    }
}

/// A radius above the wire minimum does not prove arc provenance.
/// Each coarse profile leaves more than the strict cover tolerance.
#[test]
fn coarse_fillets_without_arc_provenance_stay_refused() {
    for (r, n) in [(0.25, 4), (0.5, 4), (0.25, 3), (0.165, 4)] {
        let why = cut(r, n)
            .expect_err("unproven facets keep the strict cover tolerance")
            .to_string();
        assert!(
            why.contains("inside corner") && why.contains("0.1620"),
            "r{r} in {n}: {why}"
        );
    }
}

/// Done means: a meshed radius under the minimum is still refused, and
/// so is a sharp corner.
#[test]
fn a_meshed_fillet_under_the_minimum_and_a_sharp_corner_stay_refused() {
    for (r, n) in [(0.15, 4), (0.1, 4), (0.1, 30), (0.155, 3)] {
        let why = cut(r, n).expect_err("refused").to_string();
        assert!(why.contains("inside corner") && why.contains("0.1620"), "r{r} in {n}: {why}");
    }
    let sharp = Loop::new(vec![
        Point2::new(-5.0, -5.0),
        Point2::new(5.0, -5.0),
        Point2::new(5.0, 5.0),
        Point2::new(-5.0, 5.0),
    ])
    .unwrap();
    let why = wire_contour(&[sharp], &[], &params()).expect_err("refused").to_string();
    assert!(why.contains("inside corner"), "{why}");
}

/// A lone 22.5° kink between long straight runs is a corner, not a
/// facet of a round: its 3 µm rest is still refused.
#[test]
fn a_lone_kink_between_long_sides_stays_refused() {
    // The r2 square in fine facets, with the bottom flat (x -3..3) bent
    // outward at x = 0 so that it turns 22.5° there: the rest at that
    // inside corner is 0.162 * (1 / cos 11.25° - 1) = 3.2 µm.
    let d = 3.0 * 11.25_f64.to_radians().tan();
    let mut pts = square(10.0, 2.0, 30).points().to_vec();
    let at = pts.iter().position(|q| (q.x - 3.0).abs() < 1e-9 && (q.y + 5.0).abs() < 1e-9).unwrap();
    pts.insert(at, Point2::new(0.0, -5.0 - d));
    let why = wire_contour(&[Loop::new(pts).unwrap()], &[], &params()).expect_err("refused").to_string();
    assert!(why.contains("inside corner") && why.contains("near X0.000 Y-5.597"), "{why}");
}

/// A drawn polygon is not a mesh: a 16-sided die opening of radius 5
/// turns 22.5° at each corner like the r0.25 mesh, but its chords stand
/// 0.1 mm off their circle - each corner's 3.2 µm rest is refused.
#[test]
fn a_drawn_polygon_with_facet_sized_turns_stays_refused() {
    let pts = (0..16).map(|s| {
        let t = (22.5 * f64::from(s)).to_radians();
        Point2::new(5.0 * t.cos(), 5.0 * t.sin())
    });
    let why = wire_contour(&[Loop::new(pts.collect()).unwrap()], &[], &params()).expect_err("refused").to_string();
    assert!(why.contains("inside corner") && why.contains("0.0032 mm standing"), "{why}");
}

/// The same drawn polygon at R0.5 passes the former radius and sag
/// guards. Its sharp corners still leave 3.2 µm, above COVER_TOL.
#[test]
fn a_small_drawn_polygon_with_facet_sized_turns_stays_refused() {
    let pts = (0..16).map(|s| {
        let t = (22.5 * f64::from(s)).to_radians();
        Point2::new(0.5 * t.cos(), 0.5 * t.sin())
    });
    let why = wire_contour(&[Loop::new(pts.collect()).unwrap()], &[], &params())
        .expect_err("refused")
        .to_string();
    assert!(
        why.contains("inside corner") && why.contains("0.0032 mm standing"),
        "{why}"
    );
}

/// A short V notch is not a run of facets: its tip turns 22.5° on a
/// small circle, but its neighbours turn the other way - refused.
#[test]
fn a_short_notch_with_one_facet_sized_turn_stays_refused() {
    let d = 0.2 * 11.25_f64.to_radians().tan();
    let mut pts = square(10.0, 2.0, 30).points().to_vec();
    let at = pts.iter().position(|q| (q.x - 3.0).abs() < 1e-9 && (q.y + 5.0).abs() < 1e-9).unwrap();
    for (i, q) in [Point2::new(-0.2, -5.0), Point2::new(0.0, -5.0 - d), Point2::new(0.2, -5.0)].into_iter().enumerate() {
        pts.insert(at + i, q);
    }
    let why = wire_contour(&[Loop::new(pts).unwrap()], &[], &params()).expect_err("refused").to_string();
    assert!(why.contains("inside corner") && why.contains("near X0.000 Y-5.040"), "{why}");
}

/// An r0.25 hole in eight 45° facets (each chord 0.019 mm off its
/// circle) stays refused: its rest is 13.3 µm, above COVER_TOL.
#[test]
fn a_round_in_facets_past_the_turn_cap_stays_refused() {
    let pts = (0..8).map(|s| {
        let t = (45.0 * f64::from(s)).to_radians();
        Point2::new(0.25 * t.cos(), 0.25 * t.sin())
    });
    let why = wire_contour(&[Loop::new(pts.collect()).unwrap()], &[], &params()).expect_err("refused").to_string();
    assert!(why.contains("inside corner") && why.contains("0.0133 mm standing"), "{why}");
}
