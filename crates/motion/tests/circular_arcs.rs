//! #1558: the circular kernel cuts each ring as two half-turn arcs
//! about the authored centre (G3 for climb, G2 otherwise - the travel
//! the old 512-chord polygon had), and where the ring is walked as
//! a ramp along the first cut (the helix standing down) the ramp rides
//! the ring's own circle as helical arcs, never 512 fixed chords.
//!
//! Provenance: our own tests over the kernel's circle law (standard
//! circular interpolation, RS-274 G2/G3); no vendor material consulted.
//!
//! Gate map: covers camcraft_motion::machining2d (circular), camcraft_motion::ring_zig.

use camcraft_motion::cl::{Move, MoveKind, Toolpath};
use camcraft_motion::entry::Entry3D;
use camcraft_motion::geo2d::{Loop, Point2, Region};
use camcraft_motion::machining2d::{circular, CircularParams};

const C: Point2 = Point2 { x: -35.0, y: 2.9 };

fn region() -> Region {
    let sq = |x, y| Point2::new(C.x + x, C.y + y);
    Region::new(vec![Loop::new(vec![sq(-9.0, -9.0), sq(9.0, -9.0), sq(9.0, 9.0), sq(-9.0, 9.0)]).unwrap()]).unwrap()
}

/// The issue's hole: Ø6, a Ø4 cutter, 0.5 stepover, 1 mm stepdown
/// through an 11 mm cut.
fn params(entry: Entry3D, climb: bool) -> CircularParams {
    CircularParams {
        links: camcraft_motion::links::Links::PLANAR,
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
        feed: 500.0,
        plunge_feed: 250.0,
        climb,
        entry,
        helix_radius: 1.0,
    }
}

fn path(p: &CircularParams) -> Toolpath {
    circular(&region(), p).expect("a legal hole")
}

fn rad(m: &Move, q: camcraft_motion::geometry::Point3) -> f64 {
    let c = m.arc.map_or(C, |a| a.center);
    (q.x - c.x).hypot(q.y - c.y)
}

/// Two rings (R 1.0 and 0.5) at 13 levels: 26 ring-levels. With arcs
/// each is two moves plus its entry (a ramp of a dozen helical arcs
/// where the helix stands down) and hops: the issue's ceiling is ~500
/// (it measured 74,131 with the 512-gon).
#[test]
fn the_issue_hole_posts_a_few_hundred_moves_not_tens_of_thousands() {
    for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp] {
        let t = path(&params(entry, true));
        assert!(t.moves.len() < 500, "{entry:?}: {} moves", t.moves.len());
    }
}

#[test]
fn each_ring_level_is_two_half_turn_arcs_about_the_centre() {
    for climb in [true, false] {
        let t = path(&params(Entry3D::Plunge, climb));
        let arcs: Vec<&Move> = t.moves.iter().filter(|m| m.arc.is_some()).collect();
        // 2 rings x 13 levels (-0.508 by 1 mm to -11.508, then -11.524) x 2 halves.
        assert_eq!(arcs.len(), 52, "climb {climb}");
        for m in &arcs {
            let a = m.arc.expect("arc");
            assert_eq!(m.kind, MoveKind::Feed);
            assert!((a.center.x - C.x).abs() < 1e-9 && (a.center.y - C.y).abs() < 1e-9, "centre {:?}", a.center);
            // Old travel: CCW for climb (the polygon's positive angles), CW otherwise.
            assert_eq!(a.ccw, climb, "direction");
            let (r0, r1) = (rad(m, m.from), rad(m, m.to));
            assert!((r0 - r1).abs() < 2e-6, "endpoints on one circle: {r0} {r1}");
            assert!(((r0 - 1.0).abs() < 2e-6) || ((r0 - 0.5).abs() < 2e-6), "ring radius {r0}");
            assert!(m.from.z == m.to.z, "a ring is planar");
            // A half turn: the endpoints stand diametrically opposite.
            let d = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
            assert!((d - 2.0 * r0).abs() < 4e-6, "half turn: chord {d} on radius {r0}");
        }
        // The first ring starts where the polygon did: angle 0, the +X side.
        let first = arcs[0];
        assert!((first.from.x - (C.x + 1.0)).abs() < 2e-6 && (first.from.y - C.y).abs() < 2e-6, "{:?}", first.from);
    }
}

/// The ramp along the first cut rides the ring's own circle as helical
/// arcs (out with the ring's sense, back against it), a handful per
/// level - never the polygon's chords.
#[test]
fn the_ramp_rides_the_ring_as_helical_arcs() {
    for climb in [true, false] {
        // The helix has no room on this ring and stands down to the
        // zig-zag ramp along the first cut (its note says so).
        let t = path(&params(Entry3D::Helix, climb));
        assert!(t.notes.iter().any(|n| n.contains("stands down to a ramp")), "{:?}", t.notes);
        let ramps: Vec<&Move> = t
            .moves
            .iter()
            .filter(|m| m.kind == MoveKind::Feed && m.from.z != m.to.z && (m.from.x != m.to.x || m.from.y != m.to.y))
            .collect();
        assert!(!ramps.is_empty(), "the ramp entry ramps");
        let mut senses = (0, 0);
        for m in &ramps {
            let a = m.arc.expect("a ramp move on a ring is an arc, not a chord");
            assert!((a.center.x - C.x).abs() < 1e-9 && (a.center.y - C.y).abs() < 1e-9, "centre {:?}", a.center);
            let (r0, r1) = (rad(m, m.from), rad(m, m.to));
            assert!((r0 - r1).abs() < 2e-6, "on one ring: {r0} {r1}");
            assert!(m.to.z < m.from.z, "a ramp only falls");
            if a.ccw == climb { senses.0 += 1 } else { senses.1 += 1 }
        }
        assert!(senses.0 > 0 && senses.0 == senses.1, "out and back legs pair up: {senses:?}");
        // 26 ring-levels; each ramp is a few out-and-back legs of at most
        // two half turns each (the 512-gon took ~2,700 chords per ramp).
        assert!(ramps.len() <= 26 * 12, "{} ramp moves", ramps.len());
    }
}


/// An arc move's sweep in its own sense, radians in (0, 2 pi].
fn sweep(m: &Move) -> f64 {
    let a = m.arc.expect("an arc");
    let ang = |p: camcraft_motion::geometry::Point3| (p.y - a.center.y).atan2(p.x - a.center.x);
    let d = if a.ccw { ang(m.to) - ang(m.from) } else { ang(m.from) - ang(m.to) };
    let d = d.rem_euclid(2.0 * std::f64::consts::PI);
    if d <= 0.0 { 2.0 * std::f64::consts::PI } else { d }
}

fn is_ramp(m: &Move) -> bool {
    m.kind == MoveKind::Feed && m.from.z != m.to.z && (m.from.x != m.to.x || m.from.y != m.to.y)
}

/// #1558 review: the ring ramp keeps the ramp law. Every helical piece
/// descends at the ramp angle at most, measured along the arc it flies
/// (radius times its sweep, in its own sense); each ramp's out and back
/// legs all sweep the same planned leg (to the endpoint lattice); and the ramp's whole XY travel
/// is exactly what the angle needs for its drop - so no piece is steep
/// and none flies the long way round.
#[test]
fn the_ring_ramp_holds_the_ramp_angle_on_every_piece() {
    let tan = camcraft_motion::links::DEFAULT_RAMP_ANGLE.to_radians().tan();
    for climb in [true, false] {
        let t = path(&params(Entry3D::Helix, climb));
        let mut ramps: Vec<Vec<&Move>> = Vec::new();
        let mut run: Vec<&Move> = Vec::new();
        for m in &t.moves {
            if is_ramp(m) {
                run.push(m);
            } else if !run.is_empty() {
                ramps.push(std::mem::take(&mut run));
            }
        }
        if !run.is_empty() {
            ramps.push(run);
        }
        assert!(ramps.len() >= 26, "climb {climb}: {} ramps", ramps.len());
        for r in &ramps {
            let (mut xy, mut dz) = (0.0, 0.0);
            let mut legs: Vec<f64> = Vec::new();
            let mut sense = None;
            for m in r {
                let rho = rad(m, m.from);
                let len = rho * sweep(m);
                let d = m.from.z - m.to.z;
                // 1e-5 mm of XY slack: the endpoint lattice.
                assert!(d > 0.0 && d <= tan * (len + 1e-5), "climb {climb}: a piece at {:.4} deg", (d / len).atan().to_degrees());
                let ccw = m.arc.map(|a| a.ccw);
                if sense != ccw {
                    legs.push(0.0);
                    sense = ccw;
                }
                if let Some(l) = legs.last_mut() {
                    *l += len;
                }
                xy += len;
                dz += d;
            }
            assert!(legs.len().is_multiple_of(2), "climb {climb}: legs pair out and back: {legs:?}");
            assert!(legs.iter().all(|l| (l - legs[0]).abs() < 1e-5), "climb {climb}: every leg the planned leg: {legs:?}");
            assert!((xy * tan - dz).abs() <= 1e-6 * dz.max(1.0), "climb {climb}: travel {xy} for drop {dz}");
        }
    }
}

/// #1558 review: a tiny last level (a few microns: here 0.3 um, then
/// 3 um, below the 1 mm ladder) never posts an arc whose ends a post
/// could round together - checked at three-decimal mm and four-decimal
/// inch, the coarsest lengths the shipped posts write. A control reads
/// such an arc as a full circle.
#[test]
fn no_arc_rounds_to_a_full_circle_on_a_tiny_last_level() {
    for tail in [0.0003, 0.003] {
        for entry in [Entry3D::Plunge, Entry3D::Helix, Entry3D::Ramp] {
            let mut p = params(entry, true);
            p.bottom_z = -11.508 - tail;
            let t = path(&p);
            let arcs: Vec<&Move> = t.moves.iter().filter(|m| m.arc.is_some()).collect();
            assert!(!arcs.is_empty());
            for m in arcs {
                let mm = |v: f64| (v * 1e3).round();
                let inch = |v: f64| (v / 25.4 * 1e4).round();
                for (unit, f) in [("mm", &mm as &dyn Fn(f64) -> f64), ("inch", &inch)] {
                    assert!(
                        f(m.from.x) != f(m.to.x) || f(m.from.y) != f(m.to.y),
                        "{entry:?} tail {tail}: an arc {:?} -> {:?} posts as a full circle in {unit}",
                        m.from,
                        m.to
                    );
                }
                let c = (m.to.x - m.from.x).hypot(m.to.y - m.from.y);
                assert!(c >= camcraft_motion::ring_zig::MIN_ARC_CHORD, "{entry:?} tail {tail}: chord {c}");
            }
        }
    }
}
