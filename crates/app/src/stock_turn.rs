//! The stock's turning radius (#1666): the radius a setup's stock sweeps
//! when it turns about the spindle axis, the world Z line through the
//! work offset's origin. Provenance: our own; the largest distance of a
//! polyhedron from a line is at one of its vertices (distance to a line
//! is a convex function), from general geometry.
//!
//! The axis-aligned stock box says nothing about the shape inside it: a
//! round bar's radius is its box's half-width, a square bar's is half
//! its diagonal. The lathe posts read this radius (`StockTurnRadius`) for
//! the moves before and after G30, so a box stock's corners are known.

use flecs_ecs::prelude::World;

use crate::geometry::transform_point;
use crate::model;

/// The largest distance of the stock mesh of `setup` from the world Z
/// line through `axis` (X, Y), in world mm. None when the setup has no
/// stock, the stock has no finite vertex, or `axis` is not finite.
#[must_use]
pub fn turn_radius_for(world: &World, setup: Option<&str>, axis: [f64; 2]) -> Option<f64> {
    if !(axis[0].is_finite() && axis[1].is_finite()) {
        return None;
    }
    let name = crate::setup::stock_entity_name(world, setup);
    let entity = model::find_by_name(world, &name)?;
    if !model::is_stock(world, &entity) {
        return None;
    }
    let tris = model::facets_of(world, &entity)?;
    let loc = model::transform_of(world, &entity).unwrap_or_default();
    let mut r: Option<f64> = None;
    for t in tris.iter() {
        for p in [t.a, t.b, t.c] {
            let p = transform_point(p, loc.pos, loc.rot);
            let d = (p.x - axis[0]).hypot(p.y - axis[1]);
            if d.is_finite() {
                r = Some(r.map_or(d, |m| m.max(d)));
            }
        }
    }
    r
}
