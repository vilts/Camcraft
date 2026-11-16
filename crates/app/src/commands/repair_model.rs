//! RepairModel (M13.11, the M1.1 "Mesh repair" leftover): weld a model's
//! facets within a tolerance and close its boundary loops, on the user's
//! say-so only - never on import (Prime Directive: a repair that moves
//! geometry silently is a wrong answer that looks right). One undo unit:
//! the repaired facets land through `UpdateModel`, so undo restores the
//! exact previous facets and revision, and a `FromModel` stock copy
//! follows the propagation law in the same step.
//!
//! Provenance: our own command pattern over `geometry::repair` (public
//! algorithms, cited there). No HSMWorks material consulted.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError, UpdateModel};
use crate::geo2d::GeoError;
use crate::geometry::repair::{self, RepairReport};
use crate::geometry::Mesh;
use crate::model;

pub struct RepairModel {
    pub name: String,
    pub weld_tol: f64,
    pub max_hole_edges: usize,
    inner: UpdateModel,
    report: RepairReport,
}

impl RepairModel {
    /// Runs the repair now (construction time) so the history only ever
    /// holds a repair that succeeded. Refusals, by name: a missing or
    /// non-model entity, a model without geometry, a non-finite or
    /// negative tolerance, a budget under 3 edges, every `geometry::repair`
    /// refusal (non-manifold, non-planar, over budget, ...), and a model
    /// with nothing to repair - that last one is a refusal, not a no-op
    /// command, so the history never records a change that was not one.
    pub fn new(
        world: &World,
        name: &str,
        weld_tol: f64,
        max_hole_edges: usize,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        if !model::is_model(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is not a model - it is a setup's stock or work offset, a setup, a tool, an operation or other CAM data"
            )));
        }
        let Some(facets) = model::facets_of(world, &entity) else {
            return Err(CommandError::Invalid(format!("'{name}' has no geometry to repair")));
        };
        if !weld_tol.is_finite() || weld_tol < 0.0 {
            return Err(CommandError::Invalid(format!(
                "weld tolerance must be a finite, non-negative length (got {weld_tol})"
            )));
        }
        if max_hole_edges < 3 {
            return Err(CommandError::Invalid(format!(
                "largest hole to fill must be at least 3 edges (got {max_hole_edges})"
            )));
        }
        let mesh = Mesh::from_shared(facets);
        let named = |e: GeoError| CommandError::Invalid(format!("'{name}': {e}"));
        let welded = repair::weld(&mesh, weld_tol).map_err(named)?;
        let filled = repair::fill_holes(&welded.mesh, max_hole_edges).map_err(named)?;
        let report = welded.then(filled);
        if !report.changed() {
            return Err(CommandError::Invalid(format!(
                "nothing to repair in '{name}': {} vertices, {} triangles, no open edges",
                report.vertices_after, report.triangles_after
            )));
        }
        let inner = UpdateModel::new(world, name, report.mesh.clone())?.keeping_fidelity();
        Ok(RepairModel {
            name: name.to_string(),
            weld_tol,
            max_hole_edges,
            inner,
            report,
        })
    }

    #[must_use]
    pub fn report(&self) -> &RepairReport {
        &self.report
    }

    /// The report as one status line.
    #[must_use]
    pub fn summary(&self) -> String {
        self.report.describe()
    }
}

impl std::fmt::Debug for RepairModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepairModel")
            .field("name", &self.name)
            .field("weld_tol", &self.weld_tol)
            .field("max_hole_edges", &self.max_hole_edges)
            .field("report", &self.report.describe())
            .finish_non_exhaustive()
    }
}

impl Command for RepairModel {
    fn apply(&self, world: &World) {
        self.inner.apply(world);
    }

    fn invert(&self, world: &World) {
        self.inner.invert(world);
    }
}
