//! SetModelSource (#2463): link a model to a file, or unlink it, as one undo
//! step. A linked model is imported again when its file gets a new SHA-256
//! (the UI's link check); an unlinked model is never touched. Linking
//! records the file's digest as it is now and does not import it.
//! Provenance: our own command, the pattern of the app's other name-keyed
//! invertible commands.

use std::path::Path;

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::ModelSource;

pub struct SetModelSource {
    name: String,
    source: Option<ModelSource>,
    prev: Option<ModelSource>,
}

impl SetModelSource {
    /// Link model `name` to the file at `path`, recording its digest now.
    /// A missing model, setup furniture, or a file that cannot be read is
    /// refused by name.
    pub fn link(world: &World, name: &str, path: &Path) -> Result<Self, CommandError> {
        let prev = Self::model_source(world, name)?;
        let source = crate::model_source::read(path).map_err(|e| CommandError::Invalid(format!("cannot link '{name}': {e}")))?;
        Ok(SetModelSource { name: name.to_string(), source: Some(source), prev })
    }

    /// Unlink model `name`: its file is no longer checked. A model that is
    /// not linked is refused, so the history gets no empty step.
    pub fn unlink(world: &World, name: &str) -> Result<Self, CommandError> {
        let prev = Self::model_source(world, name)?;
        if prev.is_none() {
            return Err(CommandError::Invalid(format!("'{name}' is not linked to a file")));
        }
        Ok(SetModelSource { name: name.to_string(), source: None, prev })
    }

    /// The source of model `name` as it stands.
    fn model_source(world: &World, name: &str) -> Result<Option<ModelSource>, CommandError> {
        let entity = model::find_by_name(world, name).ok_or_else(|| CommandError::NotFound(name.to_string()))?;
        if !model::is_model(world, &entity) {
            return Err(CommandError::Invalid(format!("'{name}' is not a model")));
        }
        Ok(crate::model_source::of(world, &entity))
    }

    fn write(&self, world: &World, source: Option<&ModelSource>) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            crate::model_source::set(world, &entity, source);
        }
    }
}

impl Command for SetModelSource {
    fn apply(&self, world: &World) {
        self.write(world, self.source.as_ref());
    }

    fn invert(&self, world: &World) {
        self.write(world, self.prev.as_ref());
    }
}
