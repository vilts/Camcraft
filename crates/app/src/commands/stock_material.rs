//! SetStockMaterial (#2015): state the workpiece material of the active
//! setup's stock - and seed it: every operation of that setup takes its
//! tool's seeds for the material now in force there (the feed of a row
//! naming it, the row's stepdown / stepover; `cutting_rows`), all in one
//! undo step. `None` clears it, and the document's material speaks for
//! the setup again. The tools' running rpm / feed follow the document's
//! material only (a tool is shared by every setup).
//!
//! Provenance: our own command; the shape follows `SetStockSourceOnly`
//! (resolve the stock at construction, snapshot, pure world writes in
//! apply/invert) and `SetWorkMaterial`'s composite of op edits.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError, EditOperationParams};
use crate::cutting_rows;
use crate::model;
use crate::setup;
use crate::stock_material;
use crate::tools::WorkMaterial;

pub struct SetStockMaterial {
    /// The stock entity, resolved from the active setup at construction.
    stock_name: String,
    prev: Option<WorkMaterial>,
    next: Option<WorkMaterial>,
    ops: Vec<EditOperationParams>,
}

impl SetStockMaterial {
    /// Refused by name when the active setup has no stock yet, or the
    /// stock already states `next`.
    pub fn new(world: &World, next: Option<WorkMaterial>) -> Result<Self, CommandError> {
        let active = setup::active(world);
        let stock_name = setup::stock_entity_name(world, active.as_deref());
        let entity = model::find_by_name(world, &stock_name)
            .filter(|e| model::is_stock(world, e))
            .ok_or_else(|| CommandError::Invalid(format!("no stock '{stock_name}' - set the stock first")))?;
        let prev = stock_material::of_entity(world, &entity);
        if prev == next {
            return Err(CommandError::Invalid(format!("'{stock_name}' already states that workpiece material")));
        }
        let material = next.or_else(|| crate::cutting::material_of(world));
        let here = setup::effective(world, active.as_deref());
        let mut ops = Vec::new();
        for (name, e) in model::named_entities(world) {
            let Some(op) = model::operation_of(world, &e) else { continue };
            if setup::effective(world, op.setup.as_deref()) != here {
                continue;
            }
            let Some(tool) = model::find_by_name(world, &op.tool).and_then(|t| model::tool_of(world, &t)) else {
                continue;
            };
            if let Some(params) = cutting_rows::reseeded_params(&op.params, &tool, material) {
                ops.push(EditOperationParams::new(world, &name, params)?);
            }
        }
        Ok(Self { stock_name, prev, next, ops })
    }

    /// How many operations the material's rows re-seeded.
    #[must_use]
    pub fn seeded(&self) -> usize {
        self.ops.len()
    }

    /// The stock entity the material was stated on.
    #[must_use]
    pub fn stock_name(&self) -> &str {
        &self.stock_name
    }
}

impl Command for SetStockMaterial {
    fn apply(&self, world: &World) {
        stock_material::land(world, &self.stock_name, self.next);
        for o in &self.ops {
            o.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        for o in self.ops.iter().rev() {
            o.invert(world);
        }
        stock_material::land(world, &self.stock_name, self.prev);
    }
}
