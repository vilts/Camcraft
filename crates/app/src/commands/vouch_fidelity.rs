//! VouchModelFidelity (#1339): a person vouches for a mesh of unknown
//! fidelity - a model of a project saved before v59 with no recorded
//! chord (#1056) - so its operations are judged against it as an exact
//! mesh and can post again without a re-import. One undo unit: the mark
//! leaves the model and the stocks copied from it, the record of who
//! vouched lands on the model, and undo puts every mark back.
//!
//! Provenance: our own command pattern over `model_fidelity` and
//! `model_vouch` (#1339). No vendor material consulted.
//!
//! ## The law here
//!
//! - Refused by name at construction: an empty name of who vouches, a
//!   missing entity, an entity that is not a model, a model that is not
//!   marked, a model whose chord is RECORDED coarse (#864: a measurement,
//!   never vouched away), and an incomplete model (#1610: walls missing).
//! - The stocks cleared with the model are those copied straight from it
//!   that carry the unknown mark themselves. A copy with a recorded chord
//!   keeps it.
//! - Nothing is regenerated: against an unknown mesh the laws read
//!   untightened and the gate refuses, so the toolpaths that exist are
//!   the ones an exact mesh gives.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError, History};
use crate::model;
use crate::model_fidelity::{self, is_unknown, UNKNOWN_MM};
use crate::model_vouch;
use crate::schema::FidelityVouched;
use crate::stock::StockShape;

#[derive(Debug, Clone)]
pub struct VouchModelFidelity {
    name: String,
    /// The record that lands: who vouched, at the model's revision.
    record: FidelityVouched,
    /// The record the model held before (a word given for older facets).
    prev: Option<FidelityVouched>,
    /// The from-model stock copies whose mark leaves with the model's.
    copies: Vec<String>,
}

impl VouchModelFidelity {
    pub fn new(world: &World, name: &str, by: &str) -> Result<Self, CommandError> {
        let by = model_vouch::checked_by(by).map_err(CommandError::Invalid)?;
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        if !model::is_model(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is not a model - it is a setup's stock or work offset, a setup, a tool, an operation or other CAM data; vouch for the model its mesh came from"
            )));
        }
        let Some(chord) = model_fidelity::chord_of(world, &entity) else {
            return Err(CommandError::Invalid(format!(
                "'{name}' is not marked fidelity unknown: there is nothing to vouch for"
            )));
        };
        if !is_unknown(chord) {
            return Err(CommandError::Invalid(format!(
                "'{name}' has a recorded fidelity ({}); only a mesh of unknown fidelity can be vouched for - re-import it with a larger STEP facet budget, or export the file again with a tighter tolerance",
                model_fidelity::summary(world, chord)
            )));
        }
        if let Some(hole) = crate::mesh_open::hole_of(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' was imported incomplete - {hole}: a mesh with walls missing cannot be vouched for; re-export the model, or import an STL"
            )));
        }
        let copies = model::named_entities(world)
            .into_iter()
            .filter(|(_, e)| matches!(model::stock_def_of(world, e), Some(StockShape::FromModel { source }) if source == name))
            .filter(|(_, e)| model_fidelity::chord_of(world, e).is_some_and(is_unknown))
            .map(|(n, _)| n)
            .collect();
        Ok(VouchModelFidelity {
            name: name.to_string(),
            record: FidelityVouched { by, rev: model::rev_of(world, &entity) },
            prev: model_vouch::record_of(world, &entity),
            copies,
        })
    }

    #[must_use]
    pub fn by(&self) -> &str {
        &self.record.by
    }

    /// The stock copies whose mark leaves with the model's.
    #[must_use]
    pub fn copies(&self) -> &[String] {
        &self.copies
    }

    /// What the history says of this step: the model and who vouched.
    #[must_use]
    pub fn label(&self) -> String {
        format!("Vouch for the mesh of '{}' (by {})", self.name, self.record.by)
    }

    /// Execute as one labelled undo step, so the history names who vouched.
    pub fn run(self, history: &mut History, world: &World) {
        tracing::info!(model = %self.name, by = %self.record.by, copies = ?self.copies, "fidelity vouched");
        history.begin_group(self.label());
        history.execute(world, self);
        history.commit_group();
    }

    /// The model and its copies, as they are now.
    fn marked(&self, world: &World) -> impl Iterator<Item = flecs_ecs::prelude::Entity> {
        std::iter::once(&self.name).chain(&self.copies).filter_map(|n| model::find_by_name(world, n)).collect::<Vec<_>>().into_iter()
    }
}

impl Command for VouchModelFidelity {
    fn apply(&self, world: &World) {
        for e in self.marked(world) {
            model_fidelity::set(world, &e, None);
        }
        if let Some(e) = model::find_by_name(world, &self.name) {
            model_vouch::set(world, &e, Some(self.record.clone()));
        }
    }

    fn invert(&self, world: &World) {
        for e in self.marked(world) {
            model_fidelity::set(world, &e, Some(UNKNOWN_MM));
        }
        if let Some(e) = model::find_by_name(world, &self.name) {
            model_vouch::set(world, &e, self.prev.clone());
        }
    }
}
