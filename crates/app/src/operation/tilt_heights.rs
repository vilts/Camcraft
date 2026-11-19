//! The 3+2 height fold (#2389): a tilted operation's reference heights
//! measured along its turned tool axis, not as the setup's world Z.
//!
//! Provenance: rigid-frame projection of the referenced geometry onto
//! the tool axis; our own frame doctrine (#763), no vendor material.
//!
//! The generation fold is `h - origin.z` (`run_job`, `source_region`):
//! a Fixed height keeps that law (the number is spoken in the setup's Z,
//! as the tilt suites pin). A reference height (stock, model, work
//! offset) is a place on the geometry, and its distance from the plane
//! is read along the tool axis: the highest (lowest) point of the
//! referenced geometry in the local frame, handed back as
//! `z_local + origin.z` so the fold's `- origin.z` lands exactly on the
//! local value. An upright frame (no tilt, or A0 B0) reads the setup
//! resolve verbatim: every prismatic path keeps its bits.

use flecs_ecs::prelude::World;

use super::*;
use crate::cl::TiltedFrame;

/// One height in the fold's coordinate (see the module doc). `frame` is
/// the op's checked tilt frame; None or upright = [`resolve_height`].
/// A Selection reference keeps the setup resolve: a tilted op refuses a
/// picked source by name before any height is used.
#[must_use]
pub fn fold_height(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    frame: Option<&TiltedFrame>,
    h: Height,
) -> Option<f64> {
    let Some(f) = frame.filter(|f| !f.is_upright()) else {
        return resolve_height(world, setup, src, h);
    };
    let span = |facets: &[Triangle]| -> Option<(f64, f64)> {
        let mut it = facets.iter().flat_map(|t| [t.a, t.b, t.c]).map(|p| f.z_local(p));
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), z| (lo.min(z), hi.max(z))))
    };
    let base = match h.reference {
        HeightReference::Fixed
        | HeightReference::TopOfSelection
        | HeightReference::BottomOfSelection => return resolve_height(world, setup, src, h),
        HeightReference::TopOfModel | HeightReference::BottomOfModel => {
            let (lo, hi) = span(&crate::setup_align::parts_facets_in(world, setup))?;
            if matches!(h.reference, HeightReference::TopOfModel) { hi } else { lo }
        }
        HeightReference::TopOfStock | HeightReference::BottomOfStock => {
            let name = crate::setup::stock_entity_name(world, setup);
            let entity = model::find_by_name(world, &name)?;
            if !model::is_stock(world, &entity) {
                return None;
            }
            let (lo, hi) = span(&model::world_facets(world, &entity)?)?;
            if matches!(h.reference, HeightReference::TopOfStock) { hi } else { lo }
        }
        HeightReference::WorkOffset => {
            let entity = crate::setup::wcs_entity(world, setup)?;
            f.z_local(model::transform_of(world, &entity).unwrap_or_default().pos)
        }
    };
    let z = base + f.origin.z + h.offset;
    z.is_finite().then_some(z)
}

/// Every height of `params` in the fold's coordinate; the refusals are
/// [`resolve_params`]'s, word for word.
pub fn resolve_params_folded(
    world: &World,
    setup: Option<&str>,
    src: Option<&SrcRef>,
    params: &OpParams,
    frame: Option<&TiltedFrame>,
) -> Result<ResolvedHeights, String> {
    resolve_params_with(world, setup, src, params, &|h| fold_height(world, setup, src, frame, h))
}
