//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::{Loop, Point2, Region};
use super::*;

#[test]
fn band_snaps_to_both_coverage_centers() {
    assert_eq!(band(0.0, 20.0, 5.0, 5.0).ok(), Some(vec![5.0, 10.0, 15.0]));
    assert_eq!(band(0.0, 20.0, 5.0, 12.0).ok(), Some(vec![5.0, 15.0]));
    assert_eq!(band(0.0, 20.0, 5.0, 7.0).ok(), Some(vec![5.0, 12.0, 15.0]));
    // Thinner than one tool: the single center row, still covering.
    assert_eq!(band(0.0, 6.0, 5.0, 3.0).ok(), Some(vec![3.0]));
}

fn ring(n: usize) -> Ring {
    // Unit circle-ish square: 4 vertices, perimeter 4 (0,0)-(1,0)-(1,1)-(0,1).
    let _ = n;
    Ring::new(vec![
        Point2::new(0.0, 0.0),
        Point2::new(1.0, 0.0),
        Point2::new(1.0, 1.0),
        Point2::new(0.0, 1.0),
    ])
}

#[test]
fn ring_walks_by_arc_length() {
    let r = ring(0);
    assert_eq!(r.len, 4.0);
    assert_eq!(r.at(0.5), Point2::new(0.5, 0.0));
    assert_eq!(r.at(1.5), Point2::new(1.0, 0.5));
    assert_eq!(r.at(4.5), r.at(0.5), "wraps");
    let closed = r.arc_between(0.0, r.len);
    assert_eq!(&closed[..4], &r.pts[..], "full ring = all vertices");
    assert_eq!(closed[4], closed[0], "closes exactly on the start");
    assert_eq!(closed.len(), 5);
    let mid = r.arc_between(0.5, 2.5);
    assert_eq!(
        mid,
        vec![
            Point2::new(0.5, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.5, 1.0),
        ]
    );
}

#[test]
fn tab_windows_leave_even_keeps_and_merge_across_the_seam() {
    let r = ring(0);
    // Two tabs (width 1) centered at s = 1 and 3: the keeps between
    // and around them; the seam keeps merge into one crossing arc.
    assert_eq!(r.tab_keeps(2, 1.0), vec![(3.5, 4.5), (1.5, 2.5)]);
    let k = r.tab_keeps(2, 0.5);
    assert_eq!(k, vec![(3.25, 4.75), (1.25, 2.75)]);
    // Tabs touching end to end leave nothing to cut.
    assert_eq!(r.tab_keeps(4, 1.0 + 1e-9), Vec::new());
}

fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ])
    .expect("test loops are valid")
}

#[test]
fn scan_row_agrees_with_contains() {
    // 40x40 plate with a 10x10 island dead center.
    let region = Region::new(vec![sq(0.0, 0.0, 40.0, 40.0), sq(15.0, 15.0, 25.0, 25.0)])
        .expect("nested loops");
    let segs = scan_row(&region, 20.0, false);
    assert_eq!(segs, vec![(0.0, 15.0), (25.0, 40.0)]);
    for &(a, b) in &segs {
        assert!(region.contains(Point2::new((a + b) * 0.5, 20.0)));
    }
    assert!(!region.contains(Point2::new(20.0, 20.0)), "island interior");

    // The island's local minimum row (y = 15) has both bottom edges
    // counted by the lower-inclusive rule and splits like any other
    // straddling row; only a local maximum bridges. At y = 25 the
    // island contributes no crossings, and the one interval's points
    // over the island's span graze its top wall without entering it.
    assert_eq!(scan_row(&region, 15.0, false), vec![(0.0, 15.0), (25.0, 40.0)]);
    assert_eq!(scan_row(&region, 25.0, false), vec![(0.0, 40.0)]);

    // The topmost row needs the upper-inclusive flip to find its
    // crossings at all.
    assert_eq!(scan_row(&region, 40.0, false), Vec::new());
    assert_eq!(scan_row(&region, 40.0, true), vec![(0.0, 40.0)]);
}

#[test]
fn chord_clears_separates_grazing_from_crossing() {
    let region = Region::new(vec![sq(0.0, 0.0, 40.0, 40.0), sq(15.0, 15.0, 25.0, 25.0)])
        .expect("nested loops");
    let p = |x: f64, y: f64| Point2::new(x, y);
    // Through the island: refused.
    assert!(!chord_clears(&region, p(5.0, 20.0), p(35.0, 20.0)));
    // Along the outer wall: the tangency a row connector uses.
    assert!(chord_clears(&region, p(40.0, 5.0), p(40.0, 35.0)));
    // Inside material, no crossings.
    assert!(chord_clears(&region, p(5.0, 5.0), p(10.0, 35.0)));
    // Starting outside: refused.
    assert!(!chord_clears(&region, p(-1.0, 5.0), p(35.0, 20.0)));
    // Boundary corners read as closed-inside.
    assert!(chord_clears(&region, p(40.0, 40.0), p(40.0, 30.0)));
}

fn contour_side(side: Side) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 10.0,
        retract_z: 5.0,
        top_z: 1.0,
        bottom_z: 1.0,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: crate::entry::Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false,
        links: crate::links::Links::PLANAR,
    }
}

/// XY of every point the tool CUTS at (plunges and feeds, no rapids).
fn cut_xy(path: &Toolpath) -> Vec<Point2> {
    path.moves
        .iter()
        .filter(|m| m.kind != MoveKind::Rapid)
        .map(|m| Point2::new(m.to.x, m.to.y))
        .collect()
}

/// Twice the signed area of the point sequence walked in order:
/// negative means clockwise (CCW is positive).
fn signed_walk(pts: &[Point2]) -> f64 {
    let mut s = 0.0;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        s += a.x * b.y - b.x * a.y;
    }
    s
}

#[test]
fn contour_inside_shrinks_and_flips_travel() {
    // A 20x20 square region eroded by r = 3 gives the sharp inset
    // [23,37]^2 (negative round offset keeps convex corners sharp);
    // #1555: climb (M3) keeps the material on the RIGHT of travel, and
    // inside a bore the material stands outside the ring - so climb
    // walks the stored CCW ring as it is: counter-clockwise.
    let region = Region::new(vec![sq(20.0, 20.0, 40.0, 40.0)]).expect("test square");
    let path = contour(&region, &contour_side(Side::Inside)).expect("inside contour");
    let pts = cut_xy(&path);
    // M8.2 A: the level's first cut is the certified dive into the bore's
    // bounded air, then the link joins the ring at its entry vertex.
    let (dive, ring) = pts.split_first().expect("a dive and a ring");
    let edge = |v: f64| (v - 23.0).abs() < 1e-6 || (v - 37.0).abs() < 1e-6;
    assert!(
        !edge(dive.x) && !edge(dive.y) && dive.x > 23.0 && dive.x < 37.0 && dive.y > 23.0
        && dive.y < 37.0,
        "the dive must land inside the inset, not on its wall: {dive:?}"
    );
    assert!(
        ring.iter().all(|p| edge(p.x) && edge(p.y)),
        "cut points off the inset square: {pts:?}"
    );
    let corner = |cx: f64, cy: f64| {
        ring.iter()
            .any(|p| (p.x - cx).abs() < 1e-6 && (p.y - cy).abs() < 1e-6)
    };
    assert!(
        corner(23.0, 23.0)
            && corner(37.0, 23.0)
            && corner(37.0, 37.0)
            && corner(23.0, 37.0),
        "inset ring corners missing: {pts:?}"
    );
    assert!(signed_walk(ring) > 0.0, "climb inside a bore runs counter-clockwise (M3)");
}

#[test]
fn conventional_inside_travels_reversed() {
    // Climb is the process, not a winding: conventional Inside walks
    // the stored (CCW) ring reversed, the material on the left (#1555).
    let region = Region::new(vec![sq(20.0, 20.0, 40.0, 40.0)]).expect("test square");
    let mut p = contour_side(Side::Inside);
    p.climb = false;
    let path = contour(&region, &p).expect("inside contour");
    let pts = cut_xy(&path);
    // M8.2 A: the certified dive leads, the reversed ring follows.
    let (dive, ring) = pts.split_first().expect("a dive and a ring");
    assert!(
        dive.x > 23.0 && dive.x < 37.0 && dive.y > 23.0 && dive.y < 37.0,
        "the dive must land inside the inset, not on its wall: {dive:?}"
    );
    assert!(ring.iter().all(|q| (q.x - 23.0).abs() < 1e-6 || (q.x - 37.0).abs() < 1e-6));
    assert!(signed_walk(ring) < 0.0, "conventional inside a bore runs clockwise (M3)");
}

#[test]
fn contour_auto_matches_outside() {
    // Auto is the Outside law inside the generator: explicit-region
    // callers (goldens, direct tests) stay byte-for-byte today's path.
    let region = Region::new(vec![sq(20.0, 20.0, 40.0, 40.0)]).expect("test square");
    let a = contour(&region, &contour_side(Side::Auto)).expect("auto contour");
    let o = contour(&region, &contour_side(Side::Outside)).expect("outside contour");
    assert_eq!(a, o, "Auto must resolve to Outside here");
}

#[test]
fn inside_on_small_bore_refuses() {
    // A 4x4 ring eroded by r = 3 collapses: named refusal, never a
    // clamped path (Prime Directive).
    let region = Region::new(vec![sq(18.0, 18.0, 22.0, 22.0)]).expect("test square");
    match contour(&region, &contour_side(Side::Inside)) {
        Err(GenError::Empty(msg)) => {
            assert!(msg.contains("narrower than the tool"), "{msg}")
        }
        other => panic!("expected a named Empty refusal, got {other:?}"),
    }
}

// --- M2.4 C: the boring ride and the thread helix ---------------------

fn bore_hole(xy: [f64; 2], radius: f64, stepped: bool) -> BoreHole {
    BoreHole {
        xy: Point2::new(xy[0], xy[1]),
        radius,
        top_z: 0.0,
        floor_z: -4.0,
        through: false,
        stepped,
    }
}

fn bore_params(holes: Vec<BoreHole>, dia: f64) -> BoreParams {
    BoreParams {
        links: crate::links::Links::PLANAR,
        holes,
        tool_diameter: dia,
        corner_radius: 0.0,
        stock_to_leave: 0.0,
        stepdown: 2.0,
        through_allowance: 1.0,
        clearance_z: 5.0,
        retract_z: None,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        tolerance: 0.01,
        entry: crate::entry::Entry3D::Plunge,
        helix_radius: 0.0,
    }
}

#[test]
fn bore_rides_the_wall_circle() {
    // A 6 mm mill boring a 16 mm hole keeps its CENTER 5 mm from the
    // axis - the wall circle minus one tool radius, at every cut move.
    // (#1556: the 20 mm hole this read once left a core - refused now.)
    let path = bore(&bore_params(vec![bore_hole([0.0, 0.0], 8.0, false)], 6.0))
        .expect("a 6 mm mill fits a 16 mm bore");
    let cuts: Vec<_> = path.moves.iter().filter(|m| m.kind != MoveKind::Rapid).collect();
    assert!(!cuts.is_empty(), "a bored hole posts cut moves");
    for m in &cuts {
        // Radial sweeps pass through the center by law; ring chords
        // and arcs stand on rc = 5 (lattice is 1 um).
        let rad = (m.to.x * m.to.x + m.to.y * m.to.y).sqrt();
        let from = (m.from.x * m.from.x + m.from.y * m.from.y).sqrt();
        assert!(
            (rad - 5.0).abs() < 1e-3 || rad < 5.0 + 1e-3,
            "cut point left the bore circle: {:?}", m.to
        );
        let _ = from;
    }
    // Two levels (0, -2, -4 via cut_levels): the ladder reaches floor.
    assert!(cuts.iter().any(|m| (m.to.z + 4.0).abs() < 1e-9), "floor cut");
}

#[test]
fn bore_refuses_a_mill_it_cannot_turn() {
    // 24 mm tool in a 20 mm hole: rc would stand negative - named,
    // never an empty path.
    let err = bore(&bore_params(vec![bore_hole([0.0, 0.0], 10.0, false)], 24.0))
        .expect_err("no circle left inside");
    assert!(format!("{err:?}").contains("no circle left inside"), "{err:?}");
}

#[test]
fn bore_declines_the_counterbore_fold() {
    let err = bore(&bore_params(vec![bore_hole([3.0, 4.0], 6.0, true)], 4.0))
        .expect_err("a stepped hole is declined by name");
    assert!(format!("{err:?}").contains("counterbore fold"), "{err:?}");
}
