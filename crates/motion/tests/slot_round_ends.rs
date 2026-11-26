//! A Slot wider than its tool cuts its full outline, round ends included (#2906).
//! Gate map: covers camcraft_motion::machining2d (slots).
//!
//! The check is closed form: each point on the wall of the finished
//! stadium must lie within the tool radius of the tool centre path, and
//! no tool centre point may leave the cutter-centre stadium.

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::Point2;
use camcraft_motion::links::Links;
use camcraft_motion::machining2d::{SlotParams, slot};
use camcraft_motion::verify::crossed_pairs;

/// The issue's case: tool 6 mm, slot 10 mm wide, two levels of 3 mm.
fn params(climb: bool) -> SlotParams {
    SlotParams {
        tool_diameter: 6.0,
        width: 10.0,
        stock_to_leave: 0.0,
        clearance_z: 17.0,
        retract_z: None,
        top_z: 12.0,
        bottom_z: 6.0,
        stepdown: 3.0,
        stepover: 2.0,
        feed: 600.0,
        plunge_feed: 150.0,
        climb,
        entry: Entry3D::Plunge,
        helix_radius: 1.0,
        links: Links::PLANAR,
        blend_turns: false,
        blend_radius: None,
    }
}

/// Distance from `p` to the segment `a`-`b`.
fn seg_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 { (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (a.x + t * dx, a.y + t * dy);
    ((p.x - qx).powi(2) + (p.y - qy).powi(2)).sqrt()
}

/// The feed chords of the path at level `z` (tool centre, XY).
fn level_chords(path: &Toolpath, z: f64) -> Vec<(Point2, Point2)> {
    path.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Feed && (m.from.z - z).abs() < 1e-6 && (m.to.z - z).abs() < 1e-6)
        .flat_map(|m| m.chords(1e-4))
        .collect()
}

/// Points on the wall of the stadium of half-width `w` on `a` -> `b`
/// (here a -> b runs along +X): the two half circles and the two sides.
fn wall_points(a: Point2, b: Point2, w: f64) -> Vec<Point2> {
    let mut pts = Vec::new();
    for k in 0..=360 {
        let t = std::f64::consts::PI * k as f64 / 360.0;
        // `b`'s cap from +Y through +X to -Y; `a`'s cap mirrored.
        pts.push(Point2::new(b.x + w * t.sin(), b.y + w * t.cos()));
        pts.push(Point2::new(a.x - w * t.sin(), a.y + w * t.cos()));
    }
    for k in 0..=200 {
        let x = a.x + (b.x - a.x) * k as f64 / 200.0;
        pts.push(Point2::new(x, a.y + w));
        pts.push(Point2::new(x, a.y - w));
    }
    pts
}

fn check(climb: bool, width: f64, stepover: f64) {
    check_with(climb, width, stepover, false);
}

fn check_with(climb: bool, width: f64, stepover: f64, blend_turns: bool) {
    let (a, b) = (Point2::new(-25.0, 15.0), Point2::new(25.0, 15.0));
    let p = SlotParams { width, stepover, blend_turns, blend_radius: Some(1.0), ..params(climb) };
    let Ok(path) = slot(a, b, &p) else { panic!("slot path") };
    if blend_turns {
        assert!(path.notes.iter().any(|n| n.contains("rounded")), "no row end rounded: {:?}", path.notes);
    }
    // The verifier's crossing law (a): the lap meets each row join only
    // at the join's own end points, never across a flattened arc chord.
    assert_eq!(crossed_pairs(&path, 0.01), Vec::<[usize; 2]>::new(), "climb={climb} width={width}");
    let r = p.tool_diameter * 0.5;
    let h = p.width * 0.5 - r;
    for z in [9.0, 6.0] {
        let chords = level_chords(&path, z);
        assert!(!chords.is_empty(), "no cut at level {z}");
        // No tool centre point leaves the cutter-centre stadium (no gouge).
        for &(s, e) in &chords {
            for q in [s, e] {
                let d = seg_dist(q, a, b);
                assert!(d <= h + 2e-3, "climb={climb} z={z}: centre {q:?} is {d:.4} from the axis, past {h}");
            }
        }
        // Each wall point is within the tool radius of the centre path.
        let mut worst = (0.0_f64, Point2::new(0.0, 0.0));
        for w in wall_points(a, b, p.width * 0.5) {
            let d = chords.iter().map(|&(s, e)| seg_dist(w, s, e)).fold(f64::INFINITY, f64::min);
            let left = d - r;
            if left > worst.0 {
                worst = (left, w);
            }
        }
        assert!(
            worst.0 <= 0.01,
            "climb={climb} z={z}: {:.3} mm left on the wall at {:?}",
            worst.0,
            worst.1
        );
    }
}

#[test]
fn wide_slot_cuts_its_round_ends_climb() {
    check(true, 10.0, 2.0);
}

#[test]
fn wide_slot_cuts_its_round_ends_conventional() {
    check(false, 10.0, 2.0);
}

/// Rows at -7, -3, 1, 5, 7: no row end sits on a cap's tip.
#[test]
fn wide_slot_with_rows_off_the_tips_cuts_its_round_ends() {
    check(true, 20.0, 4.0);
    check(false, 20.0, 4.0);
}

/// #1759 with #2906: row ends on arc turns still end in the wall lap, so
/// the round ends are cut to size and nothing crosses or gouges.
#[test]
fn wide_slot_with_arc_turns_still_cuts_its_round_ends() {
    for climb in [true, false] {
        check_with(climb, 10.0, 2.0, true);
        check_with(climb, 20.0, 4.0, true);
    }
}
