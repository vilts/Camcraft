//! SetWcsFlip (#1720): set which axes of a setup's one WCS run reversed
//! (X+ becomes X-). The flags ride the WCS entity (`schema::WcsFlip`);
//! the origin and rotation are not touched. A WCS that was never placed
//! is placed at the identity, as `SetWcsOrigin` would, and the inverse
//! removes it again. The post mirrors a section of a flipped X or Y
//! (#1649, `post::wcs_flip`); a flipped Z and the other cases it does
//! not carry are refused by name (`setup_wcs_flip`).
//!
//! Provenance: our own design notes over our own WCS model; no vendor
//! material consulted.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::{Location, Wcs};
use crate::setup_wcs_flip;

pub struct SetWcsFlip {
    /// The setup's WCS entity at construction time.
    entity_name: String,
    flip: [bool; 3],
    /// The flags before; `None` means the entity did not exist yet, so
    /// the inverse removes it.
    prev: Option<[bool; 3]>,
}

impl SetWcsFlip {
    /// Validates now: the setup exists (`None`: the active one) and its
    /// WCS name is not taken by an entity that is not a WCS.
    pub fn for_setup(world: &World, setup: Option<&str>, flip: [bool; 3]) -> Result<Self, CommandError> {
        if let Some(s) = setup
            && !crate::setup::exists(world, s)
        {
            return Err(CommandError::NotFound(s.to_owned()));
        }
        let setup = match setup {
            Some(s) => Some(s.to_owned()),
            None => crate::setup::active(world),
        };
        let entity_name = crate::setup::wcs_entity_name(world, setup.as_deref());
        let prev = match model::find_by_name(world, &entity_name) {
            Some(entity) => {
                if !model::is_wcs(world, &entity) {
                    return Err(CommandError::NameConflict(entity_name));
                }
                Some(setup_wcs_flip::of_entity(world, &entity))
            }
            None => None,
        };
        Ok(SetWcsFlip { entity_name, flip, prev })
    }

    /// The entity this command writes.
    #[must_use]
    pub fn entity_name(&self) -> &str {
        &self.entity_name
    }

    /// The flags this command sets (X, Y, Z).
    #[must_use]
    pub fn flip(&self) -> [bool; 3] {
        self.flip
    }
}

impl Command for SetWcsFlip {
    fn apply(&self, world: &World) {
        // Pure world write from stored fields: idempotent under redo.
        let entity = match model::find_by_name(world, &self.entity_name) {
            Some(entity) => entity,
            None => {
                let e = model::spawn(world, &self.entity_name);
                e.entity_view(world).add(Wcs);
                model::set_transform(world, &e, Location::default());
                e
            }
        };
        setup_wcs_flip::set(world, &entity, self.flip);
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.entity_name) else {
            return; // already undone (idempotent undo path)
        };
        match self.prev {
            Some(flip) => setup_wcs_flip::set(world, &entity, flip),
            None => model::remove(world, &entity), // we created it
        }
    }
}
