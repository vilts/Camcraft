//! From a resolved pick to the selection the pages store (#692 step 2):
//! a feature point is a `Vertex`, a welded face its `Face`/`Patch` key,
//! and a feature edge the `Chain` the asking page takes - the tangent
//! chain, the run (one straight stretch or one arc) or the one segment.
//! Plus the one rule read backwards, for picks made by id (MCP): a stored
//! vertex or chain is pickable only if a click could have landed on it.
//!
//! Provenance: our own code over this repo's `geometry::feature` chains
//! and runs (M1.5 / M11.5 design); no vendor material consulted.

use std::sync::Arc;

use flecs_ecs::prelude::World;

use crate::geometry::feature::{self, Chain};
use crate::model;
use crate::operation::{FeatureSel, GeoSel};

use super::feature_graph::{FeatureGraph, feature_graph_shared};
use super::feature_pick::{FeaturePick, PickQuery, PickTarget, pick_feature};

/// What an edge pick selects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EdgeUnit {
    /// The tangent-continuous chain the edge lies on (operation geometry).
    #[default]
    Chain,
    /// The run: one straight stretch or one arc (a boundary, an axis).
    Run,
    /// That one segment alone (the Alt pick).
    Segment,
}

/// The feature graph of the named entity's mounted facets (shared cache:
/// a pointer compare on a hit).
#[must_use]
pub fn graph_of(world: &World, entity: &str) -> Option<Arc<FeatureGraph>> {
    let e = model::find_by_name(world, entity)?;
    let tris = model::facets_of(world, &e)?;
    Some(feature_graph_shared(&tris))
}

fn edge_selection(g: &FeatureGraph, edge: u32, unit: EdgeUnit) -> Option<FeatureSel> {
    let has = |c: &&Chain| c.edges.contains(&edge);
    let id = match unit {
        EdgeUnit::Segment => feature::singleton_chain_id(&g.features, edge)?,
        EdgeUnit::Run => g.runs().iter().find(has)?.id,
        EdgeUnit::Chain => g.chains().iter().find(has)?.id,
    };
    Some(FeatureSel::Chain(id))
}

/// The selection a pick stands for on `graph` (the picked entity's).
/// A centre (#692) stores the round run it is derived from; `None` for a
/// centre whose run no longer reads as a circle, or a face the topology
/// could not name.
#[must_use]
pub fn selection_of(graph: &FeatureGraph, pick: &FeaturePick, unit: EdgeUnit) -> Option<FeatureSel> {
    match &pick.target {
        PickTarget::Point(p) => Some(FeatureSel::Vertex(p.id)),
        PickTarget::Face { sel, .. } => *sel,
        PickTarget::Edge { edge } => edge_selection(graph, *edge, unit),
        PickTarget::Centre { arc } => super::centre::centre_selection(graph, *arc),
    }
}

/// The one pick over the world, resolved to its stored selection: the
/// pick (for the highlight) and what a page would store for it.
#[must_use]
pub fn pick_selection(
    world: &World,
    q: &PickQuery,
    unit: EdgeUnit,
    skip: impl Fn(&str) -> bool,
) -> Option<(FeaturePick, Option<FeatureSel>)> {
    let pick = pick_feature(world, q, skip)?;
    let sel = graph_of(world, &pick.entity).and_then(|g| selection_of(&g, &pick, unit));
    Some((pick, sel))
}

/// Does every edge of `chain` lie on a feature edge of `graph`?
#[must_use]
pub fn is_feature_chain(graph: &FeatureGraph, chain: &Chain) -> bool {
    !chain.edges.is_empty() && chain.edges.iter().all(|e| graph.edges.binary_search(e).is_ok())
}

/// The model's tangent chains a click can land on: those made of feature
/// edges only (never the facet seams of a smooth wall).
#[must_use]
pub fn feature_chains(graph: &FeatureGraph) -> Vec<Chain> {
    graph.chains().iter().filter(|c| is_feature_chain(graph, c)).cloned().collect()
}

/// The one rule read backwards, for a pick made by id: a vertex must be a
/// feature point and a chain must run along feature edges. Faces and
/// welded walls always qualify. An id the model no longer holds is left
/// to the command's own stale-pick check. Err is a reason the user (or
/// agent) can act on.
pub fn check_pickable(world: &World, g: &GeoSel) -> Result<(), String> {
    let Some(graph) = graph_of(world, &g.entity) else { return Ok(()) };
    match g.feature {
        FeatureSel::Vertex(id) => {
            let known = feature::resolve_vertex(&graph.features, id).is_some();
            if !known || graph.points.iter().any(|p| p.id == id) {
                return Ok(());
            }
            Err(format!(
                "{} of '{}' is not a feature point - a point is a corner where model edges meet or the end \
                 of an open edge, never a vertex along a smooth edge or inside a face",
                g.feature, g.entity
            ))
        }
        FeatureSel::Chain(id) => match feature::resolve_chain(&graph.features, id) {
            Some(c) if !is_feature_chain(&graph, &c) => Err(format!(
                "{} of '{}' runs along the mesh's own triangles, not a model edge - pick an edge where two \
                 faces meet",
                g.feature, g.entity
            )),
            _ => Ok(()),
        },
        FeatureSel::Face(_) | FeatureSel::Patch(_) => Ok(()),
    }
}
