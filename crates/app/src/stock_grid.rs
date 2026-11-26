//! Provenance: document stock placement and analytic grid construction from our own design (#2177, #2758).

use crate::geometry::Triangle;
use crate::stock_sim::{Dexel, auto_cell};

/// The document's stock, as an axis-aligned grid in world. Box and
/// cylinder use analytic grids translated to the `Stock` entity's place.
/// A laid cylinder uses its round vertical sections (#2758); a stock
/// off the machine axes is refused. Model
/// stock builds from its own world facets, the same mesh the viewport
/// draws. `simulate_document` and re-machining (M3.1 D) read the stock
/// through this law - one builder, two consumers, so a rest mask and
/// the deviation verdict always describe the same solid.
pub fn stock_grid(
    world: &flecs_ecs::prelude::World,
    setup: Option<&str>,
    tool_r: f64,
) -> Result<Dexel, String> {
    use crate::geometry::transform_point;
    use crate::stock::StockShape;
    use crate::model;

    let stock_name = crate::setup::stock_entity_name(world, setup);
    let stock_e = model::find_by_name(world, &stock_name)
        .ok_or_else(|| format!("no stock '{stock_name}' - the program sim judges material, not air"))?;
    let shape = model::stock_def_of(world, &stock_e)
        .ok_or("the stock entity lost its shape")?;
    // #2177: a square-turned stock is its unturned twin; oblique refused.
    let (shape, loc) = crate::stock_follow::sim_frame(&shape, model::transform_of(world, &stock_e).unwrap_or_default())?;
    let axis = transform_point(crate::geometry::Point3::new(0.0, 0.0, 1.0), crate::geometry::Point3::ZERO, loc.rot);
    let laid = matches!(shape, StockShape::Cylinder { .. }) && axis.z.abs() < 0.5;
    let along_x = axis.x.abs() > 0.5;
    let stock_facets: Vec<Triangle> = match &shape {
        StockShape::FromModel { .. } => model::facets_of(world, &stock_e)
            .unwrap_or_default()
            .iter()
            .map(|t| Triangle::new(
                transform_point(t.a, loc.pos, loc.rot),
                transform_point(t.b, loc.pos, loc.rot),
                transform_point(t.c, loc.pos, loc.rot),
            ))
            .collect(),
        _ => Vec::new(),
    };
    let (span_x, span_y) = match &shape {
        StockShape::Box { dx, dy, .. } => (*dx, *dy),
        StockShape::Cylinder { diameter, height } if laid => {
            if along_x { (*height, *diameter) } else { (*diameter, *height) }
        }
        StockShape::Cylinder { diameter, .. } => (*diameter, *diameter),
        StockShape::FromModel { .. } => {
            let (mut x0, mut x1, mut y0, mut y1) =
                (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
            for t in &stock_facets {
                for v in [t.a, t.b, t.c] {
                    x0 = x0.min(v.x);
                    x1 = x1.max(v.x);
                    y0 = y0.min(v.y);
                    y1 = y1.max(v.y);
                }
            }
            ((x1 - x0).max(0.0), (y1 - y0).max(0.0))
        }
    };
    // A program none of whose ops sweeps a tool through material (a
    // probe-only program, #592: the probe stays out of the plan) has no
    // radius to size the lattice by - `min` over nothing is +inf, and
    // an infinite cell was refused as "invalid stock simulation setup",
    // naming the wrong cause. The stock then simply stands uncut: size
    // its lattice by the span alone (the grid ceiling's floor), and the
    // verdict reads the raw stock against the part - clean when they
    // agree, rest where they don't, never a gouge it did not cut.
    // Exactly +inf (the empty fold), nothing wider: a NaN or negative
    // radius still reaches `auto_cell` and is refused by name (review
    // of #599) - only "no tool cut" falls back to the span.
    let sizing_r = if tool_r == f64::INFINITY { 0.0 } else { tool_r };
    let cell = auto_cell(sizing_r, span_x, span_y);
    let dex = match &shape {
        StockShape::Cylinder { diameter, height } if laid => {
            let mut d = Dexel::laid_cylinder(*diameter, *height, along_x, cell).map_err(|e| e.to_string())?;
            // The builder starts at the lower end of the horizontal axis.
            let mut pos = loc.pos;
            if along_x && axis.x < 0.0 { pos.x -= height; }
            if !along_x && axis.y < 0.0 { pos.y -= height; }
            d.translate_origin(pos.x, pos.y, pos.z);
            d
        }
        StockShape::FromModel { .. } => {
            Dexel::from_facets(&stock_facets, cell).map_err(|e| e.to_string())?
        }
        s => {
            let mut d = Dexel::from_shape(s, cell).map_err(|e| e.to_string())?;
            // The analytic grids are built origin-centered; the stock
            // entity's place moves the lattice itself, so the moves
            // stay in the world frame the viewport and post read.
            d.translate_origin(loc.pos.x, loc.pos.y, loc.pos.z);
            d
        }
    };
    Ok(dex)
}
