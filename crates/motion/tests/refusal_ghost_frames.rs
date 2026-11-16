//! #1015: the part-way refusals that needed a frame map or a merge
//! before they could hand back a ghost. Each test pins that the ghost
//! lands where the part is - in the frame the finished path is in -
//! by checking it against the path the same generator emits when it
//! succeeds: the 3-axis raster (and the 5-axis kinds that pass its
//! plan through), the rotary parallel (planned frame-local, carried
//! out by the same pose law as its path), and the multi-axis curve
//! walk (chains walked before the refused one).
//!
//! Provenance: our own tests over our own refusal model; no vendor
//! material consulted.
//!
//! Gate map (#1015): covers camcraft_motion::finishing3d (raster,
//! rings), camcraft_motion::multiaxis (rotary, rotary_links, curve,
//! parallel5d), camcraft_motion::machining2d (gen_error, ghost).

use std::f64::consts::TAU;

use camcraft_motion::cl::{MoveKind, Toolpath};
use camcraft_motion::finishing3d::{self, Finish3DParams, FinishPattern};
use camcraft_motion::geometry::{cube, Point3, Triangle};
use camcraft_motion::machining2d::GenError;
use camcraft_motion::multiaxis::{contour_curve, parallel5d, rotary_parallel, AxisControl, CurveParams};

fn p3(x: f64, y: f64, z: f64) -> Point3 {
    Point3::new(x, y, z)
}

/// A finish whose shallow raster runs at a stated stepover; `steep` is
/// the flank spacing SteepShallow's ring ladder reads - a tiny one
/// asks for more levels than the cap, so the ladder refuses AFTER the
/// raster is built, and a `None` lets the same raster succeed.
fn finish(top_z: f64, bottom_z: f64, d: f64, steep: Option<f64>) -> Finish3DParams {
    Finish3DParams {
        pattern: FinishPattern::SteepShallow,
        stepover: Some(d / 3.0),
        tool_diameter: d,
        tolerance: 0.05,
        clearance_z: top_z + 15.0,
        retract_z: None,
        top_z,
        bottom_z,
        feed: 800.0,
        plunge_feed: 200.0,
        climb: true,
        ramp: false,
        min_step: None,
        ramp_angle: 45.0,
        entry_ramp: false,
        steep_stepover: steep,
        pencil_reference: None,
    }
}

fn refused(r: Result<Toolpath, GenError>) -> GenError {
    match r {
        Ok(t) => panic!("expected the ring ladder's level cap to refuse, got {} moves", t.moves.len()),
        Err(e) => e,
    }
}

#[test]
fn the_raster_refused_at_its_rings_hands_back_the_world_raster() {
    // An off-origin plate: the raster runs over its top, then the ring
    // ladder refuses its level cap. The ghost is the raster built - the
    // very moves the same finish emits first when it succeeds.
    let plate = cube(p3(100.0, 50.0, 0.0), p3(130.0, 70.0, 10.0)).facets.to_vec();
    let e = refused(finishing3d::parallel(&plate, &finish(10.0, 0.0, 6.0, Some(1e-5))));
    assert!(e.to_string().contains("levels (cap"), "{e}");
    let g = e.ghost().expect("the raster was built before the rings refused");
    let ok = finishing3d::parallel(&plate, &finish(10.0, 0.0, 6.0, None)).expect("the same raster, rings allowed");
    assert!(g.len() < ok.moves.len());
    assert_eq!(g.moves(), &ok.moves[..g.len()], "the ghost is the world path's own opening");
    let on_top: Vec<_> = g.moves().iter().filter(|m| m.kind == MoveKind::Feed && (m.to.z - 10.0).abs() < 1e-6).collect();
    assert!(!on_top.is_empty(), "the raster rode the plate's top");
    assert!(on_top.iter().all(|m| (97.0..=133.0).contains(&m.to.x) && (47.0..=73.0).contains(&m.to.y)));
    // The 5-axis raster plans in the world and poses after: its
    // refusal hands back the same world plan, not a frame of its own.
    let e5 = refused(parallel5d(&plate, &finish(10.0, 0.0, 6.0, Some(1e-5)), &AxisControl::default(), None));
    assert_eq!(e5.ghost().map(|h| h.moves().to_vec()), Some(g.moves().to_vec()));
}

/// A closed cylinder of radius `rad`, `len` long, laid along world X
/// with its axis through (y0, z0): the rotary parallel's planning
/// frame is nothing like the world here.
fn laid_cylinder(rad: f64, len: f64, y0: f64, z0: f64) -> Vec<Triangle> {
    let n = 32usize;
    let at = |j: usize, r: f64, x: f64| {
        let th = TAU * j as f64 / n as f64;
        p3(x, y0 + r * th.cos(), z0 + r * th.sin())
    };
    let mut t = Vec::new();
    for j in 0..n {
        let (a1, a2, b1, b2) = (at(j, rad, 0.0), at(j + 1, rad, 0.0), at(j, rad, len), at(j + 1, rad, len));
        t.push(Triangle::new(a1, b2, a2));
        t.push(Triangle::new(a1, b1, b2));
        t.push(Triangle::new(p3(0.0, y0, z0), a2, a1));
        t.push(Triangle::new(p3(len, y0, z0), b1, b2));
    }
    t
}

#[test]
fn the_rotary_parallel_ghost_rides_the_pose_law_to_the_world_bar() {
    // #1015: the rotary raster is planned on frame-LOCAL facets; its
    // ghost is carried out by the same pose law and world exit as the
    // finished path, so every cut point lands on the world bar's skin
    // and on the very tips the successful path posts.
    let bar = laid_cylinder(10.0, 40.0, 50.0, 20.0);
    let c = AxisControl { lead_deg: 0.0, lag_deg: 0.0, max_tilt_deg: None };
    let e = refused(rotary_parallel(&bar, &finish(30.0, 10.0, 2.0, Some(1e-5)), &c));
    assert!(e.to_string().contains("levels (cap"), "{e}");
    let g = e.ghost().expect("the laid raster was built before the rings refused");
    let ok = rotary_parallel(&bar, &finish(30.0, 10.0, 2.0, None), &c).expect("the same bar, rings allowed");
    // A tip on a facet of the 32-gon stands between its flat's apothem
    // and the true radius.
    let rho = |q: Point3| (q.y - 50.0).hypot(q.z - 20.0);
    let apothem = 10.0 * (std::f64::consts::PI / 32.0).cos();
    let skin = |q: Point3| (apothem - 1e-6..=10.0 + 1e-6).contains(&rho(q)) && (0.0..=40.0).contains(&q.x);
    let cuts: Vec<Point3> = g.moves().iter().filter(|m| m.kind == MoveKind::Feed && skin(m.to)).map(|m| m.to).collect();
    assert!(cuts.len() > 10, "the raster's cut points touch the world bar: {} of {}", cuts.len(), g.len());
    for q in &cuts {
        let d = ok.moves.iter().map(|m| (m.to - *q).dot(m.to - *q)).fold(f64::INFINITY, f64::min).sqrt();
        assert!(d < 1e-6, "ghost tip {q:?} is not a tip of the posted path ({d} off)");
    }
    // Nothing of the ghost stands inside the bar.
    assert!(g.moves().iter().all(|m| rho(m.to) > apothem - 1e-6), "a ghost point inside the work");
}

fn curve_params() -> CurveParams {
    CurveParams { tool_diameter: 6.0, clearance_z: 25.0, retract_z: None, feed: 500.0, plunge_feed: 100.0, climb: true }
}

#[test]
fn a_refused_drive_curve_hands_back_the_chains_walked_before_it() {
    // #1015: the curve walk posts world tips chain by chain; a chain
    // that stands off the part refuses with the earlier chains built -
    // exactly the moves the lone first chain's path opens with.
    let block = cube(Point3::ZERO, p3(40.0, 40.0, 15.0)).facets.to_vec();
    let first = vec![p3(5.0, 20.0, 15.0), p3(35.0, 20.0, 15.0)];
    let floating = vec![p3(5.0, 20.0, 40.0), p3(35.0, 20.0, 40.0)];
    let c = AxisControl::default();
    let e = contour_curve(&[first.clone(), floating], &block, &curve_params(), &c, None).expect_err("chain 2 floats");
    assert!(e.to_string().contains("off the model"), "{e}");
    let alone = contour_curve(&[first], &block, &curve_params(), &c, None).expect("chain 1 alone rides the top");
    let g = e.ghost().expect("chain 1 was walked");
    // The lone path adds only its closing retract after the chain.
    assert_eq!(g.moves(), &alone.moves[..alone.moves.len() - 1]);
    // A refusal on the first chain has nothing walked: no ghost.
    let lone = contour_curve(&[vec![p3(5.0, 20.0, 40.0), p3(35.0, 20.0, 40.0)]], &block, &curve_params(), &c, None);
    assert!(lone.is_err_and(|e| e.ghost().is_none()));
}
