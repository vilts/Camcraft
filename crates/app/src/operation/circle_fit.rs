//! The Circular operation's circle read off a geometry selection: a
//! picked bore rim, round pocket floor or a face holding one round hole
//! hands the ring ladder its centre and radius instead of the typed ones.
//!
//! Provenance: our own law - area centroid by the shoelace moments
//! (standard polygon geometry, e.g. Bourke, "Calculating the area and
//! centroid of a polygon", 1988) and the mean vertex radius about it,
//! accepted only within a stated deviation; no vendor material consulted.

use super::*;
use crate::units::{self, Unit};

/// The deviation every contour vertex may stand from the fitted circle:
/// a tessellated rim puts its vertices ON the surface, so anything past a
/// hundredth is a shape that is not round, not a coarse round one.
pub const CIRCLE_FIT_TOL: f64 = 0.01;

/// A loop's fitted circle: its area centroid, the mean distance of its
/// vertices from it, and the worst vertex's deviation with its position.
fn fit_loop(l: &Loop) -> Option<(Point2, f64, f64, Point2)> {
    let pts = l.points();
    if pts.len() < 3 {
        return None;
    }
    let (mut a2, mut cx, mut cy) = (0.0f64, 0.0f64, 0.0f64);
    for (i, p) in pts.iter().enumerate() {
        let q = pts[(i + 1) % pts.len()];
        let w = p.x * q.y - q.x * p.y;
        a2 += w;
        cx += (p.x + q.x) * w;
        cy += (p.y + q.y) * w;
    }
    if !a2.is_finite() || a2.abs() < 1e-12 {
        return None;
    }
    let c = Point2::new(cx / (3.0 * a2), cy / (3.0 * a2));
    let dist = |p: &Point2| ((p.x - c.x).powi(2) + (p.y - c.y).powi(2)).sqrt();
    let r = pts.iter().map(dist).sum::<f64>() / pts.len() as f64;
    let (mut dev, mut at) = (0.0f64, pts[0]);
    for p in pts {
        let d = (dist(p) - r).abs();
        if d > dev {
            dev = d;
            at = *p;
        }
    }
    (c.x.is_finite() && c.y.is_finite() && r.is_finite() && r > 0.0).then_some((c, r, dev, at))
}

/// The circle a picked region stands for: its single outer contour when
/// that is round, else its single round hole (a face picked around a
/// bore). Refused by name with the outer contour's worst deviation.
pub fn circle_of_region(region: &Region, unit: Unit) -> Result<(Point2, f64), GenError> {
    let outers: Vec<&Loop> = region.loops().iter().filter(|l| l.is_ccw()).collect();
    let holes: Vec<&Loop> = region.loops().iter().filter(|l| !l.is_ccw()).collect();
    let fits = |l: &Loop| fit_loop(l).filter(|f| f.2 <= CIRCLE_FIT_TOL);
    if let [o] = outers.as_slice()
        && let Some((c, r, ..)) = fits(o)
    {
        return Ok((c, r));
    }
    let round_holes: Vec<(Point2, f64, f64, Point2)> = holes.iter().filter_map(|h| fits(h)).collect();
    if let [(c, r, ..)] = round_holes.as_slice() {
        return Ok((*c, *r));
    }
    let worst = outers.first().and_then(|o| fit_loop(o));
    let detail = match worst {
        Some((_, _, dev, at)) => format!(
            "deviation {} {} at ({}, {})",
            units::len(dev, unit, 3),
            unit.suffix(),
            units::len(at.x, unit, 3),
            units::len(at.y, unit, 3)
        ),
        None => format!("{} separate areas, no single round contour", outers.len()),
    };
    Err(GenError::Invalid(format!(
        "the selection is not a circle ({detail}) - Circular needs a round hole or face; pick its rim, or clear the selection to use the typed centre and radius"
    )))
}

/// The selection's circle for a Circular operation as the world stands
/// now - what the op page echoes read-only. `None` when the op is not a
/// Circular one or carries no picks (the typed centre and radius rule).
#[must_use]
pub fn circular_fit_of(world: &World, op: &Operation) -> Option<Result<(Point2, f64), String>> {
    if !matches!(op.params, OpParams::Circular { .. }) {
        return None;
    }
    if !matches!(&op.src, SrcRef::Selection(s) if !s.is_empty()) {
        return None;
    }
    Some(match region_of(world, op) {
        Some(r) => circle_of_region(&r, crate::units::of(world)).map_err(|e| e.to_string()),
        None => Err("the selection resolves no closed region".into()),
    })
}
