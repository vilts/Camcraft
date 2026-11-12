//! #946: in Boundary mode law (b) read an XZ / YZ (G18 / G19) arc only at
//! its chord joints, so a sharp wall vertex standing between two joints
//! was never measured against the cutter. Such an arc's XY footprint is
//! a straight segment (planar) or a gentle curve (helical); it now
//! answers on that footprint to the walls
//! (`sim::verify::arc_wall_plane`). The numbers that settle the issue's
//! P1 argument: an r 200 arc whose joints stand ~1.9 mm apart in XY reads
//! a vertex 3.90 mm off its footprint at >= 4.016 mm from every joint -
//! clear of the 3.99 mm floor - so a 90 um gouge passed before the fix.
//! Also the model-backed pin of the whole-move reading (#905's review).
//!
//! Gate map: covers camcraft_sim::verify (section, arc_wall,
//! arc_wall_plane, law (b)).
//!
//! Provenance: our own fixtures; point-to-segment distance from
//! elementary plane geometry; the tangency law is our own
//! (sim::verify::section). No vendor material.

use camcraft_sim::cl::{ArcPlane, Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::{Point3, Triangle, cube};
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

/// The gouge verdict tolerance (`GOUGE_TOL`).
const TOL: f64 = 0.01;
/// The cutter radius.
const R: f64 = 4.0;
/// The arc: r 200, 0.048 rad, its low end at z -1.
const RAD: f64 = 200.0;
const SWEEP: f64 = 0.048;

fn p2(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn floor() -> f64 {
    R - TOL
}

fn tool() -> ToolGeom {
    ToolGeom {
        radius: R,
        nose_radius: 0.0,
        flute_len: 30.0,
        shank_radius: R,
        shank_len: 40.0,
        holder_radius: 20.0,
        holder_len: 60.0,
    }
}

fn judge(t: &Toolpath, region: &Region, model: Option<&[Triangle]>) -> Vec<Warning> {
    let g = tool();
    let cx = VerifyContext {
        region,
        stock: None,
        model,
        tool: &g,
        cutter: None,
        mode: Mode::Boundary,
        stock_top_z: 0.0,
        clearance_z: 5.0,
        stock_to_leave: 0.0,
        gouge_tol: TOL,
        model_excess: 0.0,
        z_tol: 1e-6,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
    };
    verify(t, &cx)
}

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Loop {
    match Loop::new(vec![p2(x0, y0), p2(x1, y0), p2(x1, y1), p2(x0, y1)]) {
        Ok(l) => l,
        Err(e) => panic!("rect: {e:?}"),
    }
}

/// The vertical arc in `plane`: centre 199 above its low end, sweeping
/// from the bottom of the circle up one side by `SWEEP`, at normal
/// coordinate 0 rising to `rise` (0: planar). The low end stands at the
/// XY origin, z -1; the footprint runs ~9.6 mm along +X (XZ) or -Y (YZ).
/// `forward`: cut from the low end up, else back down.
fn arc(plane: ArcPlane, rise: f64, forward: bool) -> Move {
    // In (u, v): XZ is (z, x), YZ is (y, z); the circle's bottom is at
    // pi (XZ: u = z least) or -pi/2 (YZ: v = z least).
    let (cw, bottom) = match plane {
        ArcPlane::Xz => (p2(0.0, 199.0), std::f64::consts::PI),
        _ => (p2(0.0, 199.0), -std::f64::consts::FRAC_PI_2),
    };
    let (cu, cv) = plane.center_uv(cw);
    let at = |t: f64, w: f64| plane.point(cu + RAD * t.cos(), cv + RAD * t.sin(), w);
    // XZ climbs toward +x with decreasing angle, YZ toward -y likewise.
    let (lo, hi) = (at(bottom, 0.0), at(bottom - SWEEP, rise));
    let (from, to, ccw) = if forward { (lo, hi, false) } else { (hi, lo, true) };
    Move::new(MoveKind::Feed, from, to, Some(500.0)).with_arc_in(cw, ccw, plane)
}

fn path(m: Move) -> Toolpath {
    Toolpath { moves: vec![m], ..Toolpath::default() }
}

/// The joints the old reading measured: the union at the whole and the
/// quarter tolerance.
fn joints(m: &Move) -> Vec<Point2> {
    [TOL, TOL * 0.25].into_iter().flat_map(|t| m.chords(t)).flat_map(|(a, b)| [a, b]).collect()
}

/// The footprint point farthest from every joint (dense samples of the
/// true arc): where a wall vertex hides from the joint reading.
fn hidden_spot(m: &Move) -> Point2 {
    let Some(a) = m.arc else { panic!("an arc") };
    let js = joints(m);
    let gap = |p: Point2| js.iter().map(|j| (j.x - p.x).hypot(j.y - p.y)).fold(f64::INFINITY, f64::min);
    (0..=4000)
        .filter_map(|k| a.point_at(m.from, m.to, k as f64 / 4000.0))
        .map(|p| p2(p.x, p.y))
        .max_by(|p, q| gap(*p).total_cmp(&gap(*q)))
        .unwrap_or(p2(0.0, 0.0))
}

/// A pocket with a sharp island vertex `d` off the footprint point `at`,
/// on the side `side` (+1 / -1) across the footprint's line, its sides
/// opening away (half-angle ~14 deg). `plane` names the line's axis.
fn region_at(plane: ArcPlane, at: Point2, d: f64, side: f64) -> (Region, Point2) {
    // Away from the line, and along it.
    let ((ux, uy), (tx, ty)) = match plane {
        ArcPlane::Xz => ((0.0, side), (1.0, 0.0)),
        _ => ((side, 0.0), (0.0, 1.0)),
    };
    let v = p2(at.x + ux * d, at.y + uy * d);
    let s = |k: f64| p2(v.x + ux * 20.0 + tx * 5.0 * k, v.y + uy * 20.0 + ty * 5.0 * k);
    let island = Loop::new(vec![v, s(1.0), s(-1.0)]);
    match island.and_then(|i| Region::new(vec![rect(-60.0, -60.0, 60.0, 60.0), i])) {
        Ok(r) => (r, v),
        Err(e) => panic!("region: {e:?}"),
    }
}

#[test]
fn a_vertex_between_a_vertical_arcs_joints_past_the_floor_is_a_gouge() {
    for plane in [ArcPlane::Xz, ArcPlane::Yz] {
        for rise in [0.0, 0.2] {
            for forward in [true, false] {
                let m = arc(plane, rise, forward);
                let spot = hidden_spot(&m);
                for side in [1.0, -1.0] {
                    // 90 um past the floor: every joint reads it clear.
                    let (region, v) = region_at(plane, spot, 3.90, side);
                    let seen = joints(&m).iter().map(|j| (j.x - v.x).hypot(j.y - v.y)).fold(f64::INFINITY, f64::min);
                    let tag = format!("{plane:?} rise {rise} forward {forward} side {side}");
                    assert!(seen >= floor(), "{tag}: the joints read it clear: {seen}");
                    assert_eq!(judge(&path(m), &region, None), vec![Warning::Gouge { at: 0 }], "{tag}");
                    for d in [3.5, 3.8, 3.98] {
                        let (region, _) = region_at(plane, spot, d, side);
                        assert_eq!(judge(&path(m), &region, None), vec![Warning::Gouge { at: 0 }], "{tag}, d {d}");
                    }
                }
            }
        }
    }
}

#[test]
fn the_same_vertex_clear_of_the_floor_stays_clean() {
    for plane in [ArcPlane::Xz, ArcPlane::Yz] {
        for (rise, ds) in [(0.0, [3.995, 4.0, 4.5]), (0.2, [4.0, 4.5, 6.0])] {
            for forward in [true, false] {
                let m = arc(plane, rise, forward);
                let spot = hidden_spot(&m);
                for side in [1.0, -1.0] {
                    for d in ds {
                        let (region, _) = region_at(plane, spot, d, side);
                        let tag = format!("{plane:?} rise {rise} forward {forward} side {side} d {d}");
                        assert_eq!(judge(&path(m), &region, None), Vec::new(), "{tag}");
                    }
                }
            }
        }
    }
}

/// With a model, the section's edges are walls: a part corner standing
/// between a vertical arc's joints convicts, the same corner clear of
/// the floor passes. The part is a block whose corner points at the
/// footprint (half-angle 45 deg), standing z -10..10 over the arc.
#[test]
fn a_model_corner_between_a_vertical_arcs_joints_is_judged() {
    for plane in [ArcPlane::Xz, ArcPlane::Yz] {
        let m = arc(plane, 0.0, true);
        let spot = hidden_spot(&m);
        for (d, gouge) in [(3.90, true), (4.5, false)] {
            // XZ: the line runs along x, the block stands at +y; YZ: along
            // y, the block at +x.
            let v = match plane {
                ArcPlane::Xz => p2(spot.x, spot.y + d),
                _ => p2(spot.x + d, spot.y),
            };
            let facets = diamond(v, plane);
            let region = Region::new(vec![rect(-60.0, -60.0, 60.0, 60.0)]).unwrap_or_else(|e| panic!("region: {e:?}"));
            let want = if gouge { vec![Warning::Gouge { at: 0 }] } else { Vec::new() };
            assert_eq!(judge(&path(m), &region, Some(&facets)), want, "{plane:?} d {d}");
        }
    }
}

/// A square block rotated 45 deg in XY, one corner at `v` pointing back
/// at the footprint line (-y for XZ, -x for YZ), z -10..10, as a closed
/// mesh: an axis-aligned cube with its XY turned about `v`.
fn diamond(v: Point2, plane: ArcPlane) -> Vec<Triangle> {
    let side = 10.0;
    let c = cube(Point3::new(0.0, 0.0, -10.0), Point3::new(side, side, 10.0));
    // Turn the cube's (0, 0) corner to `v`, its diagonal along `away`.
    let away = match plane {
        ArcPlane::Xz => std::f64::consts::FRAC_PI_2,
        _ => 0.0,
    };
    let rot = away - std::f64::consts::FRAC_PI_4;
    let (s, k) = rot.sin_cos();
    let turn = |p: Point3| Point3::new(v.x + p.x * k - p.y * s, v.y + p.x * s + p.y * k, p.z);
    c.facets
        .iter()
        .map(|t| Triangle { a: turn(t.a), b: turn(t.b), c: turn(t.c) })
        .collect()
}
