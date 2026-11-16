//! `SetSetupWorkholdingRpm` (#1031): state the rated maximum rpm of a
//! setup's chuck or collet, change it, or clear it (`None`). Construction
//! validates the value (finite, at least 1, at most
//! [`crate::setup_workholding::MAX_STATED_RPM`]), snapshots the previous
//! one and refuses a no-op; `apply` / `invert` are pure writes of
//! `SetupDef::workholding_max_rpm`, so undo puts the previous limit back
//! exactly.
//!
//! Provenance: our own design (the `SetSetupPostOptions` shape, #389); no
//! vendor implementation inspected.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::SetupDef;

/// Write a setup's workholding rpm limit; `None` states none.
#[derive(Debug)]
pub struct SetSetupWorkholdingRpm {
    setup: String,
    prev: Option<f64>,
    next: Option<f64>,
}

impl SetSetupWorkholdingRpm {
    pub fn new(world: &World, setup: &str, next: Option<f64>) -> Result<Self, CommandError> {
        if let Some(why) = next.and_then(crate::setup_workholding::invalid) {
            return Err(CommandError::Invalid(why));
        }
        let entity = model::find_by_name(world, setup).ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.workholding_max_rpm)
            .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
        if prev == next {
            return Err(CommandError::Invalid(match next {
                None => format!("setup '{setup}' states no workholding maximum rpm"),
                Some(r) => format!("setup '{setup}' already states a workholding maximum of {r} rpm"),
            }));
        }
        Ok(Self { setup: setup.to_owned(), prev, next })
    }

    #[must_use]
    pub fn setup(&self) -> &str {
        &self.setup
    }
}

fn write(world: &World, setup: &str, value: Option<f64>) {
    let Some(entity) = model::find_by_name(world, setup) else {
        return;
    };
    let view = entity.entity_view(world);
    if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
        def.workholding_max_rpm = value;
        view.set(def);
    }
}

impl Command for SetSetupWorkholdingRpm {
    fn apply(&self, world: &World) {
        write(world, &self.setup, self.next);
    }

    fn invert(&self, world: &World) {
        write(world, &self.setup, self.prev);
    }
}
