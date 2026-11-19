//! Issue #870: where the setup's WCS origin sits, said in words ("top of stock, front-left corner") for the WCS page and the setup sheet - our own wording of the shop's usual program-zero points (a corner, an edge or the centre of the stock, on its top or bottom); no vendor material consulted.
//!
//! The words read the origin against the box the WCS page anchors on:
//! the stock box, or the model's bounds when there is no stock (the
//! page's fallback, `interact::anchor_reference`). X runs left to right,
//! Y front to back (the operator stands at -Y) and Z bottom to top. A
//! point on one of the box's 27 anchor points is named; any other point
//! is given as signed distances from the box's left side, front and top,
//! so the words never claim a corner the origin is not on.

use flecs_ecs::prelude::World;

use crate::geometry::{Aabb, Point3};
use crate::{model, setup, setup_align};

/// How near an anchor point the origin must be to be named by it, mm.
/// The page's anchor buttons write the exact point.
const NEAR: f64 = 1e-3;

/// Where the active WCS origin of setup `ctx` sits, in words, lengths
/// written by `len` (mm in, text out). An unplaced WCS is at the world
/// origin and is read there.
pub fn origin_words(world: &World, ctx: Option<&str>, len: &dyn Fn(f64) -> String) -> String {
    let pos = setup::wcs_entity(world, ctx)
        .and_then(|e| model::transform_of(world, &e))
        .map_or(Point3::new(0.0, 0.0, 0.0), |l| l.pos);
    let stock = model::stock_world_aabb_for(world, ctx).map(|b| (b, "stock"));
    match stock.or_else(|| setup_align::solid_aabb_in(world, ctx).map(|b| (b, "model"))) {
        Some((b, what)) => words_on(pos, &b, what, len),
        None => "no stock or model to place it against".to_owned(),
    }
}

/// `pos` against box `b` (named `what`: "stock" or "model").
pub fn words_on(pos: Point3, b: &Aabb, what: &str, len: &dyn Fn(f64) -> String) -> String {
    let finite = [pos.x, pos.y, pos.z, b.min.x, b.min.y, b.min.z, b.max.x, b.max.y, b.max.z];
    if !finite.iter().all(|v| v.is_finite()) {
        return format!("not known (the origin or the {what} box is not finite)");
    }
    let side = |v: f64, lo: f64, hi: f64| {
        if (v - lo).abs() <= NEAR {
            Some(0)
        } else if (v - hi).abs() <= NEAR {
            Some(2)
        } else if (v - (lo + hi) * 0.5).abs() <= NEAR {
            Some(1)
        } else {
            None
        }
    };
    let at = (side(pos.x, b.min.x, b.max.x), side(pos.y, b.min.y, b.max.y), side(pos.z, b.min.z, b.max.z));
    let (Some(x), Some(y), Some(z)) = at else {
        return format!(
            "off the {what}'s corners: X {} from its left side, Y {} from its front, Z {} from its top",
            signed(pos.x - b.min.x, len),
            signed(pos.y - b.min.y, len),
            signed(pos.z - b.max.z, len),
        );
    };
    let level = ["bottom", "mid-height", "top"][z];
    let xs = ["left", "centre", "right"][x];
    let ys = ["front", "middle", "back"][y];
    let plan = match (x, y) {
        (1, 1) => "centre".to_owned(),
        (1, _) => format!("{ys} edge, centred"),
        (_, 1) => format!("{xs} edge, centred"),
        _ => format!("{ys}-{xs} corner"),
    };
    format!("{level} of {what}, {plan}")
}

fn signed(v: f64, len: &dyn Fn(f64) -> String) -> String {
    let v = if v.abs() < NEAR { 0.0 } else { v };
    if v > 0.0 { format!("+{}", len(v)) } else { len(v) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mm(v: f64) -> String {
        format!("{v:.2} mm")
    }

    fn block() -> Aabb {
        Aabb { min: Point3::new(-50.0, -25.0, 0.0), max: Point3::new(50.0, 25.0, 20.0) }
    }

    #[test]
    fn every_anchor_point_is_named() {
        let b = block();
        let w = |x, y, z| words_on(Point3::new(x, y, z), &b, "stock", &mm);
        assert_eq!(w(-50.0, -25.0, 20.0), "top of stock, front-left corner");
        assert_eq!(w(50.0, 25.0, 0.0), "bottom of stock, back-right corner");
        assert_eq!(w(0.0, 0.0, 20.0), "top of stock, centre");
        assert_eq!(w(0.0, -25.0, 10.0), "mid-height of stock, front edge, centred");
        assert_eq!(w(50.0, 0.0, 20.0), "top of stock, right edge, centred");
        // Within the tolerance the anchor buttons' exact writes sit in.
        assert_eq!(w(-50.0004, -25.0, 20.0), "top of stock, front-left corner");
    }

    #[test]
    fn a_point_off_the_anchors_is_given_as_distances_never_as_a_corner() {
        let b = block();
        let w = words_on(Point3::new(-38.0, -25.0, 21.5), &b, "model", &mm);
        assert_eq!(w, "off the model's corners: X +12.00 mm from its left side, Y 0.00 mm from its front, Z +1.50 mm from its top");
        let below = words_on(Point3::new(-50.0, -25.0, 0.5), &b, "stock", &mm);
        assert!(below.contains("Z -19.50 mm from its top"), "{below}");
    }

    #[test]
    fn a_non_finite_origin_or_box_says_so() {
        let b = block();
        assert!(words_on(Point3::new(f64::NAN, 0.0, 0.0), &b, "stock", &mm).starts_with("not known"));
        let bad = Aabb { min: Point3::new(0.0, 0.0, f64::INFINITY), ..block() };
        assert!(words_on(Point3::new(0.0, 0.0, 0.0), &bad, "stock", &mm).starts_with("not known"));
    }
}
