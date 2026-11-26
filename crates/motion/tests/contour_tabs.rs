//! 2D Contour tab bridge width (#2480): the tab width is the narrowest
//! width of the bridge that stays, which is on the centre line of the
//! cut channel (the tool-centre ring). The cutter disc reaches `D/2`
//! past each end of a tab window, so the window on the ring is the tab
//! width plus the tool diameter. Measured here as the swept disc of the
//! posted feed moves against points on the tool-centre ring.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour, ring).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour};

const D: f64 = 6.0;
const SIDE: f64 = 40.0;
const BOTTOM: f64 = -4.0;

fn square() -> Region {
    let pts = [(0.0, 0.0), (SIDE, 0.0), (SIDE, SIDE), (0.0, SIDE)];
    Region::new(vec![
        Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap(),
    ])
    .unwrap()
}

fn params(tab_count: u32, tab_width: f64) -> ContourParams {
    ContourParams {
        tool_diameter: D,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: BOTTOM,
        stepdown: 2.0,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count,
        tab_width,
        tab_height: 1.0,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: Links::PLANAR,
        picks: Default::default(),
    }
}

/// Points on the tool-centre ring (the square offset by D/2, round
/// corners), spaced `step` apart along it.
fn centre_ring(step: f64) -> Vec<Point2> {
    let r = D / 2.0;
    let mut out = Vec::new();
    let n_side = (SIDE / step).round() as usize;
    let n_arc = ((std::f64::consts::FRAC_PI_2 * r) / step).round() as usize;
    // Each side, then the corner arc that follows it (counter-clockwise).
    let sides = [
        (
            Point2::new(0.0, -r),
            Point2::new(1.0, 0.0),
            Point2::new(SIDE, 0.0),
            -std::f64::consts::FRAC_PI_2,
        ),
        (
            Point2::new(SIDE + r, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(SIDE, SIDE),
            0.0,
        ),
        (
            Point2::new(SIDE, SIDE + r),
            Point2::new(-1.0, 0.0),
            Point2::new(0.0, SIDE),
            std::f64::consts::FRAC_PI_2,
        ),
        (
            Point2::new(-r, SIDE),
            Point2::new(0.0, -1.0),
            Point2::new(0.0, 0.0),
            std::f64::consts::PI,
        ),
    ];
    for (start, dir, corner, a0) in sides {
        for i in 0..n_side {
            let s = SIDE * i as f64 / n_side as f64;
            out.push(Point2::new(start.x + dir.x * s, start.y + dir.y * s));
        }
        for i in 0..n_arc {
            let a = a0 + std::f64::consts::FRAC_PI_2 * i as f64 / n_arc as f64;
            out.push(Point2::new(corner.x + r * a.cos(), corner.y + r * a.sin()));
        }
    }
    out
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll > 0.0 {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / ll).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

/// The feed chords at the bottom (tabbed) level.
fn bottom_chords(t: &Toolpath) -> Vec<(Point2, Point2)> {
    t.moves
        .iter()
        .filter(|m| {
            m.kind == MoveKind::Feed
                && (m.from.z - BOTTOM).abs() < 1e-9
                && (m.to.z - BOTTOM).abs() < 1e-9
        })
        .flat_map(|m| m.chords(0.001))
        .collect()
}

/// The lengths of the runs of the centre ring that the cutter disc does
/// not reach at the bottom level: the bridge widths on the channel's
/// centre line.
fn bridge_widths(t: &Toolpath, step: f64) -> Vec<f64> {
    let chords = bottom_chords(t);
    let ring = centre_ring(step);
    let r = D / 2.0;
    let uncut: Vec<bool> = ring
        .iter()
        .map(|&p| chords.iter().all(|&(a, b)| seg_dist(p, a, b) > r + 1e-6))
        .collect();
    // Start the walk at a cut point, so no run is split by the wrap.
    let Some(first_cut) = uncut.iter().position(|u| !u) else {
        return vec![ring.len() as f64 * step];
    };
    let n = ring.len();
    let mut widths = Vec::new();
    let mut run = 0usize;
    for k in 1..=n {
        if uncut[(first_cut + k) % n] {
            run += 1;
        } else if run > 0 {
            widths.push(run as f64 * step);
            run = 0;
        }
    }
    widths
}

/// The uncut run of the centre ring is never shorter than the tab width
/// (the sampling step aside). This is the centre-line length only: where
/// a tab is on or near a corner, the true neck (the chord between the two
/// discs, less `D`) is narrower than this run, so the window grows there
/// and the run comes out longer. `contour_tab_bridge.rs` measures the true
/// neck; the upper bound here only says that a tab on this square's
/// straight sides does not grow.
fn at_least(bridge: f64, tab: f64) -> bool {
    bridge >= tab - 0.03 && bridge <= tab + 0.5
}

#[test]
fn a_tab_as_wide_as_the_tool_still_leaves_a_bridge_of_the_tab_width() {
    // #2480: a 6 mm tab with a 6 mm cutter left no bridge at all before.
    let t = contour(&square(), &params(4, D)).unwrap();
    let w = bridge_widths(&t, 0.01);
    assert_eq!(w.len(), 4, "four bridges stand: {w:?} {:?}", t.notes);
    for b in &w {
        assert!(
            at_least(*b, D),
            "each bridge is the tab width on the channel centre line: {w:?}"
        );
    }
}

#[test]
fn a_tab_narrower_than_the_tool_is_not_cut_through() {
    // 5 mm tabs, 6 mm cutter: before #2480 the two bites overlapped by 1 mm.
    let t = contour(&square(), &params(4, 5.0)).unwrap();
    let w = bridge_widths(&t, 0.01);
    assert_eq!(w.len(), 4, "four bridges stand: {w:?}");
    for b in &w {
        assert!(at_least(*b, 5.0), "{w:?}");
    }
}

#[test]
fn a_wide_tab_keeps_its_full_width() {
    // 10 mm tabs: before #2480 the bridge was 10 - 6 = 4 mm. Three tabs on
    // this ring put one across a corner arc: there the chord between the
    // two end discs is shorter than the arc, so that window grows (true
    // neck pinned in contour_tab_bridge.rs) and its centre-line run is
    // longer. The two tabs on straight runs keep the tab width exactly.
    let t = contour(&square(), &params(3, 10.0)).unwrap();
    let w = bridge_widths(&t, 0.01);
    assert_eq!(w.len(), 3, "{w:?}");
    assert!(w.iter().all(|b| *b >= 10.0 - 0.03), "{w:?}");
    assert!(w.iter().filter(|b| at_least(**b, 10.0)).count() >= 2, "{w:?}");
}

#[test]
fn levels_above_the_tab_cut_the_whole_ring() {
    let t = contour(&square(), &params(4, 5.0)).unwrap();
    let above: Vec<(Point2, Point2)> = t
        .moves
        .iter()
        .filter(|m| {
            m.kind == MoveKind::Feed && (m.from.z + 2.0).abs() < 1e-9 && (m.to.z + 2.0).abs() < 1e-9
        })
        .flat_map(|m| m.chords(0.001))
        .collect();
    let r = D / 2.0;
    let missed = centre_ring(0.05)
        .into_iter()
        .filter(|&p| above.iter().all(|&(a, b)| seg_dist(p, a, b) > r + 1e-6))
        .count();
    assert_eq!(missed, 0, "the untabbed level leaves nothing on the ring");
}

#[test]
fn tabs_that_fill_the_loop_are_named_not_silent() {
    // 6 tabs of 25 mm with the 6 mm cutter: 6 x 31 mm > the 178.8 mm
    // centre ring, so the tabbed level has nothing to cut - said so.
    let t = contour(&square(), &params(6, 25.0)).unwrap();
    assert!(
        bottom_chords(&t).is_empty(),
        "the tabbed level cuts nothing"
    );
    assert!(
        t.notes.iter().any(|n| n.contains("fill the whole loop")),
        "{:?}",
        t.notes
    );
}
