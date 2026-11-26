//! The wire laws on true arcs (#3937): a die circle cut as two half arcs
//! passes against the opening's waste-side ring, an arc 2 um into the
//! product is a gouge naming its move, and an arc on a leaning wire is
//! unreadable. Provenance: our own fixtures.

use super::*;
use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Loop;
use crate::geometry::Point3;

const K: f64 = 0.2;
const R: f64 = 5.0;

/// The circle of radius `r` as chords inscribed in it, within `ARC_SAG`.
fn inscribed(r: f64) -> Region {
    let n = (std::f64::consts::TAU / (2.0 * (1.0 - ARC_SAG / r).acos())).ceil() as usize;
    let pts = (0..n)
        .map(|j| {
            let t = std::f64::consts::TAU * j as f64 / n as f64;
            Point2::new(r * t.cos(), r * t.sin())
        })
        .collect();
    Region::new(vec![Loop::new(pts).expect("loop")]).expect("region")
}

/// From the origin to the circle of radius `r` about `c` on its -X side,
/// two half arcs round it, and back.
fn circle_path(r: f64, c: Point2, taper: bool) -> Toolpath {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let mut b = PathBuilder::new(p(0.0, 0.0));
    if taper {
        b.set_axis(Some([0.01, 0.0, 1.0]));
    }
    b.push(MoveKind::Feed, p(c.x - r, c.y), None);
    b.push_arc(MoveKind::Feed, p(c.x + r, c.y), None, c, true);
    b.push_arc(MoveKind::Feed, p(c.x - r, c.y), None, c, true);
    b.push(MoveKind::Feed, p(0.0, 0.0), None);
    b.finish()
}

fn run(tp: &Toolpath, taper: bool) -> WireVerdict {
    let stock = Region::new(vec![Loop::new(vec![
        Point2::new(-15.0, -15.0),
        Point2::new(15.0, -15.0),
        Point2::new(15.0, 15.0),
        Point2::new(-15.0, 15.0),
    ])
    .expect("loop")])
    .expect("stock");
    let part = inscribed(R);
    let sections = [0.0, 12.0].iter().map(|&z| WireSection { z, part: part.clone(), stock: Some(stock.clone()) }).collect();
    judge(&WireJob {
        path: tp,
        kerf_offset: K,
        skim_offsets: Vec::new(),
        side: CutSide::Die,
        start: Some(Point2::new(0.0, 0.0)),
        sections,
        burnout: false,
        cutoff_from: None,
        capture: true,
        taper,
        verify_tol: 0.0,
        taper_chord: false,
    })
}

#[test]
fn a_die_circle_cut_as_two_half_arcs_passes_and_frees_its_slug() {
    let v = run(&circle_path(R - K, Point2::new(0.0, 0.0), false), false);
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert!(v.captured && v.slugs.len() == 1 && v.slugs[0].product_area < SLUG_AREA_TOL, "{:?}", v.slugs);
}

#[test]
fn an_arc_two_microns_into_the_product_is_a_gouge_naming_its_move() {
    // The centre moved 2 um towards +X: the arcs' far side burns the opening.
    let v = run(&circle_path(R - K, Point2::new(0.002, 0.0), false), false);
    let g = v.findings.iter().find(|f| matches!(f, WireFinding::Gouge { .. })).expect("gouge");
    let WireFinding::Gouge { move_no, deepest, .. } = g else { unreachable!() };
    assert!(matches!(*move_no, 2 | 3), "an arc move: {move_no}");
    assert!(*deepest > 0.0019 && *deepest < 0.0021, "{deepest}");
}

#[test]
fn an_arc_on_a_leaning_wire_is_unreadable() {
    let v = run(&circle_path(R - K, Point2::new(0.0, 0.0), true), true);
    assert!(v.findings.iter().any(|f| matches!(f, WireFinding::Unreadable(w) if w.contains("an arc on a leaning wire"))), "{:?}", v.findings);
}

/// #3215: a die circle read back from a meshed hole (63 facet chords, a
/// 5 um sagitta) passes between the chords and the true circle. With
/// the model's chord stated it passes and says so; with no tolerance it
/// is a gouge; no undercut law runs on the vertical cut.
#[test]
fn arcs_read_from_facets_pass_under_the_model_chord_on_a_vertical_cut() {
    let r = 4.0;
    let pts = (0..63)
        .map(|j| {
            let t = std::f64::consts::TAU * f64::from(j) / 63.0;
            Point2::new(r * t.cos(), r * t.sin())
        })
        .collect();
    let part = Region::new(vec![Loop::new(pts).expect("loop")]).expect("region");
    let stock = Region::new(vec![Loop::new(vec![
        Point2::new(-15.0, -15.0),
        Point2::new(15.0, -15.0),
        Point2::new(15.0, 15.0),
        Point2::new(-15.0, 15.0),
    ])
    .expect("loop")])
    .expect("stock");
    let tp = circle_path(r - K, Point2::new(0.0, 0.0), false);
    let judged = |tol: f64, chord: bool| {
        let sections = [0.0, 12.0].iter().map(|&z| WireSection { z, part: part.clone(), stock: Some(stock.clone()) }).collect();
        judge(&WireJob {
            path: &tp,
            kerf_offset: K,
            skim_offsets: Vec::new(),
            side: CutSide::Die,
            start: Some(Point2::new(0.0, 0.0)),
            sections,
            burnout: false,
            cutoff_from: None,
            capture: true,
            taper: false,
            verify_tol: tol,
            taper_chord: chord,
        })
    };
    let bare = judged(0.0, false);
    assert!(bare.findings.iter().any(|f| matches!(f, WireFinding::Gouge { .. })), "{:?}", bare.findings);
    let v = judged(0.005, true);
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert!(v.chord);
    let said = v.tolerance_said().expect("the tolerance is said");
    assert!(said.contains("the model's chord tolerance 0.0050"), "{said}");
}
