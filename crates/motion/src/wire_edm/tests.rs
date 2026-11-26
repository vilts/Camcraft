//! The wire contour's laws (#532): a die and a punch on one profile
//! stand apart by exactly the kerf, the corner minimum and the start
//! hole are refused by name, degenerate profiles give clean errors,
//! and the moves state no pose.
//!
//! The wire and gap below are SYNTHETIC - a plausible size, from no
//! machine's manual (#290 D4, D10).

use super::ring::pt_ring;
use super::*;
use crate::cl::MoveKind;

const WIRE: f64 = 0.25;
const GAP: f64 = 0.04;
/// Wire radius + gap.
const K: f64 = 0.165;

fn params(side: WireSide, start: Option<(f64, f64)>) -> WireParams {
    WireParams {
        wire_diameter: WIRE,
        gap: GAP,
        condition: "C110".into(),
        side,
        start: start.map(|(x, y)| Point2::new(x, y)),
        plane_z: -12.0,
        tabs: None,
        stock: None,
        clearance: 1.0,
        sharp_corners: false,
    }
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    Loop::new(vec![Point2::new(x0, y0), Point2::new(x1, y0), Point2::new(x1, y1), Point2::new(x0, y1)]).unwrap()
}

/// A `w` x `h` rectangle centred on the origin with corners of radius
/// `r`, each round drawn in 3-degree facets.
fn rounded(w: f64, h: f64, r: f64) -> Loop {
    let (hx, hy) = (w * 0.5 - r, h * 0.5 - r);
    let mut pts = Vec::new();
    for (cx, cy, a0) in [(hx, hy, 0.0_f64), (-hx, hy, 90.0), (-hx, -hy, 180.0), (hx, -hy, 270.0)] {
        for s in 0..=30 {
            let t = (a0 + 3.0 * f64::from(s)).to_radians();
            pts.push(Point2::new(cx + r * t.cos(), cy + r * t.sin()));
        }
    }
    Loop::new(pts).unwrap()
}

/// The wire-centre points of the profile's own cut: every move's end
/// but the lead-in's arrival and the lead-out's return.
fn ride(path: &Toolpath) -> Vec<Point2> {
    path.moves[..path.moves.len() - 1].iter().map(|m| Point2::new(m.to.x, m.to.y)).collect()
}

/// Where the closed polyline `pts` crosses the line x = 0 highest.
fn top_at_x0(pts: &[Point2]) -> f64 {
    let n = pts.len();
    (0..n)
        .filter_map(|i| {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            ((a.x <= 0.0) != (b.x <= 0.0)).then(|| a.y + (b.y - a.y) * (0.0 - a.x) / (b.x - a.x))
        })
        .fold(f64::NEG_INFINITY, f64::max)
}

fn refusal(loops: &[Loop], chains: &[Vec<Point2>], p: &WireParams) -> String {
    wire_contour(loops, chains, p).expect_err("refused").to_string()
}

/// Done means (1): a die and a punch on the same profile are offset to
/// opposite sides, and their paths stand apart by exactly the kerf.
#[test]
fn a_die_and_a_punch_on_one_profile_stand_apart_by_exactly_the_kerf() {
    let profile = rounded(40.0, 20.0, 2.0);
    let region = Region::new(vec![profile.clone()]).unwrap();
    let die = wire_contour(std::slice::from_ref(&profile), &[], &params(WireSide::Die, Some((0.0, 0.0)))).unwrap();
    let punch = wire_contour(std::slice::from_ref(&profile), &[], &params(WireSide::Punch, Some((-30.0, 0.0)))).unwrap();
    let (d, p) = (ride(&die), ride(&punch));
    // Opposite sides: the die's wire inside the profile, the punch's outside.
    assert!(d.iter().all(|q| region.contains(*q)), "the die's wire stays inside the profile");
    assert!(p.iter().all(|q| !region.contains(*q)), "the punch's wire stays outside the profile");
    // Each stands the kerf offset off the profile: never nearer (to the
    // offsetter's lattice), and on the straight sides exactly there.
    for (name, pts) in [("die", &d), ("punch", &p)] {
        let near = pts.iter().map(|q| pt_ring(*q, profile.points())).fold(f64::INFINITY, f64::min);
        let far = pts.iter().map(|q| pt_ring(*q, profile.points())).fold(0.0, f64::max);
        assert!((near - K).abs() < 1e-6, "{name}: the nearest the wire comes is {near}, the offset is {K}");
        assert!(far - K < 2e-4, "{name}: the farthest path vertex stands {far} off (a 3-degree facet's mitre)");
    }
    // The straight sides: the die's path at y = 10 - k, the punch's at
    // y = 10 + k - apart by wire diameter + 2 x gap, the kerf width.
    let (yd, yp) = (top_at_x0(&d), top_at_x0(&p));
    assert!((yd - (10.0 - K)).abs() < 1e-9 && (yp - (10.0 + K)).abs() < 1e-9, "die {yd}, punch {yp}");
    let kerf = params(WireSide::Die, None).kerf_width();
    assert!((kerf - 0.33).abs() < 1e-12, "kerf width = 0.25 + 2 x 0.04");
    assert!(((yp - yd) - kerf).abs() < 1e-9, "the two paths stand {} apart, the kerf is {kerf}", yp - yd);
    // The notes say the side and the offset's two terms.
    assert!(die.notes[0].contains("die side") && die.notes[0].contains("0.1650") && die.notes[0].contains("C110"), "{:?}", die.notes);
    assert!(punch.notes[0].contains("punch side") && punch.notes[0].contains("kerf width 0.3300"), "{:?}", punch.notes);
}

/// Done means (2a): a corner tighter than wire radius + gap is refused
/// by name - on the side where it is an INSIDE corner for the wire.
#[test]
fn a_corner_tighter_than_the_wire_is_refused_by_name() {
    // A sharp rectangle: four inside corners for a die's wire.
    let sharp = rect(-20.0, -10.0, 20.0, 10.0);
    let why = refusal(std::slice::from_ref(&sharp), &[], &params(WireSide::Die, Some((0.0, 0.0))));
    assert!(why.contains("inside corner") && why.contains("minimum inside radius is 0.1650 mm"), "{why}");
    assert!(why.contains("wire radius 0.1250 + spark gap 0.0400") && why.contains("die side"), "{why}");
    // It names the spot: one of the four corners.
    let at = wire_contour(std::slice::from_ref(&sharp), &[], &params(WireSide::Die, Some((0.0, 0.0)))).expect_err("refused").location().expect("a spot");
    assert!((at.x.abs() - 20.0).abs() < 1e-9 && (at.y.abs() - 10.0).abs() < 1e-9 && at.z == -12.0, "{at:?}");
    // The same corners are OUTSIDE corners for a punch's wire: cut, and sharp.
    let punch = wire_contour(&[sharp], &[], &params(WireSide::Punch, Some((-30.0, 0.0)))).unwrap();
    assert!(ride(&punch).iter().any(|q| (q.x - (20.0 + K)).abs() < 1e-9 && (q.y - (10.0 + K)).abs() < 1e-9), "the mitred corner");
    // A fillet under the minimum is refused; one over it is cut.
    let tight = refusal(&[rounded(40.0, 20.0, 0.1)], &[], &params(WireSide::Die, Some((0.0, 0.0))));
    assert!(tight.contains("inside corner") && tight.contains("0.1650"), "{tight}");
    assert!(wire_contour(&[rounded(40.0, 20.0, 0.2)], &[], &params(WireSide::Die, Some((0.0, 0.0)))).is_ok());
    // A notch narrower than the kerf is an inside feature for a punch.
    let notched = Loop::new(vec![
        Point2::new(-20.0, -10.0),
        Point2::new(20.0, -10.0),
        Point2::new(20.0, 10.0),
        Point2::new(0.1, 10.0),
        Point2::new(0.1, 5.0),
        Point2::new(-0.1, 5.0),
        Point2::new(-0.1, 10.0),
        Point2::new(-20.0, 10.0),
    ])
    .unwrap();
    let slot = refusal(&[notched], &[], &params(WireSide::Punch, Some((-30.0, 0.0))));
    assert!(slot.contains("slot") && slot.contains("punch side"), "{slot}");
}

/// Done means (2b): a closed profile with no start hole is refused by
/// name; so is a start hole on the product's side or too near it.
#[test]
fn a_missing_or_misplaced_start_hole_is_refused_by_name() {
    let profile = rounded(40.0, 20.0, 2.0);
    let none = refusal(std::slice::from_ref(&profile), &[], &params(WireSide::Die, None));
    assert!(none.contains("no start hole is stated") && none.contains("threaded through solid material"), "{none}");
    let none = refusal(std::slice::from_ref(&profile), &[], &params(WireSide::Punch, None));
    assert!(none.contains("no start hole is stated") && none.contains("outside the profile"), "{none}");
    let out = refusal(std::slice::from_ref(&profile), &[], &params(WireSide::Die, Some((-30.0, 0.0))));
    assert!(out.contains("lies outside the profile") && out.contains("slug"), "{out}");
    let inside = refusal(std::slice::from_ref(&profile), &[], &params(WireSide::Punch, Some((0.0, 0.0))));
    assert!(inside.contains("lies inside the profile") && inside.contains("punch"), "{inside}");
    // 0.1 mm inside the wall: nearer than the 0.165 mm offset.
    let near = refusal(std::slice::from_ref(&profile), &[], &params(WireSide::Die, Some((0.0, 9.9))));
    assert!(near.contains("closer to the profile than the kerf offset (0.1650 mm)") && near.contains("die"), "{near}");
    let near = refusal(std::slice::from_ref(&profile), &[], &params(WireSide::Punch, Some((0.0, 10.1))));
    assert!(near.contains("closer to the profile than the kerf offset") && near.contains("punch"), "{near}");
    let on = refusal(&[profile], &[], &params(WireSide::Die, Some((0.0, 10.0 - K))));
    assert!(on.contains("stands on the wire path itself"), "{on}");
}

/// Degenerate input errors cleanly, by name - never a panic.
#[test]
fn degenerate_profiles_and_numbers_are_refused_by_name() {
    let p = params(WireSide::Die, Some((0.0, 0.0)));
    let profile = rounded(40.0, 20.0, 2.0);
    let open = refusal(&[], &[vec![Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)]], &p);
    assert!(open.contains("the profile is open") && open.contains("no punch or die side"), "{open}");
    assert!(refusal(&[], &[], &p).contains("no profile to cut"));
    let two = refusal(&[profile.clone(), rect(50.0, 0.0, 60.0, 10.0)], &[], &p);
    assert!(two.contains("2 closed profiles") && two.contains("its own operation"), "{two}");
    let bow = Loop::new(vec![Point2::new(-5.0, -5.0), Point2::new(5.0, 5.0), Point2::new(5.0, -5.0), Point2::new(-5.0, 5.0)]).unwrap();
    assert!(refusal(&[bow], &[], &p).contains("crosses itself"));
    let line = Loop::new(vec![Point2::new(-5.0, 0.0), Point2::new(0.0, 0.0), Point2::new(5.0, 0.0)]).unwrap();
    assert!(refusal(&[line], &[], &p).contains("no area"));
    // A zero-length segment in a good ring is just a repeated point.
    let mut pts = profile.points().to_vec();
    pts.insert(5, pts[5]);
    assert!(wire_contour(&[Loop::new(pts).unwrap()], &[], &p).is_ok());
    // Smaller than the wire; an hourglass whose neck is under the kerf.
    let tiny = refusal(&[rounded(0.3, 0.3, 0.14)], &[], &p);
    assert!(tiny.contains("too small for the wire"), "{tiny}");
    let hourglass = Loop::new(vec![
        Point2::new(-10.0, -5.0),
        Point2::new(-0.5, -0.1),
        Point2::new(0.5, -0.1),
        Point2::new(10.0, -5.0),
        Point2::new(10.0, 5.0),
        Point2::new(0.5, 0.1),
        Point2::new(-0.5, 0.1),
        Point2::new(-10.0, 5.0),
    ])
    .unwrap();
    let neck = refusal(&[hourglass], &[], &params(WireSide::Die, Some((-8.0, 0.0))));
    assert!(neck.contains("neck under the kerf width (0.3300 mm)"), "{neck}");
    // The numbers' door.
    for (edit, says) in [
        ((|q: &mut WireParams| q.wire_diameter = 0.0) as fn(&mut WireParams), "wire diameter must be a positive number"),
        (|q| q.wire_diameter = f64::NAN, "wire diameter must be a positive number"),
        (|q| q.gap = -0.01, "spark gap must be zero or more"),
        (|q| q.gap = f64::INFINITY, "spark gap must be zero or more"),
        (|q| q.plane_z = f64::NAN, "programmed plane"),
        (|q| q.start = Some(Point2::new(f64::NAN, 0.0)), "start hole's position"),
    ] {
        let mut q = p.clone();
        edit(&mut q);
        let why = refusal(std::slice::from_ref(&profile), &[], &q);
        assert!(why.contains(says), "{why}");
    }
}

/// D2: a vertical cut states no pose and rides one plane - it posts as
/// XY. The cut is threaded at the start hole, leads straight to the
/// path's nearest point, closes the loop and returns.
#[test]
fn the_moves_ride_one_plane_with_no_pose_from_and_to_the_start_hole() {
    let profile = rounded(40.0, 20.0, 2.0);
    let path = wire_contour(&[profile], &[], &params(WireSide::Die, Some((3.0, -1.0)))).unwrap();
    assert!(path.is_chained() && path.is_prismatic() && !path.has_arcs());
    assert!(path.moves.iter().all(|m| m.axis.is_none() && m.rotary.is_none()), "no pose is stated");
    assert!(path.moves.iter().all(|m| m.kind == MoveKind::Feed && m.feed.is_none()), "burning moves, the control's rate");
    assert!(path.moves.iter().all(|m| m.from.z == -12.0 && m.to.z == -12.0), "one programmed plane");
    let (start, end) = (path.start().unwrap(), path.end().unwrap());
    assert_eq!((start.x, start.y), (3.0, -1.0));
    assert_eq!((end.x, end.y), (3.0, -1.0), "the lead-out returns to the threading point");
    // The lead-in meets the path square, at its nearest point: straight
    // down from (3, -1) to the bottom side at y = -10 + k.
    let entry = path.moves[0].to;
    assert!((entry.x - 3.0).abs() < 1e-9 && (entry.y - (-10.0 + K)).abs() < 1e-9, "{entry:?}");
    let back = &path.moves[path.moves.len() - 1];
    assert_eq!((back.from.x, back.from.y), (entry.x, entry.y), "the loop closes where it began");
    // Counter-clockwise: the ride's signed area is positive.
    let pts = ride(&path);
    let n = pts.len();
    let area: f64 = (0..n).map(|i| pts[i].x * pts[(i + 1) % n].y - pts[(i + 1) % n].x * pts[i].y).sum::<f64>() * 0.5;
    assert!(area > 0.0, "counter-clockwise");
    assert!(path.notes.iter().any(|n| n.contains("threaded at X3.000 Y-1.000") && n.contains("lead-in of 8.835 mm")), "{:?}", path.notes);
}

fn pass(id: &str, gap: f64, reverse: bool) -> WirePass {
    WirePass { condition: id.into(), wire_diameter: WIRE, gap, reverse }
}

/// #538: no skims is the single pass exactly; each skim walks its own
/// offset from the threading point and back, a reversed one clockwise.
#[test]
fn skims_cut_their_own_offsets_and_none_is_the_single_pass() {
    let sq = [rect(-20.0, -10.0, 20.0, 10.0)];
    let p = params(WireSide::Punch, Some((-30.0, 0.0)));
    let one = wire_contour(&sq, &[], &p).unwrap();
    assert_eq!(wire_passes(&sq, &[], &p, &[]).unwrap(), one);
    let three = wire_passes(&sq, &[], &p, &[pass("S21", 0.02, false), pass("S31", 0.01, true)]).unwrap();
    let start = Point3::new(-30.0, 0.0, -12.0);
    let returns: Vec<usize> = three.moves.iter().enumerate().filter(|(_, m)| m.to == start).map(|(i, _)| i).collect();
    assert_eq!(returns.len(), 3, "one return to the threading point per pass");
    for (i, k) in [K, 0.145, 0.135].iter().enumerate() {
        let lead = if i == 0 { &three.moves[0] } else { &three.moves[returns[i - 1] + 1] };
        assert!((lead.to.x - (-20.0 - k)).abs() < 1e-6 && lead.to.y.abs() < 1e-6, "pass {i}: {:?}", lead.to);
    }
    // The reversed pass goes up the left side first.
    assert!(three.moves[returns[1] + 2].to.y > 0.0);
    assert!(three.moves[returns[0] + 2].to.y < 0.0);
    assert!(three.notes.iter().any(|n| n.contains("skim 2 - condition S31") && n.contains("reversed")), "{:?}", three.notes);
}

/// #538: each pass law refuses by name.
#[test]
fn skim_laws_refuse_by_name() {
    let p = params(WireSide::Die, Some((0.0, 0.0)));
    let err = |s: &[WirePass]| check_passes(&p, s).unwrap_err().to_string();
    assert!(err(&[pass(" ", 0.02, false)]).contains("skim 1 names no EDM condition"));
    assert!(err(&[pass("S1", 0.01, false), pass("S2", 0.02, false)]).contains("cut past the finished profile"));
    assert!(err(&[pass("S1", 0.05, false)]).contains("rough pass (C110) stands 0.1650 mm"));
    let thin = WirePass { wire_diameter: 0.2, ..pass("W", 0.02, false) };
    assert!(err(&[thin]).contains("one threading cuts every pass with one wire"));
    assert!(err(&[pass("S", f64::NAN, false)]).contains("spark gap"));
    assert!(check_passes(&p, &[pass("S1", 0.02, false), pass("S2", 0.02, true)]).is_ok(), "equal offsets are allowed");
    // An open run (#1109) is cut once: skims are refused at the door.
    let open = params(WireSide::Left, None);
    let e = check_passes(&open, &[pass("S1", 0.02, false)]).unwrap_err().to_string();
    assert!(e.contains("an open run is cut once"), "{e}");
    assert!(check_passes(&open, &[]).is_ok(), "an open run alone is not refused here");
    // Tabs (#539) are cut on the rough pass alone: skims beside them are
    // refused by name.
    let tabs = TabParams { count: Some(1), width: Some(1.0), at: Vec::new(), cutoff: None };
    let tabbed = WireParams { tabs: Some(tabs), ..params(WireSide::Die, Some((0.0, 0.0))) };
    let e = check_passes(&tabbed, &[pass("S1", 0.02, false)]).unwrap_err().to_string();
    assert!(e.contains("skims are cut on an untabbed profile only"), "{e}");
}

/// #536: a zero taper is the vertical cut itself; a constant taper
/// leans every side by exactly its angle, stands the wire upright at
/// the start hole, and refuses an angle past the machine's maximum and
/// a side the lean folds over - each by name.
#[test]
fn a_taper_leans_every_side_by_its_angle_and_zero_is_the_vertical_cut() {
    let loops = [rect(-20.0, -10.0, 20.0, 10.0)];
    let p = params(WireSide::Punch, Some((-30.0, 0.0)));
    let t = |a: f64, max: Option<f64>| WireTaper { angle_deg: a, top_z: 0.0, max_deg: max, machine: "M".into(), corner: WireCorner::Mitre };
    let flat = wire_contour(&loops, &[], &p).unwrap();
    assert_eq!(wire_contour_tapered(&loops, &[], &p, &t(0.0, None)).unwrap().moves, flat.moves);
    let path = wire_contour_tapered(&loops, &[], &p, &t(3.0, Some(15.0))).unwrap();
    let lean = |a: [f64; 3]| a[0].hypot(a[1]).atan2(a[2]).to_degrees();
    // The lead-in's end: the middle of the left side, leaning 3 degrees
    // outward (-X) going down - its pose points +X toward the upper guide.
    let first = path.moves[0].axis.unwrap();
    assert!((lean(first) - 3.0).abs() < 1e-9 && first[0] > 0.0 && first[1].abs() < 1e-12, "{first:?}");
    let h = 12.0 * 3.0_f64.to_radians().tan();
    assert!((path.moves[0].to.x - (-20.0 - K / 3.0_f64.to_radians().cos() - h)).abs() < 1e-6, "{:?}", path.moves[0].to);
    assert!(path.moves.iter().all(|m| m.to.z == -12.0 && m.feed.is_none()));
    assert_eq!(path.moves.last().unwrap().axis, Some([0.0, 0.0, 1.0]), "upright at the start hole");
    let err = wire_contour_tapered(&loops, &[], &p, &t(30.0, Some(15.0))).unwrap_err().to_string();
    assert!(err.contains("30.000 degree taper") && err.contains("at most 15.000"), "{err}");
    let err = wire_contour_tapered(&loops, &[], &p, &t(3.0, None)).unwrap_err().to_string();
    assert!(err.contains("states no edm.max_taper_deg"), "{err}");
    // A 1 mm punch narrowed 20 degrees downward over 12 mm closes on itself.
    let small = [rect(-0.5, -0.5, 0.5, 0.5)];
    let pin = params(WireSide::Punch, Some((-5.0, 0.0)));
    let err = wire_contour_tapered(&small, &[], &pin, &t(-20.0, Some(30.0))).unwrap_err().to_string();
    assert!(err.contains("folds the side"), "{err}");
}

/// #1164: the cone corner rule holds the lean at the angle through
/// each corner (the lower end on an arc about the top corner), and a
/// lead-in that meets a corner under it is refused by name.
#[test]
fn the_cone_rule_holds_the_lean_and_refuses_a_lead_into_a_corner() {
    let loops = [rect(-20.0, -10.0, 20.0, 10.0)];
    let t = WireTaper { angle_deg: 3.0, top_z: 0.0, max_deg: Some(15.0), machine: "M".into(), corner: WireCorner::Cone };
    let path = wire_contour_tapered(&loops, &[], &params(WireSide::Punch, Some((-30.0, 0.0))), &t).unwrap();
    let lean = |a: [f64; 3]| a[0].hypot(a[1]).atan2(a[2]).to_degrees();
    let (_, cut) = path.moves.split_last().unwrap();
    assert!(cut.iter().all(|m| (lean(m.axis.unwrap()) - 3.0).abs() < 1e-9));
    // Four quarter turns of 45 two-degree steps, plus the sides and leads.
    assert_eq!(path.moves.len(), 4 * 45 + 7);
    let err = wire_contour_tapered(&loops, &[], &params(WireSide::Punch, Some((-30.0, -20.0))), &t).unwrap_err().to_string();
    assert!(err.contains("lead-in meets the corner") && err.contains("cone corner rule"), "{err}");
}
