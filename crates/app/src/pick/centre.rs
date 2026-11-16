//! Circle and arc centres as feature points (#692): a picker that takes
//! points may also opt in to the centre of a round feature edge
//! ([`super::PickKinds::centres`]). A centre is stored as the round run it
//! is derived from (a `Chain` selection) and read back through #759's
//! `circle_of_run`, so a point slot that gets an edge reads its circle's
//! centre - and refuses a straight or non-circular one by name.
//!
//! Provenance: our own code over this repo's feature runs and #759's
//! circle fit (textbook analytic geometry); generic CAD picking from the
//! issue's spec, no vendor material consulted.

use flecs_ecs::prelude::World;

use crate::model;
use crate::operation::{FeatureSel, GeoSel};

use super::circle::{CircleRefusal, circle_centre_in, circle_of_run};
use super::feature_graph::FeatureGraph;

/// The stored selection a centre pick stands for: the run (one arc or one
/// whole rim) holding the round edge `arc` (an index into
/// [`FeatureGraph::arcs`]), checked circular by `circle_of_run`. `None`
/// when the arc no longer resolves or its run is refused.
#[must_use]
pub fn centre_selection(graph: &FeatureGraph, arc: usize) -> Option<FeatureSel> {
    let edge = *graph.arcs.get(arc)?.edges.first()?;
    let run = graph.runs().iter().find(|r| r.edges.contains(&edge))?;
    circle_of_run(&graph.features, run).ok()?;
    Some(FeatureSel::Chain(run.id))
}

/// The centre of a round edge pick, in the DOCUMENT frame (as modelled):
/// what a point slot (three points, an axis by two points) reads for an
/// edge. A face, a vertex, a straight edge or a curve that is no circle
/// is refused by its [`CircleRefusal`].
pub fn centre_of(world: &World, g: &GeoSel) -> Result<[f64; 3], CircleRefusal> {
    circle_centre_in(world, g, model::transform_of).map(|c| c.centre)
}

/// [`centre_of`] for a point slot, refusing in the slot's words: an edge
/// that gives no centre is named with the reason.
pub fn centre_point(world: &World, g: &GeoSel) -> Result<[f64; 3], String> {
    centre_of(world, g).map_err(|why| match why {
        CircleRefusal::Gone(_) | CircleRefusal::NotAnEdge(_) => why.to_string(),
        _ => format!(
            "{} of '{}' is an edge with no centre ({why}) - a point is a vertex or a circular edge's centre",
            g.feature, g.entity
        ),
    })
}
