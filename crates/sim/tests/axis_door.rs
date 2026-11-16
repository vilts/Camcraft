//! #1312: the verifier's door for a move's tool axis. A non-finite or
//! zero-length axis is refused by name (the NonFinite law covers the
//! axis, not only `from`/`to`), and a finite axis that is not unit
//! length is normalised once, so the reach laws convict a scaled axis
//! exactly as its unit direction. Both of the issue's reproductions
//! convict here.
//!
//! Provenance: our own fixtures, specced from the Prime Directive (a
//! pose the laws cannot read is refused, never passed); no vendor
//! material.
//!
//! Gate map (#1312): covers camcraft_sim::verify (axis_door, engine, reach).

use camcraft_sim::cl::{Move, MoveKind, Toolpath};
use camcraft_sim::geo2d::{Loop, Point2, Region};
use camcraft_sim::geometry::Point3;
use camcraft_sim::verify::{Mode, ToolGeom, VerifyContext, Warning, verify};

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Region {
    let l = Loop::new(vec![
        Point2::new(x0, y0),
        Point2::new(x1, y0),
        Point2::new(x1, y1),
        Point2::new(x0, y1),
    ]);
    match l.and_then(|l| Region::new(vec![l])) {
        Ok(r) => r,
        Err(e) => panic!("rect: {e:?}"),
    }
}

/// A unit axis tilted `rad` off +Z toward +X.
fn tilt(rad: f64) -> [f64; 3] {
    [rad.sin(), 0.0, rad.cos()]
}

fn mv(kind: MoveKind, from: Point3, to: Point3, axis: Option<[f64; 3]>) -> Move {
    Move { rotary: None, kind, from, to, feed: Some(500.0), arc: None, axis }
}

/// A plunge to Z -25 at the pocket centre, then a feed along X to 2,
/// every move held at `axis`.
fn program(axis: Option<[f64; 3]>) -> Toolpath {
    Toolpath::new(vec![
        mv(MoveKind::Plunge, Point3::new(0.0, 0.0, 5.0), Point3::new(0.0, 0.0, -25.0), axis),
        mv(MoveKind::Feed, Point3::new(0.0, 0.0, -25.0), Point3::new(2.0, 0.0, -25.0), axis),
    ])
}

/// A 5 mm shank over a 3 mm cutter: in the tight pocket below it reaches
/// the stock wall upright (#938's fixture).
fn wide_shank() -> ToolGeom {
    ToolGeom {
        radius: 3.0,
        nose_radius: 0.0,
        flute_len: 10.0,
        shank_radius: 5.0,
        shank_len: 20.0,
        holder_radius: 15.0,
        holder_len: 30.0,
    }
}

fn judge(p: &Toolpath) -> Vec<Warning> {
    let tool = wide_shank();
    let region = rect(-4.0, -4.0, 6.0, 4.0);
    let stock = rect(-40.0, -40.0, 40.0, 40.0);
    let cx = VerifyContext {
        region: &region,
        stock: Some(&stock),
        model: None,
        tool: &tool,
        cutter: None,
        mode: Mode::Area,
        stock_top_z: 0.0,
        clearance_z: 30.0,
        stock_to_leave: 0.0,
        gouge_tol: 0.01,
        z_tol: 1e-3,
        finish_tol: None,
        fixtures: &[],
        spindle: None,
        ..Default::default()
    };
    verify(p, &cx)
}

fn refused(w: &[Warning]) -> Vec<usize> {
    w.iter()
        .filter_map(|w| match w {
            Warning::NonFinite { at } => Some(*at),
            _ => None,
        })
        .collect()
}

fn shank_hits(w: &[Warning]) -> Vec<usize> {
    w.iter()
        .filter_map(|w| match w {
            Warning::ShankHit { at, .. } => Some(*at),
            _ => None,
        })
        .collect()
}

#[test]
fn the_upright_fixture_convicts_the_shank() {
    assert_eq!(shank_hits(&judge(&program(None))), vec![0, 1]);
    assert!(refused(&judge(&program(None))).is_empty());
}

/// Reproduction 1: `[1e-12, 0, inf]` was read as tilted and the reach law
/// convicted nothing - the move passed clean. Now the axis is refused by
/// name on every move that states it.
#[test]
fn a_non_finite_axis_is_refused_by_name() {
    for bad in [[1e-12, 0.0, f64::INFINITY], [f64::NAN, 0.0, 1.0], [0.0, f64::NEG_INFINITY, 0.0]] {
        let w = judge(&program(Some(bad)));
        assert_eq!(refused(&w), vec![0, 1], "{bad:?}: {w:?}");
    }
}

#[test]
fn a_zero_or_overflowing_axis_is_refused_by_name() {
    for bad in [[0.0, 0.0, 0.0], [f64::MAX, 0.0, f64::MAX]] {
        let w = judge(&program(Some(bad)));
        assert_eq!(refused(&w), vec![0, 1], "{bad:?}: {w:?}");
    }
}

/// Reproduction 2: a 7x-scaled axis 2e-9 rad off the column lost its
/// ShankHit. Normalised at the door it convicts exactly as its unit
/// direction, and is not refused.
#[test]
fn a_scaled_axis_is_judged_as_its_unit_direction() {
    let u = tilt(2e-9);
    let unit = judge(&program(Some(u)));
    assert_eq!(shank_hits(&unit), vec![0, 1], "{unit:?}");
    let scaled = judge(&program(Some([u[0] * 7.0, u[1] * 7.0, u[2] * 7.0])));
    assert_eq!(scaled, unit);
    let upright_scaled = judge(&program(Some([0.0, 0.0, 7.0])));
    assert_eq!(upright_scaled, judge(&program(None)));
}

#[test]
fn one_bad_axis_refuses_only_its_own_move() {
    let mut p = program(Some([0.0, 0.0, 1.0]));
    if let Some(m) = p.moves.get_mut(1) {
        m.axis = Some([0.0, f64::NAN, 1.0]);
    }
    assert_eq!(refused(&judge(&p)), vec![1]);
}
