//! What a model checked at reduced accuracy says (#2996): the plain-words
//! clause the model page and MCP add, and the header notes every posted
//! program of such a document carries.
//!
//! Provenance: our own design (maintainer decision on #2996, 2026-11-20).
//! The law itself is `verify::excess_floor`; this module only says what
//! it did. No vendor material consulted.
//!
//! A model is WIDENED when its kept chord error passes `DEV_TOL` and stays
//! inside `verify::WIDE_CHORD_LIMIT`, and only the file tolerance caused
//! it: its true-surface bound is the chord plus the mesh allowance.
//! A model of unknown fidelity, or one past
//! the limit, is refused by name instead and says nothing here.

use flecs_ecs::prelude::World;

use crate::model;
use crate::model_fidelity::{chord_of, is_unknown};
use crate::stock::StockShape;
use crate::stock_sim::DEV_TOL;
use crate::verify::WIDE_CHORD_LIMIT;

/// Whether a recorded chord error widens the verdict tolerance.
#[must_use]
pub fn widened(chord_mm: f64, file_tolerance: bool) -> bool {
    file_tolerance && chord_mm.is_finite() && !is_unknown(chord_mm) && chord_mm > DEV_TOL + 1e-12 && chord_mm <= WIDE_CHORD_LIMIT + 1e-9
}

/// "checks on this model allow 0.024 mm, not 0.010 mm" for a widened
/// chord, in the document's unit; `None` for any other chord.
#[must_use]
pub fn clause(world: &World, chord_mm: f64, file_tolerance: bool) -> Option<String> {
    widened(chord_mm, file_tolerance).then(|| format!("checks on this model allow {}, not {}", len_bound(world, surface_bound(chord_mm)), len(world, DEV_TOL)))
}

/// One header line for each widened part of the document: a model, or
/// a stock copied from one (named by its source, which a person
/// re-imports). Fixtures are judged by a grown box, never by this
/// tolerance, and say nothing. Sorted and without repeats.
#[must_use]
pub fn program_notes(world: &World) -> Vec<String> {
    let mut lines: Vec<String> = model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| !model::is_fixture(world, e))
        .filter_map(|(name, e)| {
            let chord = chord_of(world, &e).filter(|c| widened(*c, crate::model_fidelity::file_tolerance_of(world, &e)))?;
            let shown = match model::stock_def_of(world, &e) {
                Some(StockShape::FromModel { source }) => source,
                _ => name,
            };
            Some(format!(
                "Reduced accuracy: model {shown} was checked to {}, not {}",
                len_bound(world, surface_bound(chord)),
                len(world, DEV_TOL)
            ))
        })
        .collect();
    lines.sort();
    lines.dedup();
    lines
}

/// The largest cut into the true surface that can pass the mesh check.
#[must_use]
pub fn surface_bound(chord_mm: f64) -> f64 {
    chord_mm + crate::verify::excess_floor_for(DEV_TOL, crate::model_fidelity::excess(chord_mm), true).unwrap_or(0.0)
}

/// Round a safety bound upward at the document's existing display precision.
fn len_bound(world: &World, mm: f64) -> String {
    let unit = crate::units::of(world);
    let (precision, scale) = match unit {
        crate::units::Unit::Millimeters => (3, 1_000.0),
        crate::units::Unit::Inches => (5, 100_000.0),
    };
    let bound = (crate::units::to_display(mm, unit) * scale).ceil() / scale;
    format!("{bound:.precision$} {}", unit.suffix())
}

/// "0.019 mm" in the document's unit.
fn len(world: &World, mm: f64) -> String {
    let u = crate::units::of(world);
    format!("{} {}", crate::units::len(mm, u, 3), u.suffix())
}
