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
use camcraft_motion::links::{DEFAULT_RAMP_ANGLE, Links};
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
        blend_turns: false,
        links: Links::PLANAR,
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
