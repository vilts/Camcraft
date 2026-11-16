//! `SetSetupNoseToAxis` (#821): state the setup's own spindle-nose-to-
//! rotary-centerline figure (measured on the stack the rotary stands on),
//! change it, or clear it (`None`: judge from the machine file less the
//! stack, `crate::rotary_stack`). Construction validates the figure (a
//! finite length over zero, at most `MAX_NOSE_MM`) and refuses a no-op;
//! `apply` / `invert` are pure writes of `SetupDef::nose_to_axis`.
//!
//! Provenance: our own design (the `SetSetupPostOptions` shape, #389); no
//! vendor implementation inspected.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::SetupDef;

/// No stated nose figure past this, mm (5 m): a typo, never a machine.
pub const MAX_NOSE_MM: f64 = 5000.0;

/// Write a setup's stated `nose_to_axis`; `None` clears it.
#[derive(Debug)]
pub struct SetSetupNoseToAxis {
    setup: String,
    prev: Option<f64>,
    next: Option<f64>,
}

impl SetSetupNoseToAxis {
    /// Refused: an unknown setup, a figure that is not a finite length
    /// over zero (or past `MAX_NOSE_MM`), or no change.
    pub fn new(world: &World, setup: &str, next: Option<f64>) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, setup).ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
        let prev = entity
            .entity_view(world)
            .try_get::<&SetupDef>(|d| d.nose_to_axis)
            .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))?;
        if let Some(v) = next
            && !(v.is_finite() && v > 0.0 && v <= MAX_NOSE_MM)
        {
            return Err(CommandError::Invalid(format!(
                "setup '{setup}': the stated nose_to_axis must be a length over zero and at most {MAX_NOSE_MM} mm (got {v})"
            )));
        }
        if prev == next {
            return Err(CommandError::Invalid(match next {
                None => format!("setup '{setup}' states no nose_to_axis of its own"),
                Some(v) => format!("setup '{setup}' already states nose_to_axis {v} mm"),
            }));
        }
        Ok(Self { setup: setup.to_owned(), prev, next })
    }

    /// The figure the command lands.
    #[must_use]
    pub fn next(&self) -> Option<f64> {
        self.next
    }
}

fn write(world: &World, setup: &str, value: Option<f64>) {
    let Some(entity) = model::find_by_name(world, setup) else {
        return;
    };
    let view = entity.entity_view(world);
    if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
        def.nose_to_axis = value;
        view.set(def);
    }
}

impl Command for SetSetupNoseToAxis {
    fn apply(&self, world: &World) {
        write(world, &self.setup, self.next);
    }

    fn invert(&self, world: &World) {
        write(world, &self.setup, self.prev);
    }
}
