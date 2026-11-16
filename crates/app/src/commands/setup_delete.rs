//! Setup deletion and its guards, split from commands::setup (#398).
//! Provenance: our own design over general re-clamp practice; no vendor implementation inspected.

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::World;

use super::setup::StockSnap;
use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{Location, STOCK_NAME, SetupDef, StockDef, LEGACY_WCS_NAMES, Wcs};
use crate::setup;

/// Delete a setup, its private furniture, and its claim on the program.
/// Refused while any operation still machines in it.
pub struct DeleteSetup {
    name: String,
    def: SetupDef,
    /// Furniture that goes with the setup: entities this setup alone
    /// names. Shared legacy names (`Stock`, `G54`..) and anything
    /// another setup points at are NOT deleted.
    stock: Option<StockSnap>,
    wcs: Option<(String, Location)>,
    prev_active: Option<String>,
}

impl DeleteSetup {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, name)
            .ok_or_else(|| CommandError::NotFound(name.to_string()))?;
        let def = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.clone())
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a setup")))?;
        // Refusal: an operation that would orphan its own program
        // position. `belongs` is the single predicate the order law uses.
        // #155: a Pause / Home entry of its list holds it the same way.
        for (op_name, op_e) in crate::program_entry::sequence(world) {
            let declared = crate::program_entry::declared_setup(world, &op_e);
            if setup::belongs(world, declared.as_deref(), name) {
                return Err(CommandError::Invalid(format!(
                    "'{op_name}' machines in '{name}' - move or delete it first"
                )));
            }
        }
        // #316: a setup placed relative to this one would lose the setup
        // its edits follow - refused by name, like an operation.
        if let Some(child) = crate::setup_relative::dependents(world, name).first() {
            return Err(CommandError::Invalid(format!(
                "'{child}' is placed relative to '{name}' - unlink it first"
            )));
        }
        let owned = |n: &str| {
            n != STOCK_NAME
                && !LEGACY_WCS_NAMES.contains(&n)
                && !setup::is_furniture_of_other(world, n, name)
        };
        let stock = if owned(&def.stock) {
            model::find_by_name(world, &def.stock).and_then(|e| {
                model::stock_def_of(world, &e).map(|shape| StockSnap {
                    name: def.stock.clone(),
                    shape,
                    location: model::transform_of(world, &e).unwrap_or_default(),
                    facets: model::facets_of(world, &e).unwrap_or_default(),
                    coarse_mm: crate::model_fidelity::chord_of(world, &e),
                })
            })
        } else {
            None
        };
        let wcs = if owned(&def.wcs) {
            model::find_by_name(world, &def.wcs)
                .filter(|e| model::is_wcs(world, e))
                .and_then(|e| model::transform_of(world, &e))
                .map(|loc| (def.wcs.clone(), loc))
        } else {
            None
        };
        Ok(DeleteSetup {
            name: name.to_string(),
            def,
            stock,
            wcs,
            prev_active: setup::active(world),
        })
    }
}

impl Command for DeleteSetup {
    fn apply(&self, world: &World) {
        if let Some(s) = &self.stock
            && let Some(e) = model::find_by_name(world, &s.name)
        {
            model::remove(world, &e);
        }
        if let Some((wname, _)) = &self.wcs
            && let Some(e) = model::find_by_name(world, wname)
        {
            model::remove(world, &e);
        }
        if let Some(e) = model::find_by_name(world, &self.name) {
            model::remove(world, &e);
        }
        // The active-setup singleton is left pointing where it pointed:
        // `setup::active`'s existence fallback makes a deleted name read
        // as the first remaining setup, exactly the law.
    }

    fn invert(&self, world: &World) {
        let entity = model::spawn(world, &self.name);
        entity.entity_view(world).set(self.def.clone());
        if let Some(s) = &self.stock {
            let e = model::spawn(world, &s.name);
            model::set_transform(world, &e, s.location);
            model::set_facets(world, &e, s.facets.clone());
            crate::model_fidelity::set(world, &e, s.coarse_mm);
            e.entity_view(world).set(StockDef {
                shape: s.shape.clone(),
            });
        }
        if let Some((wname, loc)) = &self.wcs {
            let e = model::spawn(world, wname);
            e.entity_view(world).add(Wcs);
            model::set_transform(world, &e, *loc);
        }
        setup::set_active(world, self.prev_active.clone());
    }
}
