//! Operation geometry pick command: set the feature selection an operation machines.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::feature;
use crate::model;
use crate::operation::{ChainOverride, FeatureSel, GeoSel, OpParams, SrcRef};
use crate::schema::Operation;

/// Set which features an operation machines - the pick list behind
/// [`SrcRef::Selection`]. Unlike a links edit the stamp survives:
/// staleness is derived from the digest, so re-picking the same
/// geometry stays fresh, and a different pick flips the operation
/// Stale on its own. Empty picks are refused - clearing reverts the
/// source with [`EditOperationLinks`] instead.
pub struct SetOperationGeometry {
    name: String,
    prev: Operation,
    next: Operation,
}

impl SetOperationGeometry {
    pub fn new(world: &World, name: &str, items: Vec<GeoSel>) -> Result<Self, CommandError> {
        Self::build(world, name, items, None)
    }

    /// #1864: atomically replace geometry picks and Contour per-chain overrides
    /// as a single undoable command (used by MCP `set_operation_geometry`).
    pub fn with_chain_overrides(
        world: &World,
        name: &str,
        items: Vec<GeoSel>,
        overrides: Vec<ChainOverride>,
    ) -> Result<Self, CommandError> {
        Self::build(world, name, items, Some(overrides))
    }

    fn build(
        world: &World,
        name: &str,
        items: Vec<GeoSel>,
        overrides: Option<Vec<ChainOverride>>,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        if items.is_empty() {
            return Err(CommandError::Invalid(
                "a selection needs at least one feature - revert the source to clear it".into(),
            ));
        }
        // The pick list is the user's: order kept, exact repeats (same
        // entity and feature) collapsed - a doubled ring would be a
        // zero-wide sliver where it bounds.
        let mut deduped: Vec<GeoSel> = Vec::with_capacity(items.len());
        for g in items {
            if !deduped.iter().any(|d| d.entity == g.entity && d.feature == g.feature) {
                deduped.push(g);
            }
        }
        for g in &deduped {
            valid_pick(world, g)?;
        }
        let src = SrcRef::Selection(deduped);
        super::gates::sketch_gate(world, prev.setup.as_deref(), &src, &prev.params)?;
        // A re-pick moves every From: Selection height (M11.3 A): the
        // new picks must answer them, in order, before the swap lands.
        // Params that never read the selection pass exactly as before.
        let mut reads_sel = false;
        prev.params.for_each_height(|h| reads_sel |= h.reference.is_selection());
        if reads_sel {
            super::gates::heights_gate(world, prev.setup.as_deref(), &src, &prev.params)?;
        }
        // A pocket-family pick that bounds no area from above is refused
        // here, at the pick, with the fix - Generate stays the backstop.
        super::gates::area_gate(world, prev.setup.as_deref(), &src, &prev.params)?;
        // A probe's pick seeds its touch (#171) in this same command, so
        // one undo takes back the pick and the seed together; a pick the
        // probe cannot touch is refused here, by name.
        let mut params = match crate::operation::probe_from_picks(world, prev.setup.as_deref(), &prev.params, src_items(&src)) {
            Some(seeded) => seeded.map_err(CommandError::Invalid)?,
            None => prev.params.clone(),
        };
        // #1504: a Slot's picks seed the slot that fills them, in this
        // same command - one undo takes back the pick and the seed.
        params = crate::operation::slot_from_picks(world, prev.setup.as_deref(), &params, src_items(&src)).unwrap_or(params);
        if let Some(ovs) = overrides {
            match &mut params {
                OpParams::Contour { chain_overrides, .. } => {
                    *chain_overrides = ovs;
                    if !params.is_sane() {
                        return Err(CommandError::Invalid(
                            "duplicate or invalid per-chain override in selection".into(),
                        ));
                    }
                }
                _ if !ovs.is_empty() => {
                    return Err(CommandError::Invalid(format!(
                        "per-chain side and direction overrides apply to 2D Contour only, not {}",
                        params.kind()
                    )));
                }
                _ => {}
            }
        }
        if !prev.area.do_not_touch.is_empty()
            && let Some(why) = crate::operation::avoid_overlap_issue(
                world,
                &prev.area.do_not_touch.picks,
                Some(&src),
                &prev.area,
            )
        {
            return Err(CommandError::Invalid(why));
        }
        let next = Operation { src, params, ..prev.clone() };
        Ok(SetOperationGeometry { name: name.to_string(), prev, next })
    }
}

/// The pick list of a selection source.
fn src_items(src: &SrcRef) -> &[GeoSel] {
    match src {
        SrcRef::Selection(items) => items,
        _ => &[],
    }
}

/// One pick, checked against the model as it stands: the entity exists,
/// is solid, has geometry to select from, and the feature id resolves
/// right now (interactive adds never queue a stale pick; ids that
/// vanish later go Stale at read time, not Invalid at load).
fn valid_pick(world: &World, g: &GeoSel) -> Result<(), CommandError> {
    if g.entity.is_empty() {
        return Err(CommandError::Invalid(
            "a selected feature names no entity".into(),
        ));
    }
    let Some(e) = model::find_by_name(world, &g.entity) else {
        return Err(CommandError::NotFound(g.entity.clone()));
    };
    if model::is_wcs(world, &e) {
        return Err(CommandError::Invalid(format!(
            "'{}' is a work offset, not a solid",
            g.entity
        )));
    }
    if let Some(sketch) = crate::sketch::sketch_of(world, &e) {
        let found = match &g.feature {
            FeatureSel::SketchChain(id) => sketch.data.chains().find(*id).is_some(),
            FeatureSel::SketchLayer(layer) => sketch.data.layers().iter().any(|l| l == layer),
            _ => {
                return Err(CommandError::Invalid(format!(
                    "'{}' is a DXF profile - pick a sketch chain or layer, not a {}",
                    g.entity, g.feature
                )));
            }
        };
        if !found {
            return Err(CommandError::Invalid(format!(
                "{} is not in DXF profile '{}'",
                crate::pick::geo_label(world, g),
                g.entity
            )));
        }
        return Ok(());
    }
    let Some(features) = model::features_of(world, &e) else {
        return Err(CommandError::Invalid(format!(
            "'{}' has no geometry to select from",
            g.entity
        )));
    };
    let found = match &g.feature {
        FeatureSel::Face(id) => features.faces.iter().any(|f| f.id == *id),
        FeatureSel::Chain(id) => crate::model::resolve_chain(&features, *id).is_some(),
        FeatureSel::Patch(id) => feature::patches(&features).iter().any(|p| p.id == *id),
        FeatureSel::Vertex(id) => feature::resolve_vertex(&features, *id).is_some(),
        FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_) => {
            return Err(CommandError::Invalid(format!(
                "'{}' is a model, not a DXF profile",
                g.entity
            )));
        }
    };
    if !found {
        return Err(CommandError::Invalid(format!(
            "{} is not in the model",
            crate::pick::geo_label(world, g)
        )));
    }
    Ok(())
}

impl Command for SetOperationGeometry {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}
