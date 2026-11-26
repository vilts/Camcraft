//! The stated verify tolerance (#1170) on a synthetic square punch
//! whose path stands 12 um too near: refused as a gouge with none
//! stated, refused by the tolerance's name when it is too small, passed
//! with the deepest reading said when it covers it; the threading law
//! never uses it, and a tolerance that is no tolerance is unreadable.
//! Provenance: our own fixtures.

use super::*;
use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Loop;
use crate::geometry::Point3;

const K: f64 = 0.2;
/// How far the path's kerf enters the part.
const IN: f64 = 0.012;

fn square(h: f64) -> Region {
    let l = Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)]).expect("loop");
    Region::new(vec![l]).expect("region")
}

fn judged(h: f64, start: Point2, verify_tol: f64) -> WireVerdict {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let mut b = PathBuilder::new(p(start.x, start.y));
    for (x, y) in [(-h, 0.0), (-h, -h), (h, -h), (h, h), (-h, h), (-h, 0.0), (start.x, start.y)] {
        b.push(MoveKind::Feed, p(x, y), None);
    }
    let (tp, part, stock) = (b.finish(), square(5.0), square(15.0));
    let sections = [0.001, 6.0, 11.999].iter().map(|&z| WireSection { z, part: part.clone(), stock: Some(stock.clone()) }).collect();
    judge(&WireJob {
        path: &tp,
        kerf_offset: K,
        skim_offsets: Vec::new(),
        side: CutSide::Punch,
        start: Some(start),
        sections,
        burnout: false,
        cutoff_from: None,
        capture: true,
        taper: false,
        verify_tol,
        taper_chord: false,
    })
}

const START: Point2 = Point2 { x: -10.0, y: 0.0 };

#[test]
fn no_stated_tolerance_judges_the_facets_as_they_stand() {
    let v = judged(5.0 + K - IN, START, 0.0);
    let [WireFinding::Gouge { depth, tol, .. }] = v.findings.as_slice() else { panic!("{:?}", v.findings) };
    assert!((depth - IN).abs() < 1e-9 && *tol == 0.0, "{depth} {tol}");
    assert!(!v.findings[0].describe().contains("verify tolerance"), "{}", v.findings[0].describe());
    assert!(v.under.is_none() && v.verify_tol == 0.0);
}

#[test]
fn a_tolerance_below_the_reading_refuses_by_its_name() {
    let v = judged(5.0 + K - IN, START, 0.003);
    let [g @ WireFinding::Gouge { depth, tol, .. }] = v.findings.as_slice() else { panic!("{:?}", v.findings) };
    assert!((depth - IN).abs() < 1e-9 && (*tol - 0.003).abs() < 1e-12, "{depth} {tol}");
    let said = g.describe();
    assert!(said.contains("enters the part by 0.0120 mm") && said.contains("past the stated verify tolerance 0.0030 mm"), "{said}");
}

#[test]
fn a_tolerance_that_covers_the_reading_passes_and_says_the_reading() {
    let v = judged(5.0 + K - IN, START, 0.0125);
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    let under = v.under.expect("the reading the tolerance let pass");
    assert!((under.depth - IN).abs() < 1e-9, "{under:?}");
    assert!((v.verify_tol - 0.0125).abs() < 1e-12);
    assert_eq!(v.slugs.len(), 1, "the punch still drops: {:?}", v.slugs);
    // A path at the kerf offset needs no tolerance: none is said used.
    let clean = judged(5.0 + K, START, 0.0125);
    assert!(clean.findings.is_empty() && clean.under.is_none(), "{clean:?}");
}

#[test]
fn the_threading_law_does_not_use_the_tolerance() {
    // The start hole stands 5 um nearer the part than the kerf offset.
    let v = judged(5.0 + K, Point2::new(-5.0 - K + 0.005, 0.0), 0.05);
    assert!(matches!(v.findings.as_slice(), [WireFinding::Threading { .. }]), "{:?}", v.findings);
}

#[test]
fn a_tolerance_that_is_no_tolerance_is_unreadable() {
    for bad in [-0.001, f64::NAN, f64::INFINITY, K, 1.0] {
        let v = judged(5.0 + K, START, bad);
        let [WireFinding::Unreadable(why)] = v.findings.as_slice() else { panic!("{bad}: {:?}", v.findings) };
        assert!(why.contains("the verify tolerance") && why.contains("below the kerf offset 0.2000 mm"), "{why}");
    }
}
