//! The hold law (#539): a freed piece with nothing holding it is
//! refused, a stated capture lets it drop, and a tabbed cut holds the
//! slug through its main cut and frees it in the cut-off.
//! Provenance: our own fixtures.

use super::*;
use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Loop;
use crate::geometry::Point3;

const K: f64 = 0.2;

fn square(h: f64) -> Region {
    let l = Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)]).expect("loop");
    Region::new(vec![l]).expect("region")
}

fn walk(start: Point2, pts: &[(f64, f64)]) -> Toolpath {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let mut b = PathBuilder::new(p(start.x, start.y));
    for &(x, y) in pts {
        b.push(MoveKind::Feed, p(x, y), None);
    }
    b.finish()
}

fn job(tp: &Toolpath, cutoff_from: Option<usize>, capture: bool) -> WireVerdict {
    let sections = [0.001, 6.0].iter().map(|&z| WireSection { z, part: square(5.0), stock: Some(square(15.0)) }).collect();
    judge(&WireJob {
        path: tp,
        kerf_offset: K,
        skim_offsets: Vec::new(),
        side: CutSide::Die,
        start: Some(Point2::new(0.0, 0.0)),
        sections,
        cutoff_from,
        capture,
        taper: false,
        burnout: false,
    })
}

const H: f64 = 5.0 - K;

/// The closed die cut, untabbed.
fn closed() -> Vec<(f64, f64)> {
    vec![(-H, 0.0), (-H, -H), (H, -H), (H, H), (-H, H), (-H, 0.0), (0.0, 0.0)]
}

/// The main cut leaving one tab on the top side between X-1 and X1
/// (a 2 mm gap: a 1.6 mm bridge), then the cut-off across it.
fn tabbed() -> (Vec<(f64, f64)>, usize) {
    let main = vec![
        (-H, 0.0), (-H, -H), (H, -H), (H, H), (1.0, H),
        (H, H), (H, -H), (-H, -H), (-H, 0.0), (-H, H), (-1.0, H),
        (-H, H), (-H, 0.0), (0.0, 0.0),
    ];
    let m = main.len();
    let mut all = main;
    all.extend([(-H, 0.0), (-H, H), (-1.0, H), (1.0, H), (-1.0, H), (-H, H), (-H, 0.0), (0.0, 0.0)]);
    (all, m)
}

#[test]
fn an_unheld_slug_is_refused_unless_captured() {
    let o = Point2::new(0.0, 0.0);
    let tp = walk(o, &closed());
    let v = job(&tp, None, false);
    let f = v.findings.iter().find(|f| matches!(f, WireFinding::Unsupported { .. })).expect("unsupported");
    assert!(f.describe().contains("drops unsupported") && f.describe().contains("state tabs"), "{}", f.describe());
    let v = job(&tp, None, true);
    assert!(v.findings.is_empty() && v.captured && !v.held, "{v:?}");
}

#[test]
fn tabs_hold_the_slug_until_the_cut_off_frees_it() {
    let o = Point2::new(0.0, 0.0);
    let (pts, m) = tabbed();
    let tp = walk(o, &pts);
    let v = job(&tp, Some(m), false);
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert!(v.held && !v.captured, "{v:?}");
    assert_eq!(v.slugs.len(), 1, "the cut-off frees the slug: {:?}", v.slugs);
    assert!(v.slugs[0].product_area < SLUG_AREA_TOL, "{:?}", v.slugs);
    // A "tabbed" path whose main cut already closes: the tabs hold nothing.
    let mut shut = closed();
    let n = shut.len();
    shut.extend(closed());
    let v = job(&walk(o, &shut), Some(n), false);
    let f = v.findings.iter().find(|f| matches!(f, WireFinding::TabsDoNotHold { .. })).expect("not held");
    assert!(f.describe().contains("the tabs do not hold it"), "{}", f.describe());
}

/// The main cut's boundary is exact (M3 on PR #1158): its last move is
/// judged with it, the cut-off's first move is not. Case A's main cut
/// closes the gap on its LAST move, so the tabs hold nothing; case B's
/// main cut stops one move short and the cut-off's FIRST move closes
/// it, so the tabs hold. Reading one point less misses A's closing
/// move; one point more takes B's cut-off move into the main cut.
#[test]
fn the_main_cut_ends_on_its_own_last_move() {
    let o = Point2::new(0.0, 0.0);
    // A: lead in, round the square, the last main move shuts the left side.
    let main = [(-H, 0.0), (-H, -H), (H, -H), (H, H), (-H, H), (-H, 0.0)];
    let mut pts = main.to_vec();
    pts.push((0.0, 0.0));
    let v = job(&walk(o, &pts), Some(main.len()), false);
    let f = v.findings.iter().find(|f| matches!(f, WireFinding::TabsDoNotHold { .. }));
    assert!(f.is_some() && !v.held, "the last main move closes the cut: {v:?}");
    // B: the main cut leaves the left side's upper half; the cut-off shuts it.
    let main = [(-H, 0.0), (-H, -H), (H, -H), (H, H), (-H, H)];
    let mut pts = main.to_vec();
    pts.extend([(-H, 0.0), (0.0, 0.0)]);
    let v = job(&walk(o, &pts), Some(main.len()), false);
    assert!(v.findings.is_empty(), "the cut-off's first move is not the main cut's: {:?}", v.findings);
    assert!(v.held && v.slugs.len() == 1, "{v:?}");
}
