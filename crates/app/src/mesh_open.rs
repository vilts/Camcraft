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
//!   the model carries the hole itself, beside the sentinel it carries
//!   (#1700): the copy's refusal names its source and that hole, also once
//!   the source is deleted - through undo, setup clones and save/load.
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
/// the refusal rides with the facets. For the commands that land facets
/// on a model or a stock copied from one (import, update, delete, set
/// stock, setup clone and delete), their undo and the project loader
/// only; call it after `model_fidelity::set`.
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

/// The hole a stock copied from `source` lands with (#1700): the one
/// `source` carries - its own when it is an incomplete model, the one it
/// was copied with when it is itself a copy. `model_fidelity::chord_behind`
/// is the sentinel's twin.
#[must_use]
pub fn hole_behind(world: &World, source: &str) -> Option<String> {
    model::find_by_name(world, source).and_then(|e| hole_of(world, &e))
}

/// What a refusal says follows "model 'X' was" for a mesh of unknown
/// fidelity: the hole of the model named `name` when it was imported
/// incomplete, else the pre-v59 note.
#[must_use]
pub fn unknown_note(world: &World, name: &str) -> String {
    note_for(hole_behind(world, name).as_deref())
}

/// The same words for a hole already in hand (#1700): a stock copy's own,
/// when the model it names is gone.
#[must_use]
pub fn note_for(hole: Option<&str>) -> String {
    match hole {
        Some(hole) => incomplete(hole),
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
        None => crate::model_fidelity::summary_with_cause(world, chord_mm, crate::model_fidelity::file_tolerance_of(world, entity)),
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
        None => crate::model_fidelity::facts_with_cause(world, chord_mm, crate::model_fidelity::file_tolerance_of(world, entity)),
    }
}
