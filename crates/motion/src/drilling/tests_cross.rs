//! M5.2 F2: recognition of RADIAL walls (cross-drilled holes) and
//! the live-tooling cycle built from them. The fixtures are closed
//! solids with punched caps - the parity through-law is a theorem
//! about closed boundaries, so a shortcut mesh here could only pin
//! a fiction.

use super::tests::{circle, circle_cw, drill_tool, ring_walls};
use super::*;
use crate::cl::MoveKind;
use crate::geometry::{Point3, Triangle};
use crate::machining2d::GenError;
use crate::tools::{Tool, ToolKind};

/// The point at along-coordinate `s` and transverse coordinates
/// (`pu`, `pv`) on axes `u`, `v` - `cylinder::coord` read backward.
fn sp(along: usize, u: usize, v: usize, s: f64, pu: f64, pv: f64) -> Point3 {
    let mut c = [0.0; 3];
    c[along] = s;
    c[u] = pu;
    c[v] = pv;
    Point3::new(c[0], c[1], c[2])
}

/// A quad as two triangles, the same cut `ring_walls` uses.
fn quad(a: Point3, b: Point3, c: Point3, d: Point3) -> Vec<Triangle> {
    vec![Triangle::new(a, b, c), Triangle::new(a, c, d)]
}

/// The wall of a cylinder about an axis-aligned line: `poly` is the
/// transverse ring in the (u, v) plane (CW to face INWARD, the hole
/// convention `ring_walls` states for Z), extruded s0..s1 along
/// `along`. Where (u, v, along) is an odd permutation of the world
/// axes the extrusion is mirrored, which flips the normals and
/// restores the inward facing.
fn wall_ring(
    along: usize,
    u: usize,
    v: usize,
    poly: &[(f64, f64)],
    s0: f64,
    s1: f64,
) -> Vec<Triangle> {
    let (s0, s1) = if matches!((u, v, along), (0, 1, 2) | (1, 2, 0) | (2, 0, 1)) {
        (s0, s1)
    } else {
        (s1, s0)
    };
    let mut f = Vec::new();
    let n = poly.len();
    for i in 0..n {
        let a = sp(along, u, v, s0, poly[i].0, poly[i].1);
        let b = sp(along, u, v, s0, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
        let c = sp(along, u, v, s1, poly[(i + 1) % n].0, poly[(i + 1) % n].1);
        let d = sp(along, u, v, s1, poly[i].0, poly[i].1);
        f.push(Triangle::new(a, b, c));
        f.push(Triangle::new(a, c, d));
    }
    f
}

/// A disc of radius `r` centered at (`cu`, `cv`), lying in the
/// plane `s`: a hole floor. Winding is irrelevant - floors are
/// caps to the weld law and surfaces to the parity ray.
// Fixture builders: the coordinate spread is the lattice's own
// (along/u/v + center + radius + count + spacing), not logic.
#[allow(clippy::too_many_arguments)]
fn disc(along: usize, u: usize, v: usize, cu: f64, cv: f64, r: f64, n: usize, s: f64) -> Vec<Triangle> {
    let p = circle(cu, cv, r, n);
    let ctr = sp(along, u, v, s, cu, cv);
    (0..n)
        .map(|i| {
            Triangle::new(
                ctr,
                sp(along, u, v, s, p[i].0, p[i].1),
                sp(along, u, v, s, p[(i + 1) % n].0, p[(i + 1) % n].1),
            )
        })
        .collect()
}

/// A rectangle (half-extents `hu`, `hv` about (`cu`, `cv`)) with a
/// circular opening of radius `r` punched out: the cap face a
/// drilled hole enters. Sector quads between circle and projected
/// boundary; the union is the annulus, so the solid stays closed.
// Same law as `disc`: a fixture builder, the spread is coordinates.
#[allow(clippy::too_many_arguments)]
fn annulus(
    along: usize,
    u: usize,
    v: usize,
    cu: f64,
    cv: f64,
    hu: f64,
    hv: f64,
    r: f64,
    n: usize,
    s: f64,
) -> Vec<Triangle> {
    let q = |k: usize| -> (f64, f64) {
        let a = std::f64::consts::TAU * (k as f64) / (n as f64);
        let (co, si) = (a.cos(), a.sin());
        let t = (co.abs() / hu).max(si.abs() / hv);
        (cu + co / t, cv + si / t)
    };
    let c = |k: usize| -> (f64, f64) {
        let a = std::f64::consts::TAU * (k as f64) / (n as f64);
        (cu + r * a.cos(), cv + r * a.sin())
    };
    let mut f = Vec::new();
    for k in 0..n {
        let qa = sp(along, u, v, s, q(k).0, q(k).1);
        let qb = sp(along, u, v, s, q(k + 1).0, q(k + 1).1);
        let ca = sp(along, u, v, s, c(k).0, c(k).1);
        let cb = sp(along, u, v, s, c(k + 1).0, c(k + 1).1);
        f.push(Triangle::new(qa, ca, cb));
        f.push(Triangle::new(qa, cb, qb));
    }
    f
}

/// The F2 fixture: a prismatic bar x in [-15, 15], y in [-5, 5],
/// carrying two X-axis holes - a BLIND one at z = 10 (wall 5..15,
/// floor disc at x = 5, +X face punched) and a THROUGH one at
/// z = -10 (wall spanning the bar, both faces punched). Every
/// cap the rays pass through is a real opening: the mesh is the
/// boundary of a solid, and the parity law is only honest then.
fn radial_bar() -> Vec<Triangle> {
    let mut t = Vec::new();
    // The four side walls: y = +-5 both blocks and the -X face
    // (the +X faces are the punched annuli, added with the holes).
    for (z0, z1) in [(5.0, 15.0), (-15.0, -5.0)] {
        t.extend(quad(
            Point3::new(-15.0, -5.0, z0),
            Point3::new(15.0, -5.0, z0),
            Point3::new(15.0, -5.0, z1),
            Point3::new(-15.0, -5.0, z1),
        ));
        t.extend(quad(
            Point3::new(-15.0, 5.0, z0),
            Point3::new(15.0, 5.0, z0),
            Point3::new(15.0, 5.0, z1),
            Point3::new(-15.0, 5.0, z1),
        ));
    }
    t.extend(quad(
        Point3::new(-15.0, -5.0, -15.0),
        Point3::new(-15.0, 5.0, -15.0),
        Point3::new(-15.0, 5.0, 15.0),
        Point3::new(-15.0, -5.0, 15.0),
    ));
    // Blind hole at (y=0, z=10), r=2, floor at x=5.
    t.extend(wall_ring(0, 1, 2, &circle_cw(0.0, 10.0, 2.0, 24), 5.0, 15.0));
    t.extend(disc(0, 1, 2, 0.0, 10.0, 2.0, 24, 5.0));
    t.extend(annulus(0, 1, 2, 0.0, 10.0, 5.0, 5.0, 2.0, 24, 15.0));
    // Through hole at (y=0, z=-10), r=2, spanning the bar.
    t.extend(wall_ring(0, 1, 2, &circle_cw(0.0, -10.0, 2.0, 24), -15.0, 15.0));
    t.extend(annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, 15.0));
    t.extend(annulus(0, 1, 2, 0.0, -10.0, 5.0, 5.0, 2.0, 24, -15.0));
    t
}

fn cross(x: f64, y: f64, z: f64, dir: Point3, r: f64, floor_d: f64, through: bool) -> CrossHole {
    CrossHole {
        origin: Point3::new(x, y, z),
        dir,
        radius: r,
        entry_d: 0.0,
        floor_d,
        through,
    }
}

fn nx() -> Point3 {
    Point3::new(-1.0, 0.0, 0.0)
}

fn ny() -> Point3 {
    Point3::new(0.0, -1.0, 0.0)
}

// ---- recognition

#[test]
fn recognize_cross_finds_blind_and_through_radial_walls() {
    let t = radial_bar();
    let found = recognize_cross(&t);
    assert_eq!(found.len(), 2, "{found:?}");
    // Sorted by (x + y, z, ...): the through hole at z = -10 first.
    assert!(found[0].through);
    assert_eq!(found[0].origin, Point3::new(15.0, 0.0, -10.0));
    assert_eq!(found[0].floor_d, -30.0);
    assert!(!found[1].through, "a floored hole must read blind");
    assert_eq!(found[1].origin, Point3::new(15.0, 0.0, 10.0));
    assert_eq!(found[1].dir, nx());
    assert_eq!(found[1].radius, 2.0);
    assert_eq!(found[1].floor_d, -10.0);
    // The bar's flat walls are straight and its caps are caps:
    // nothing here is a Z-axis hole.
    assert!(recognize(&t).is_empty());
}

#[test]
fn recognize_cross_reads_the_bar_itself_as_no_hole() {
    // A round bar: OD band (a boss by the vote law), end caps.
    let mut t = ring_walls(&circle(0.0, 0.0, 6.0, 32), 0.0, 40.0);
    t.extend(disc(2, 0, 1, 0.0, 0.0, 6.0, 32, 40.0));
    t.extend(disc(2, 0, 1, 0.0, 0.0, 6.0, 32, 0.0));
    assert!(recognize(&t).is_empty(), "the OD is not a hole");
    assert!(recognize_cross(&t).is_empty(), "no radial wall in a plain bar");
}

#[test]
fn recognize_cross_mixes_freely_with_the_z_pass() {
    // Same bar, plus an axial through hole: the two passes stay
    // independent readings, neither sees the other's walls.
    let mut t = radial_bar();
    t.extend(ring_walls(&circle_cw(0.0, 0.0, 2.0, 16), -15.0, 15.0));
    let axial = recognize(&t);
    assert_eq!(axial.len(), 1, "{axial:?}");
    assert!(axial[0].through);
    assert_eq!(axial[0].floor_z, -15.0);
    assert_eq!(recognize_cross(&t).len(), 2, "the axial wall is not radial");
}

#[test]
fn degenerate_soup_never_panics_and_never_lies() {
    let bad = vec![
        Triangle::new(
            Point3::new(f64::NAN, 0.0, 0.0),
            Point3::new(0.0, 1.0, 0.0),
            Point3::new(1.0, 0.0, 0.0),
        ),
        Triangle::new(Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, 0.0), Point3::new(0.0, 0.0, 0.0)),
    ];
    for h in recognize(&bad) {
        assert!(h.is_usable());
    }
    for h in recognize_cross(&bad) {
        assert!(h.is_usable(), "{h:?}");
    }
}

// ---- the cycle's refusals

fn xparams<'a>(tool: &'a Tool, holes: &'a [CrossHole]) -> CrossParams<'a> {
    CrossParams {
        clearance_d: 2.0,
        r_d: 1.0,
        hop_d: None,
        bottom_d: None,
        cycle: CycleChoice::Auto,
        peck: 0.0,
        dwell: 0.0,
        feed: 500.0,
        plunge_feed: 100.0,
        stock_r: 15.0,
        stock_z_max: None,
        tool,
        holes,
    }
}

#[test]
fn cross_drill_refuses_nonsense_by_name() {
    let dt = drill_tool(4.0);
    let ok = cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, false);
    assert!(matches!(cross_drill(&xparams(&dt, &[])), Err(GenError::Empty(_))), "empty list");
    let bad = [
        cross(15.0, 0.0, 10.0, nx(), 2.0, 0.0, false),
        cross(15.0, 0.0, 10.0, Point3::new(-2.0, 0.0, 0.0), 2.0, -10.0, false),
        cross(15.0, 0.0, 10.0, Point3::new(0.0, 0.0, -1.0), 2.0, -10.0, false),
        cross(f64::NAN, 0.0, 10.0, nx(), 2.0, -10.0, false),
    ];
    let names = [
        "floor not below the mouth",
        "dir not a unit",
        "a Z plunge belongs to the mill-side drill",
        "a NaN mouth is not a hole",
    ];
    for (h, n) in bad.iter().zip(names) {
        // #677: a hole-shaped refusal names its mouth (At); the rest stay Invalid.
        let e = cross_drill(&xparams(&dt, &[*h])).map(drop).err();
        assert!(matches!(e, Some(GenError::Invalid(_) | GenError::At { .. })), "{n}: {e:?}");
    }
    let z_plunge = cross_drill(&xparams(&dt, &[bad[2]])).map(drop).err();
    assert_eq!(
        z_plunge.and_then(|e| e.location()),
        Some(bad[2].origin),
        "a Z plunge's refusal marks the hole's mouth"
    );
    let across = [
        ok,
        cross(15.0, 0.0, 2.0, Point3::new(1.0, 0.0, 0.0), 2.0, -10.0, false),
    ];
    let why = cross_drill(&xparams(&dt, &across)).map(drop).err();
    match why {
        Some(GenError::Invalid(m)) => {
            assert!(m.contains("opposite stations"), "names the pair: {m}");
            assert!(m.contains("Z top"), "and names the missing witness: {m}");
        }
        other => panic!("expected an invalid refusal, got {other:?}"),
    }
    // The witness must be MEASURABLE: a NaN top is no top.
    let mut p = xparams(&dt, &across);
    p.stock_z_max = Some(f64::NAN);
    assert!(
        matches!(cross_drill(&p), Err(GenError::Invalid(m)) if m.contains("not usable")),
        "a NaN stock Z top is refused at the door, not flown blind over"
    );
    let ok_list = [ok];
    for c in [CycleChoice::Tap, CycleChoice::Bore, CycleChoice::BackBore, CycleChoice::Ream] {
        let mut p = xparams(&dt, &ok_list);
        p.cycle = c;
        assert!(matches!(cross_drill(&p), Err(GenError::Invalid(_))), "{c:?} is not radial-capable in v1");
    }
    let tap = Tool {
        kind: ToolKind::Tap,
        ..drill_tool(4.0)
    };
    assert!(
        matches!(cross_drill(&xparams(&tap, &[ok])), Err(GenError::Invalid(_))),
        "auto must not reach for a tap"
    );
}

#[test]
fn opposite_stations_take_the_axial_detour_above_the_bar() {
    let dt = drill_tool(4.0);
    let hs = [
        cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, false), // +X station
        cross(-15.0, 0.0, 2.0, Point3::new(1.0, 0.0, 0.0), 2.0, -10.0, false), // -X station
    ];
    let mut p = xparams(&dt, &hs);
    p.stock_z_max = Some(30.0);
    let t = cross_drill(&p).expect("the detour speaks where the chord cannot");
    assert!(t.is_chained() && t.all_finite());
    // z_top = 30 + clearance.max(margin) = 30 + max(2.0, 2.1).
    let z_top = 32.1;
    // THE LAW: every rapid point stands outside the bar's radius or
    // rides the swing plane above its end face - nothing dives. An
    // axial rapid travels a hole's own axis, so its START may sit in
    // the void just cut (machined truth, the F2 law); a translating
    // rapid must be clear at BOTH ends.
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        let axial = (m.to.x - m.from.x).abs() < 1e-9 || (m.to.y - m.from.y).abs() < 1e-9;
        for pt in [Some(m.to), if axial { None } else { Some(m.from) }]
            .into_iter()
            .flatten()
        {
            let rad = (pt.x * pt.x + pt.y * pt.y).sqrt();
            assert!(
                rad >= p.stock_r - 1e-6 || pt.z >= z_top - 1e-6,
                "rapid point {pt:?} inside the bar with no license to be"
            );
        }
    }
    // The swing itself exists: one rapid across the axis at the
    // plane, and the index is ALREADY the new station's pose - the
    // turret turns in the clear, not after arriving.
    let swing = t
        .moves
        .iter()
        .find(|m| {
            m.kind == MoveKind::Rapid
                && (m.from.z - z_top).abs() < 1e-6
                && (m.to.z - z_top).abs() < 1e-6
                && m.from.x * m.to.x < 0.0
        })
        .expect("a rapid crosses the spindle axis ABOVE the bar");
    assert_eq!(
        swing.axis,
        Some([-1.0, 0.0, 0.0]),
        "the -X station's tip->shank points -X: the index happened on the lift"
    );
    // No straight traverse EVER runs at mouth height between the two columns.
    assert!(
        !t.moves.iter().any(|m| {
            m.kind == MoveKind::Rapid
                && m.from.x * m.to.x < 0.0
                && m.from.z.abs() < 30.0
                && (m.from.z - m.to.z).abs() < 1e-9
        }),
        "no chord at a low Z crosses the axis"
    );
}

#[test]
fn a_negative_station_hops_on_its_own_column() {
    // start_d once read the hop plane as a SIGNED coordinate, which
    // parked every negative-side station on the far column - a rapid
    // straight through the spindle. The plane is a radial distance
    // on the mouth's own side; the law holds at endpoints and
    // mid-chord for the ordinary (quarter-turn) traverse too.
    let dt = drill_tool(4.0);
    let hs = [
        cross(-15.0, 0.0, 10.0, Point3::new(1.0, 0.0, 0.0), 2.0, -4.0, false),
        cross(0.0, -15.0, 2.0, Point3::new(0.0, 1.0, 0.0), 2.0, -4.0, false),
    ];
    let t = cross_drill(&xparams(&dt, &hs)).unwrap();
    assert!(t.is_chained() && t.all_finite());
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        let axial = (m.to.x - m.from.x).abs() < 1e-9 || (m.to.y - m.from.y).abs() < 1e-9;
        for f in if axial { 2..=2 } else { 0..=2 } {
            let g = f as f64 * 0.5;
            let r = (m.from.x + (m.to.x - m.from.x) * g).hypot(m.from.y + (m.to.y - m.from.y) * g);
            assert!(r >= 15.0 - 1e-6, "traverse dives to {r} between the columns");
        }
    }
    // The -X station's own column, not the +X one: its first
    // approach rapid ENDS at a negative x.
    let first = t.moves.iter().find(|m| m.kind == MoveKind::Rapid).unwrap();
    assert!(first.to.x < 0.0, "the hop stands on the mouth's side: {first:?}");
}

#[test]
fn cross_drill_refuses_a_bottom_above_the_mouth() {
    let dt = drill_tool(4.0);
    let hs = [cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, false)];
    let mut p = xparams(&dt, &hs);
    p.bottom_d = Some(15.5); // a plane standing OUTSIDE the entry
    let air = cross_drill(&p).map(drop).err();
    assert!(matches!(air, Some(GenError::At { .. })), "a hole that cuts air refuses: {air:?}");
    assert_eq!(air.and_then(|e| e.location()), Some(hs[0].origin), "and marks that hole's mouth (#677)");
    let mut p = xparams(&dt, &hs);
    p.stock_r = f64::NAN;
    assert!(matches!(cross_drill(&p), Err(GenError::Invalid(_))), "an unmeasurable stock refuses");
}

// ---- the cycle's truths

#[test]
fn cross_drill_cuts_chains_and_tags_every_pose() {
    let dt = drill_tool(4.0);
    let holes = [
        cross(15.0, 0.0, 10.0, nx(), 2.0, -2.0, false),
        cross(0.0, 15.0, -8.0, ny(), 1.5, -2.0, false),
    ];
    let t = cross_drill(&xparams(&dt, &holes)).unwrap();
    assert!(t.is_chained() && t.all_finite());
    assert!(t.cycles.is_empty(), "v1: the chain is the radial truth");
    assert!(t.moves.iter().all(|m| m.axis.is_some()));
    // Block order: hole 1's cuts ride -X; the station change's
    // travel rapid is tagged with the OLD pose (the turret swings
    // after it arrives); hole 2's cuts ride -Y. The tag states
    // tip->shank, so hole 1's inward -X plunge wears +X.
    let travel = t
        .moves
        .iter()
        .find(|m| {
            m.kind == MoveKind::Rapid
                && m.axis == Some([1.0, 0.0, 0.0])
                && m.to.x == 0.0 && m.to.y > 0.0
        })
        .expect("the travel rapid rides the old station's pose");
    assert_eq!(travel.feed, None);
    let cut_x = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge && m.to.x < 15.0).count();
    let cut_y = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge && m.to.y > 0.0 && m.to.x <= 1e-9).count();
    assert!(cut_x >= 1 && cut_y >= 1);
}

#[test]
fn every_traverse_rapid_stays_clear_of_the_stock() {
    // The law the chord fix exists for: a station-to-station rapid
    // may never dive its tip (let alone its rim, d/2 off-axis)
    // inside the bar's radius - at the endpoints AND mid-chord.
    let dt = drill_tool(4.0);
    let holes = [
        cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, false),
        cross(0.0, 15.0, -8.0, ny(), 2.0, -31.0, true),
    ];
    let stock_r = 15.0;
    let t = cross_drill(&xparams(&dt, &holes)).unwrap();
    for m in t.moves.iter().filter(|m| m.kind == MoveKind::Rapid) {
        // An axial rapid travels the hole's own axis: its START may
        // sit inside the hole it is leaving (that void is machined
        // truth); its END must stand clear of the stock. A non-
        // axial rapid TRANSLATES, so it must clear the bar - rim
        // allowance included - at every point of the chord.
        let axial = (m.to.x - m.from.x).abs() < 1e-9 || (m.to.y - m.from.y).abs() < 1e-9;
        let lo = if axial { 2 } else { 0 };
        for f in lo..=2 {
            let g = f as f64 * 0.5;
            let p = Point3::new(
                m.from.x + (m.to.x - m.from.x) * g,
                m.from.y + (m.to.y - m.from.y) * g,
                0.0,
            );
            let need = if axial { stock_r } else { stock_r + 2.0 + 0.05 };
            let r = (p.x * p.x + p.y * p.y).sqrt();
            assert!(r >= need - 1e-6, "rapid {m:?} sample {f} at radius {r} < {need}");
        }
    }
    // The through hole must exit THROUGH_CLEAR past the far face:
    // its deepest point sits below x = -15... and y = -16 for the -Y one.
    let deepest_y = t.moves.iter().map(|m| m.to.y.min(m.from.y)).fold(f64::INFINITY, f64::min);
    assert!(deepest_y < -15.0, "through hole must clear the exit face, got {deepest_y}");
}

#[test]
fn blind_depth_is_exact_and_through_adds_the_exit_allowance() {
    let dt = drill_tool(2.0);
    // An -X plunge reaches depth at SMALLER x: the deepest point of
    // the path is its minimum coordinate, not its maximum.
    let t = cross_drill(&xparams(&dt, &[cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, false)])).unwrap();
    let deepest_x = t
        .moves
        .iter()
        .map(|m| m.to.x.min(m.from.x))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(deepest_x, 5.0, "blind floor at x = 5, exactly");
    let t = cross_drill(&xparams(&dt, &[cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, true)])).unwrap();
    let deepest_x = t
        .moves
        .iter()
        .map(|m| m.to.x.min(m.from.x))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(deepest_x, 5.0 - THROUGH_CLEAR, "through exit rides PAST the floor");
}

#[test]
fn spot_dims_the_mouth_by_two_and_clamps_at_a_shallow_floor() {
    let dt = drill_tool(4.0);
    let hs = [cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, false)];
    let mut p = xparams(&dt, &hs);
    p.cycle = CycleChoice::Spot;
    let deepest = |t: &crate::cl::Toolpath| {
        t.moves
            .iter()
            .map(|m| m.to.x.min(m.from.x))
            .fold(f64::INFINITY, f64::min)
    };
    let t = cross_drill(&p).unwrap();
    assert_eq!(deepest(&t), 15.0 - SPOT_DEPTH, "the dimple is 2 below the MOUTH, not the rapid plane");
    let shallow = [cross(15.0, 0.0, 10.0, nx(), 2.0, -0.5, false)];
    let mut q = xparams(&dt, &shallow);
    q.cycle = CycleChoice::Spot;
    let t = cross_drill(&q).unwrap();
    assert_eq!(deepest(&t), 14.5, "a dimple must stop at the floor, never break through");
}

#[test]
fn auto_peck_marches_and_the_chip_break_hop_never_lifts_out() {
    let dt = drill_tool(2.0);
    let hole = [cross(15.0, 0.0, 10.0, nx(), 2.0, -20.0, false)];
    let mut p = xparams(&dt, &hole);
    p.cycle = CycleChoice::Peck;
    let t = cross_drill(&p).unwrap();
    let plunges = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert!(plunges > 3, "a 20-deep hole must peck, got {plunges} plunges");
    // G73 shape: between pecks the tool hops ~CHIP_BREAK up and
    // immediately back down - never out past the mouth plane
    // (x = 15); only the final retract reaches the r_d plane.
    let rapids: Vec<f64> = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid)
        .map(|m| m.to.x)
        .collect();
    assert!(rapids.iter().any(|&x| x < 15.0), "chip-break hops stay in the hole: {rapids:?}");
    // (The only things above the r_d plane are the stock-clear
    // end/hop rapids: x <= stock_r + clearance + margin = 17.1.)
    assert!(rapids.iter().all(|&x| x <= 17.1 + 1e-6), "no rapid runs wild outside the clearance plane");
    p.cycle = CycleChoice::Auto; // 21 deep on a 2 mm tool: the Auto law resolves to G83
    let t = cross_drill(&p).unwrap();
    let mid_lifts = t
        .moves
        .iter()
        .filter(|m| m.kind == MoveKind::Rapid)
        .filter(|m| (m.to.x - 16.0).abs() < 0.1)
        .count();
    let plunges = t.moves.iter().filter(|m| m.kind == MoveKind::Plunge).count();
    assert!(mid_lifts >= plunges - 1, "G83 fully retracts to r_d between pecks");
}

#[test]
fn bottom_d_overrides_with_an_absolute_plane() {
    let dt = drill_tool(4.0);
    let hs = [cross(15.0, 0.0, 10.0, nx(), 2.0, -10.0, false)];
    let mut p = xparams(&dt, &hs);
    p.bottom_d = Some(7.0);
    let t = cross_drill(&p).unwrap();
    let deepest_x = t
        .moves
        .iter()
        .map(|m| m.to.x.min(m.from.x))
        .fold(f64::INFINITY, f64::min);
    assert_eq!(deepest_x, 7.0, "the override plane wins over the floor");
}

#[test]
fn recognition_feeds_the_cycle_end_to_end() {
    // The whole F2 loop over one mesh: find the walls, then run
    // them - no gouge, no lie, chain intact.
    let t = radial_bar();
    let found = recognize_cross(&t);
    let stock_r = t
        .iter()
        .flat_map(|tr| [tr.a, tr.b, tr.c])
        .map(|p| (p.x * p.x + p.y * p.y).sqrt())
        .fold(0.0f64, f64::max);
    let dt = drill_tool(4.0);
    let path = cross_drill(&xparams(&dt, &found)).unwrap_or_else(|e| panic!("{e}"));
    assert!(path.is_chained() && path.all_finite());
    assert!(path.cycles.is_empty());
    let cut_blind = path.moves.iter().any(|m| m.kind == MoveKind::Plunge && (m.to.z - 10.0).abs() < 1e-9);
    let cut_through = path.moves.iter().any(|m| m.kind == MoveKind::Plunge && (m.to.z + 10.0).abs() < 1e-9);
    assert!(cut_blind && cut_through, "both recognized walls get cut");
    assert!(stock_r > 15.0 && stock_r < 16.0);
}
