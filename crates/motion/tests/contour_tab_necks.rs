//! 2D Contour tabs against every cut of their level (#2561): the neck of each
//! tab, read from the material that the posted moves leave (shape mask), for
//! a tab beside a U-turn of its own ring and a tab beside a second ring.
//!
//! Gate map (M4.4): covers camcraft_motion::machining2d (contour, tab_bridge, tab_clear).

mod common;

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::geometry::{Point3, Triangle};
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour, contour_on_part};
use camcraft_motion::path_picks::{PathPick, PathPicks};
use common::tab_neck::{Neck, tab_necks};

const D: f64 = 6.0;
const W: f64 = 4.0;
const BOTTOM: f64 = -4.0;
const TAB_H: f64 = 1.0;
const CELL: f64 = 0.1;

fn lp(pts: &[(f64, f64)]) -> Loop {
    Loop::new(pts.iter().map(|&(x, y)| Point2::new(x, y)).collect()).unwrap()
}

fn params(count: u32) -> ContourParams {
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
        tab_count: count,
        tab_width: W,
        tab_height: TAB_H,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: Links::PLANAR,
        picks: PathPicks::default(),
    }
}

fn necks(part: &Region, t: &Toolpath) -> Vec<Neck> {
    tab_necks(part, t, D, BOTTOM + TAB_H, CELL).unwrap()
}

/// A 60 x 40 part with a notch `g` wide in its top edge, its floor at
/// `y = floor` (25 deep at `floor = 15`).
fn notched_to(g: f64, floor: f64) -> Region {
    let (a, b) = (30.0 - g * 0.5, 30.0 + g * 0.5);
    Region::new(vec![lp(&[(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (b, 40.0), (b, floor), (a, floor), (a, 40.0), (0.0, 40.0)])]).unwrap()
}

/// Two parts `g` apart: 30 x 30, and 30 x 50 beside it.
fn pair(g: f64) -> Region {
    Region::new(vec![
        lp(&[(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)]),
        lp(&[(30.0 + g, 0.0), (60.0 + g, 0.0), (60.0 + g, 50.0), (30.0 + g, 50.0)]),
    ])
    .unwrap()
}

/// The meter reads the tab width to about one cell.
const TOL: f64 = 0.25;

/// A loop is held: the note names the cut that reaches its tab.
fn held(t: &Toolpath) -> usize {
    t.notes.iter().filter(|s| s.starts_with("tabs:") && s.contains("cuts into it") && s.contains("cut nothing on this loop")).count()
}

/// The number of tabs, each with a neck of the tab width or more.
fn all_hold(part: &Region, t: &Toolpath, what: &str) -> usize {
    let n = necks(part, t);
    for k in &n {
        assert!(k.width >= W - TOL, "{what}: the tab at ({:.2}, {:.2}) has a neck of {:.3} mm, under the tab width {W}: {:?}", k.at.x, k.at.y, k.width, t.notes);
    }
    n.len()
}

/// Nothing is cut under the tab top.
fn cuts_nothing_at_the_tabs(t: &Toolpath) -> bool {
    t.moves.iter().all(|m| m.kind == MoveKind::Rapid || m.from.z.min(m.to.z) >= BOTTOM + TAB_H - 1e-9)
}

#[test]
fn the_meter_reads_the_tab_width_on_a_plain_part() {
    let part = Region::new(vec![lp(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])]).unwrap();
    let t = contour(&part, &params(4)).unwrap();
    assert_eq!(all_hold(&part, &t, "plain"), 4);
    assert!(necks(&part, &t).iter().all(|k| k.width <= W + TOL), "the tabs are not wider than asked");
    assert!(t.notes.iter().all(|s| !s.starts_with("tabs:")), "{:?}", t.notes);
}

/// The notched part with two tabs, the second one on the notch wall.
fn notch_path(g: f64) -> (Region, Toolpath) {
    notch_path_to(g, 15.0)
}

fn notch_path_to(g: f64, floor: f64) -> (Region, Toolpath) {
    let part = notched_to(g, floor);
    let mut p = params(2);
    p.picks.start = vec![PathPick::xy(Point2::new(30.0 - g * 0.5 - D * 0.5, 30.0))];
    let t = contour(&part, &p).unwrap();
    (part, t)
}

#[test]
fn a_tab_that_a_u_turn_of_its_ring_cuts_into_holds_the_loop() {
    // A 10 mm notch and a 6 mm cutter: the ring goes down the notch and
    // comes back 4 mm beside itself. The return run cut the tab on the
    // notch wall off its stock (a neck of 0), with no note.
    let (part, t) = notch_path(10.0);
    assert_eq!(held(&t), 1, "{:?} necks {:?}", t.notes, necks(&part, &t));
    assert!(t.notes.iter().any(|s| s.contains("passes 4.000 mm from the tab")), "{:?}", t.notes);
    assert!(cuts_nothing_at_the_tabs(&t), "a held loop cuts nothing under the tab top");
}

#[test]
fn a_tab_beside_a_narrow_strip_is_named() {
    // A 14 mm notch: the two runs are 8 mm apart, so 2 mm of stock stands
    // between the kerfs. The tab keeps its width; the strip beside it is
    // narrower, and a note says so.
    let (part, t) = notch_path(14.0);
    assert_eq!(held(&t), 0, "{:?}", t.notes);
    assert!(t.notes.iter().any(|s| s.starts_with("tabs:") && s.contains("leaves only 2.000 mm of material beside it")), "{:?}", t.notes);
    let n = necks(&part, &t);
    assert_eq!(n.len(), 2, "{n:?}");
    let narrow = n.iter().map(|k| k.width).fold(f64::INFINITY, f64::min);
    assert!((narrow - 2.0).abs() <= TOL, "the strip is the neck: {n:?}");
}

#[test]
fn a_tab_with_room_beside_it_has_no_note() {
    // A 20 mm notch: 8 mm of stock between the kerfs, the tab width twice.
    let (part, t) = notch_path(20.0);
    assert_eq!(all_hold(&part, &t, "wide notch"), 2);
    assert!(t.notes.iter().all(|s| !s.starts_with("tabs:")), "{:?}", t.notes);
    let both = pair(20.0);
    let t = contour(&both, &params(4)).unwrap();
    assert_eq!(all_hold(&both, &t, "wide pair"), 8);
    assert!(t.notes.iter().all(|s| !s.starts_with("tabs:")), "{:?}", t.notes);
}

#[test]
fn a_tab_that_a_second_ring_cuts_into_holds_the_loop() {
    // Two parts 10 mm apart: each ring passes 4 mm from a tab of the other
    // one and cut it off (a neck of 0), with no note. Both loops are held.
    let both = pair(10.0);
    let t = contour(&both, &params(4)).unwrap();
    assert_eq!(held(&t), 2, "{:?} necks {:?}", t.notes, necks(&both, &t));
    assert!(cuts_nothing_at_the_tabs(&t), "held loops cut nothing under the tab top");
    // 14 mm apart: 2 mm of stock between the kerfs, named for each loop.
    let both = pair(14.0);
    let t = contour(&both, &params(4)).unwrap();
    assert_eq!(held(&t), 0, "{:?}", t.notes);
    let named = t.notes.iter().filter(|s| s.starts_with("tabs:") && s.contains("leaves only 2.000 mm of material beside it")).count();
    assert_eq!(named, 2, "{:?}", t.notes);
    assert_eq!(necks(&both, &t).len(), 8);
}

#[test]
fn a_tab_that_a_u_turn_in_a_shallow_notch_cuts_into_holds_the_loop() {
    // Review of #2705: an 18 mm deep notch, 10 mm wide. The return run
    // passes 4 mm from the tab on the notch wall, but the ring turns back
    // before it is `W + D` from the tab. That return run was not read: no
    // note, and a neck of 0.
    let (part, t) = notch_path_to(10.0, 22.0);
    assert_eq!(held(&t), 1, "{:?} necks {:?}", t.notes, necks(&part, &t));
    assert!(t.notes.iter().any(|s| s.contains("passes 4.000 mm from the tab")), "{:?}", t.notes);
    assert!(cuts_nothing_at_the_tabs(&t), "a held loop cuts nothing under the tab top");
}

/// A box, its faces wound outward.
fn boxf(min: [f64; 3], max: [f64; 3]) -> Vec<Triangle> {
    let v = |i: usize| Point3::new([min[0], max[0]][i & 1], [min[1], max[1]][(i >> 1) & 1], [min[2], max[2]][(i >> 2) & 1]);
    let quad = |a: usize, b: usize, c: usize, d: usize| [Triangle::new(v(a), v(b), v(c)), Triangle::new(v(a), v(c), v(d))];
    [quad(4, 5, 7, 6), quad(0, 2, 3, 1), quad(0, 1, 5, 4), quad(2, 6, 7, 3), quad(1, 3, 7, 5), quad(2, 0, 4, 6)].concat()
}

#[test]
fn a_ring_that_stops_above_the_tabs_does_not_hold_another_loop() {
    // Review of #2705: two islands 10 mm apart. The second one stands on a
    // shelf at Z = -1, so its ring stops at -1 and cuts nothing at the
    // tabbed level -4. The deep island was held for a cut that is not there.
    let part = pair(10.0);
    let mut mesh = boxf([0.0, 0.0, BOTTOM], [30.0, 30.0, 0.0]);
    mesh.extend(boxf([40.0, 0.0, -1.0], [70.0, 50.0, 0.0]));
    mesh.extend(boxf([39.0, -5.0, BOTTOM], [75.0, 55.0, -1.0]));
    let mut p = params(4);
    p.stepdown = 1.0;
    let t = contour_on_part(&part, &mesh, &p).unwrap();
    assert_eq!(held(&t), 0, "{:?}", t.notes);
    let deep: Vec<_> = t.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.from.z.min(m.to.z) < -1.0 - 1e-9).collect();
    assert!(deep.iter().all(|m| m.from.x.max(m.to.x) < 37.0), "the shelf ring cuts below -1");
    assert!(
        deep.iter().any(|m| (m.from.z - BOTTOM).abs() < 1e-9 && (m.to.z - BOTTOM).abs() < 1e-9),
        "the deep island is cut at the tabbed level"
    );
}

/// The second review of #2705: the loop is held by name and cuts nothing
/// under the tab top, or every tab keeps the tab width. Never a thin fin.
/// Returns true when the loop is held.
fn held_or_whole(part: &Region, t: &Toolpath, tabs: usize, what: &str) -> bool {
    if held(t) > 0 {
        assert!(cuts_nothing_at_the_tabs(t), "{what}: a held loop cuts nothing under the tab top");
        return true;
    }
    assert_eq!(all_hold(part, t, what), tabs, "{what}: {:?}", t.notes);
    false
}

/// The 10 mm notch (its ring 4 mm wide, floor at y = 18) with the start
/// picked at `at`.
fn notch_from(at: (f64, f64), count: u32, lead: Lead) -> (Region, Toolpath) {
    let part = notched_to(10.0, 15.0);
    let mut p = params(count);
    p.lead = lead;
    p.picks.start = vec![PathPick::xy(Point2::new(at.0, at.1))];
    let t = contour(&part, &p).unwrap();
    (part, t)
}

#[test]
fn a_tab_that_ends_on_the_return_run_holds_the_loop() {
    // Second review of #2705: the pick is on the return leg, 2 mm above the
    // notch floor. The tab grows back round the floor; the cut after it runs
    // up the return leg 4 mm from the tab and never comes nearer, so it was
    // read as a departure: no note, and the tab cut to a fin.
    for count in [1, 2] {
        let (part, t) = notch_from((32.0, 20.0), count, Lead::None);
        let what = format!("return leg, {count} tabs");
        assert!(held_or_whole(&part, &t, count as usize, &what), "{what}: {:?} necks {:?}", t.notes, necks(&part, &t));
        assert_eq!(held(&t), 1, "{what}: {:?}", t.notes);
    }
}

#[test]
fn a_tab_that_ends_on_the_notch_floor_or_its_corner_holds_the_loop() {
    // The tab end on the notch floor, and exactly on its near corner.
    for at in [(30.0, 18.0), (32.0, 18.0)] {
        for count in [1, 2] {
            let (part, t) = notch_from(at, count, Lead::None);
            let what = format!("floor {at:?}, {count} tabs");
            assert!(held_or_whole(&part, &t, count as usize, &what), "{what}: {:?} necks {:?}", t.notes, necks(&part, &t));
        }
    }
}

#[test]
fn a_tab_beside_a_return_run_with_a_lead_holds_the_loop() {
    // The return-leg case with a perpendicular lead on every keep.
    for count in [1, 2] {
        let (part, t) = notch_from((32.0, 20.0), count, Lead::Perpendicular { length: 4.0 });
        let what = format!("return leg with a lead, {count} tabs");
        assert!(held_or_whole(&part, &t, count as usize, &what), "{what}: {:?} necks {:?}", t.notes, necks(&part, &t));
    }
}

#[test]
fn a_tab_in_a_v_notch_keeps_its_width_or_holds_the_loop() {
    // A V-notch 10 mm wide and 25 deep: its ring ends in a sharp tip near
    // (30, 30.3). The tab ends 2 mm up the second leg, so it grows back
    // round the tip, and the two legs part slowly.
    let part = Region::new(vec![lp(&[(0.0, 0.0), (60.0, 0.0), (60.0, 40.0), (35.0, 40.0), (30.0, 15.0), (25.0, 40.0), (0.0, 40.0)])]).unwrap();
    for count in [1, 2] {
        let mut p = params(count);
        p.picks.start = vec![PathPick::xy(Point2::new(30.392, 32.258))];
        let t = contour(&part, &p).unwrap();
        held_or_whole(&part, &t, count as usize, &format!("V-notch, {count} tabs"));
    }
}

#[test]
fn a_narrow_inside_slot_or_bore_keeps_its_tab_ring() {
    // Inside contours under about 2 D across: the far side of the ring runs
    // nearer than D to the tab. The loop is held, with a `tabs:` note.
    let slot = Region::new(vec![lp(&[(0.0, 0.0), (60.0, 0.0), (60.0, 7.0), (0.0, 7.0)])]).unwrap();
    let bore = Region::new(vec![Loop::new((0..72).map(|k| Point2::new(5.0 * (k as f64 * 5.0f64).to_radians().cos(), 5.0 * (k as f64 * 5.0f64).to_radians().sin())).collect()).unwrap()]).unwrap();
    for (what, region) in [("slot 60 x 7", &slot), ("bore 10", &bore)] {
        for count in [1, 2] {
            let mut p = params(count);
            p.side = Side::Inside;
            let t = contour(region, &p).unwrap();
            let named = t.notes.iter().any(|s| s.starts_with("tabs:") && s.contains("cut nothing on this loop"));
            assert!(named, "{what}, {count} tabs: {:?}", t.notes);
            assert!(cuts_nothing_at_the_tabs(&t), "{what}, {count} tabs: a held loop cuts nothing under the tab top");
        }
    }
}

#[test]
fn a_tab_near_a_corner_of_a_plain_part_is_not_held() {
    // No false hold: picks along the ring round one corner, outside (a
    // rounded ring corner) and inside (a sharp one), 1 and 2 tabs.
    let square = Region::new(vec![lp(&[(0.0, 0.0), (40.0, 0.0), (40.0, 40.0), (0.0, 40.0)])]).unwrap();
    let outside = [(43.0, 30.0), (43.0, 34.0), (43.0, 37.0), (43.0, 40.0), (42.6, 41.5), (41.5, 42.6), (40.0, 43.0), (37.0, 43.0), (33.0, 43.0)];
    let inside = [(37.0, 28.0), (37.0, 32.0), (37.0, 35.0), (37.0, 37.0), (35.0, 37.0), (32.0, 37.0), (28.0, 37.0)];
    for count in [1, 2] {
        for at in outside {
            let mut p = params(count);
            p.picks.start = vec![PathPick::xy(Point2::new(at.0, at.1))];
            let t = contour(&square, &p).unwrap();
            let what = format!("outside {at:?}, {count} tabs");
            assert!(!held_or_whole(&square, &t, count as usize, &what), "{what}: {:?}", t.notes);
        }
        for at in inside {
            let mut p = params(count);
            p.side = Side::Inside;
            p.picks.start = vec![PathPick::xy(Point2::new(at.0, at.1))];
            let t = contour(&square, &p).unwrap();
            assert_eq!(held(&t), 0, "inside {at:?}, {count} tabs: {:?}", t.notes);
        }
    }
}
