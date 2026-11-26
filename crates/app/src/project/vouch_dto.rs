//! The file mirror of a vouched mesh (#1339): who vouched for a model of
//! unknown fidelity, and the geometry revision the word was given for.
//! Absent is "nobody vouched". The key needs no version step: an old
//! reader that drops it loads the same unmarked model and loses only the
//! name, as with `mesh_open`.
//! Provenance: our own DTO, the pattern of the other entity keys.

use flecs_ecs::prelude::{Entity, World};
use serde::{Deserialize, Serialize};

use crate::schema::FidelityVouched;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct VouchDto {
    pub by: String,
    pub rev: u64,
}

impl VouchDto {
    /// The row of a model whose record still speaks for its facets; a
    /// stale record (other facets, or a flag again) is not written.
    #[must_use]
    pub fn of(world: &World, entity: &Entity) -> Option<Self> {
        let by = crate::model_vouch::by_of(world, entity)?;
        Some(VouchDto { by, rev: crate::model::rev_of(world, entity) })
    }

    #[must_use]
    pub fn to_record(&self) -> FidelityVouched {
        FidelityVouched { by: self.by.clone(), rev: self.rev }
    }

    /// A vouch row is a model's, names who vouched, and never rides beside
    /// a fidelity flag: a mesh cannot be both vouched for and flagged.
    pub fn check(&self, owner: &str, is_model: bool, flagged: bool) -> Result<(), String> {
        if !is_model {
            return Err(format!("'{owner}' records a fidelity vouch but is not a model"));
        }
        if flagged {
            return Err(format!("'{owner}' records a fidelity vouch and a fidelity flag at once"));
        }
        match crate::model_vouch::checked_by(&self.by) {
            Ok(by) if by == self.by => Ok(()),
            _ => Err(format!("'{owner}' records a fidelity vouch with no usable name")),
        }
    }
}
