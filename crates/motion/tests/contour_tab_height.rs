//! 2D Contour tab height (#2579): the top of a tab is at the bottom plus
//! the tab height for any stepdown. The stepdown ladder gets one more
//! level at that plane when no level is there, so the last full ring is
//! cut at the tab top and not one stepdown above the bottom. Measured
//! here from the posted feed moves: for each point of the tool-centre
//! line inside a tab window, the lowest level whose pass goes through it.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour, contour_params, contour_whole).

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour, contour_along_chain, contour_whole_bodies};

const D: f64 = 6.0;
const SIDE: f64 = 40.0;

fn square() -> Region {
    let pts = [(0.0, 0.0), (SIDE, 0.0), (SIDE, SIDE), (0.0, SIDE)];
    Region::new(vec![Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()]).unwrap()
}

fn params(bottom_z: f64, stepdown: f64, tab_height: f64) -> ContourParams {
    ContourParams {
        tool_diameter: D,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z,
        stepdown,
        feed: 500.0,
        plunge_feed: 100.0,
        climb: true,
        side: Side::Outside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 4,
        tab_width: 5.0,
        tab_height,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: Links::PLANAR,
        picks: Default::default(),
    }
}

fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let ll = dx * dx + dy * dy;
    let t = if ll > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / ll).clamp(0.0, 1.0) } else { 0.0 };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

/// The level passes: each feed move that stays at one height, as its
/// height and its chords.
fn passes(t: &Toolpath) -> Vec<(f64, Vec<(Point2, Point2)>)> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - m.to.z).abs() < 1e-12)
        .map(|m| (m.to.z, m.chords(0.001)))
        .collect()
}

/// The distinct heights of the level passes, from the top down.
fn level_zs(t: &Toolpath) -> Vec<f64> {
    let mut zs: Vec<f64> = passes(t).iter().map(|(z, _)| *z).collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    zs
}

/// The heights of the tabs above the bottom: for each point of the four
/// straight tool-centre lines that the bottom level does not go through,
/// the lowest level that goes through it, less the bottom.
fn tab_heights(t: &Toolpath, bottom: f64) -> Vec<f64> {
    let r = D / 2.0;
    let ps = passes(t);
    let on = |p: Point2, z: f64| {
        ps.iter().any(|(pz, ch)| (pz - z).abs() < 1e-9 && ch.iter().any(|&(a, b)| seg_dist(p, a, b) < 1e-6))
    };
    let zs = level_zs(t);
    let mut out = Vec::new();
    for k in 0..=160 {
        let s = SIDE * f64::from(k) / 160.0;
        let line = [
            Point2::new(s, -r),
            Point2::new(SIDE + r, s),
            Point2::new(s, SIDE + r),
            Point2::new(-r, s),
        ];
        for p in line {
            if on(p, bottom) {
                continue;
            }
            // `zs` goes down, so the last level that goes through `p` is
            // the lowest one: the top of the material left here.
            let top = zs.iter().copied().rfind(|&z| on(p, z));
            out.push(top.map_or(f64::INFINITY, |z| z - bottom));
        }
    }
    out
}

fn assert_tab_height(bottom: f64, stepdown: f64, tab_height: f64) {
    let t = contour(&square(), &params(bottom, stepdown, tab_height)).unwrap();
    let hs = tab_heights(&t, bottom);
    assert!(hs.len() >= 8, "tabs stand at the bottom level: {} points, levels {:?}", hs.len(), level_zs(&t));
    for h in &hs {
        assert!(
            (h - tab_height).abs() < 1e-9,
            "tab height {h} is not {tab_height} (stepdown {stepdown}, levels {:?})",
            level_zs(&t)
        );
    }
}

#[test]
fn a_stepdown_larger_than_the_tab_height_keeps_the_tab_height() {
    // The report: 4 mm stepdown, 1.5 mm tabs. The tabs were 4 mm tall.
    assert_tab_height(-8.0, 4.0, 1.5);
}

#[test]
fn a_single_level_cut_keeps_the_tab_height() {
    // One stepdown is the whole depth: before, no full ring was cut.
    assert_tab_height(-4.0, 4.0, 1.5);
}

#[test]
fn a_stepdown_smaller_than_the_tab_height_keeps_the_tab_height() {
    // 1 mm stepdown, 1.5 mm tabs: the tabs were 2 mm tall.
    assert_tab_height(-4.0, 1.0, 1.5);
}

#[test]
fn the_tab_top_level_is_one_more_level_in_the_ladder() {
    let t = contour(&square(), &params(-8.0, 4.0, 1.5)).unwrap();
    assert_eq!(level_zs(&t), vec![-4.0, -6.5, -8.0]);
}

#[test]
fn a_level_already_at_the_tab_top_adds_no_level() {
    let t = contour(&square(), &params(-4.0, 2.0, 2.0)).unwrap();
    assert_eq!(level_zs(&t), vec![-2.0, -4.0]);
    assert_tab_height(-4.0, 2.0, 2.0);
}

#[test]
fn a_tab_as_tall_as_the_cut_adds_no_level() {
    // The tab top is at Top Z or above it: each level has the tabs.
    let t = contour(&square(), &params(-4.0, 2.0, 4.0)).unwrap();
    assert_eq!(level_zs(&t), vec![-2.0, -4.0]);
    let t = contour(&square(), &params(-4.0, 2.0, 6.0)).unwrap();
    assert_eq!(level_zs(&t), vec![-2.0, -4.0]);
}

#[test]
fn no_tabs_adds_no_level() {
    let p = ContourParams { tab_count: 0, ..params(-8.0, 4.0, 1.5) };
    let t = contour(&square(), &p).unwrap();
    assert_eq!(level_zs(&t), vec![-4.0, -8.0]);
}

#[test]
fn an_open_chain_has_no_tabs_and_gets_no_tab_level() {
    let run = [Point2::new(0.0, 0.0), Point2::new(SIDE, 0.0)];
    let t = contour_along_chain(&run, &params(-8.0, 4.0, 1.5)).unwrap();
    assert_eq!(level_zs(&t), vec![-4.0, -8.0]);
}

#[test]
fn a_level_one_float_step_under_the_tab_top_is_the_tab_top() {
    // 0.2 mm steps from 0 give -3.0000000000000004, under the tab top at
    // -4 + 1 = -3: that level carried the tabs, and they were 1.2 mm tall.
    let t = contour(&square(), &params(-4.0, 0.2, 1.0)).unwrap();
    assert_eq!(level_zs(&t).len(), 20, "no level is added: {:?}", level_zs(&t));
    assert_tab_height(-4.0, 0.2, 1.0);
}

/// The square as a closed block from `bottom` up to Z 0.
fn block(bottom: f64) -> Vec<Triangle> {
    let c = [(0.0, 0.0), (SIDE, 0.0), (SIDE, SIDE), (0.0, SIDE)];
    let at = |k: usize, z: f64| Point3::new(c[k % 4].0, c[k % 4].1, z);
    let mut f = vec![
        Triangle::new(at(0, 0.0), at(1, 0.0), at(2, 0.0)),
        Triangle::new(at(0, 0.0), at(2, 0.0), at(3, 0.0)),
        Triangle::new(at(0, bottom), at(2, bottom), at(1, bottom)),
        Triangle::new(at(0, bottom), at(3, bottom), at(2, bottom)),
    ];
    for k in 0..4 {
        f.push(Triangle::new(at(k, bottom), at(k + 1, bottom), at(k + 1, 0.0)));
        f.push(Triangle::new(at(k, bottom), at(k + 1, 0.0), at(k, 0.0)));
    }
    f
}

#[test]
fn a_whole_model_contour_keeps_the_tab_height() {
    let t = contour_whole_bodies(&square(), &block(-8.0), &[], &params(-8.0, 4.0, 1.5)).unwrap();
    assert_eq!(level_zs(&t), vec![-4.0, -6.5, -8.0], "{:?}", t.notes);
    let hs = tab_heights(&t, -8.0);
    assert!(hs.len() >= 8 && hs.iter().all(|h| (h - 1.5).abs() < 1e-9), "{hs:?}");
}
