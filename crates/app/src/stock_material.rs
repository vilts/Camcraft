//! A setup stock's workpiece material (#2015): read and land the
//! `StockMaterial` component on a setup's stock entity, and resolve the
//! material that picks a tool's cutting-data row for that setup - the
//! stock's, else the document's, else none (the `Unknown` row).
//!
//! Provenance: our own design; the read/land split follows
//! `stock_source_only` (#1094). Only `SetStockMaterial`, the project
//! loader and the stock's delete/undo land it.

use flecs_ecs::prelude::*;

use crate::model;
use crate::schema::StockMaterial;
use crate::tools::WorkMaterial;

/// The material the stock entity `e` states, if any.
#[must_use]
pub fn of_entity(world: &World, e: &Entity) -> Option<WorkMaterial> {
    e.entity_view(world).try_get::<&StockMaterial>(|m| m.0)
}

/// The material the stock of the declared setup states, if any.
#[must_use]
pub fn of_setup(world: &World, setup: Option<&str>) -> Option<WorkMaterial> {
    crate::setup::stock_entity(world, setup).and_then(|e| of_entity(world, &e))
}

/// The material in force for a setup's operations: its stock's, else
/// the document's. `None` reads the tools' `Unknown` rows.
#[must_use]
pub fn in_force(world: &World, setup: Option<&str>) -> Option<WorkMaterial> {
    of_setup(world, setup).or_else(|| crate::cutting::material_of(world))
}

/// Write (or, `None`, clear) the stock entity's material. A stock that
/// is gone is skipped (idempotent replay).
pub fn land_entity(world: &World, e: &Entity, m: Option<WorkMaterial>) {
    let view = e.entity_view(world);
    match m {
        Some(m) => {
            view.set(StockMaterial(m));
        }
        None => {
            view.remove(world.component::<StockMaterial>());
        }
    }
}

/// [`land_entity`] by the stock's name.
pub fn land(world: &World, stock_name: &str, m: Option<WorkMaterial>) {
    if let Some(e) = model::find_by_name(world, stock_name) {
        land_entity(world, &e, m);
    }
}
