//! `SetSetupWorkOffset` (#196, #342, #931): label a setup's one WCS with
//! the work offset (a 0-based pick, offset `pick + 1`) its post writes
//! for it, or clear the label back to offset 1. Only the offset changes -
//! the setup's WCS origin, and every height measured from it, stay where
//! they are. The limit is the setup's own post's (`setup_offsets::check`);
//! core keeps only a sanity bound.
//!
//! Provenance: our own design; RS-274 / ISO 6983 work-offset selection
//! (one of six controller zero registers per clamping) is general
//! machine-shop practice - no vendor implementation inspected. Like its
//! setup siblings, construction validates and snapshots both values;
//! `apply`/`invert` are pure writes of the snapshot.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::SetupDef;
use crate::setup_offsets;

/// Write a setup's work-offset word. `None` clears the label.
pub struct SetSetupWorkOffset {
    setup: String,
    prev: Option<usize>,
    next: Option<usize>,
}

impl SetSetupWorkOffset {
    pub fn new(world: &World, setup: &str, slot: Option<usize>) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, setup)
            .ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.work_offset)
            .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
        if let Some(i) = slot {
            if i > setup_offsets::MAX_SLOT {
                return Err(CommandError::Invalid(format!(
                    "work offset {} does not exist - offsets run 1 to {}",
                    i.saturating_add(1),
                    setup_offsets::MAX_SLOT + 1
                )));
            }
            // #931: the setup's own post bounds the offset, by name.
            let n = u32::try_from(i).map_or(u32::MAX, |i| i.saturating_add(1));
            setup_offsets::check(world, setup, n).map_err(CommandError::Invalid)?;
        }
        if prev == slot {
            let n = u32::try_from(slot.unwrap_or(0)).map_or(u32::MAX, |i| i.saturating_add(1));
            return Err(CommandError::Invalid(format!(
                "setup '{setup}' already posts at {}",
                setup_offsets::offset_name(world, Some(setup), n)
            )));
        }
        Ok(SetSetupWorkOffset {
            setup: setup.to_string(),
            prev,
            next: slot,
        })
    }

    fn write(&self, world: &World, slot: Option<usize>) {
        let Some(entity) = model::find_by_name(world, &self.setup) else {
            return;
        };
        let view = entity.entity_view(world);
        if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
            def.work_offset = slot;
            view.set(def);
        }
    }
}

impl Command for SetSetupWorkOffset {
    fn apply(&self, world: &World) {
        self.write(world, self.next);
    }

    fn invert(&self, world: &World) {
        self.write(world, self.prev);
    }
}
