//! Fixture commands: tag, place, and unmake the workholding hardware a
//! part rides on (M3.2 stage B).
//!
//! Provenance: our own design; a fixture being an imported solid the
//! shop bolts down is plain machine-shop practice. Like every sibling
//! here, construction reads the world, validates, and snapshots;
//! `apply` and `invert` are pure writes, so undo/redo replay bytes and
//! never re-derive.
//!
//! ## The laws these commands keep
//!
//! - **A fixture is a model wearing a marker.** So there are two ways to
//!   make one: import a solid and [`TagFixture`] it (the UI path - STEP
//!   parsing runs on the import worker, never in a command
//!   constructor), or [`AddFixtureModel`], which parses synchronously at
//!   construction for headless callers and tests. Both land on the same
//!   components every model uses: `Facets`, `Location`, and the
//!   `FixtureDef` binding. No command here builds geometry.
//! - **The marker is the only fixture state.** The binding is a
//!   declared setup (`None` = first/implicit, the stage-A law); the pose
//!   is the entity's `Location` like any part's. [`UntagFixture`]
//!   demotes to a plain model - the geometry stays, the collision law
//!   stops seeing it; [`MoveFixture`] is the same move any model
//!   receives, gated to fixture-bearing entities because the page that
//!   offers it shows only those.
//! - **`Machine/Table` is prose, not an entity.** Every command here
//!   refuses that name with the honest reason: the table exists while a
//!   `.mch` says so, and its geometry is whatever the user machined
//!   against, not a thing to move or delete.
//! - **Deleting is the whole-entity inverse of having.** Everything the
//!   fixture owned - marker, placement, facets - is snapshotted at
//!   construction and restored verbatim by the inverse, so redo-after-
//!   undo reproduces bytes, not re-derivations.

use flecs_ecs::prelude::{Entity, World};

use crate::commands::{Command, CommandError};
use crate::fixture::{self, MACHINE_TABLE};
use crate::geometry::{normalize_quat, Triangle};
use crate::model;
use crate::schema::{FixtureDef, Location};
use crate::setup;

pub use super::fixture_move::MoveFixture;

/// The placement validation every pose write shares, in the
/// `SetStock` voice: a unit quaternion (normalized at the chokepoint)
/// and a finite position, or the command never exists. Shared by every
/// pose-writing command (`MoveFixture`, `MoveModel`) so the placement
/// validation is one law, not two that drift.
pub(crate) fn check_location(location: Location, what: &str) -> Result<Location, CommandError> {
    let mut location = location;
    location.rot = normalize_quat(location.rot).ok_or_else(|| {
        CommandError::Invalid(format!("{what} rotation is not a quaternion"))
    })?;
    if !location.pos.is_finite() {
        return Err(CommandError::Invalid(format!(
            "{what} placement must be finite"
        )));
    }
    Ok(location)
}

/// The gate every fixture-named command runs first: the table row is
/// virtual, everything else must name a live entity.
pub(super) fn require_writable(world: &World, name: &str) -> Result<Entity, CommandError> {
    if name == MACHINE_TABLE {
        return Err(CommandError::Invalid(
            "the machine table is derived from the machine file - it is not a fixture you can change".into(),
        ));
    }
    model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_string()))
}

/// Tag an existing model as a fixture of setup context `setup` (or
/// re-bind an existing fixture: the previous binding is snapshotted, so
/// undo restores it, not "untagged"). The model's geometry and pose are
/// already someone's business - `ImportModel`/`UpdateModel` own those
/// writes; this command touches only the marker.
pub struct TagFixture {
    name: String,
    prev: Option<FixtureDef>,
    next: FixtureDef,
}

impl TagFixture {
    pub fn new(world: &World, name: &str, setup_name: Option<&str>) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        if !model::is_model(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is not a model, so it cannot be marked as a fixture - it is a setup's stock or work offset, a setup, a tool, an operation or other CAM data"
            )));
        }
        if let Some(s) = setup_name
            && !setup::exists(world, s)
        {
            return Err(CommandError::NotFound(s.to_string()));
        }
        let prev = fixture::def_of(world, &entity);
        Ok(TagFixture {
            name: name.to_string(),
            // A re-bind keeps whether the steel turns: that is
            // [`SetFixtureFixed`]'s write, not the binding's.
            next: FixtureDef {
                setup: setup_name.map(str::to_string),
                fixed: prev.as_ref().is_some_and(|d| d.fixed),
                // A re-bind keeps a built-in vise's parameters too: its
                // geometry is unchanged.
                vise: prev.as_ref().and_then(|d| d.vise.clone()),
                // ...and a plate's grid and place (#819).
                plate: prev.as_ref().map(|d| d.plate.clone()).unwrap_or_default(),
                // ...and a mod-vise jaw block's (#820).
                mod_jaw: prev.as_ref().and_then(|d| d.mod_jaw.clone()),
                accessory: prev.as_ref().and_then(|d| d.accessory.clone()),
            },
            prev,
        })
    }
}

/// Declare whether a fixture stands still while a rotary turns the part
/// (a table, a tailstock: judged in the machine frame) or turns with it
/// (a chuck: judged in the path's own frame - the default) - M13.8 (2).
pub struct SetFixtureFixed {
    name: String,
    prev: FixtureDef,
    next: FixtureDef,
}

impl SetFixtureFixed {
    pub fn new(world: &World, name: &str, fixed: bool) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        let prev = fixture::def_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
        let next = FixtureDef { fixed, ..prev.clone() };
        Ok(SetFixtureFixed { name: name.to_string(), prev, next })
    }
}

impl Command for SetFixtureFixed {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

impl Command for TagFixture {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return;
        };
        match &self.prev {
            Some(def) => {
                entity.entity_view(world).set(def.clone());
            }
            None => {
                let _ = entity
                    .entity_view(world)
                    .remove(world.component::<FixtureDef>());
            }
        }
    }
}

/// Untag: the entity stays, its geometry stays, the collision law stops
/// seeing it. Snapshots the binding so undo re-tags exactly.
pub struct UntagFixture {
    name: String,
    prev: FixtureDef,
    /// #819: an untagged plate stops being one - its riders leave it.
    riders: crate::plate::mount::Unmounts,
}

impl UntagFixture {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        let prev = fixture::def_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
        Ok(UntagFixture {
            name: name.to_string(),
            prev,
            riders: crate::plate::mount::Unmounts::of(world, name),
        })
    }
}

impl Command for UntagFixture {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            let _ = entity
                .entity_view(world)
                .remove(world.component::<FixtureDef>());
        }
        self.riders.write(world, true);
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
        self.riders.write(world, false);
    }
}

/// Delete a fixture solid outright - marker, pose, and geometry all
/// come back on undo. Everything the entity owned goes into the
/// snapshot; the inverse puts it back byte for byte.
pub struct DeleteFixture {
    name: String,
    def: FixtureDef,
    location: Location,
    facets: std::sync::Arc<[Triangle]>,
    /// #819: a deleted plate's riders leave it (they stay where they
    /// stand), so a later plate given the same name never carries them.
    riders: crate::plate::mount::Unmounts,
    /// #1094: a raw-stock-only mark naming this model goes with it (the
    /// print check in `stock_source` is the law; this is tidiness).
    marks: super::stock_source_only::MarkDrops,
}

impl DeleteFixture {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let entity = require_writable(world, name)?;
        let def = fixture::def_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a fixture")))?;
        Ok(DeleteFixture {
            name: name.to_string(),
            def,
            location: model::transform_of(world, &entity).unwrap_or_default(),
            facets: model::facets_of(world, &entity).unwrap_or_default(),
            riders: crate::plate::mount::Unmounts::of(world, name),
            marks: super::stock_source_only::MarkDrops::of(world, name),
        })
    }
}

impl Command for DeleteFixture {
    fn apply(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.name) {
            model::remove(world, &e);
        }
        self.riders.write(world, true);
        self.marks.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.riders.write(world, false);
        if model::find_by_name(world, &self.name).is_some() {
            return; // already restored (idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.location);
        model::set_facets(world, &entity, self.facets.clone());
        entity.entity_view(world).set(self.def.clone());
        self.marks.write(world, false);
    }
}

/// Headless convenience: parse a mesh file at construction (the law
/// that no UI command constructor reads disk is why the UI imports a
/// model on its worker and then [`TagFixture`]s it - this exists for
/// tests and batch scripts, where construction IS off the frame). The
/// new entity starts unplaced at the origin, revision 1, bound to no
/// setup (the implicit first).
pub struct AddFixtureModel {
    name: String,
    facets: std::sync::Arc<[Triangle]>,
    /// #864: the chord error of a mesh imported coarser than promised.
    coarse_mm: Option<f64>,
}

impl AddFixtureModel {
    pub fn new(
        world: &World,
        name: &str,
        path: &std::path::Path,
    ) -> Result<Self, CommandError> {
        if name.trim().is_empty() {
            return Err(CommandError::Invalid(
                "fixture name must not be empty".into(),
            ));
        }
        if name == MACHINE_TABLE {
            return Err(CommandError::Invalid(
                "'Machine/Table' names the derived machine table".into(),
            ));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_string()));
        }
        let graded = crate::geometry::import::load_model_graded(path)?;
        Ok(AddFixtureModel {
            name: name.to_string(),
            facets: graded.mesh.facets,
            coarse_mm: graded.coarse_mm,
        })
    }
}

impl Command for AddFixtureModel {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            // Redo replay: make it idempotent by re-finding, the
            // ImportModel convention.
            if fixture::def_of(world, &entity).is_none() {
                entity.entity_view(world).set(FixtureDef::default());
            }
            return;
        }
        let entity = model::spawn(world, &self.name);
        model::set_facets(world, &entity, self.facets.clone());
        model::set_rev(world, &entity, 1);
        crate::model_fidelity::set(world, &entity, self.coarse_mm);
        entity.entity_view(world).set(FixtureDef::default());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}
