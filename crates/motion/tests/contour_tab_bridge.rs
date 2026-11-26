//! 2D Contour tab bridge as true material (#2480 review): the narrowest
//! material left at a tab is the smallest distance between the cut on its
//! two sides, less the tool diameter. Measured here from the posted moves:
//! every cutting move below the tab top, split into the runs between the
//! hops, and each run against the next one at its level (leads, plunges and
//! path ends included). Corners, a curved wall, every lead family, inside
//! and conventional.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour, ring, tab_bridge).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour};
use camcraft_motion::path_picks::{PathPick, PathPicks};

const BOTTOM: f64 = -4.0;
const TAB_H: f64 = 1.0;

fn region(pts: Vec<Point2>) -> Region {
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    region(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)])
}

fn disc(r: f64, n: usize) -> Region {
    region(
        (0..n)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / n as f64;
                Point2::new(r * a.cos(), r * a.sin())
            })
            .collect(),
    )
}

fn params(d: f64, side: Side, count: u32, w: f64, lead: Lead) -> ContourParams {
    ContourParams {
        tool_diameter: d,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: BOTTOM,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side,
        lead,
        tolerance: 0.01,
        tab_count: count,
        tab_width: w,
        tab_height: TAB_H,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: Links::PLANAR,
        picks: PathPicks::default(),
    }
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / ll).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

fn orient(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

fn chord_dist((p0, p1): (Point2, Point2), (q0, q1): (Point2, Point2)) -> f64 {
    if orient(p0, p1, q0) * orient(p0, p1, q1) < 0.0 && orient(q0, q1, p0) * orient(q0, q1, p1) < 0.0 {
        return 0.0;
    }
    seg_dist(p0, q0, q1).min(seg_dist(p1, q0, q1)).min(seg_dist(q0, p0, p1)).min(seg_dist(q1, p0, p1))
}

/// The cutting runs below the tab top, in posted order: `(level, chords)`.
/// A run ends at a rapid or at a move that stays at or above the tab top.
fn runs(t: &Toolpath, top: f64, chord: f64) -> Vec<(f64, Vec<(Point2, Point2)>)> {
    let mut out: Vec<(f64, Vec<(Point2, Point2)>)> = Vec::new();
    let mut cur: Option<(f64, Vec<(Point2, Point2)>)> = None;
    for m in &t.moves {
        let low = m.from.z.min(m.to.z);
        if m.kind != MoveKind::Rapid && low < top - 1e-9 {
            let r = cur.get_or_insert((low, Vec::new()));
            r.0 = r.0.min(low);
            r.1.extend(m.chords(chord));
        } else if let Some(r) = cur.take() {
            out.push(r);
        }
    }
    out.extend(cur);
    out
}

/// The true bridge at every tab of every tabbed level: each run against
/// the next run of its level (cyclic), smallest distance less `d`. Also
/// the number of runs at the bottom level.
fn bridges(t: &Toolpath, d: f64) -> (Vec<f64>, usize) {
    let (b, n, _) = bridges_below(t, d, BOTTOM + TAB_H, 0.001);
    (b, n)
}

/// [`bridges`] for a tab top at `top`, and the number of levels below it
/// that have two or more runs.
fn bridges_below(t: &Toolpath, d: f64, top: f64, chord: f64) -> (Vec<f64>, usize, usize) {
    let rs = runs(t, top, chord);
    let mut levels: Vec<f64> = rs.iter().map(|r| r.0).collect();
    levels.sort_by(f64::total_cmp);
    levels.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let mut out = Vec::new();
    let (mut at_bottom, mut tabbed) = (0, 0);
    for z in levels {
        let lv: Vec<&Vec<(Point2, Point2)>> = rs.iter().filter(|r| (r.0 - z).abs() < 1e-9).map(|r| &r.1).collect();
        if (z - BOTTOM).abs() < 1e-9 {
            at_bottom = lv.len();
        }
        if lv.len() < 2 {
            continue;
        }
        tabbed += 1;
        for j in 0..lv.len() {
            let (a, b) = (lv[j], lv[(j + 1) % lv.len()]);
            let gap = a.iter().flat_map(|&x| b.iter().map(move |&y| chord_dist(x, y))).fold(f64::INFINITY, f64::min);
            out.push(gap - d);
        }
    }
    (out, at_bottom, tabbed)
}

/// Arc fit (0.01) and chords (0.001) aside, no bridge is under `w`.
const TOL: f64 = 0.02;

/// Every fixture here fits its tabs: one cut run between each two tabs, and
/// every bridge `>= w`. A level with no cut (the "fill" path) fails.
fn holds(t: &Toolpath, d: f64, w: f64, count: usize, what: &str) {
    let (b, n) = bridges(t, d);
    assert_eq!(n, count, "{what}: one cut run between each two tabs: {b:?} {:?}", t.notes);
    for x in &b {
        assert!(*x >= w - TOL, "{what}: a bridge of {x:.3} mm is under the tab width {w}: {b:?}");
    }
}

#[test]
fn inside_corner_tabs_keep_the_tab_width() {
    // Review B1: inside rect 60 x 60, D = 10 - a 50 x 50 centre ring with
    // sharp corners, the two tab centres 100 mm apart on it, on corners.
    // Before: the end discs stood 9.90 mm apart, under D - no bridge.
    let p = params(10.0, Side::Inside, 2, 4.0, Lead::None);
    let t = contour(&rect(0.0, 0.0, 60.0, 60.0), &p).unwrap();
    let (b, _) = bridges(&t, 10.0);
    assert!(b.iter().all(|x| *x < 4.5), "a tab on a corner, not on a straight run: {b:?}");
    holds(&t, 10.0, 4.0, 2, "inside corner");
}

#[test]
fn outside_tab_centred_on_a_corner_from_a_picked_start() {
    // Review B1, outside: the ring rounds the sharp part corner (50, 0) on
    // an arc of D/2. A picked start puts the last tab window (W + D long,
    // ending on the pick) with its middle on that arc's middle. Climb,
    // outside travels clockwise: after the arc the ring runs along y = -5
    // toward -x, so the pick sits (W + D)/2 less half the arc past it.
    // Before: the chord across the corner left 1.42 mm at W = 4.
    for w in [4.0, 1.5] {
        let d = 10.0;
        let mut p = params(d, Side::Outside, 3, w, Lead::None);
        let half = (w + d) * 0.5;
        let arc_half = std::f64::consts::FRAC_PI_4 * d * 0.5;
        p.picks.start = vec![PathPick::xy(Point2::new(50.0 - (half - arc_half), -d * 0.5))];
        let t = contour(&rect(0.0, 0.0, 50.0, 50.0), &p).unwrap();
        holds(&t, d, w, 3, &format!("picked corner tab, W = {w}"));
    }
}

#[test]
fn bore_tabs_keep_the_tab_width() {
    // Review B1: a bore on a 25 mm centre radius, D = 10, W = 4: the chord
    // of the old window left a 3.82 mm neck.
    let p = params(10.0, Side::Inside, 3, 4.0, Lead::None);
    let t = contour(&disc(30.0, 720), &p).unwrap();
    holds(&t, 10.0, 4.0, 3, "bore");
}

#[test]
fn every_lead_family_keeps_the_tab_width() {
    // Review B2, on the PR's own fixture: 40 x 40, 6 mm cutter, 4 tabs,
    // outside. Before: Linear 4 and Arc r3 / Tangential r2 at W = 3 left no
    // bridge, with no note.
    let leads = [
        Lead::None,
        Lead::Perpendicular { length: 4.0 },
        Lead::Linear { length: 4.0 },
        Lead::Arc { radius: 3.0 },
        Lead::Tangential { radius: 2.0 },
    ];
    for lead in leads {
        for w in [3.0, 6.0, 10.0] {
            let p = params(6.0, Side::Outside, 4, w, lead);
            let t = contour(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
            holds(&t, 6.0, w, 4, &format!("{lead:?}, W = {w}"));
        }
    }
}

#[test]
fn inside_and_conventional_straight_walls_keep_the_tab_width() {
    // Review N3: a straight-wall case inside, and conventional both sides.
    for (side, climb) in [(Side::Inside, true), (Side::Inside, false), (Side::Outside, false)] {
        for lead in [Lead::None, Lead::Arc { radius: 3.0 }] {
            let mut p = params(6.0, side, 4, 5.0, lead);
            p.climb = climb;
            let t = contour(&rect(0.0, 0.0, 80.0, 80.0), &p).unwrap();
            holds(&t, 6.0, 5.0, 4, &format!("{side:?} climb={climb} {lead:?}"));
        }
    }
}

#[test]
fn a_huge_tab_count_fills_the_loop_without_a_huge_allocation() {
    // Review N1: the count comes unchecked from the app or MCP. The
    // windows alone fill the loop, so the fit names it before it reserves
    // one entry per tab (u32::MAX entries would be about 137 GB).
    let p = params(6.0, Side::Outside, u32::MAX, 3.0, Lead::None);
    let t = contour(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
    let (_, n) = bridges(&t, 6.0);
    assert_eq!(n, 0, "nothing is cut at the tabbed level");
    assert!(t.notes.iter().any(|s| s.starts_with("tabs:") && s.contains("fill the whole loop")), "{:?}", t.notes);
}

#[test]
fn every_tabbed_level_keeps_the_tab_width() {
    // #2561: three levels under the tab top (-2, -3, -4 under -1). Each
    // level walks the same fitted keeps, so each has one run between each
    // two tabs and no bridge under the tab width.
    for (side, lead) in [(Side::Outside, Lead::None), (Side::Outside, Lead::Arc { radius: 3.0 }), (Side::Inside, Lead::None)] {
        let mut p = params(6.0, side, 4, 5.0, lead);
        p.stepdown = 1.0;
        p.tab_height = 3.0;
        let t = contour(&rect(0.0, 0.0, 80.0, 80.0), &p).unwrap();
        let (b, n, levels) = bridges_below(&t, 6.0, BOTTOM + 3.0, 0.001);
        assert_eq!(levels, 3, "{side:?} {lead:?}: three tabbed levels: {:?}", t.notes);
        assert_eq!(n, 4, "{side:?} {lead:?}: one cut run between each two tabs at the bottom");
        assert_eq!(b.len(), 12, "{side:?} {lead:?}: four bridges on each of three levels: {b:?}");
        for x in &b {
            assert!(*x >= 5.0 - TOL, "{side:?} {lead:?}: a bridge of {x:.3} mm is under the tab width 5: {b:?}");
        }
    }
}

#[test]
fn a_tab_that_grows_over_a_picked_start_says_so() {
    // #2561: 15 tabs on the 50 x 50 part (D = 10) leave 1.43 mm between
    // the windows. From this pick the first tab stands on the corner at
    // (0, 0): it grows backward by more than that to keep its width, so no
    // cut is left at the pick. The first cut starts after the tab; before,
    // no note said that the start moved.
    let mut p = params(10.0, Side::Outside, 15, 4.0, Lead::None);
    p.picks.start = vec![PathPick::xy(Point2::new(4.5, -5.0))];
    let t = contour(&rect(0.0, 0.0, 50.0, 50.0), &p).unwrap();
    let note = t.notes.iter().find(|s| s.starts_with("tabs:") && s.contains("start point (4.500, -5.000)"));
    assert!(note.is_some_and(|s| s.contains("the tabbed levels start at (")), "{:?}", t.notes);
    let (b, n) = bridges(&t, 10.0);
    assert!((2..15).contains(&n),"fewer cut runs than tabs, and the loop is still cut: {n}");
    for x in &b {
        assert!(*x >= 4.0 - TOL, "a bridge of {x:.3} mm is under the tab width 4: {b:?}");
    }
    // A pick on a straight run keeps its start: no note.
    p.tab_count = 3;
    p.picks.start = vec![PathPick::xy(Point2::new(25.0, -5.0))];
    let t = contour(&rect(0.0, 0.0, 50.0, 50.0), &p).unwrap();
    assert!(t.notes.iter().all(|s| !s.contains("start point")), "{:?}", t.notes);
}

#[test]
fn the_bridge_holds_on_the_true_arcs_to_a_tenth_of_a_micron() {
    // #2561: the fit measures its bridges on arcs flattened in 0.05 mm
    // steps, with 1e-6 mm of slack and no margin for the chord error. Read
    // here on the posted arcs in chords of 1e-5 mm: no bridge is under the
    // tab width by more than 1e-4 mm, at lead arcs down to 0.5 mm and on a
    // bore.
    for lead in [Lead::Arc { radius: 3.0 }, Lead::Tangential { radius: 2.0 }, Lead::Arc { radius: 0.5 }, Lead::Tangential { radius: 0.5 }] {
        let p = params(6.0, Side::Outside, 4, 3.0, lead);
        let t = contour(&rect(0.0, 0.0, 40.0, 40.0), &p).unwrap();
        let (b, n, _) = bridges_below(&t, 6.0, BOTTOM + TAB_H, 1e-5);
        assert_eq!(n, 4, "{lead:?}: {:?}", t.notes);
        assert!(b.iter().all(|x| *x >= 3.0 - 1e-4), "{lead:?}: {b:?}");
    }
    let p = params(10.0, Side::Inside, 3, 4.0, Lead::None);
    let t = contour(&disc(30.0, 720), &p).unwrap();
    let (b, n, _) = bridges_below(&t, 10.0, BOTTOM + TAB_H, 1e-5);
    assert_eq!(n, 3, "bore: {:?}", t.notes);
    assert!(b.iter().all(|x| *x >= 4.0 - 1e-4), "bore: {b:?}");
}
