//! UpdateModel: replace a model entity's geometry with a new revision,
//! snapshotting the previous facets and revision counter at construction
//! (the DeleteEntity pattern) so undo restores them exactly.
//!
//! Provenance: our own design - the command pattern here mirrors the app's
//! existing invertible commands; the feature diff it can report comes from
//! `geometry::revision`. No HSMWorks internals involved.
//!
//! This is the undoable half of M3.1's "model update detection": the
//! re-import path (UI `finish_import`) routes a same-named model here, and
//! staleness of dependent operations falls out of the stamp - which digests
//! the geometry signature, not a facet count - with no extra marking pass.
//!
//! The propagation law (M3.1 B): a `FromModel` stock is a copy of the model
//! it names, so the copy is re-derived inside this same command. One user
//! action, one undo unit - the viewer, the simulator and every stock-based
//! operation read a material solid, never a stale shard.

use flecs_ecs::prelude::{Entity, World};

use crate::commands::{Command, CommandError};
use crate::geometry::{Mesh, revision};
use crate::model;
use crate::schema::{Facets, FixtureDef};
use crate::stock::StockShape;

/// Every stock entity that is a `FromModel` copy of `name` (M3.1 B's
/// gate: an update never writes into a box stock or into a copy of some
/// other model). M3.2 A widens this from "the stock" to "every setup's
/// stock" - two setups may re-place copies of the same model, and a
/// re-import must refresh both, the propagation law being about the
/// material, not about one setup's furniture.
fn stock_copies(world: &World, name: &str) -> Vec<Entity> {
    model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| model::is_stock(world, e))
        .filter(|(_, e)| {
            matches!(model::stock_def_of(world, e),
                Some(StockShape::FromModel { source }) if source == name)
        })
        .map(|(_, e)| e)
        .collect()
}

pub struct UpdateModel {
    pub name: String,
    pub mesh: Mesh,
    /// The component as it stood: `None` means the entity had no geometry
    /// at all (a spawned-empty model), and undo must not leave an empty
    /// `Facets` behind where there was no component.
    prev_facets: Option<std::sync::Arc<[crate::geometry::Triangle]>>,
    prev_rev: u64,
    /// The derived stock's facets when a `FromModel` stock copied this
    /// model at construction (`Some`) - the propagation law's side of the
    /// snapshot, so undo trades model and material back together.
    prev_stock: Option<std::sync::Arc<[crate::geometry::Triangle]>>,
    /// The derived stock's own flag as it stood (#1056): the copy carries
    /// the fidelity of the facets it holds, traded with them both ways.
    prev_stock_coarse_mm: Option<f64>,
    /// ... and its hole (#1700), traded with the flag.
    prev_stock_hole: Option<String>,
    /// The fidelity of the new mesh and of the one it replaces (#864): the
    /// flag describes the facets, so it is traded with them, both ways.
    coarse_mm: Option<f64>,
    file_tolerance: bool,
    prev_file_tolerance: bool,
    prev_stock_causes: Vec<(String, bool)>,
    prev_coarse_mm: Option<f64>,
    /// #1610: the hole of an incomplete mesh, traded the same way.
    hole: Option<String>,
    prev_hole: Option<String>,
    /// #917: the closed solids of the facets it replaces, traded back.
    prev_solids: Vec<std::ops::Range<usize>>,
    /// The STEP facet budget each mesh was made under (#1054), traded
    /// with the facets the same way; `None` is the default.
    facet_budget: Option<u64>,
    prev_facet_budget: Option<u64>,
    /// #744: raw STEP text for STEP-imported models, traded with facets.
    step_source: Option<String>,
    prev_step_source: Option<String>,
    /// #2463: the linked source of each mesh, traded with the facets.
    source: Option<crate::schema::ModelSource>,
    prev_source: Option<crate::schema::ModelSource>,
    /// #826: a fixture plate's marker, when the model is one. Its grid was
    /// measured on the old mesh, so it goes with it and the riders leave
    /// the plate (the safe reading: the user re-detects), in this same
    /// undo step - the rule `ImportFixture` already kept for itself.
    plate: Option<Box<(FixtureDef, crate::plate::mount::Unmounts)>>,
    /// #1094: a raw-stock-only mark was said of the geometry it replaces,
    /// so a re-import in place drops it, in this same undo step.
    marks: super::stock_source_only::MarkDrops,
}

impl UpdateModel {
    /// Snapshots the world now (construction time), so undo/redo are exact
    /// whatever happens between now and then. The target must exist and be
    /// a model: a missing name is `NotFound`, setup furniture is
    /// `Invalid` - an import must never overwrite the stock solid or a WCS
    /// slot just because someone named the file `G54.stl`.
    pub fn new(world: &World, name: &str, mesh: Mesh) -> Result<Self, CommandError> {
        if mesh.is_empty() {
            return Err(CommandError::Invalid(format!(
                "'{name}' would update the model to empty geometry"
            )));
        }
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        if !model::is_model(world, &entity) {
            return Err(CommandError::Invalid(format!(
                "'{name}' is not a model - it is a setup's stock or work offset, a setup, a tool, an operation or other CAM data"
            )));
        }
        // Every copy of this model holds the same local facets, so one
        // snapshot serves them all (divergence is what this command
        // prevents, not a state it must describe).
        let copies = stock_copies(world, name);
        let prev_stock = copies.first().and_then(|s| model::facets_of(world, s));
        let prev_stock_coarse_mm = copies.first().and_then(|s| crate::model_fidelity::chord_of(world, s));
        let prev_stock_hole = copies.first().and_then(|s| crate::mesh_open::hole_of(world, s));
        Ok(UpdateModel {
            name: name.to_string(),
            mesh,
            prev_facets: model::facets_of(world, &entity),
            prev_rev: model::rev_of(world, &entity),
            prev_stock,
            prev_stock_coarse_mm,
            prev_stock_hole,
            coarse_mm: None,
            file_tolerance: false,
            prev_file_tolerance: crate::model_fidelity::file_tolerance_of(world, &entity),
            prev_stock_causes: copies.iter().filter_map(|s| {
                model::name_of(world, s).map(|name| (name, crate::model_fidelity::file_tolerance_of(world, s)))
            }).collect(),
            prev_coarse_mm: crate::model_fidelity::chord_of(world, &entity),
            hole: None,
            prev_hole: crate::mesh_open::hole_of(world, &entity),
            prev_solids: crate::mesh_solids::of(world, &entity),
            facet_budget: None,
            prev_facet_budget: crate::model_fidelity::budget_of(world, &entity),
            step_source: None,
            prev_step_source: crate::step_source::of(world, &entity),
            source: None,
            prev_source: crate::model_source::of(world, &entity),
            plate: crate::fixture::def_of(world, &entity)
                .filter(|d| d.plate.plate.is_some())
                .map(|d| Box::new((d, crate::plate::mount::Unmounts::of(world, name)))),
            marks: super::stock_source_only::MarkDrops::of(world, name),
        })
    }

    /// State the fidelity the new mesh was imported at (#864). Unstated,
    /// the update lands at full fidelity - a generated or STL mesh.
    #[must_use]
    pub fn with_fidelity(mut self, coarse_mm: Option<f64>) -> Self {
        self.coarse_mm = coarse_mm;
        self.file_tolerance = false;
        self
    }

    /// Record that only the file tolerance required a coarser chord.
    #[must_use]
    pub fn with_file_tolerance(mut self, file_tolerance: bool) -> Self {
        self.file_tolerance = file_tolerance;
        self
    }

    /// State the hole of a new mesh imported incomplete (#1610,
    /// `ImportModel::hole`); unstated, the new mesh is whole.
    #[must_use]
    pub fn with_hole(mut self, hole: Option<String>) -> Self {
        self.hole = hole;
        self
    }

    /// Keep the fidelity the model already carries (#864): for an update
    /// that reshapes the same facets (a repair), which makes them no
    /// truer to the surface they were imported from.
    #[must_use]
    pub fn keeping_fidelity(mut self) -> Self {
        self.coarse_mm = self.prev_coarse_mm;
        self.file_tolerance = self.prev_file_tolerance;
        self.hole.clone_from(&self.prev_hole);
        self.facet_budget = self.prev_facet_budget;
        self.step_source.clone_from(&self.prev_step_source);
        self.source.clone_from(&self.prev_source);
        self
    }

    /// State the STEP facet budget the new mesh was imported under
    /// (#1054); unstated, the update records the default.
    #[must_use]
    pub fn with_budget(mut self, facet_budget: Option<u64>) -> Self {
        self.facet_budget = facet_budget;
        self
    }

    /// Attach or clear the raw STEP source for STEP-imported geometry (#744).
    #[must_use]
    pub fn with_step_source(mut self, step_source: Option<String>) -> Self {
        self.step_source = step_source;
        self
    }

    /// Record the file the new mesh was read from (#2463); unstated, the
    /// update leaves the model unlinked.
    #[must_use]
    pub fn with_source(mut self, source: Option<crate::schema::ModelSource>) -> Self {
        self.source = source;
        self
    }

    /// Drop the plate snapshot, so apply/invert leave the plate's grid and
    /// its riders' mount markers alone. For `SetPlate`, which edits the
    /// grid and re-seats the riders itself (#826 review).
    #[must_use]
    pub(crate) fn keeping_plate(mut self) -> Self {
        self.plate = None;
        self
    }

    /// What moved between the two revisions this command trades. Computed
    /// on demand (the message wants it once, at import); cached Features
    /// make both sides cheap after the first call anywhere.
    #[must_use]
    pub fn changes(&self) -> revision::FeatureChanges {
        revision::changes(
            &model::features_of_local(self.prev_facets.as_deref().unwrap_or(&[])),
            &model::features_of_local(&self.mesh.facets),
        )
    }
}

impl Command for UpdateModel {
    fn apply(&self, world: &World) {
        // The entity to update is the one that carries this name (the
        // name-keyed law). Re-spawn if a DeleteEntity interleaving deleted
        // it out from under a redo, mirroring ImportModel's idempotence.
        let entity = model::find_by_name(world, &self.name)
            .unwrap_or_else(|| model::spawn(world, &self.name));
        model::set_facets(world, &entity, self.mesh.facets.clone());
        model::set_rev(world, &entity, self.prev_rev + 1);
        crate::mesh_solids::set(world, &entity, self.mesh.solid_ranges());
        crate::model_fidelity::set_with_cause(world, &entity, self.coarse_mm, self.file_tolerance);
        crate::model_fidelity::set_budget(world, &entity, self.facet_budget);
        crate::mesh_open::set(world, &entity, self.hole.as_deref());
        crate::step_source::set(world, &entity, self.step_source.as_deref());
        crate::model_source::set(world, &entity, self.source.as_ref());
        // Propagation (M3.1 B): the derived stock's copy is local-frame
        // facets - the very mesh above (SetStock's copy law) - and the
        // stock's own placement is never touched: re-importing a model
        // updates the material it was cut from, it does not move the
        // setup. The re-check at apply time is for redo after a
        // DeleteEntity interleaving, exactly as the model re-spawn above.
        if self.prev_stock.is_some() {
            for stock in stock_copies(world, &self.name) {
                model::set_facets(world, &stock, self.mesh.facets.clone());
                crate::model_fidelity::set_with_cause(world, &stock, self.coarse_mm, self.file_tolerance);
                crate::mesh_open::set(world, &stock, self.hole.as_deref());
            }
        }
        if let Some(p) = &self.plate {
            let mut off = p.0.clone();
            off.plate.plate = None;
            entity.entity_view(world).set(off);
            p.1.write(world, true);
        }
        self.marks.write(world, true);
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            match &self.prev_facets {
                Some(tris) => model::set_facets(world, &entity, tris.clone()),
                None => {
                    // No facet component to restore: the id, not the Rust
                    // type, is what `remove` takes (flecs_ecs 0.2.2 law).
                    entity.entity_view(world).remove(world.component::<Facets>());
                }
            }
            model::set_rev(world, &entity, self.prev_rev);
            crate::mesh_solids::set(world, &entity, &self.prev_solids);
            crate::model_fidelity::set_with_cause(world, &entity, self.prev_coarse_mm, self.prev_file_tolerance);
            crate::model_fidelity::set_budget(world, &entity, self.prev_facet_budget);
            crate::mesh_open::set(world, &entity, self.prev_hole.as_deref());
            crate::step_source::set(world, &entity, self.prev_step_source.as_deref());
            crate::model_source::set(world, &entity, self.prev_source.as_ref());
            if let Some(p) = &self.plate {
                entity.entity_view(world).set(p.0.clone());
                p.1.write(world, false);
            }
        }
        if let Some(tris) = &self.prev_stock {
            for stock in stock_copies(world, &self.name) {
                model::set_facets(world, &stock, tris.clone());
                let file_tolerance = model::name_of(world, &stock).and_then(|name| {
                    self.prev_stock_causes.iter().find(|(saved, _)| saved == &name).map(|(_, cause)| *cause)
                }).unwrap_or(false);
                crate::model_fidelity::set_with_cause(world, &stock, self.prev_stock_coarse_mm, file_tolerance);
                crate::mesh_open::set(world, &stock, self.prev_stock_hole.as_deref());
            }
        }
        self.marks.write(world, false);
    }
}
