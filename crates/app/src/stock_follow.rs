//! A machine-square stock that keeps its allowance round a turned part
//! (#561 locate, #571 every `MoveModel` turn): when a move turns the part
//! in the setup, a box or cylinder stock that stands square to the setup
//! stays square and is re-fitted round the part's new place with the
//! same allowance it had on each side - top stays top, the -X side stays
//! the -X side. Pure: the commands snapshot what this returns.
//!
//! Provenance: our own design over plain shop practice - the blank is
//! clamped square in the vise and the part is "inside" it by its
//! allowances; parametric stock is the box / cylinder of
//! `camcraft_geometry::stock`. No vendor material consulted.
//!
//! ## The laws here
//!
//! - **Only a square, parametric stock.** A box or cylinder whose axes
//!   lie along the setup's (any quarter turn) - anything else (a stock
//!   copied from the model, a stock already turned off the axes) is
//!   `Ok(None)` and the caller keeps its rigid follow.
//! - **Allowances are read in the stock's own frame**, before and after,
//!   so a stock that never turns never changes size: a pure translation
//!   moves it by exactly the part's shift and keeps its dimensions.
//! - **A refit that cannot keep its allowance is refused by name** (a
//!   stock that did not wrap the part and would end with no size).

use crate::geometry::{Point3, transform_inverse_point, transform_point};
use crate::schema::Location;
use crate::stock::StockShape;

/// A quaternion within this of a signed axis permutation is "square".
const SQUARE_TOL: f64 = 1e-9;
/// A dimension within this (mm) of the old one is the old one.
const SAME_MM: f64 = 1e-6;

/// Does `rot` carry every axis onto a (signed) axis?
#[must_use]
pub fn is_square(rot: [f64; 4]) -> bool {
    [Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0), Point3::new(0.0, 0.0, 1.0)]
        .into_iter()
        .all(|e| {
            let v = transform_point(e, Point3::ZERO, rot);
            let c = [v.x.abs(), v.y.abs(), v.z.abs()];
            let ones = c.iter().filter(|a| (*a - 1.0).abs() <= SQUARE_TOL).count();
            let zeros = c.iter().filter(|a| **a <= SQUARE_TOL).count();
            ones == 1 && zeros == 2
        })
}

/// (min, max) of points in the stock-local frame of `loc`.
fn local_bounds(loc: Location, pts: &[Point3]) -> Option<([f64; 3], [f64; 3])> {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in pts {
        let q = transform_inverse_point(*p, loc.pos, loc.rot);
        for (i, v) in [q.x, q.y, q.z].into_iter().enumerate() {
            if !v.is_finite() {
                return None;
            }
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    (lo[0] <= hi[0]).then_some((lo, hi))
}

fn snap(v: f64, old: f64) -> f64 {
    if (v - old).abs() <= SAME_MM { old } else { (v * 1e6).round() / 1e6 }
}

/// The stock's new shape and place when the part's points (setup frame)
/// go from `before` to `after`; `Ok(None)` when the stock is not a
/// square parametric block (the caller carries it rigidly).
pub fn keep_square(
    name: &str,
    shape: &StockShape,
    loc: Location,
    before: &[Point3],
    after: &[Point3],
) -> Result<Option<(StockShape, Location)>, String> {
    if matches!(shape, StockShape::FromModel { .. }) || !is_square(loc.rot) {
        return Ok(None);
    }
    let refused = || format!("the stock '{name}' cannot keep its allowance round the turned part - set the stock again");
    let (Some((blo, bhi)), Some((alo, ahi))) = (local_bounds(loc, before), local_bounds(loc, after)) else {
        return Err(refused());
    };
    let positive = |v: f64| v.is_finite() && v > 0.0;
    let (next, origin) = match shape {
        StockShape::Box { dx, dy, dz } => {
            let (slo, shi) = ([-dx * 0.5, -dy * 0.5, 0.0], [dx * 0.5, dy * 0.5, *dz]);
            let lo: [f64; 3] = std::array::from_fn(|i| alo[i] - (blo[i] - slo[i]));
            let hi: [f64; 3] = std::array::from_fn(|i| ahi[i] + (shi[i] - bhi[i]));
            let d = [snap(hi[0] - lo[0], *dx), snap(hi[1] - lo[1], *dy), snap(hi[2] - lo[2], *dz)];
            if !d.iter().all(|v| positive(*v)) {
                return Err(refused());
            }
            let origin = Point3::new(0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]), lo[2]);
            (StockShape::Box { dx: d[0], dy: d[1], dz: d[2] }, origin)
        }
        StockShape::Cylinder { diameter, height } => {
            // The axis keeps its offset from the part's footprint centre;
            // the radial allowance is over the part's farthest point.
            let shift = [0.5 * (alo[0] + ahi[0] - blo[0] - bhi[0]), 0.5 * (alo[1] + ahi[1] - blo[1] - bhi[1])];
            let reach = |pts: &[Point3], c: [f64; 2]| {
                pts.iter()
                    .map(|p| transform_inverse_point(*p, loc.pos, loc.rot))
                    .map(|q| (q.x - c[0]).hypot(q.y - c[1]))
                    .fold(0.0_f64, f64::max)
            };
            let allowance = 0.5 * diameter - reach(before, [0.0, 0.0]);
            let d = snap(2.0 * (reach(after, shift) + allowance), *diameter);
            let lo = alo[2] - blo[2];
            let h = snap((ahi[2] + (height - bhi[2])) - lo, *height);
            if !(positive(d) && positive(h)) {
                return Err(refused());
            }
            (StockShape::Cylinder { diameter: d, height: h }, Point3::new(shift[0], shift[1], lo))
        }
        StockShape::FromModel { .. } => return Ok(None),
    };
    Ok(Some((next, Location { pos: transform_point(origin, loc.pos, loc.rot), rot: loc.rot })))
}

/// The refusal for a stock turned off the machine axes (#2177).
pub const OBLIQUE_STOCK: &str =
    "the stock stands turned off the machine axes - the simulation cuts only a stock square to them; set the stock again";
/// #2177: the stock as the simulation models it - one law for the
/// program sim, the carried body and the viewport's traced stock. An
/// unturned stock is returned as it is (bytes for bytes). A box turned
/// by a quarter-turn multiple (within [`SQUARE_TOL`]: a locate's float
/// noise included) is the same solid as an unturned box with its sides
/// swapped; an upright cylinder (axis up or down) is an unturned one
/// with its floor at its lower end. A cylinder along X or Y keeps its
/// placement for the analytic laid-cylinder builder (#2758).
/// A stock made from the model is read
/// from its placed mesh, so any turn stands. Anything else is refused
/// by name.
///
/// # Errors
/// [`OBLIQUE_STOCK`].
pub fn sim_frame(shape: &StockShape, loc: Location) -> Result<(StockShape, Location), String> {
    if loc.rot == crate::geometry::IDENTITY_Q || matches!(shape, StockShape::FromModel { .. }) {
        return Ok((shape.clone(), loc));
    }
    if !is_square(loc.rot) {
        return Err(OBLIQUE_STOCK.into());
    }
    let axis = |e: Point3| transform_point(e, Point3::ZERO, loc.rot);
    // The world axis (0, 1, 2) a local axis lands on, and its sign.
    let lands = |v: Point3| {
        let c = [v.x, v.y, v.z];
        let i = (0..3).fold(0, |b, k| if c[k].abs() > c[b].abs() { k } else { b });
        (i, c[i] > 0.0)
    };
    let up = lands(axis(Point3::new(0.0, 0.0, 1.0)));
    match shape {
        StockShape::Box { dx, dy, dz } => {
            let mut d = [0.0; 3];
            for (e, len) in [(Point3::new(1.0, 0.0, 0.0), *dx), (Point3::new(0.0, 1.0, 0.0), *dy), (Point3::new(0.0, 0.0, 1.0), *dz)] {
                d[lands(axis(e)).0] = len;
            }
            let c = transform_point(Point3::new(0.0, 0.0, 0.5 * dz), loc.pos, loc.rot);
            let pos = Point3::new(c.x, c.y, c.z - 0.5 * d[2]);
            Ok((StockShape::Box { dx: d[0], dy: d[1], dz: d[2] }, Location { pos, rot: crate::geometry::IDENTITY_Q }))
        }
        StockShape::Cylinder { height, .. } if up.0 == 2 => {
            let z = if up.1 { loc.pos.z } else { loc.pos.z - height };
            let pos = Point3::new(loc.pos.x, loc.pos.y, z);
            Ok((shape.clone(), Location { pos, rot: crate::geometry::IDENTITY_Q }))
        }
        StockShape::Cylinder { .. } => Ok((shape.clone(), loc)),
        StockShape::FromModel { .. } => Ok((shape.clone(), loc)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::quat_from_euler;

    #[test]
    fn the_sim_frame_keeps_square_stock_and_refuses_oblique_stock() {
        let b = StockShape::Box { dx: 22.0, dy: 12.0, dz: 10.0 };
        let at = Location { pos: Point3::new(5.0, 6.0, 7.0), ..Location::default() };
        assert_eq!(sim_frame(&b, at), Ok((b.clone(), at)), "unturned: unchanged");
        // Locate noise (#2177): the same box, the turn dropped.
        let noisy = Location { rot: [0.0, 0.0, -5e-14, 1.0], ..at };
        let (s, l) = sim_frame(&b, noisy).expect("noise is square");
        assert_eq!(s, b);
        assert!(l.rot == crate::geometry::IDENTITY_Q && (l.pos.x - 5.0).abs() < 1e-9 && (l.pos.z - 7.0).abs() < 1e-9);
        // A quarter turn about Z swaps the sides round the same centre.
        let quarter = Location { rot: quat_from_euler(0.0, 0.0, std::f64::consts::FRAC_PI_2), ..at };
        let (s, l) = sim_frame(&b, quarter).expect("quarter turn");
        assert_eq!(s, StockShape::Box { dx: 12.0, dy: 22.0, dz: 10.0 });
        assert!((l.pos.x - 5.0).abs() < 1e-9 && (l.pos.y - 6.0).abs() < 1e-9 && (l.pos.z - 7.0).abs() < 1e-9, "{l:?}");
        // Stood on its side about X: dz becomes the depth, the floor drops.
        let side = Location { rot: quat_from_euler(std::f64::consts::FRAC_PI_2, 0.0, 0.0), ..at };
        let (s, l) = sim_frame(&b, side).expect("on its side");
        assert_eq!(s, StockShape::Box { dx: 22.0, dy: 10.0, dz: 12.0 });
        assert!((l.pos.z - 1.0).abs() < 1e-9 && l.rot == crate::geometry::IDENTITY_Q, "{l:?}");
        let oblique = Location { rot: quat_from_euler(0.0, 0.0, 0.7), ..at };
        assert_eq!(sim_frame(&b, oblique), Err(OBLIQUE_STOCK.into()));
        let c = StockShape::Cylinder { diameter: 10.0, height: 20.0 };
        let flipped = Location { rot: quat_from_euler(std::f64::consts::PI, 0.0, 0.0), ..at };
        let (_, l) = sim_frame(&c, flipped).expect("upside down is upright");
        assert!((l.pos.z + 13.0).abs() < 1e-9, "{l:?}");
        assert_eq!(sim_frame(&c, side), Ok((c.clone(), side)));
        let m = StockShape::FromModel { source: "part".into() };
        assert_eq!(sim_frame(&m, oblique), Ok((m.clone(), oblique)), "the placed mesh stands");
    }

    fn corners(lo: [f64; 3], hi: [f64; 3]) -> Vec<Point3> {
        (0..8)
            .map(|k| Point3::new(if k & 1 == 0 { lo[0] } else { hi[0] }, if k & 2 == 0 { lo[1] } else { hi[1] }, if k & 4 == 0 { lo[2] } else { hi[2] }))
            .collect()
    }

    #[test]
    fn a_translation_moves_the_box_by_the_part_shift_and_keeps_its_size() {
        let shape = StockShape::Box { dx: 22.0, dy: 12.0, dz: 9.0 };
        let loc = Location { pos: Point3::new(10.0, 5.0, -1.0), ..Location::default() };
        let before = corners([0.0, 0.0, 0.0], [20.0, 10.0, 8.0]);
        let after = corners([5.0, 5.0, 5.0], [25.0, 15.0, 13.0]);
        let (s, at) = keep_square("stock", &shape, loc, &before, &after).expect("ok").expect("square");
        assert_eq!(s, shape);
        assert!((at.pos.x - 15.0).abs() < 1e-12 && (at.pos.y - 10.0).abs() < 1e-12 && (at.pos.z - 4.0).abs() < 1e-12);
    }

    #[test]
    fn a_turned_part_refits_with_each_side_allowance_kept() {
        // Part 20 x 10 x 8 stood on end (10 x 8 x 20 in the setup), stock
        // with 1 mm round it, 0 under, 2 on top.
        let before = corners([0.0, 0.0, 0.0], [10.0, 8.0, 20.0]);
        let shape = StockShape::Box { dx: 12.0, dy: 10.0, dz: 22.0 };
        let loc = Location { pos: Point3::new(5.0, 4.0, 0.0), ..Location::default() };
        let after = corners([0.0, 0.0, 0.0], [20.0, 10.0, 8.0]);
        let (s, at) = keep_square("stock", &shape, loc, &before, &after).expect("ok").expect("square");
        assert_eq!(s, StockShape::Box { dx: 22.0, dy: 12.0, dz: 10.0 });
        assert!((at.pos.x - 10.0).abs() < 1e-9 && (at.pos.y - 5.0).abs() < 1e-9 && at.pos.z.abs() < 1e-9, "{:?}", at.pos);
    }

    #[test]
    fn a_stock_off_the_axes_or_from_the_model_is_not_ours() {
        let pts = corners([0.0; 3], [1.0; 3]);
        let tilted = Location { rot: quat_from_euler(0.3, 0.0, 0.0), ..Location::default() };
        let b = StockShape::Box { dx: 1.0, dy: 1.0, dz: 1.0 };
        assert_eq!(keep_square("s", &b, tilted, &pts, &pts), Ok(None));
        let model = StockShape::FromModel { source: "part".into() };
        assert_eq!(keep_square("s", &model, Location::default(), &pts, &pts), Ok(None));
        let quarter = Location { rot: quat_from_euler(std::f64::consts::FRAC_PI_2, 0.0, 0.0), ..Location::default() };
        assert!(is_square(quarter.rot) && !is_square(tilted.rot));
    }

    #[test]
    fn a_cylinder_keeps_its_radial_and_end_allowance() {
        let before = corners([-5.0, -5.0, 0.0], [5.0, 5.0, 10.0]);
        let shape = StockShape::Cylinder { diameter: 20.0, height: 12.0 };
        let after = corners([95.0, -5.0, 3.0], [105.0, 5.0, 13.0]);
        let (s, at) = keep_square("s", &shape, Location::default(), &before, &after).expect("ok").expect("square");
        assert_eq!(s, shape);
        assert!((at.pos.x - 100.0).abs() < 1e-9 && at.pos.y.abs() < 1e-9 && (at.pos.z - 3.0).abs() < 1e-9);
    }
}
