//! Model fidelity reports. Provenance: our own cause-aware verdict and unit formatting.

use super::{UNKNOWN_NOTE, is_unknown};
use crate::geometry::import::step;
use flecs_ecs::prelude::World;

/// The plain-words line the model page and the import report show.
#[must_use]
pub fn summary(world: &World, chord_mm: f64) -> String {
    summary_with_cause(world, chord_mm, false)
}

/// The model summary with its recorded import cause.
#[must_use]
pub fn summary_with_cause(world: &World, chord_mm: f64, file_tolerance: bool) -> String {
    if is_unknown(chord_mm) {
        return format!("Fidelity unknown: {UNKNOWN_NOTE}");
    }
    let said = format!(
        "Reduced fidelity: surfaces are approximated to within {}",
        within(world, chord_mm)
    );
    // #2996: a widened model says what its checks allow.
    match crate::fidelity_note::clause(world, chord_mm, file_tolerance) {
        Some(clause) => format!("{said}; {clause}"),
        None => said,
    }
}

/// The same facts for MCP and the script API: the achieved error in mm,
/// the importer's promise, and the sentence a person would be shown.
#[must_use]
pub fn facts(world: &World, chord_mm: f64) -> serde_json::Value {
    facts_with_cause(world, chord_mm, false)
}

/// MCP facts with the same cause used by the verdict.
#[must_use]
pub fn facts_with_cause(world: &World, chord_mm: f64, file_tolerance: bool) -> serde_json::Value {
    if is_unknown(chord_mm) {
        return serde_json::json!({
            "fidelity_unknown": true,
            "promised_mm": step::CHORD_CAP,
            "note": format!("Fidelity unknown: this model was {UNKNOWN_NOTE}. Its operations are refused at the post until then."),
        });
    }
    // #2996: a widened model's checks are looser than normal, not stricter.
    let checks = if crate::fidelity_note::widened(chord_mm, file_tolerance) {
        "A gouge smaller than that is not found, and the posted program says so in its header"
    } else {
        "Gouge checks against this model are stricter; if it is too coarse to check, its operations are refused at the post"
    };
    serde_json::json!({
        "approximation_mm": chord_mm,
        "promised_mm": step::CHORD_CAP,
        "note": format!(
            "{}. {checks}. Re-import with a larger STEP facet budget, or export the file again with a tighter tolerance, to clear this.",
            summary_with_cause(world, chord_mm, file_tolerance)
        ),
    })
}

/// "0.012 mm" in the document's unit.
pub(super) fn within(world: &World, chord_mm: f64) -> String {
    let u = crate::units::of(world);
    format!("{} {}", crate::units::len(chord_mm, u, 3), u.suffix())
}
