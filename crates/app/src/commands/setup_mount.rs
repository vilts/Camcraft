//! `SetSetupMachine` / `SetSetupPost` (#343): give a setup its own machine
//! or post, change it, or let it follow the document's again (`None`).
//! Construction validates exactly like `LoadMachine` / `SelectPost` (a
//! machine stating an impossible limit, an unreadable or broken `.cps`, a
//! registry name that does not exist - each refused by name) and
//! snapshots both values; `apply` / `invert` are pure writes, so undo puts
//! the previous mount back byte for byte.
//!
//! Provenance: our own design (setup furniture, the `SetSetupWorkOffset`
//! shape); no vendor implementation inspected.

use flecs_ecs::core::EntityViewGet; // try_get is not re-exported in prelude (0.2.2)
use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::machine::Machine;
use crate::model;
use crate::schema::{MachineDef, PostRef, SetupDef};

fn def_of(world: &World, setup: &str) -> Result<SetupDef, CommandError> {
    let entity = model::find_by_name(world, setup)
        .ok_or_else(|| CommandError::NotFound(setup.to_string()))?;
    entity
        .entity_view(world)
        .try_get::<&SetupDef>(|d| d.clone())
        .ok_or_else(|| CommandError::Invalid(format!("'{setup}' is not a setup")))
}

fn write(world: &World, setup: &str, edit: impl FnOnce(&mut SetupDef)) {
    let Some(entity) = model::find_by_name(world, setup) else {
        return;
    };
    let view = entity.entity_view(world);
    if let Some(mut def) = view.try_get::<&SetupDef>(|d| d.clone()) {
        edit(&mut def);
        view.set(def);
    }
}

/// Write a setup's own machine; `None` follows the document's.
#[derive(Debug)]
pub struct SetSetupMachine {
    setup: String,
    prev: Option<MachineDef>,
    next: Option<MachineDef>,
}

impl SetSetupMachine {
    /// `next`: `Some((path, machine))` mounts that machine on the setup
    /// (`machine: None` - deliberately no machine); `None` follows the
    /// document again.
    pub fn new(
        world: &World,
        setup: &str,
        next: Option<(String, Option<Machine>)>,
    ) -> Result<Self, CommandError> {
        let def = def_of(world, setup)?;
        if let Some((_, Some(m))) = &next
            && let Some(r) = crate::machine::tooling_refusal(m)
        {
            return Err(CommandError::Invalid(format!("machine {r}")));
        }
        if let Some((_, Some(m))) = &next
            && !m.is_sane()
        {
            return Err(CommandError::Invalid(
                "machine states a limit that cannot be meant".to_owned(),
            ));
        }
        let next = next.map(|(path, machine)| MachineDef { path, machine });
        if def.machine == next {
            return Err(CommandError::Invalid(match next {
                None => format!("setup '{setup}' already follows the document's machine"),
                Some(_) => format!("setup '{setup}' already mounts that machine"),
            }));
        }
        Ok(Self { setup: setup.to_owned(), prev: def.machine, next })
    }

    /// Mount a shipped library machine by NAME (the `LoadMachine::builtin`
    /// law: a stale name refuses, never mounts another machine).
    pub fn builtin(world: &World, setup: &str, name: &str) -> Result<Self, CommandError> {
        let text = crate::posting::builtin_machine_text(name).map_err(CommandError::Invalid)?;
        let m = crate::machine::parse_mch(text).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some((crate::posting::builtin_path(name), Some(m))))
    }

    /// Read and parse a `.mch` file, then mount it; the refusal names the file.
    pub fn from_file(world: &World, setup: &str, path: &std::path::Path) -> Result<Self, CommandError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| CommandError::Invalid(format!("machine file {}: {e}", path.display())))?;
        let m = crate::machine::parse_mch(&text).map_err(|e| CommandError::Invalid(format!("{}: {e}", path.display())))?;
        Self::new(world, setup, Some((path.display().to_string(), Some(m))))
    }

    /// Mount a Machines-pane catalogue machine (#384), read and
    /// validated again (`library::read_for_assign`): a refused file
    /// never becomes the setup's machine.
    pub fn from_catalogue(
        world: &World,
        setup: &str,
        source: &crate::machine::library::MachineSource,
    ) -> Result<Self, CommandError> {
        let (path, m) = crate::machine::library::read_for_assign(source).map_err(CommandError::Invalid)?;
        Self::new(world, setup, Some((path, Some(m))))
    }

    #[must_use]
    pub fn setup(&self) -> &str {
        &self.setup
    }
}

impl Command for SetSetupMachine {
    fn apply(&self, world: &World) {
        let next = self.next.clone();
        write(world, &self.setup, |d| d.machine = next);
    }

    fn invert(&self, world: &World) {
        let prev = self.prev.clone();
        write(world, &self.setup, |d| d.machine = prev);
    }
}

/// Write a setup's own post; `None` follows the document's.
#[derive(Debug)]
pub struct SetSetupPost {
    setup: String,
    prev: Option<PostRef>,
    next: Option<PostRef>,
}

impl SetSetupPost {
    pub fn new(world: &World, setup: &str, next: Option<PostRef>) -> Result<Self, CommandError> {
        let def = def_of(world, setup)?;
        if let Some(p) = &next {
            super::post::validate(p)?;
        }
        if def.post == next {
            return Err(CommandError::Invalid(match next {
                None => format!("setup '{setup}' already follows the document's post"),
                Some(_) => format!("setup '{setup}' already posts with that post"),
            }));
        }
        Ok(Self { setup: setup.to_owned(), prev: def.post, next })
    }
}

impl Command for SetSetupPost {
    fn apply(&self, world: &World) {
        let next = self.next.clone();
        write(world, &self.setup, |d| d.post = next);
    }

    fn invert(&self, world: &World) {
        let prev = self.prev.clone();
        write(world, &self.setup, |d| d.post = prev);
    }
}
