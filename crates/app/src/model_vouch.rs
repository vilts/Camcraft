//! Vouched meshes (#1339): the record that a person vouched for a mesh of
//! unknown fidelity (`model_fidelity::UNKNOWN_MM`), so that a model whose
//! source file is gone, or whose mesh is exact (an STL or OBJ import), can
//! post again without a re-import.
//!
//! Provenance: our own design (#1339, a follow-up of #1056). No vendor
//! material consulted.
//!
//! ## The law here
//!
//! - Only `commands::VouchModelFidelity` clears the unknown mark this way,
//!   and only the unknown mark: a recorded coarse chord (#864) is a
//!   measurement and an incomplete mesh (#1610) has walls missing. No
//!   person's word replaces either.
//! - The record names who vouched and the geometry revision the word was
//!   given for. It speaks only while the model still has that revision and
//!   carries no fidelity flag: a later import, update or repair lands
//!   other facets, and a word given for the old ones says nothing of them.
//! - The record is saved with the project (`fidelity_vouched`), so the
//!   model page and `get_job` still say who vouched after a reload.

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::model;
use crate::schema::FidelityVouched;

/// The longest name a vouch records, in characters.
pub const MAX_BY: usize = 120;

/// `by` as the record keeps it: trimmed, one line, not empty, at most
/// [`MAX_BY`] characters. `Err` says what is wrong in the user's words.
pub fn checked_by(by: &str) -> Result<String, String> {
    let by = by.trim();
    if by.is_empty() {
        return Err("say who vouches for the mesh: a name is required".into());
    }
    if by.chars().any(char::is_control) {
        return Err("the name of who vouches must be one line of plain text".into());
    }
    if by.chars().count() > MAX_BY {
        return Err(format!("the name of who vouches must be at most {MAX_BY} characters"));
    }
    Ok(by.to_string())
}

/// The stored record, live or not. For the command's undo and the project
/// writer's own checks; everything a person reads goes through [`by_of`].
#[must_use]
pub fn record_of(world: &World, entity: &Entity) -> Option<FidelityVouched> {
    entity.entity_view(world).try_get::<&FidelityVouched>(|v| v.clone())
}

/// Who vouched for the mesh `entity` holds now; `None` when nobody did, or
/// when the word was given for other facets or the mesh is flagged again.
#[must_use]
pub fn by_of(world: &World, entity: &Entity) -> Option<String> {
    let record = record_of(world, entity)?;
    let live = record.rev == model::rev_of(world, entity) && crate::model_fidelity::chord_of(world, entity).is_none();
    live.then_some(record.by).filter(|by| !by.is_empty())
}

/// Write or clear the record. For `VouchModelFidelity`, its undo and the
/// project loader only.
pub fn set(world: &World, entity: &Entity, record: Option<FidelityVouched>) {
    let view = entity.entity_view(world);
    match record {
        Some(r) => {
            view.set(r);
        }
        None => {
            view.remove(world.component::<FidelityVouched>());
        }
    }
}

/// The plain-words line the model page shows for a vouched mesh.
#[must_use]
pub fn summary(by: &str) -> String {
    format!("Fidelity vouched for by {by}: gouge checks read this mesh as exact. Undo, or a fresh import, ends that.")
}

/// The same facts for MCP and the script API.
#[must_use]
pub fn facts(by: &str) -> serde_json::Value {
    serde_json::json!({ "by": by, "note": summary(by) })
}
