//! Import a solid as a fixture in ONE undo unit (#339 slice 4): the
//! landed mesh (a fresh `ImportModel`, or an `UpdateModel` when the name
//! already carries a model) and the `FixtureDef` binding, applied and
//! inverted together. One Ctrl+Z takes the whole import away - never a
//! half state of "imported, but no longer workholding".
//!
//! The mesh is parsed on the import worker as always; this command is
//! built on the main thread from the worker's finished `ImportModel`,
//! so its constructor never reads disk.
//!
//! Provenance: our own composition of the existing import and fixture
//! commands; no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError, ImportModel, UpdateModel};
use crate::fixture::{self, MACHINE_TABLE};
use crate::model;
use crate::schema::FixtureDef;
use crate::setup;

/// The landing half: a spawn or an update of the named model.
enum Landing {
    Fresh(ImportModel),
    Update(UpdateModel),
}

pub struct ImportFixture {
    name: String,
    landing: Landing,
    /// The marker before (an update of a model already marked keeps its
    /// old binding for the undo).
    prev: Option<FixtureDef>,
    next: FixtureDef,
    /// #819: a re-import drops a plate's grid, so its riders leave it.
    riders: crate::plate::mount::Unmounts,
}

impl ImportFixture {
    /// Bind the worker's finished import to setup context `setup_name`
    /// (`None` = the implicit first setup). A name that already carries a
    /// model updates it (M3.1) and marks it; setup furniture under the
    /// name is refused by `UpdateModel` itself.
    pub fn new(world: &World, import: ImportModel, setup_name: Option<&str>) -> Result<Self, CommandError> {
        let name = import.name.clone();
        if name == MACHINE_TABLE {
            return Err(CommandError::Invalid("'Machine/Table' names the derived machine table".into()));
        }
        if let Some(s) = setup_name
            && !setup::exists(world, s)
        {
            return Err(CommandError::NotFound(s.to_string()));
        }
        let existing = model::find_by_name(world, &name);
        let prev = existing.as_ref().and_then(|e| fixture::def_of(world, e));
        // #819: a re-import drops a plate's grid, so its riders leave it.
        let riders = if prev.as_ref().is_some_and(|d| d.plate.plate.is_some()) {
            crate::plate::mount::Unmounts::of(world, &name)
        } else {
            crate::plate::mount::Unmounts::default()
        };
        let landing = match existing {
            Some(_) => Landing::Update(
                UpdateModel::new(world, &name, import.mesh)?
                    .with_fidelity(import.coarse_mm)
                    .with_budget(import.facet_budget),
            ),
            None => Landing::Fresh(import),
        };
        Ok(ImportFixture {
            name,
            landing,
            // A re-import replaces the geometry, so a built-in vise's
            // parameters no longer describe it: plain steel from here.
            // A plate's grid goes with its old geometry too (#819); a
            // place on another plate stays.
            next: FixtureDef {
                setup: setup_name.map(str::to_string),
                fixed: prev.as_ref().is_some_and(|d| d.fixed),
                vise: None,
                mod_jaw: None,
                accessory: None,
                plate: crate::plate::PlateLink { plate: None, mount: prev.as_ref().and_then(|d| d.plate.mount.clone()) },
            },
            riders,
            prev,
        })
    }

    /// The landed model's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// What an update changed (`None` for a fresh import).
    #[must_use]
    pub fn update_summary(&self) -> Option<String> {
        match &self.landing {
            Landing::Fresh(_) => None,
            Landing::Update(u) => Some(u.changes().describe()),
        }
    }
}

impl Command for ImportFixture {
    fn apply(&self, world: &World) {
        match &self.landing {
            Landing::Fresh(c) => c.apply(world),
            Landing::Update(c) => c.apply(world),
        }
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
        self.riders.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.riders.write(world, false);
        if let Some(entity) = model::find_by_name(world, &self.name) {
            match &self.prev {
                Some(def) => {
                    entity.entity_view(world).set(def.clone());
                }
                None => {
                    let _ = entity.entity_view(world).remove(world.component::<FixtureDef>());
                }
            }
        }
        match &self.landing {
            Landing::Fresh(c) => c.invert(world),
            Landing::Update(c) => c.invert(world),
        }
    }
}
