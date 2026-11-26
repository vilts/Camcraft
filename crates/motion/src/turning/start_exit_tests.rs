//! Pins of the turning pick law (#3331) on a hand-made path over a
//! revolved bar: no pick changes nothing, a held pick lays one rapid and
//! one feed, an arc pass stays whole, and each law refuses by name.
//! Provenance: our own tests; no vendor material.

use super::*;
use crate::cl::{ArcMove, CompSide, CompSpan};
use crate::geo2d::Point2;
use crate::geometry::Point3;

const TAU: f64 = std::f64::consts::TAU;

/// A bar about Z: radius 6, z -16..16.
fn bar() -> Vec<Triangle> {
    let v = |j: usize, r: f64, z: f64| {
        let th = TAU * j as f64 / 16.0;
        Point3::new(r * th.cos(), r * th.sin(), z)
    };
    let axis = |z: f64| Point3::new(0.0, 0.0, z);
    let mut out = Vec::new();
    for j in 0..16 {
        let (a1, a2, b1, b2) = (v(j, 6.0, -16.0), v(j + 1, 6.0, -16.0), v(j, 6.0, 16.0), v(j + 1, 6.0, 16.0));
        out.push(Triangle::new(a1, a2, b2));
        out.push(Triangle::new(a1, b2, b1));
        out.push(Triangle::new(axis(-16.0), a2, a1));
        out.push(Triangle::new(axis(16.0), b1, b2));
    }
    out
}

fn law() -> PickLaw<'static> {
    PickLaw { kind: "Test Kind", nose: 0.8, leave: 0.0, tolerance: 0.05 }
}

/// The world point of lathe (Z, X) in the bar's own frame.
fn at(f: &Frame, z: f64, x: f64) -> Point3 {
    f.point(x, f.station_of(z))
}

/// Home above the bar, a rapid beside it, a plunge to the skin's offset,
/// one pass up the wall, a retract, and home again.
fn path(f: &Frame) -> Toolpath {
    let (home, side, skin, top) = (at(f, 30.0, 12.0), at(f, 0.0, 12.0), at(f, 0.0, 6.8), at(f, 10.0, 6.8));
    Toolpath::new(vec![
        Move::new(MoveKind::Rapid, home, side, None),
        Move::new(MoveKind::Feed, side, skin, Some(60.0)),
        Move::new(MoveKind::Feed, skin, top, Some(120.0)),
        Move::new(MoveKind::Rapid, top, at(f, 10.0, 12.0), None),
        Move::new(MoveKind::Rapid, at(f, 10.0, 12.0), home, None),
    ])
}

fn pick(z: f64, x: f64) -> PathPick {
    PathPick { xy: Point2::new(x, 0.0), z: Some(z) }
}

fn picks(start: Option<PathPick>, exit: Option<PathPick>) -> PathPicks {
    PathPicks { start: start.into_iter().collect(), exit: exit.into_iter().collect() }
}

fn zx(f: &Frame, q: Point3) -> (f64, f64) {
    let (s, r) = f.sr(q);
    (f.height_of(s), r)
}

fn near(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
}

fn refusal(p: &PathPicks, law: &PickLaw) -> String {
    let facets = bar();
    let f = Frame::about(&facets).expect("a frame");
    match with_picks(path(&f), &facets, None, p, law) {
        Err(GenError::Invalid(why)) => why,
        other => panic!("the pick must be refused: {:?}", other.map(|t| t.moves.len())),
    }
}

#[test]
fn no_pick_gives_the_path_back_unchanged() {
    let facets = bar();
    let f = Frame::about(&facets).expect("a frame");
    let plain = path(&f);
    // Not even the law's numbers are read: an old program cannot change.
    let dead = PickLaw { kind: "Test Kind", nose: 0.0, leave: f64::NAN, tolerance: 0.0 };
    let out = with_picks(plain.clone(), &facets, None, &PathPicks::default(), &dead).expect("unchanged");
    assert_eq!(out.moves, plain.moves);
}

#[test]
fn a_start_pick_lays_one_rapid_and_one_feed() {
    let facets = bar();
    let f = Frame::about(&facets).expect("a frame");
    let plain = path(&f);
    let out = with_picks(plain.clone(), &facets, None, &picks(Some(pick(-2.0, 9.0)), None), &law()).expect("held");
    assert_eq!(out.moves.len(), plain.moves.len());
    let (rapid, feed) = (out.moves[0], out.moves[1]);
    assert!(rapid.kind == MoveKind::Rapid && rapid.feed.is_none() && near(zx(&f, rapid.to), (-2.0, 9.0)));
    assert_eq!(rapid.from, plain.moves[0].from, "the rapid leaves from the opening point");
    assert!(feed.kind == MoveKind::Feed && near(zx(&f, feed.from), (-2.0, 9.0)));
    assert_eq!((feed.to, feed.feed), (plain.moves[1].to, plain.moves[1].feed), "the first feed ends where it ended");
    assert_eq!(&out.moves[2..], &plain.moves[2..], "the rest is the kernel's path");
}

#[test]
fn an_exit_pick_lays_one_feed_and_one_rapid_home() {
    let facets = bar();
    let f = Frame::about(&facets).expect("a frame");
    let plain = path(&f);
    let out = with_picks(plain.clone(), &facets, None, &picks(None, Some(pick(14.0, 10.0))), &law()).expect("held");
    assert_eq!(&out.moves[..3], &plain.moves[..3], "the passes are the kernel's");
    assert_eq!(out.moves.len(), 5);
    let (feed, rapid) = (out.moves[3], out.moves[4]);
    assert!(feed.kind == MoveKind::Feed && feed.from == plain.moves[2].to && near(zx(&f, feed.to), (14.0, 10.0)));
    assert_eq!(feed.feed, Some(120.0), "at the last pass's feed");
    assert!(rapid.kind == MoveKind::Rapid && rapid.feed.is_none() && rapid.from == feed.to);
    assert_eq!(rapid.to, plain.moves[4].to, "the program ends home as before");
}

#[test]
fn an_arc_pass_stays_whole() {
    let facets = bar();
    let f = Frame::about(&facets).expect("a frame");
    let mut arcs = path(&f);
    let arc = Some(ArcMove { center: Point2::new(0.0, 0.0), ccw: true, plane: Default::default() });
    arcs.moves[1].arc = arc;
    arcs.moves[2].arc = arc;
    let both = picks(Some(pick(-2.0, 9.0)), Some(pick(14.0, 10.0)));
    let out = with_picks(arcs.clone(), &facets, None, &both, &law()).expect("held");
    // Start: rapid, a line to the arc's own start, then the arc as it was.
    assert!(out.moves[1].arc.is_none() && out.moves[1].to == arcs.moves[1].from);
    assert_eq!(out.moves[2], arcs.moves[1]);
    assert_eq!(out.moves[3], arcs.moves[2]);
    // Exit: a line from the last arc's end, and a rapid home.
    let n = out.moves.len();
    assert!(out.moves[n - 2].arc.is_none() && out.moves[n - 2].from == arcs.moves[2].to);
    assert!(out.moves[n - 1].arc.is_none() && out.moves[n - 1].kind == MoveKind::Rapid);
}

#[test]
fn each_law_refuses_by_name() {
    let l = law();
    let start = |p| picks(Some(p), None);
    let exit = |p| picks(None, Some(p));
    for (p, words) in [
        (start(pick(0.0, 3.0)), &["start point", "inside the stock"][..]),
        (start(pick(0.0, 6.5)), &["start point", "inside the stock"]),
        (exit(pick(-20.0, 3.0)), &["exit point", "chuck side"]),
        (start(PathPick::xy(Point2::new(9.0, 0.0))), &["start point", "needs a Z"]),
        // From home (z 30, r 12) to a start on the axis side of the far end: through the bar.
        (start(pick(17.0, 0.0)), &["start point", "to the first pass", "crosses the part"]),
        // From the pass's end (z 10, r 6.8) through the bar's corner, then along the wall it cut.
        (exit(pick(17.0, 0.0)), &["exit point", "from the last pass", "crosses the part"]),
        (exit(pick(14.0, 6.8)), &["exit point", "runs along the surface"]),
    ] {
        let why = refusal(&p, &l);
        for w in std::iter::once(&"Test Kind").chain(words) {
            assert!(why.contains(w), "the refusal says \"{w}\": {why}");
        }
    }
    let why = refusal(&start(pick(-2.0, 9.0)), &PickLaw { nose: 0.0, ..l });
    assert!(why.contains("nose radius"), "{why}");
}

#[test]
fn a_rapid_through_the_bar_is_refused_by_name() {
    let facets = bar();
    let f = Frame::about(&facets).expect("a frame");
    // The program opens on the far side of the bar's free end, at the axis.
    let mut p = path(&f);
    p.moves[0].from = at(&f, 30.0, 0.0);
    let got = with_picks(p, &facets, None, &picks(Some(pick(0.0, 9.0)), None), &law());
    let Err(GenError::Invalid(why)) = got else { panic!("refused") };
    assert!(why.contains("rapid to the start point") && why.contains("crosses the stock"), "{why}");
}

#[test]
fn a_path_with_a_canned_cycle_takes_no_pick() {
    let facets = bar();
    let f = Frame::about(&facets).expect("a frame");
    let mut p = path(&f);
    p.comps.push(CompSpan { side: CompSide::Left, radius: 0.8, wear: false, first: 1, last: 2, programmed: Vec::new() });
    let got = with_picks(p, &facets, None, &picks(Some(pick(-2.0, 9.0)), None), &law());
    let Err(GenError::Invalid(why)) = got else { panic!("refused") };
    assert!(why.contains("canned cycles"), "{why}");
}

#[test]
fn the_segment_distance_is_zero_only_on_a_crossing() {
    assert_eq!(seg_seg((0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)), 0.0);
    assert!((seg_seg((0.0, 1.0), (0.0, 2.0), (-1.0, 0.0), (1.0, 0.0)) - 1.0).abs() < 1e-12);
    assert!((point_seg((3.0, 4.0), (0.0, 0.0), (0.0, 0.0)) - 5.0).abs() < 1e-12, "a zero-length segment is a point");
}
