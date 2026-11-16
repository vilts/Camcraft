//! The stock that follows a part alignment (#326, #561): split out of
//! `setup_align` (#738) along its seam - which stock follows the part, and
//! where it stands after the change (moved, re-fitted square to the setup
//! frame, or kept square round the part with its allowance).
//!
//! Provenance: our own design; the transform math is standard rigid-body
//! algebra. No vendor implementation inspected.

use flecs_ecs::prelude::World;

use crate::commands::CommandError;
use crate::commands::fixture::check_location;
use crate::geometry::{Point3, Triangle, transform_point};
use crate::model;
use crate::schema::Location;
use crate::setup_align;
use crate::stock::{self, StockShape};

pub(super) type StockState = (StockShape, Location, std::sync::Arc<[Triangle]>);

/// The setup's stock as it must stand after the alignment changes from
/// `prev` to `next`, when it follows the part; `None` when it does not
/// follow (or there is no stock yet).
pub(super) fn following_stock(
    world: &World,
    setup: &str,
    prev: Option<Location>,
    next: Option<Location>,
    square: bool,
) -> Result<Option<(String, StockState, StockState)>, CommandError> {
    let name = crate::setup::stock_entity_name(world, Some(setup));
    let Some(entity) = model::find_by_name(world, &name) else { return Ok(None) };
    let Some(shape) = model::stock_def_of(world, &entity) else { return Ok(None) };
    let parts = model::model_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .map(|(n, _)| n)
        .collect::<Vec<_>>();
    let follows = match &shape {
        StockShape::FromModel { source } => parts.contains(source),
        _ => parts.len() == 1,
    };
    if !follows {
        return Ok(None);
    }
    if crate::setup::is_furniture_of_other(world, &name, setup) {
        return Err(CommandError::Invalid(format!(
            "the stock '{name}' is shared with another setup - turning the part here would move it there too; \
             give this setup its own stock first"
        )));
    }
    let loc = model::transform_of(world, &entity).unwrap_or_default();
    let facets = model::facets_of(world, &entity).unwrap_or_default();
    // The change as one transform: undo the old alignment, apply the new.
    let undo = prev.map_or_else(Location::default, setup_align::inverse);
    let delta = setup_align::compose(next.unwrap_or_default(), undo);
    if square && let Some(after) = square_refit(world, &name, &shape, (loc, &facets), &parts, (prev, next))? {
        return Ok(Some((name, (shape, loc, facets), after)));
    }
    let after = match &shape {
        StockShape::FromModel { .. } => {
            let moved = check_location(setup_align::compose(delta, loc), &format!("stock '{name}'"))?;
            (shape.clone(), moved, facets.clone())
        }
        StockShape::Box { dx, dy, dz } => {
            let corners = [-0.5, 0.5]
                .into_iter()
                .flat_map(|x| [-0.5, 0.5].map(|y| (x, y)))
                .flat_map(|(x, y)| [0.0, 1.0].map(|z| Point3::new(x * dx, y * dy, z * dz)));
            let moved: Vec<Point3> = corners.map(|p| place(delta, loc, p)).collect();
            refit_box(&name, &moved)?
        }
        StockShape::Cylinder { diameter, height } => {
            let bottom = place(delta, loc, Point3::ZERO);
            let top = place(delta, loc, Point3::new(0.0, 0.0, *height));
            let axis = Point3::new(top.x - bottom.x, top.y - bottom.y, top.z - bottom.z);
            // Written so a NaN reads as "not upright" and refuses.
            let upright = axis.z.abs() >= height * (1.0 - 1e-9);
            if !upright {
                return Err(CommandError::Invalid(format!(
                    "the cylinder stock '{name}' would lie on its side - only turns that keep it upright \
                     can carry it; set the stock again after aligning"
                )));
            }
            let pos = Point3::new(bottom.x, bottom.y, bottom.z.min(top.z));
            let shape = StockShape::Cylinder { diameter: *diameter, height: *height };
            let mesh = mesh_of(&shape, &name)?;
            (shape, check_location(Location { pos, ..Location::default() }, &format!("stock '{name}'"))?, mesh)
        }
    };
    Ok(Some((name, (shape, loc, facets), after)))
}

/// #561: the square block re-fitted round the one part with its
/// allowance kept (`None`: not a square parametric block - the turned
/// block is re-fitted as before).
fn square_refit(
    world: &World,
    name: &str,
    shape: &StockShape,
    (loc, facets): (Location, &std::sync::Arc<[Triangle]>),
    parts: &[String],
    (prev, next): (Option<Location>, Option<Location>),
) -> Result<Option<StockState>, CommandError> {
    let [part] = parts else { return Ok(None) };
    let Some(pe) = model::find_by_name(world, part) else { return Ok(None) };
    let Some(tris) = model::facets_of(world, &pe) else { return Ok(None) };
    let doc = model::transform_of(world, &pe).unwrap_or_default();
    let seen = |a: Option<Location>| {
        let at = a.map_or(doc, |a| setup_align::compose(a, doc));
        tris.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| transform_point(p, at.pos, at.rot)).collect::<Vec<_>>()
    };
    let fit = crate::stock_follow::keep_square(name, shape, loc, &seen(prev), &seen(next)).map_err(CommandError::Invalid)?;
    let Some((sized, at)) = fit else { return Ok(None) };
    let at = check_location(at, &format!("stock '{name}'"))?;
    let mesh = if &sized == shape { facets.clone() } else { mesh_of(&sized, name)? };
    Ok(Some((sized, at, mesh)))
}

/// A stock-local point through its placement, then through the change.
fn place(delta: Location, loc: Location, p: Point3) -> Point3 {
    let world_p = transform_point(p, loc.pos, loc.rot);
    transform_point(world_p, delta.pos, delta.rot)
}

/// The box square to the setup frame that holds the turned block.
fn refit_box(name: &str, pts: &[Point3]) -> Result<StockState, CommandError> {
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    for p in pts {
        for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
            lo[i] = lo[i].min(v);
            hi[i] = hi[i].max(v);
        }
    }
    // A micron of float noise on a quarter turn is not a new size.
    let snap = |v: f64| (v * 1e6).round() / 1e6;
    let shape = StockShape::Box {
        dx: snap(hi[0] - lo[0]),
        dy: snap(hi[1] - lo[1]),
        dz: snap(hi[2] - lo[2]),
    };
    let mesh = mesh_of(&shape, name)?;
    let pos = Point3::new(snap(0.5 * (lo[0] + hi[0])), snap(0.5 * (lo[1] + hi[1])), snap(lo[2]));
    let at = check_location(Location { pos, ..Location::default() }, &format!("stock '{name}'"))?;
    Ok((shape, at, mesh))
}

fn mesh_of(shape: &StockShape, name: &str) -> Result<std::sync::Arc<[Triangle]>, CommandError> {
    if !shape.is_sane() || !shape.is_bounded() {
        return Err(CommandError::Invalid(format!("the stock '{name}' cannot be rebuilt in the turned frame")));
    }
    stock::stock_mesh(shape)
        .map(Into::into)
        .ok_or_else(|| CommandError::Invalid(format!("the stock '{name}' cannot be rebuilt in the turned frame")))
}
