//! The gouge depth (#3211) on lofted walls whose entry is known in
//! closed form: a vertical wire cut at the bottom section's offset
//! stands inside the part higher up, and the kerf law says how deep the
//! kerf enters there - the kerf offset plus how far the wire's centre
//! stands inside - never the kerf offset alone.
//! Provenance: our own fixtures; plain square geometry.

use super::*;
use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Loop;
use crate::geometry::Point3;

const K: f64 = 0.162;
const H: f64 = 20.0;

fn square(h: f64) -> Region {
    let l = Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)]).expect("loop");
    Region::new(vec![l]).expect("region")
}

/// A square wire path of half-width `w` from the start hole `start`,
/// judged against the part `half(z)` at Z0, Z10 and Z20.
fn judged(side: CutSide, w: f64, start: Point2, half: impl Fn(f64) -> f64) -> WireVerdict {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let mut b = PathBuilder::new(p(start.x, start.y));
    for (x, y) in [(-w, 0.0), (-w, -w), (w, -w), (w, w), (-w, w), (-w, 0.0), (start.x, start.y)] {
        b.push(MoveKind::Feed, p(x, y), None);
    }
    let tp = b.finish();
    let sections = [0.0, 10.0, H].iter().map(|&z| WireSection { z, part: square(half(z)), stock: Some(square(30.0)) }).collect();
    judge(&WireJob {
        path: &tp,
        kerf_offset: K,
        skim_offsets: Vec::new(),
        side,
        start: Some(start),
        sections,
        burnout: false,
        cutoff_from: None,
        capture: true,
        taper: false,
        verify_tol: 0.0,
        taper_chord: false,
    })
}

fn gouge(v: &WireVerdict) -> (f64, f64, f64) {
    let g = v.findings.iter().find_map(|f| match f {
        WireFinding::Gouge { z, depth, deepest, .. } => Some((*z, *depth, *deepest)),
        _ => None,
    });
    g.unwrap_or_else(|| panic!("a gouge: {:?}", v.findings))
}

#[test]
fn a_punch_whose_walls_lean_out_says_the_wall_shift_as_the_depth() {
    // The walls run from half-width 7.5 at Z0 to 10.34 at Z20; the wire
    // is cut at the Z0 offset. At height z the kerf enters by the wall's
    // shift there: 1.42 mm at Z10, 2.84 mm at Z20.
    let v = judged(CutSide::Punch, 7.5 + K, Point2::new(-15.0, 0.0), |z| 7.5 + 2.84 * z / H);
    let (z, depth, deepest) = gouge(&v);
    assert!((z - 10.0).abs() < 1e-9, "the first plane that gouges is Z10: {z}");
    assert!((depth - 1.42).abs() < 1e-6, "the depth at Z10 is the wall shift 1.42 mm: {depth}");
    assert!((deepest - 2.84).abs() < 1e-6, "the deepest reading is at Z20, 2.84 mm: {deepest}");
    let said = v.findings.iter().map(WireFinding::describe).collect::<Vec<_>>().join(" | ");
    assert!(said.contains("enters the part by 1.4200 mm") && said.contains("the deepest reading at any height is 2.8400 mm"), "{said}");
}

#[test]
fn an_open_run_along_a_leaning_wall_says_the_wall_shift_as_the_depth() {
    // The open run's kerf law reads the same entry (#1109 path).
    let v = judged(CutSide::Open, 7.5 + K, Point2::new(-15.0, 0.0), |z| 7.5 + 2.84 * z / H);
    let (z, depth, deepest) = gouge(&v);
    assert!((z - 10.0).abs() < 1e-9 && (depth - 1.42).abs() < 1e-6 && (deepest - 2.84).abs() < 1e-6, "{z} {depth} {deepest}");
}

#[test]
fn a_die_opening_that_narrows_says_the_corner_entry() {
    // The issue's lofted opening: half-width 10.34 at Z0 to 7.5 at Z20,
    // the wire cut at the Z0 offset inside it. Along a wall the kerf
    // enters by the wall's shift; at the path's corner the wire stands
    // inside both walls, sqrt(2) times as far from the opening's corner.
    let v = judged(CutSide::Die, 10.34 - K, Point2::new(0.0, 0.0), |z| 10.34 - 2.84 * z / H);
    let (z, depth, deepest) = gouge(&v);
    let corner = |shift: f64| std::f64::consts::SQRT_2 * (shift - K) + K;
    assert!((z - 10.0).abs() < 1e-9, "the first plane that gouges is Z10: {z}");
    // Move 1 (start hole to the wall's middle) is the first that gouges.
    assert!((depth - 1.42).abs() < 1e-6, "move 1 enters by the wall shift at Z10: {depth}");
    assert!((deepest - corner(2.84)).abs() < 1e-6, "the deepest reading is at Z20's corner: {deepest}");
    assert!(deepest > 2.84, "never less than the wall shift: {deepest}");
}

#[test]
fn a_kerf_that_only_grazes_the_part_still_reads_its_entry() {
    // The wire's centre stays outside the part: the depth is the kerf
    // offset less the clearance, as before (#534).
    let v = judged(CutSide::Punch, 7.5 + K - 0.05, Point2::new(-15.0, 0.0), |_| 7.5);
    let (z, depth, _) = gouge(&v);
    assert!(z.abs() < 1e-9 && (depth - 0.05).abs() < 1e-9, "{z} {depth}");
}

#[test]
fn an_inside_interval_between_coarse_samples_reports_its_full_depth() {
    // Review counterexample: all 33 former coarse samples are outside.
    let region = Region::new(vec![
        Loop::new(vec![
            Point2::new(50.5625, -1.0),
            Point2::new(52.5625, -1.0),
            Point2::new(52.5625, 1.0),
            Point2::new(50.5625, 1.0),
        ])
        .expect("loop"),
    ])
    .expect("region");
    let product = Product::Part(&region);
    let a = Point2::new(0.0, 0.0);
    let b = Point2::new(100.0, 0.0);
    let (signed, at) = dist::entry(&product, a, b);
    assert!(
        (signed + 1.0).abs() <= 100.0 / 1024.0,
        "expected signed depth -1; got {signed} at {at:?}"
    );
    assert!(
        product.contains(at),
        "the reported point must be in the product: {at:?}"
    );
    assert!((K - signed - 1.162).abs() <= 100.0 / 1024.0);
}

#[test]
fn a_stock_limited_die_entry_keeps_the_bound_at_the_stock_edge() {
    // The distance to the opening jumps from zero when the move enters
    // the stock. A global Lipschitz bound on that masked value is false.
    let opening = square(1.0);
    let stock = Region::new(vec![
        Loop::new(vec![
            Point2::new(50.5625, -1.0),
            Point2::new(52.5625, -1.0),
            Point2::new(52.5625, 1.0),
            Point2::new(50.5625, 1.0),
        ])
        .expect("loop"),
    ])
    .expect("region");
    let product = Product::Around {
        opening: &opening,
        stock: Some(&stock),
    };
    let (signed, at) = dist::entry(&product, Point2::new(0.0, 0.0), Point2::new(100.0, 0.0));
    let deepest = 52.5625 - 1.0;
    assert!(
        (signed + deepest).abs() <= 100.0 / 1024.0,
        "expected signed depth -{deepest}; got {signed} at {at:?}"
    );
    assert!(
        product.contains(at),
        "the reported point must be in the stock: {at:?}"
    );
}
