//! Incomplete models (#1610): a STEP closed shell that meshes open lands
//! as a model with its hole named, instead of being refused at import, so
//! it can be viewed and set up - and nothing is posted against it.
//!
//! Provenance: our own design (#1610, from the #1576 review). A mesh with
//! a wall missing has no inside and outside there, so no gouge or
//! collision verdict read off it can be trusted. No vendor material
//! consulted.
//!
//! ## The law here
//!
//! - The model rides the fidelity-unknown sentinel
//!   (`model_fidelity::UNKNOWN_MM`): every path that carries a chord
//!   (copies, clones, undo, save) carries the refusal with it, and every
//!   operation judged against it and the program verdict are refused by
//!   name at the post (`model_fidelity::gate_problems`); the program sim
//!   judges nothing (`model_fidelity::program_tol`).
//! - Its hole is kept beside the sentinel ([`MeshOpen`]) and saved with
//!   the project, so each refusal and the model page name the hole, not
//!   "saved before CamCraft recorded mesh fidelity". A stock copied from
//!   the model is refused naming its source, whose hole it reads by name.
//! - A hole is read only beside the sentinel, and every command that
//!   lands facets on a model lands their hole (or none) with them, so a
//!   stale hole never speaks for a whole mesh.

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::model;
use crate::model_fidelity::{chord_of, is_unknown, UNKNOWN_MM, UNKNOWN_NOTE};
use crate::schema::MeshOpen;

/// What the advice says after a hole: the mesh cannot be fixed in place.
const ADVICE: &str = "re-export the model, or import an STL";

/// The hole of `entity`'s mesh, when it was imported incomplete.
#[must_use]
pub fn hole_of(world: &World, entity: &Entity) -> Option<String> {
    if !chord_of(world, entity).is_some_and(is_unknown) {
        return None;
    }
    entity.entity_view(world).try_get::<&MeshOpen>(|m| m.hole.clone()).filter(|h| !h.is_empty())
}

/// Land or clear a hole. `Some` also marks the mesh fidelity unknown, so
/// the refusal rides with the facets. For the import, update and delete
/// commands, their undo and the project loader only; call it after
/// `model_fidelity::set`.
pub fn set(world: &World, entity: &Entity, hole: Option<&str>) {
    let view = entity.entity_view(world);
    match hole.filter(|h| !h.is_empty()) {
        Some(h) => {
            crate::model_fidelity::set(world, entity, Some(UNKNOWN_MM));
            view.set(MeshOpen { hole: h.to_string() });
        }
        None => {
            view.remove(world.component::<MeshOpen>());
        }
    }
}

/// What a refusal says follows "model 'X' was" for a mesh of unknown
/// fidelity: the hole of the model named `name` when it was imported
/// incomplete, else the pre-v59 note.
#[must_use]
pub fn unknown_note(world: &World, name: &str) -> String {
    match model::find_by_name(world, name).and_then(|e| hole_of(world, &e)) {
        Some(hole) => incomplete(&hole),
        None => UNKNOWN_NOTE.to_string(),
    }
}

fn incomplete(hole: &str) -> String {
    format!(
        "imported incomplete - {hole} - so it has walls missing and no gouge check against it can be trusted; {ADVICE}"
    )
}

/// The plain-words line the model page and the import status show for a
/// model with a fidelity record: its hole when it has one, else
/// `model_fidelity::summary`.
#[must_use]
pub fn summary(world: &World, entity: &Entity, chord_mm: f64) -> String {
    match hole_of(world, entity) {
        Some(hole) => format!("Incomplete: {hole}. Its operations are refused at the post; {ADVICE}."),
        None => crate::model_fidelity::summary(world, chord_mm),
    }
}

/// The same facts for MCP and the script API: `incomplete` and the hole
/// when it has one, else `model_fidelity::facts`.
#[must_use]
pub fn facts(world: &World, entity: &Entity, chord_mm: f64) -> serde_json::Value {
    match hole_of(world, entity) {
        Some(hole) => serde_json::json!({
            "fidelity_unknown": true,
            "incomplete": true,
            "hole": hole,
            "note": summary(world, entity, chord_mm),
        }),
        None => crate::model_fidelity::facts(world, chord_mm),
    }
}
