//! The wire laws on synthetic squares (#534): a clean punch and die
//! pass, a kerf into the part is a gouge naming its move, a missing or
//! crowded start hole is refused, and the slug is read for each side;
//! an open run (#1109) is judged by its kerf and its ends alone.
//! Provenance: our own fixtures.

use super::*;
use crate::cl::{MoveKind, PathBuilder};
use crate::geo2d::Loop;
use crate::geometry::Point3;

const K: f64 = 0.2;

fn square_loop(h: f64) -> Loop {
    Loop::new(vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)]).expect("loop")
}

fn square(h: f64) -> Region {
    Region::new(vec![square_loop(h)]).expect("region")
}

/// A closed square path of half-side `h` threaded from `start` by a
/// straight lead to its left side's middle, out the same way.
fn path(h: f64, start: Point2) -> Toolpath {
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let mut b = PathBuilder::new(p(start.x, start.y));
    for (x, y) in [(-h, 0.0), (-h, -h), (h, -h), (h, h), (-h, h), (-h, 0.0), (start.x, start.y)] {
        b.push(MoveKind::Feed, p(x, y), None);
    }
    b.finish()
}

fn sections(part: &Region, stock: Option<&Region>) -> Vec<WireSection> {
    [0.001, 6.0, 11.999].iter().map(|&z| WireSection { z, part: part.clone(), stock: stock.cloned() }).collect()
}

fn run(side: CutSide, tp: &Toolpath, start: Option<Point2>, part: &Region, stock: Option<&Region>) -> WireVerdict {
    judge(&WireJob { path: tp, kerf_offset: K, skim_offsets: Vec::new(), side, start, sections: sections(part, stock), burnout: false, cutoff_from: None, capture: true, taper: false })
}

#[test]
fn a_correct_punch_passes_and_drops_the_punch() {
    let (part, stock) = (square(5.0), square(15.0));
    let start = Point2::new(-10.0, 0.0);
    let v = run(CutSide::Punch, &path(5.0 + K, start), Some(start), &part, Some(&stock));
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert_eq!(v.slugs.len(), 1, "{:?}", v.slugs);
    assert!((v.slugs[0].product_area - 100.0).abs() < 1e-3, "the punch drops: {:?}", v.slugs);
}

#[test]
fn a_correct_die_passes_whether_the_model_is_the_opening_or_the_plate() {
    let stock = square(15.0);
    let start = Point2::new(0.0, 0.0);
    let tp = path(5.0 - K, start);
    // The model is the opening (#532's whole-model die): the product is the stock around it.
    let v = run(CutSide::Die, &tp, Some(start), &square(5.0), Some(&stock));
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert_eq!(v.slugs.len(), 1);
    // The slug: the 9.2 mm square inside the kerf, less the lead-in's slot.
    let a = v.slugs[0].area;
    assert!(v.slugs[0].product_area < SLUG_AREA_TOL && a < 84.64 && a > 84.64 - 4.6 * 2.0 * K - 0.1, "{:?}", v.slugs);
    // The model is a die plate with the opening as its hole.
    let plate = Region::new(vec![square_loop(15.0), square_loop(5.0)]).expect("plate");
    let v = run(CutSide::Die, &tp, Some(start), &plate, Some(&stock));
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert!(v.slugs.len() == 1 && v.slugs[0].product_area < SLUG_AREA_TOL, "{:?}", v.slugs);
}

#[test]
fn a_kerf_into_the_part_is_a_gouge_naming_the_move() {
    let start = Point2::new(-10.0, 0.0);
    // The path stands only half the kerf off the part.
    let v = run(CutSide::Punch, &path(5.0 + K * 0.5, start), Some(start), &square(5.0), Some(&square(15.0)));
    let g = v.findings.iter().find(|f| matches!(f, WireFinding::Gouge { .. })).expect("gouge");
    let WireFinding::Gouge { move_no, depth, .. } = g else { unreachable!() };
    assert_eq!(*move_no, 1, "the lead-in's end already burns the part");
    assert!((depth - K * 0.5).abs() < 1e-9, "{depth}");
    assert!(g.describe().contains("gouge: the kerf of move 1 enters the part"), "{}", g.describe());
}

#[test]
fn a_missing_moved_or_crowded_start_hole_is_refused() {
    let (part, stock) = (square(5.0), square(15.0));
    let start = Point2::new(-10.0, 0.0);
    let tp = path(5.0 + K, start);
    let v = run(CutSide::Punch, &tp, None, &part, Some(&stock));
    assert_eq!(v.findings, vec![WireFinding::NoStart]);
    assert!(v.findings[0].describe().contains("no start hole is stated"));
    let v = run(CutSide::Punch, &tp, Some(Point2::new(-11.0, 0.0)), &part, Some(&stock));
    assert!(v.findings.iter().any(|f| matches!(f, WireFinding::StartMoved { .. })), "{:?}", v.findings);
    // Threaded 0.1 mm off the part: the wire and its gap need 0.2.
    let near = Point2::new(-5.1, 0.0);
    let v = run(CutSide::Punch, &path(5.0 + K, near), Some(near), &part, Some(&stock));
    let t = v.findings.iter().find(|f| matches!(f, WireFinding::Threading { .. })).expect("threading");
    assert!(t.describe().contains("not clear for threading") && t.describe().contains("0.1000 mm from it"), "{}", t.describe());
}

#[test]
fn the_slug_is_read_for_the_stated_side() {
    let stock = square(15.0);
    // A die plate's hole cut as a punch: only the waste slug drops.
    let plate = Region::new(vec![square_loop(15.0), square_loop(5.0)]).expect("plate");
    let start = Point2::new(0.0, 0.0);
    let v = run(CutSide::Punch, &path(5.0 - K, start), Some(start), &plate, Some(&stock));
    let f = v.findings.iter().find(|f| matches!(f, WireFinding::WrongSlug { .. })).expect("wrong slug");
    assert!(f.describe().contains("holds none of the part"), "{}", f.describe());
    // A punch's outline cut as a die (threaded outside the part): the part drops.
    let out = Point2::new(-10.0, 0.0);
    let v = run(CutSide::Die, &path(5.0 + K, out), Some(out), &square(5.0), Some(&stock));
    let f = v.findings.iter().find(|f| matches!(f, WireFinding::WrongSlug { .. })).expect("wrong slug");
    assert!(f.describe().contains("here the part drops"), "{}", f.describe());
    // No stock around a die plate's hole: nothing drops, and that is said.
    let v = run(CutSide::Die, &path(5.0 - K, start), Some(start), &plate, None);
    assert!(v.findings.iter().any(|f| matches!(f, WireFinding::NothingFreed { .. })), "{:?}", v.findings);
}

/// #540: a burnout must free nothing. Concentric squares cut innermost
/// first burn the opening out and pass; the one die ring alone frees
/// its slug, which for a burnout is a core left to drop.
#[test]
fn a_burnout_frees_nothing_and_a_core_is_refused() {
    let (part, stock) = (square(5.0), square(15.0));
    let start = Point2::new(0.0, 0.0);
    let p = |x: f64, y: f64| Point3::new(x, y, 0.0);
    let mut b = PathBuilder::new(p(0.0, 0.0));
    for j in 1..=32 {
        let h = 0.15 * f64::from(j);
        for (x, y) in [(-h, 0.0), (-h, -h), (h, -h), (h, h), (-h, h), (-h, 0.0)] {
            b.push(MoveKind::Feed, p(x, y), None);
        }
    }
    b.push(MoveKind::Feed, p(0.0, 0.0), None);
    let burnt = b.finish();
    let job = |tp| WireJob { path: tp, kerf_offset: K, skim_offsets: Vec::new(), side: CutSide::Die, start: Some(start), sections: sections(&part, Some(&stock)), burnout: true, cutoff_from: None, capture: false, taper: false };
    let v = judge(&job(&burnt));
    assert!(v.findings.is_empty() && v.slugs.is_empty(), "{:?}", v);
    let ring = path(5.0 - K, start);
    let v = judge(&job(&ring));
    assert!(matches!(v.findings.as_slice(), [WireFinding::Core { .. }]), "{:?}", v.findings);
    assert!(v.findings[0].describe().contains("leave no core"), "{}", v.findings[0].describe());
}

/// #1109: an open run (a cut-off at x = 5 across a 30 mm stock, the part
/// left of it) - the wire at `x`, from `y0` up to `y1`.
fn open_run(x: f64, y0: f64, y1: f64) -> Toolpath {
    let mut b = PathBuilder::new(Point3::new(x, y0, 0.0));
    b.push(MoveKind::Feed, Point3::new(x, y1, 0.0), None);
    b.finish()
}

fn left_part() -> Region {
    let l = Loop::new(vec![Point2::new(-15.0, -15.0), Point2::new(5.0, -15.0), Point2::new(5.0, 15.0), Point2::new(-15.0, 15.0)]);
    Region::new(vec![l.expect("loop")]).expect("region")
}

#[test]
fn an_open_run_is_judged_by_its_kerf_with_no_start_hole_or_slug() {
    let (part, stock) = (left_part(), square(15.0));
    // Clean: the kerf right of travel stands its offset off the part,
    // both ends in the air past the stock. No start hole is stated.
    let v = run(CutSide::Open, &open_run(5.0 + K, -15.0 - 2.0 * K, 15.0 + 2.0 * K), None, &part, Some(&stock));
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert!(v.slugs.is_empty(), "an open run closes no slug: {:?}", v.slugs);
    assert_eq!(v.skipped, Some(OPEN_SKIP), "the skipped laws are said");
    // The kerf on the wrong side burns the part: a gouge, never a pass.
    let v = run(CutSide::Open, &open_run(5.0 - K, -15.0 - 2.0 * K, 15.0 + 2.0 * K), None, &part, Some(&stock));
    assert!(matches!(v.findings.as_slice(), [WireFinding::Gouge { move_no: 1, .. }]), "{:?}", v.findings);
    // An end inside the stock: the wire would be threaded in material.
    let v = run(CutSide::Open, &open_run(5.0 + K, -14.0, 15.0 + 2.0 * K), None, &part, Some(&stock));
    assert!(matches!(v.findings.as_slice(), [WireFinding::OpenEndInStock { .. }]), "{:?}", v.findings);
    assert!(v.findings[0].describe().contains("threaded and cut in the air"), "{}", v.findings[0].describe());
}

/// #538: a rough pass at `K` and a skim at its own smaller kerf `ks`,
/// both from the start hole and back to it - the skim's first move is
/// move 8 (the rough is 7 moves).
fn skimmed(h: f64, rough_at: f64, skim_at: f64, start: Point2) -> Toolpath {
    let mut tp = path(h + rough_at, start);
    tp.moves.extend(path(h + skim_at, start).moves);
    tp
}

fn run_skims(side: CutSide, tp: &Toolpath, start: Point2, skims: Vec<f64>) -> WireVerdict {
    let sections = sections(&square(5.0), Some(&square(15.0)));
    judge(&WireJob { path: tp, kerf_offset: K, skim_offsets: skims, side, start: Some(start), sections, burnout: false, cutoff_from: None, capture: true, taper: false })
}

/// #538 over #534: each move is judged at its own pass's kerf. A skim
/// at its own (smaller) offset sits with its kerf edge on the part, as
/// the rough's does - clean; read at the rough's kerf it would be a
/// false 0.05 mm gouge at move 8 (what the single-offset law said). A
/// skim nearer than its own kerf is still a gouge, by its true depth.
#[test]
fn each_pass_is_judged_at_its_own_kerf() {
    let start = Point2::new(-10.0, 0.0);
    let ks = 0.15;
    let tp = skimmed(5.0, K, ks, start);
    let v = run_skims(CutSide::Punch, &tp, start, vec![ks]);
    assert!(v.findings.is_empty(), "{:?}", v.findings);
    assert!(v.slugs.len() == 1 && (v.slugs[0].product_area - 100.0).abs() < 1e-3, "the punch drops: {:?}", v.slugs);
    // The single-offset reading of the same path: every skim move at K.
    let v = run(CutSide::Punch, &tp, Some(start), &square(5.0), Some(&square(15.0)));
    let g = v.findings.iter().find(|f| matches!(f, WireFinding::Gouge { .. })).expect("the rough kerf misreads the skim");
    assert!(matches!(g, WireFinding::Gouge { move_no: 8, depth, .. } if (depth - (K - ks)).abs() < 1e-6), "{g:?}");
    // A skim path 0.05 mm nearer than its own kerf: a gouge at move 8, 0.05 deep.
    let v = run_skims(CutSide::Punch, &skimmed(5.0, K, ks - 0.05, start), start, vec![ks]);
    let g = v.findings.iter().find(|f| matches!(f, WireFinding::Gouge { .. })).expect("a near skim gouges");
    assert!(matches!(g, WireFinding::Gouge { move_no: 8, depth, .. } if (depth - 0.05).abs() < 1e-6), "{g:?}");
}

/// #538: a path that does not return to its threading point once per
/// stated pass, or an open run stating skims, cannot be split into
/// passes - refused by name, never judged at a guessed kerf.
#[test]
fn passes_that_cannot_be_told_apart_are_unreadable() {
    let start = Point2::new(-10.0, 0.0);
    let one = path(5.0 + K, start);
    let v = run_skims(CutSide::Punch, &one, start, vec![0.15]);
    assert!(matches!(v.findings.as_slice(), [WireFinding::Unreadable(w)] if w.contains("not once per pass")), "{:?}", v.findings);
    let two = skimmed(5.0, K, 0.15, start);
    let v = run_skims(CutSide::Punch, &two, start, vec![0.15, 0.12]);
    assert!(matches!(v.findings.as_slice(), [WireFinding::Unreadable(_)]), "{:?}", v.findings);
    let v = run_skims(CutSide::Punch, &two, start, vec![f64::NAN]);
    assert!(matches!(v.findings.as_slice(), [WireFinding::Unreadable(w)] if w.contains("not a positive number")), "{:?}", v.findings);
    let v = run_skims(CutSide::Open, &open_run(5.0 + K, -20.0, 20.0), start, vec![0.15]);
    assert!(matches!(v.findings.as_slice(), [WireFinding::Unreadable(w)] if w.contains("open run")), "{:?}", v.findings);
}
