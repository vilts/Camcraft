//! The virtual model reference of a setup (#1862): what stands in for a
//! model when a setup has a stock and a placed DXF profile and no model.
//!
//! Provenance: our own design, from the maintainer's words on issue 1862
//! ("a virtual model reference, just so the operations pages gets happy
//! about a model being loaded"). `docs/decisions/1862-sketch-profile.md`
//! records it. No vendor material consulted.
//!
//! ## The laws here
//!
//! - **Derived, never stored.** The reference is read from the world
//!   ([`of`]): the setup has one or more placed profiles and the document
//!   has no part. No component and no file key holds it, so it cannot
//!   disagree with what the document holds.
//! - **No facets.** The reference names profiles, and a profile has no
//!   mesh. No 3D solid tool reads it.
//! - **A profile is a source for the 2D cut kinds only** ([`admits`]): the
//!   2D milling, plasma and wire EDM sections. Every other kind needs a
//!   solid model and refuses a profile by name ([`source_problem`]).
//! - **A 3D operation refuses a virtual reference by name.** A 3D or
//!   multi-axis milling kind in a setup whose model is virtual has no
//!   solid to cut to.
//! - **Nothing passes silently.** An operation on a profile states what
//!   it is checked against ([`checked_against`]): the stock and the
//!   profile's chains. Kinds that cannot yet machine those chains refuse
//!   generation by name ([`generation_problem`]).

use flecs_ecs::prelude::World;

use crate::operation::{OpParams, Section, SrcRef};
use crate::schema::Operation;
use crate::sketch;

/// Why a Top of Model or Bottom of Model height has no value in a setup
/// whose model is virtual (`operation::missing_source`).
pub const NO_SOLID_HEIGHT: &str =
    "the model is a DXF profile, a virtual model reference with no solid - measure this height from the stock, or fix it";

/// The virtual model reference of a setup: the profiles that stand in
/// for its model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualModel {
    /// The effective setup (`None` in a document without setups).
    pub setup: Option<String>,
    /// The placed profiles, name-sorted; never empty.
    pub profiles: Vec<String>,
}

/// Does the document hold a part: a model that is not a fixture?
#[must_use]
pub fn has_part(world: &World) -> bool {
    crate::model::model_entities(world).iter().any(|(_, e)| !crate::model::is_fixture(world, e))
}

/// The setup's virtual model reference: `Some` when the setup has one or
/// more placed profiles and the document has no part.
#[must_use]
pub fn of(world: &World, declared: Option<&str>) -> Option<VirtualModel> {
    let profiles = sketch::placed_in(world, declared);
    (!profiles.is_empty() && !has_part(world))
        .then(|| VirtualModel { setup: crate::setup::effective(world, declared), profiles })
}

/// Does a setup have a model for the operation pages: a part, or the
/// virtual reference?
#[must_use]
pub fn model_loaded(world: &World, declared: Option<&str>) -> bool {
    has_part(world) || of(world, declared).is_some()
}

/// Can this kind take a profile as its source? The 2D cut kinds only.
#[must_use]
pub fn admits(params: &OpParams) -> bool {
    matches!(params.kind().info().section, Section::Mill2D | Section::Plasma | Section::WireEdm | Section::Waterjet)
}

/// Does this kind cut to a solid model's surface in 3D?
fn cuts_a_solid(params: &OpParams) -> bool {
    matches!(params.kind().info().section, Section::Mill3D | Section::MultiAxis)
}

/// The profiles a source names: a whole profile, or picks on one.
#[must_use]
pub fn source_profiles(world: &World, src: &SrcRef) -> Vec<String> {
    let names: Vec<&str> = match src {
        SrcRef::Stock => Vec::new(),
        SrcRef::Entity(name) => vec![name.as_str()],
        SrcRef::Selection(items) => items.iter().map(|g| g.entity.as_str()).collect(),
    };
    let mut out: Vec<String> = names.into_iter().filter(|n| sketch::find(world, n).is_some()).map(str::to_owned).collect();
    out.dedup();
    out
}

/// Why this operation cannot stand on this source, by name; `None` when
/// the profile law has nothing against it. The command doors and the
/// generation door read this one function.
#[must_use]
pub fn source_problem(world: &World, declared: Option<&str>, src: &SrcRef, params: &OpParams) -> Option<String> {
    let kind = params.kind();
    let profiles = source_profiles(world, src);
    if !profiles.is_empty() {
        if let SrcRef::Selection(items) = src {
            for g in items {
                if sketch::find(world, &g.entity).is_none() {
                    return Some(format!(
                        "'{}' is a DXF profile and cannot be combined with model features in '{}' in one operation",
                        profiles[0], g.entity
                    ));
                }
            }
            if matches!(params, OpParams::Trace { .. } | OpParams::Engrave { .. })
                && let Some(g) = items.iter().find(|g| !matches!(g.feature,
                    crate::operation::FeatureSel::SketchChain(_) | crate::operation::FeatureSel::SketchLayer(_)))
            {
                return Some(format!("'{}' is a DXF profile - pick a sketch chain or layer to mark", g.entity));
            }
            if matches!(params, OpParams::WireContour(_)) {
                if let Err(why) = crate::operation::wire_sketch::pick(world, src) {
                    return Some(why);
                }
            } else if !matches!(
                params,
                OpParams::Contour { .. } | OpParams::Slot { .. } | OpParams::TrochoidalSlot { .. }
                    | OpParams::Engrave { .. } | OpParams::Trace { .. }
            ) {
                return Some(format!(
                    "'{}' is a DXF profile - an operation takes the whole profile as its source; picked chains of a profile are not supported yet",
                    profiles[0]
                ));
            }
        }
        if !admits(params) {
            return Some(format!(
                "a {kind} operation needs a solid model - '{}' is a DXF profile, a virtual model reference with no solid",
                profiles[0]
            ));
        }
        if let Some(t) = params.tilt() {
            let _ = t;
            return Some(format!(
                "a 3+2 tilted operation is not supported on DXF profile '{}' - reset orientation to World",
                profiles[0]
            ));
        }
        for name in profiles {
            let home = sketch::find(world, &name).and_then(|(_, s)| crate::setup::effective(world, s.setup.as_deref()));
            if home != crate::setup::effective(world, declared) {
                return Some(format!(
                    "'{name}' is a DXF profile on the stock of {} - an operation cuts the profiles of its own setup only",
                    home.map_or_else(|| "another setup".to_owned(), |s| format!("setup '{s}'"))
                ));
            }
        }
    }
    if cuts_a_solid(params)
        && let Some(v) = of(world, declared)
    {
        return Some(format!(
            "a {kind} operation needs a solid model - the setup has only its stock and the DXF profile '{}', a virtual model reference with no solid",
            v.profiles.first().map_or("", String::as_str)
        ));
    }
    None
}

/// What an operation on a profile is checked against, as one sentence;
/// `None` for an operation whose source is not a profile.
#[must_use]
pub fn checked_against(world: &World, op: &Operation) -> Option<String> {
    if crate::operation::wire_sketch::selected(&op.src, &op.params) {
        let (name, chain) = crate::operation::wire_sketch::pick(world, &op.src).ok()?;
        return Some(format!("checked against the stock and selected DXF chain {} in '{name}' through the stock height", chain.id));
    }
    let profiles = source_profiles(world, &op.src);
    let name = profiles.first()?;
    let (_, profile) = sketch::find(world, name)?;
    let chains = profile.data.chains();
    Some(format!(
        "checked against the stock '{}' and the chains of the DXF profile '{name}' ({} closed, {} open) - there is no solid model",
        crate::setup::stock_entity_name(world, op.setup.as_deref()),
        chains.closed.len(),
        chains.open.len(),
    ))
}

/// The generation door: an operation the profile law refuses, or a non-Contour
/// operation on a profile whose generator has not shipped yet, is refused by
/// name. `None` for every other operation.
#[must_use]
pub fn generation_problem(world: &World, op: &Operation) -> Option<String> {
    if let Some(why) = source_problem(world, op.setup.as_deref(), &op.src, &op.params) {
        return Some(why);
    }
    if matches!(op.params, OpParams::Contour { .. } | OpParams::Engrave { .. } | OpParams::Trace { .. }) || crate::operation::wire_sketch::selected(&op.src, &op.params) {
        return None;
    }
    // #1868: a slot kind reads one picked chain of a profile; the whole
    // profile names no slot.
    if matches!(op.params, OpParams::Slot { .. } | OpParams::TrochoidalSlot { .. }) {
        if matches!(op.src, SrcRef::Selection(_)) {
            return None;
        }
        return source_profiles(world, &op.src).first().map(|name| {
            format!(
                "a {} operation on the DXF profile '{name}' needs one picked chain - pick one closed slot outline or one open centerline",
                op.params.kind()
            )
        });
    }
    checked_against(world, op)
        .map(|what| format!("this operation is {what}; a toolpath from the chains of a DXF profile is not generated yet"))
}
