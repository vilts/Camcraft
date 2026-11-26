//! #3744: one ring set round a part for Auto, Inside and Outside, on a
//! dog-bone part (two discs of radius 12 at X -15 and X 15, joined by a
//! neck 8 mm wide) in a stock box of 80 x 50, a tool of radius 4 and a
//! step of 2, as a 2D Offset round the part plans it. The planner pins
//! (the same multiset of rings) are in `machining2d::ring_side`; these
//! read the posted path.

use super::{params, rect, region};
use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::geo2d::{Point2, Region};
use camcraft_motion::machining2d::{CornerRelief, PassStart, StartSide, pocket_started_on, rings_fallbacks};

const R: f64 = super::R;
const Z: f64 = super::Z;

fn disc(cx: f64, r: f64) -> Vec<Point2> {
    (0..128)
        .map(|k| {
            let a = f64::from(k) * std::f64::consts::TAU / 128.0;
            Point2::new(cx + r * a.cos(), r * a.sin())
        })
        .collect()
}

/// The part, the stock box, and the band the disc may sweep (the stock
/// grown by the radius, less the part).
fn dog_bone() -> (Region, Region, Region) {
    let part = region(vec![disc(-15.0, 12.0)])
        .union(&region(vec![disc(15.0, 12.0)]))
        .and_then(|p| p.union(&region(vec![rect(-15.0, -4.0, 15.0, 4.0)])))
        .expect("part");
    let stock = region(vec![rect(-40.0, -25.0, 40.0, 25.0)]);
    let band = stock.offset(R).and_then(|s| s.difference(&part)).expect("band");
    (part, stock, band)
}

fn path(start: PassStart, stock: Option<Region>) -> Toolpath {
    let (_, _, band) = dog_bone();
    let mut p = params();
    p.stepover = 2.0;
    let side = StartSide { start, stock, around_part: true };
    let path = pocket_started_on(&band, &p, &[], &CornerRelief::default(), &side).expect("plan");
    assert_eq!(rings_fallbacks(&path.notes), 0, "{start:?}: the one ring set is kept: {:?}", path.notes);
    path
}

/// The floor runs: each chain of planar feeds at the floor, by its two
/// ends.
fn floor_runs(path: &Toolpath) -> Vec<(Point2, Point2)> {
    let floor = |i: usize| {
        path.moves.get(i).is_some_and(|m| m.kind != MoveKind::Rapid && (m.from.z - Z).abs() < 1e-9 && (m.to.z - Z).abs() < 1e-9)
    };
    let mut runs = Vec::new();
    let mut open: Option<Point2> = None;
    for (i, m) in path.moves.iter().enumerate() {
        if !floor(i) {
            continue;
        }
        let start = open.get_or_insert(Point2::new(m.from.x, m.from.y));
        let next = path.moves.get(i + 1).filter(|n| floor(i + 1) && n.from == m.to);
        if next.is_none() {
            runs.push((*start, Point2::new(m.to.x, m.to.y)));
            open = None;
        }
    }
    runs
}

fn edge_dist(q: Point2, r: &Region) -> f64 {
    let seg = |a: Point2, b: Point2| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let t = if dd > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
        (q.x - a.x - t * dx).hypot(q.y - a.y - t * dy)
    };
    r.loops()
        .iter()
        .flat_map(|l| {
            let p = l.points();
            (0..p.len()).map(move |i| (p[i], p[(i + 1) % p.len()]))
        })
        .map(|(a, b)| seg(a, b))
        .fold(f64::INFINITY, f64::min)
}

/// #3744 pin: the Outside rings follow the part. No open floor run of an
/// Outside pass ends on the part wall (the tool centre one radius off the
/// part) before the last, which is the part wall itself; on `main` before
/// #3744, 14 of 14 runs did. A ring that the stock edge cuts ends on the
/// stock edge, never on the part.
#[test]
fn outside_rings_follow_the_part() {
    let (part, stock, _) = dog_bone();
    let runs = floor_runs(&path(PassStart::Outside, Some(stock)));
    assert!(!runs.is_empty(), "the pass cuts the floor");
    let on_part = |q: Point2| (edge_dist(q, &part) - R).abs() < 0.05;
    // The level ends with the part wall, the last ring of Outside.
    let Some((last, early)) = runs.split_last() else { panic!("no run") };
    assert!(on_part(last.1), "the level ends on the part wall: {last:?}");
    let dead: Vec<&(Point2, Point2)> = early
        .iter()
        .filter(|(a, b)| (a.x - b.x).hypot(a.y - b.y) > 0.01 && (on_part(*a) || on_part(*b)))
        .collect();
    assert!(dead.is_empty(), "{} of {} Outside floor runs end on the part wall: {dead:?}", dead.len(), runs.len());
}

/// #3744 pin: Auto gives exactly the path of one of the two sides: of
/// Outside when the stock box leaves the outer edge in air, of Inside
/// when the stock is unknown.
#[test]
fn auto_is_inside_or_outside() {
    let (_, stock, _) = dog_bone();
    let auto = path(PassStart::Auto, Some(stock.clone()));
    let outside = path(PassStart::Outside, Some(stock));
    assert!(auto.moves == outside.moves, "Auto in open air is the Outside path");
    let blind = path(PassStart::Auto, None);
    let inside = path(PassStart::Inside, None);
    assert!(blind.moves == inside.moves, "Auto with no stock footprint is the Inside path");
}

/// #3744: the one ring set posts no crossing feeds from either side (the
/// verifier's crossing law blocks the post on a crossing, #3741).
#[test]
fn the_one_ring_set_posts_no_crossing_feeds() {
    let (_, stock, _) = dog_bone();
    for start in [PassStart::Inside, PassStart::Outside] {
        let path = path(start, Some(stock.clone()));
        let planar = |i: usize| path.moves.get(i).is_some_and(|m| (m.from.z - m.to.z).abs() < 1e-9);
        let crossed: Vec<[usize; 2]> = camcraft_motion::verify::crossed_pairs(&path, 0.01)
            .into_iter()
            .filter(|pr| planar(pr[0]) && planar(pr[1]))
            .collect();
        assert!(crossed.is_empty(), "{start:?}: {crossed:?}");
    }
}
