//! LoadMachine: set the document's machine definition (M2.3 stage C) -
//! what every setup that owns no machine mounts (#343).
//!
//! Setup furniture, like `SetUnits`: no geometry changes, the command
//! writes only the world singleton and its inverse writes the previous
//! value back. The `.mch` text is parsed at command construction -
//! like every other command, a bad file is a refusal at the front
//! door, never a half-written setup.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::machine::{self, Machine};

#[derive(Debug)]
pub struct LoadMachine {
    prev: (String, Option<Machine>),
    next: (String, Option<Machine>),
}

impl LoadMachine {
    /// Mount a parsed machine, loaded from `path` for display. Passing
    /// `None` clears the setup - the checker then checks nothing, by
    /// the unstated-limit law, and that is stated, not silent.
    pub fn new(world: &World, path: String, machine: Option<Machine>) -> Result<Self, CommandError> {
        if let Some(r) = machine.as_ref().and_then(machine::tooling_refusal) {
            return Err(CommandError::Invalid(format!("machine {r}")));
        }
        if let Some(m) = &machine
            && !m.is_sane()
        {
            return Err(CommandError::Invalid(
                "machine states a limit that cannot be meant".to_owned(),
            ));
        }
        let prev = machine::document_of(world);
        if prev.0 == path && prev.1 == machine {
            return Err(CommandError::Invalid(if machine.is_none() {
                "no machine is loaded".to_owned()
            } else {
                "that machine is already loaded".to_owned()
            }));
        }
        Ok(Self {
            prev,
            next: (path, machine),
        })
    }

    /// Read and parse a `.mch` file, then mount it. The refusal names
    /// the file: a machine that loads from nowhere is the silent wrong
    /// answer wearing a setup change.
    pub fn from_file(world: &World, path: &std::path::Path) -> Result<Self, CommandError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CommandError::Invalid(format!("machine file {}: {e}", path.display())))?;
        let m = crate::machine::parse_mch(&text).map_err(|e| CommandError::Invalid(format!("{}: {e}", path.display())))?;
        Self::new(world, path.display().to_string(), Some(m))
    }

    /// Mount a built-in machine from the shipped library (M3.3 stage
    /// C) by NAME - a stale or typo'd name refuses instead of quietly
    /// mounting a different machine than the pane advertised. The
    /// mount path records the library, not a file that exists.
    pub fn builtin(world: &World, name: &str) -> Result<Self, CommandError> {
        // #2282: the shipped file is read from disk; a missing one refuses by name.
        let text = crate::posting::builtin_machine_text(name).map_err(CommandError::Invalid)?;
        let m = crate::machine::parse_mch(text).map_err(CommandError::Invalid)?;
        Self::new(world, crate::posting::builtin_path(name), Some(m))
    }

    /// Mount a machine picked in the Machines pane's catalogue (#384)
    /// onto the DOCUMENT: read and validated again here
    /// (`library::read_for_assign`), so a refused file is refused at the
    /// front door by its first refusal. The ui routes the same read
    /// through the active setup's own machine when it owns one.
    pub fn from_catalogue(
        world: &World,
        source: &crate::machine::library::MachineSource,
    ) -> Result<Self, CommandError> {
        let (path, m) = crate::machine::library::read_for_assign(source).map_err(CommandError::Invalid)?;
        Self::new(world, path, Some(m))
    }
}

impl Command for LoadMachine {
    fn apply(&self, world: &World) {
        machine::set(world, &self.next.0, self.next.1.as_ref());
    }

    fn invert(&self, world: &World) {
        machine::set(world, &self.prev.0, self.prev.1.as_ref());
    }
}
