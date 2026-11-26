//! The closed solids of a model (#917): the STEP reader gives each solid
//! of a file its own index range of the facets, so two solids that touch
//! along a wall line are read as two solids, not as one non-manifold
//! soup. The ranges ride the facets they were landed with.
//!
//! Provenance: our own design (#917). Two closed shells that share a wall
//! are a union of solids (ISO 10303-42 MANIFOLD_SOLID_BREP, one per
//! solid); the readers that slice them use the ranges to section each
//! solid alone and unite the sections. No vendor material consulted.
//!
//! ## The law here
//!
//! - Only the commands that land facets on a model (import, update,
//!   delete undo) and the project loader write the ranges, after the
//!   facets and their revision are landed.
//! - The ranges are read only at the revision and the facet count they
//!   were landed with. Any other landing (a transform, a repair, a copy)
//!   makes them stale, and every reader then reads the plain soup exactly
//!   as before #917 - a stale range never speaks for other facets.
//! - Ranges that do not tile the facets (a gap, an overlap, out of
//!   bounds) are never stored: they are dropped with a logged note.

use std::ops::Range;

use flecs_ecs::core::EntityViewGet;
use flecs_ecs::prelude::{Entity, World};

use crate::model;
use crate::schema::SolidRanges;

/// True when `ranges` tile `0..len` in order, each non-empty, and there
/// are two or more (one solid is the plain read). Checked: no overflow,
/// no index past `len`.
#[must_use]
pub fn tiles(ranges: &[Range<usize>], len: usize) -> bool {
    let mut at = 0_usize;
    for r in ranges {
        if r.start != at || r.end <= r.start || r.end > len {
            return false;
        }
        at = r.end;
    }
    ranges.len() >= 2 && at == len
}

/// Land or clear the solids of `entity`'s facets. Call it after
/// `model::set_facets` and any `model::set_rev`: the ranges record the
/// revision and the facet count that stand now. Ranges that do not tile
/// the facets are dropped with a logged note; fewer than two clear it.
pub fn set(world: &World, entity: &Entity, ranges: &[Range<usize>]) {
    let view = entity.entity_view(world);
    let len = model::facets_of(world, entity).map_or(0, |f| f.len());
    if ranges.len() >= 2 && tiles(ranges, len) {
        view.set(SolidRanges { rev: model::rev_of(world, entity), len, ranges: ranges.into() });
        return;
    }
    if ranges.len() >= 2 {
        tracing::warn!(
            "model solids: {} ranges do not tile its {len} facets; the model is read as one soup",
            ranges.len()
        );
    }
    view.remove(world.component::<SolidRanges>());
}

/// The solids of `entity`'s facets as they stand now, or none: empty when
/// none were landed, or when other facets landed since (the revision or
/// the facet count moved).
#[must_use]
pub fn of(world: &World, entity: &Entity) -> Vec<Range<usize>> {
    let rev = model::rev_of(world, entity);
    let len = model::facets_of(world, entity).map_or(0, |f| f.len());
    entity
        .entity_view(world)
        .try_get::<&SolidRanges>(|s| {
            (s.rev == rev && s.len == len && tiles(&s.ranges, len)).then(|| s.ranges.to_vec())
        })
        .flatten()
        .unwrap_or_default()
}

/// The project file's form: `[start, end]` pairs, or none for one solid.
#[must_use]
pub fn to_wire(ranges: &[Range<usize>]) -> Option<Vec<[usize; 2]>> {
    (ranges.len() >= 2).then(|| ranges.iter().map(|r| [r.start, r.end]).collect())
}

/// Read the project file's pairs back. Pairs that do not tile `len`
/// facets are dropped with a logged note naming `name` (never trusted).
#[must_use]
pub fn from_wire(pairs: Option<&[[usize; 2]]>, len: usize, name: &str) -> Vec<Range<usize>> {
    let Some(pairs) = pairs else { return Vec::new() };
    let ranges: Vec<Range<usize>> = pairs.iter().map(|p| p[0]..p[1]).collect();
    if tiles(&ranges, len) {
        return ranges;
    }
    tracing::warn!(
        "project: model '{name}' has {} solid ranges that do not tile its {len} facets; they are dropped and the model is read as one soup",
        pairs.len()
    );
    Vec::new()
}
