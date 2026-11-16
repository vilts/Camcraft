//! SetUnits: flip the document's unit (M1.1) - the display lens and,
//! since #902, the unit every post writes the program in (G20 / G21 or
//! the dialect's word; `posting::program_options`).
//!
//! No geometry or parameter value changes (every stored number stays
//! mm; the post converts at output), so the
//! command writes only the world singleton and its inverse writes the
//! previous value back - undo restores exactly what was on screen.
//! Setting the unit the document already has is refused (a no-op does
//! not belong in the history).

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::units::{self, Unit};

#[derive(Debug)]
pub struct SetUnits {
    prev: Unit,
    next: Unit,
}

impl SetUnits {
    pub fn new(world: &World, next: Unit) -> Result<Self, CommandError> {
        let prev = units::of(world);
        if prev == next {
            return Err(CommandError::Invalid(format!(
                "the document is already in {next:?}"
            )));
        }
        Ok(Self { prev, next })
    }
}

impl Command for SetUnits {
    fn apply(&self, world: &World) {
        units::set(world, self.next);
    }

    fn invert(&self, world: &World) {
        units::set(world, self.prev);
    }
}
