//! The setup stock's shape for a post's blank definition (#2769): `PARAM
//! StockShape`, `StockShapeBottom` and, for a bar, `StockShapeDiameter`.
//!
//! Provenance: our own stream vocabulary (docs/CNC-INTERMEDIATE.md); the
//! blank kinds a control can draw are from Fanuc's public Manual Guide i
//! operator's manual (behaviour only). No vendor post or output consulted.
//!
//! The stream's `StockXY*` / `StockZTop` box is the stock's world bounds.
//! A post that defines a blank (a block, a round bar) must also know what
//! the box bounds: a box, a bar along one world axis, or something no
//! simple blank describes. The kind words are:
//!
//! - `box`: a box stock whose edges run along the world axes;
//! - `cylinder-x` / `cylinder-y` / `cylinder-z`: a cylinder stock whose
//!   axis runs along that world axis (`StockShapeDiameter` is its
//!   diameter: the faceted mesh's bounds are not the true round);
//! - `rotated`: a box or cylinder turned off the world axes;
//! - `from-model`: a stock copied from a model;
//! - `from-previous`: the setup starts from what the setup before leaves.
//!
//! `StockShapeBottom` is the stock's minimum world Z. It is a name of its
//! own: `StockZBottom` stays the controller-identity law of #1666's
//! Klartext blank, so no other post's input moves.

use crate::geometry::{transform_point, Point3};
use crate::model;
use crate::post::cnc::Record;
use crate::setup_stock::StockSource;
use crate::stock::StockShape;
use flecs_ecs::prelude::World;
use serde_json::json;

/// A world axis a rotated unit vector lies on: 0 X, 1 Y, 2 Z.
fn world_axis(v: Point3) -> Option<usize> {
    const ON: f64 = 1.0 - 1e-9;
    [v.x, v.y, v.z].iter().position(|c| c.abs() >= ON)
}

/// The kind word for the setup's stock and, for a bar, its diameter.
fn kind_of(world: &World, setup: Option<&str>) -> Option<(&'static str, Option<f64>)> {
    if let Some(name) = crate::setup::effective(world, setup)
        && crate::setup_stock::effective(world, &name) == Some(StockSource::FromPrevious)
    {
        return Some(("from-previous", None));
    }
    let e = crate::setup::stock_entity(world, setup)?;
    let shape = model::stock_def_of(world, &e)?;
    let rot = model::transform_of(world, &e).unwrap_or_default().rot;
    let axis = |x: f64, y: f64, z: f64| world_axis(transform_point(Point3::new(x, y, z), Point3::ZERO, rot));
    Some(match shape {
        StockShape::Box { .. } => {
            let (a, b, c) = (axis(1.0, 0.0, 0.0), axis(0.0, 1.0, 0.0), axis(0.0, 0.0, 1.0));
            if a.is_some() && b.is_some() && c.is_some() { ("box", None) } else { ("rotated", None) }
        }
        StockShape::Cylinder { diameter, .. } => match axis(0.0, 0.0, 1.0) {
            Some(0) => ("cylinder-x", Some(diameter)),
            Some(1) => ("cylinder-y", Some(diameter)),
            Some(_) => ("cylinder-z", Some(diameter)),
            None => ("rotated", None),
        },
        StockShape::FromModel { .. } => ("from-model", None),
    })
}

/// Insert the shape PARAMs just after the stream's `StockZTop`, when the
/// setup has a stock and the stream states its box. Any other stream is
/// unchanged.
pub(super) fn insert(world: &World, setup: Option<&str>, records: &mut Vec<Record>) {
    let Some(at) = records.iter().position(|r| matches!(r, Record::Param { name, .. } if name == "StockZTop")) else {
        return;
    };
    let (Some((kind, diameter)), Some(bounds)) = (kind_of(world, setup), model::stock_world_aabb_for(world, setup)) else {
        return;
    };
    let mut add = vec![
        Record::Param { name: "StockShape".into(), value: json!(kind) },
        Record::Param { name: "StockShapeBottom".into(), value: json!(bounds.min.z) },
    ];
    if let Some(d) = diameter.filter(|d| d.is_finite() && *d > 0.0) {
        add.push(Record::Param { name: "StockShapeDiameter".into(), value: json!(d) });
    }
    let tail = records.split_off(at + 1);
    records.extend(add);
    records.extend(tail);
}
