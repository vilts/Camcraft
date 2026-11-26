//! #1664: a 2D Contour on the bore side (`Side::Inside`) with
//! `entry: Ramp` descends at no more than the ramp angle, or
//! substitutes by name. Every descending feed move is either a ramp
//! piece no steeper than the ramp angle, or an axial plunge at the
//! plunge feed that an `entry:` note names - never a straight-down feed
//! at the cutting feed with no note.
//!
//! Provenance: our own tests over the contour's bore-side ramp law
//! (standard linear and zig-zag ramp entry, RS-274 G1); no vendor
//! material consulted.
//!
//! Gate map: covers camcraft_motion::machining2d (contour, contour_bore).

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{DEFAULT_RAMP_ANGLE, Link, Links};
use camcraft_motion::machining2d::{ContourParams, Lead, Side, contour};

const FEED: f64 = 500.0;
const PLUNGE: f64 = 250.0;

/// A square of half-width `h` at the origin.
fn square(h: f64) -> Region {
    let pts = vec![Point2::new(-h, -h), Point2::new(h, -h), Point2::new(h, h), Point2::new(-h, h)];
    Region::new(vec![Loop::new(pts).unwrap()]).unwrap()
}

/// The issue's repro: a Ø6 tool on the bore side, top 0, bottom -6,
/// stepdown 2, the entry Ramp.
fn params(climb: bool) -> ContourParams {
    ContourParams {
        tool_diameter: 6.0,
        stock_to_leave: 0.0,
        clearance_z: 5.0,
        retract_z: 2.0,
        top_z: 0.0,
        bottom_z: -6.0,
        stepdown: 2.0,
        feed: FEED,
        plunge_feed: PLUNGE,
        climb,
        side: Side::Inside,
        lead: Lead::None,
        tolerance: 0.01,
        tab_count: 0,
        tab_width: 0.0,
        tab_height: 0.0,
        entry: Entry3D::Ramp,
        helix_radius: 1.0,
        blend_turns: false, blend_radius: None,
        links: Links::PLANAR,
        picks: Default::default(),
    }
}

/// The XY length a move flies: the chord of a line, radius times the
/// sweep (in its own sense) of an arc.
fn xy_len(m: &Move) -> f64 {
    match m.arc {
        None => (m.to.x - m.from.x).hypot(m.to.y - m.from.y),
        Some(a) => {
            let ang = |p: camcraft_motion::geometry::Point3| (p.y - a.center.y).atan2(p.x - a.center.x);
            let d = if a.ccw { ang(m.to) - ang(m.from) } else { ang(m.from) - ang(m.to) };
            let d = d.rem_euclid(2.0 * std::f64::consts::PI);
            let d = if d <= 0.0 { 2.0 * std::f64::consts::PI } else { d };
            (m.from.x - a.center.x).hypot(m.from.y - a.center.y) * d
        }
    }
}

/// Check the "Done means" law on `t`; returns (ramp pieces, axial dives).
fn check(t: &Toolpath, what: &str) -> (usize, usize) {
    let tan = DEFAULT_RAMP_ANGLE.to_radians().tan();
    let named = t.notes.iter().any(|n| n.starts_with("entry:"));
    let (mut ramps, mut dives) = (0, 0);
    for m in &t.moves {
        let dz = m.from.z - m.to.z;
        if m.kind == MoveKind::Rapid || dz <= 1e-9 {
            continue;
        }
        let len = xy_len(m);
        if len > 1e-9 {
            // 1e-5 mm of XY slack: the endpoint lattice.
            assert!(
                dz <= tan * (len + 1e-5),
                "{what}: a descent at {:.3} deg ({:?} -> {:?}) - steeper than the ramp angle",
                (dz / len).atan().to_degrees(),
                m.from,
                m.to
            );
            ramps += 1;
        } else {
            assert_eq!(m.feed, Some(PLUNGE), "{what}: an axial descent at {:?} not at the plunge feed", m.to);
            assert!(named, "{what}: an axial descent at {:?} with no note naming it: {:?}", m.to, t.notes);
            dives += 1;
        }
    }
    (ramps, dives)
}

/// The issue's repro: today every level is entered by a straight-down
/// feed at 500 with no note. The ring (34 mm a side) has the room for
/// the 3 deg ramp, so every level ramps and none dives.
#[test]
fn a_bore_side_ramp_entry_contour_never_descends_steeper_than_its_ramp_angle() {
    for climb in [true, false] {
        let t = contour(&square(20.0), &params(climb)).expect("a legal contour");
        let (ramps, dives) = check(&t, &format!("climb {climb}"));
        assert!(ramps > 0, "climb {climb}: the ramp entry ramps somewhere");
        assert_eq!(dives, 0, "climb {climb}: the ring has room to ramp every level; notes {:?}", t.notes);
        assert!(t.moves.iter().any(|m| m.to.z <= -6.0 + 1e-9), "climb {climb}: the path reaches the floor");
    }
}

/// A ring too short for the ramp angle substitutes by name: an axial
/// dive at the plunge feed with an `entry:` note.
#[test]
fn a_bore_side_ring_too_short_to_ramp_dives_by_name_at_the_plunge_feed() {
    let mut p = params(true);
    p.links.approach.ramp_max_len = 0.1;
    let t = contour(&square(20.0), &p).expect("a legal contour");
    let (_, dives) = check(&t, "short ramp legs");
    assert!(dives > 0, "every level dives by name: {:?}", t.notes);
}

/// Three square bores side by side (half-width 8, centres 30 mm apart).
fn three_bores() -> Region {
    let sq = |cx: f64| {
        let h = 8.0;
        let pts = vec![Point2::new(cx - h, -h), Point2::new(cx + h, -h), Point2::new(cx + h, h), Point2::new(cx - h, h)];
        Loop::new(pts).unwrap()
    };
    Region::new(vec![sq(-30.0), sq(0.0), sq(30.0)]).unwrap()
}

/// Descending feed moves of a path: (steepest in degrees, axial count).
fn descents(t: &Toolpath) -> (f64, usize) {
    let (mut worst, mut axial): (f64, usize) = (0.0, 0);
    for m in t.moves.iter().filter(|m| m.kind != MoveKind::Rapid && m.from.z - m.to.z > 1e-9) {
        let len = xy_len(m);
        if len > 1e-9 {
            worst = worst.max(((m.from.z - m.to.z) / len).atan().to_degrees());
        } else {
            axial += 1;
        }
    }
    (worst, axial)
}

/// Every ring of a multi-ring bore-side contour takes the entry, on every
/// level: the first ring did, and the rings after it plunged into solid
/// through the pass link. Ramp at 5 deg caps each descent, the helix posts
/// no axial dive, and the plunge entry still dives (the baseline).
#[test]
fn every_ring_of_a_multi_ring_bore_is_entered_by_the_stated_entry() {
    let base = contour(&three_bores(), &ContourParams { entry: Entry3D::Plunge, ..params(true) }).expect("plunge bores");
    let (_, plunged) = descents(&base);
    assert!(plunged >= 9, "the plunge entry dives at each of 3 rings x 3 levels: {plunged}");

    let mut ramp = params(true);
    ramp.links.approach.ramp_angle = 5.0;
    let t = contour(&three_bores(), &ramp).expect("ramped bores");
    let (worst, axial) = descents(&t);
    assert!(worst > 1.0 && worst <= 5.0 + 1e-3, "every ring ramps within 5 deg: {worst}");
    assert_eq!(axial, 0, "no ring plunges: {:?}", t.notes);
    assert!(!t.moves.iter().any(|m| m.kind == MoveKind::Plunge), "no plunge move at all");
    assert!(t.is_chained() && t.all_finite());

    let helix = contour(&three_bores(), &ContourParams { entry: Entry3D::Helix, helix_radius: 2.0, ..params(true) }).expect("helix bores");
    let (_, axial) = descents(&helix);
    assert_eq!(axial, 0, "no ring plunges under the helix: {:?}", helix.notes);
    assert!(!helix.moves.iter().any(|m| m.kind == MoveKind::Plunge), "no plunge move at all");
    let orbits = helix.moves.iter().filter(|m| m.arc.is_some() && m.from.z - m.to.z > 1e-9).count();
    assert!(orbits >= 3 * 3, "each of 3 rings x 3 levels descends in helix arcs: {orbits}");
    assert!(helix.is_chained() && helix.all_finite());
}

/// One ring is still entered by its ramp: no axial dive (the law above
/// leaves the first ring alone).
#[test]
fn a_single_ring_bore_still_ramps_without_an_axial_dive() {
    let one = Region::new(vec![Loop::new(vec![Point2::new(-8.0, -8.0), Point2::new(8.0, -8.0), Point2::new(8.0, 8.0), Point2::new(-8.0, 8.0)]).unwrap()]).unwrap();
    let mut p = params(true);
    p.links.approach.ramp_angle = 5.0;
    let t = contour(&one, &p).expect("one bore");
    assert_eq!(descents(&t).1, 0);
}

/// The Z of every rapid that crosses from one bore to its neighbour (the
/// rings stand 30 mm apart): the plane of the ring-to-ring traverse. The
/// 60 mm jump back to the first bore at the next level is the level link's,
/// and the first rapid is the approach from the start point.
fn crossing_planes(t: &Toolpath) -> Vec<f64> {
    t.moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid && (20.0..40.0).contains(&xy_len(m)))
        .skip(1)
        .map(|m| {
            assert!((m.from.z - m.to.z).abs() < 1e-9, "a crossing stays on one plane: {m:?}");
            m.to.z
        })
        .collect()
}

/// The traverse between rings is the PASS link's plane, whatever the level
/// link says: clearance for pass Clearance, the retract plane for pass
/// Retract, and for a keep-down link that failed its proof (review of
/// #3834, as `ring_link` did before the entry reached the rings).
#[test]
fn the_ring_to_ring_traverse_of_a_ramped_bore_rides_the_pass_link_plane() {
    let planes = |level: Link, pass: Link, max_stay_down: Option<f64>| {
        let mut p = params(true);
        p.links.approach.ramp_angle = 5.0;
        p.links.level = level;
        p.links.pass = pass;
        if let Some(d) = max_stay_down {
            p.links.max_stay_down = d;
        }
        let t = contour(&three_bores(), &p).expect("ramped bores");
        assert!(descents(&t).1 == 0, "still no axial dive: {:?}", t.notes);
        crossing_planes(&t)
    };
    for level in [Link::Retract, Link::Clearance] {
        let clear = planes(level, Link::Clearance, None);
        assert!(clear.len() == 6 && clear.iter().all(|z| (z - 5.0).abs() < 1e-9), "pass Clearance, level {level:?}: {clear:?}");
        let retract = planes(level, Link::Retract, None);
        assert!(retract.len() == 6 && retract.iter().all(|z| (z - 2.0).abs() < 1e-9), "pass Retract, level {level:?}: {retract:?}");
    }
    // Keep down whose straight link is too long to prove: the retract plane.
    let failed = planes(Link::Clearance, Link::KeepDown, Some(1.0));
    assert!(failed.len() == 6 && failed.iter().all(|z| (z - 2.0).abs() < 1e-9), "keep down, proof failed: {failed:?}");
}


/// A ring after another one is entered from its OWN lead-in: the traverse
/// rapid brings the tool there first, so the ramp's chord is certified
/// from the lead-in and not from the end of the earlier ring (a chord
/// across the plate, always blocked). Without it every ring of the Auto
/// ramp fell back to an axial dive; now only the path's first ring does,
/// once a level, as before the entry reached the rings.
#[test]
fn a_ring_after_another_is_ramped_from_its_own_lead_in_not_from_the_last_ring() {
    let t = contour(&three_bores(), &params(true)).expect("auto ramp bores");
    let (_, axial) = descents(&t);
    assert_eq!(axial, 3, "only the first ring dives, one per level: {:?}", t.notes);
    assert!(t.notes.iter().all(|n| n.contains("(25.500")), "the blocked chords are the first ring's alone: {:?}", t.notes);
}
