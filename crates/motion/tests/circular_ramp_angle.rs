//! #1609: a Circular with `entry: ramp` descends at its ramp angle, or
//! substitutes by name. Every descending feed move of the path is
//! either a ramp piece no steeper than the ramp angle (measured along
//! the arc it flies), or an axial plunge at the plunge feed that a note
//! names - never an unnamed steep "ramp" chord, never an axial move at
//! the cutting feed.
//!
//! Provenance: our own tests over the kernel's ramp law (standard
//! linear and helical ramp entry, RS-274 G1/G2/G3); no vendor material
//! consulted.
//!
//! Gate map: covers camcraft_motion::machining2d (circular, descent), camcraft_motion::ring_zig.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::links::{DEFAULT_RAMP_ANGLE, Links};
use camcraft_motion::machining2d::{CircularParams, circular};

const C: Point2 = Point2 { x: -35.0, y: 2.9 };
const FEED: f64 = 500.0;
const PLUNGE: f64 = 250.0;

fn region() -> Region {
    let sq = |x, y| Point2::new(C.x + x, C.y + y);
    Region::new(vec![Loop::new(vec![sq(-9.0, -9.0), sq(9.0, -9.0), sq(9.0, 9.0), sq(-9.0, 9.0)]).unwrap()]).unwrap()
}

/// #1558's hole: Ø6, a Ø4 cutter, 0.5 stepover, 1 mm stepdown through
/// an 11 mm cut, with the entry Ramp.
fn params(climb: bool) -> CircularParams {
    CircularParams {
        links: Links::PLANAR,
        center: C,
        radius: 3.0,
        vault: false,
        bottom_z: -11.524,
        top_z: -0.508,
        tool_diameter: 4.0,
        stock_to_leave: 0.0,
        stepover: 0.5,
        stepdown: 1.0,
        clearance_z: 5.0,
        retract_z: None,
        feed: FEED,
        plunge_feed: PLUNGE,
        climb,
        entry: Entry3D::Ramp,
        helix_radius: 1.0,
    }
}

fn path(p: &CircularParams) -> Toolpath {
    circular(&region(), p).expect("a legal hole")
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

/// The issue's "Done means": on the repro, every descending feed move
/// is at most the ramp angle, or is a named substitution at the plunge
/// feed. Today's code posts a 76 deg chord and 90 deg feeds at 500.
#[test]
fn a_ramp_entry_circular_never_descends_steeper_than_its_ramp_angle() {
    let tan = DEFAULT_RAMP_ANGLE.to_radians().tan();
    for climb in [true, false] {
        let t = path(&params(climb));
        let named = t.notes.iter().any(|n| n.starts_with("entry:"));
        let mut ramps = 0;
        for m in &t.moves {
            let dz = m.from.z - m.to.z;
            if m.kind == MoveKind::Rapid || dz <= 0.0 {
                continue;
            }
            let len = xy_len(m);
            if len > 1e-9 {
                // 1e-5 mm of XY slack: the endpoint lattice.
                assert!(
                    dz <= tan * (len + 1e-5),
                    "climb {climb}: a descent at {:.3} deg ({:?} -> {:?}) - steeper than the ramp angle",
                    (dz / len).atan().to_degrees(),
                    m.from,
                    m.to
                );
                ramps += 1;
            } else {
                assert_eq!(m.feed, Some(PLUNGE), "climb {climb}: an axial descent at {:?} not at the plunge feed", m.to);
                assert!(named, "climb {climb}: an axial descent at {:?} with no note naming it: {:?}", m.to, t.notes);
            }
        }
        assert!(ramps > 0, "climb {climb}: the ramp entry ramps somewhere");
    }
}

/// The ramp along the ring is the descent at every level of both rings:
/// no level is entered by an axial move at all on this hole, which has
/// room for the ramp on each ring.
#[test]
fn every_ring_level_of_the_ramp_entry_is_entered_by_the_ramp() {
    for climb in [true, false] {
        let t = path(&params(climb));
        let axial: Vec<&Move> = t
            .moves
            .iter()
            .filter(|m| m.kind != MoveKind::Rapid && m.from.z > m.to.z && xy_len(m) <= 1e-9)
            .collect();
        assert!(axial.is_empty(), "climb {climb}: axial descents {:?}; notes {:?}", axial.iter().map(|m| m.to).collect::<Vec<_>>(), t.notes);
    }
}
