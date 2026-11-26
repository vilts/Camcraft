//! #3554: a hole whose axis is tilted a few degrees from Z. Tests
//! only; the meshes are built here from the section of a tilted
//! circular cylinder (an ellipse, centre sliding along the tilt).

use super::recognize::FIT_TOL;
use super::tests::ring_walls;
use super::*;
use crate::geometry::{Point3, Triangle};

/// The geometry of one tilted wall: a cylinder of radius `r` whose
/// axis passes through `(cx, cy, 0)` and leans `deg` degrees from Z
/// toward +X, cut between `z0` and `z1` in `levels` bands of `n`
/// facets around.
struct TiltedWall {
    cx: f64,
    cy: f64,
    r: f64,
    deg: f64,
    z0: f64,
    z1: f64,
    levels: usize,
    n: usize,
}

/// The inward-facing wall of a `TiltedWall`. Each horizontal section
/// is the ellipse of the tilted cylinder: semi-axis `r / cos` along X,
/// `r` along Y, and its centre at `x = cx + z * tan`.
fn tilted_wall(spec: TiltedWall) -> Vec<Triangle> {
    let TiltedWall {
        cx,
        cy,
        r,
        deg,
        z0,
        z1,
        levels,
        n,
    } = spec;
    let (s, c) = deg.to_radians().sin_cos();
    let ring = |z: f64| -> Vec<Point3> {
        // CW order: a hole's wall faces its axis (as `circle_cw`).
        (0..n)
            .map(|i| {
                let a = -std::f64::consts::TAU * (i as f64) / (n as f64);
                Point3::new(cx + z * s / c + r / c * a.cos(), cy + r * a.sin(), z)
            })
            .collect()
    };
    let mut f = Vec::new();
    for k in 0..levels {
        let za = z0 + (z1 - z0) * (k as f64) / (levels as f64);
        let zb = z0 + (z1 - z0) * ((k + 1) as f64) / (levels as f64);
        let (lo, hi) = (ring(za), ring(zb));
        for i in 0..n {
            let j = (i + 1) % n;
            f.push(Triangle::new(lo[i], lo[j], hi[j]));
            f.push(Triangle::new(lo[i], hi[j], hi[i]));
        }
    }
    f
}

/// A `w x w x t` plate's outer walls plus `extra`.
fn plate(w: f64, t: f64, extra: Vec<Triangle>) -> Vec<Triangle> {
    let rect = vec![(0.0, 0.0), (w, 0.0), (w, w), (0.0, w)];
    let mut m = ring_walls(&rect, 0.0, t);
    m.extend(extra);
    m
}

/// The fixture is honest: at zero tilt the same builder gives a
/// Z hole that recognition finds. So a drop below is the tilt's
/// verdict, not a broken mesh.
#[test]
fn tilted_wall_builder_at_zero_tilt_is_a_z_hole() {
    let m = plate(
        30.0,
        10.0,
        tilted_wall(TiltedWall {
            cx: 15.0,
            cy: 15.0,
            r: 3.0,
            deg: 0.0,
            z0: 0.0,
            z1: 10.0,
            levels: 4,
            n: 32,
        }),
    );
    let h = recognize(&m);
    assert_eq!(h.len(), 1, "{h:?}");
    assert!((h[0].radius - 3.0).abs() < 1e-3, "{h:?}");
    assert!(h[0].through);
}

/// The issue's case: a 10 mm plate with one hole tilted 2 degrees and
/// one tilted 5 degrees. Neither is a Z hole (a Z drill would cut the
/// wall of the real hole), and neither is a cross hole. Over 10 mm the
/// 2-degree axis slides the section 0.35 mm - far over `FIT_TOL` - so
/// the Z circle fit refuses the wall.
#[test]
fn tilted_holes_in_a_plate_are_not_drilled_along_z() {
    let mut walls = tilted_wall(TiltedWall {
        cx: 10.0,
        cy: 15.0,
        r: 3.0,
        deg: 2.0,
        z0: 0.0,
        z1: 10.0,
        levels: 4,
        n: 32,
    });
    walls.extend(tilted_wall(TiltedWall {
        cx: 30.0,
        cy: 15.0,
        r: 3.0,
        deg: 5.0,
        z0: 0.0,
        z1: 10.0,
        levels: 4,
        n: 32,
    }));
    let m = plate(40.0, 10.0, walls);
    let z = recognize(&m);
    assert!(
        z.is_empty(),
        "a tilted hole must not pass as a Z hole: {z:?}"
    );
    let x = recognize_cross(&m);
    assert!(
        x.is_empty(),
        "a tilted hole is not a cross hole either: {} found",
        x.len()
    );
    // One band or many: the fit reads every wall vertex the same way.
    for levels in [1, 2, 8] {
        for deg in [2.0, 5.0, -2.0] {
            let m = plate(
                30.0,
                10.0,
                tilted_wall(TiltedWall {
                    cx: 15.0,
                    cy: 15.0,
                    r: 3.0,
                    deg,
                    z0: 0.0,
                    z1: 10.0,
                    levels,
                    n: 32,
                }),
            );
            assert!(recognize(&m).is_empty(), "{deg} deg, {levels} bands");
        }
    }
}

/// The bound that makes a thin wall safe. When a tilted hole is short
/// enough that the Z fit accepts it, `FIT_TOL` already bounds the
/// gouge: every wall vertex lies within `FIT_TOL` of the found circle,
/// so a drill of the found radius moves no wall by more than that.
/// The same 20 um is the chord noise any tessellated hole carries.
#[test]
fn a_tilt_the_fit_accepts_moves_no_wall_past_fit_tol() {
    let mut accepted = 0;
    for &(deg, t) in &[
        (2.0, 0.5),
        (2.0, 1.0),
        (2.0, 2.0),
        (5.0, 0.3),
        (5.0, 1.0),
        (1.0, 3.0),
    ] {
        let walls = tilted_wall(TiltedWall {
            cx: 15.0,
            cy: 15.0,
            r: 3.0,
            deg,
            z0: 0.0,
            z1: t,
            levels: 1,
            n: 48,
        });
        let pts: Vec<Point3> = walls.iter().flat_map(|f| [f.a, f.b, f.c]).collect();
        let m = plate(30.0, t, walls);
        for h in recognize(&m) {
            accepted += 1;
            let worst = pts
                .iter()
                .map(|p| ((p.x - h.xy.x).hypot(p.y - h.xy.y) - h.radius).abs())
                .fold(0.0f64, f64::max);
            // Lattice snap of the centre and radius adds well under 1 um.
            assert!(
                worst <= FIT_TOL + 1e-3,
                "{deg} deg over {t} mm: a wall vertex sits {worst} mm off the Z hole"
            );
        }
    }
    // The bound is exercised, not vacuous: the thinnest walls pass.
    assert!(
        accepted > 0,
        "no thin tilted wall was accepted; the bound was not tested"
    );
}
