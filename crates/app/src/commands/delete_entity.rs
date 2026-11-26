//! DeleteEntity: snapshot the named entity's persistent state at
//! construction, destruct on apply, recreate identically on invert.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::{FixtureDef, Location, Operation, OperationMeta, ProgramEntry, StockDef, ToolDef, Toolpath, Wcs};
use crate::stock::StockShape;
use crate::tools::Tool;

pub struct DeleteEntity {
    name: String,
    location: Location,
    facets: Option<std::sync::Arc<[Triangle]>>,
    /// #1339: a vouch comes back with the exact geometry revision it names.
    revision: u64,
    fidelity_vouched: Option<crate::schema::FidelityVouched>,
    /// #864: a reduced-fidelity model comes back flagged - an undo that
    /// dropped the flag would loosen every verdict against it.
    coarse_mm: Option<f64>,
    file_tolerance: bool,
    /// #1054: the STEP facet budget it was imported under comes back too.
    facet_budget: Option<u64>,
    /// #1610: an incomplete model comes back with its hole named.
    hole: Option<String>,
    /// #917: the closed solids of its facets come back with them.
    solids: Vec<std::ops::Range<usize>>,
    /// #744: raw STEP text preserved across delete/undo.
    step_source: Option<String>,
    /// #2463: a linked model comes back linked.
    source: Option<crate::schema::ModelSource>,
    /// Non-`None` restores the `StockDef` component; `wcs` restores the
    /// `Wcs` tag. Together with the CAM components below they keep every
    /// kind of named entity undoable with its kind intact.
    stock: Option<StockShape>,
    /// #1862: a DXF profile's setup and drawing; shared, not copied.
    sketch: Option<crate::schema::Sketch>,
    /// #2015: a stock's workpiece material, restored with it.
    stock_material: Option<crate::tools::WorkMaterial>,
    wcs: bool,
    /// #1720: a deleted WCS's flipped axes come back with its undo.
    wcs_flip: [bool; 3],
    tool: Option<Tool>,
    /// #221: a deleted tool's number, restored with it on undo.
    tool_number: Option<u32>,
    /// #368: a deleted tool's modelled body, restored with it on undo.
    tool_body: Option<crate::schema::ToolBody>,
    /// #93: a tool's drawn cutter, restored with it.
    tool_cutter: Option<camcraft_tools::spun::SpunProfile>,
    operation: Option<Operation>,
    operation_meta: Option<OperationMeta>,
    /// A Pause / Home entry of the operation list (#155), with its
    /// position (`operation_meta`).
    entry: Option<ProgramEntry>,
    /// A model deleted from the tree keeps its fixture marker (M3.2
    /// stage B) through undo too - the marker is part of the entity's
    /// identity, and an undo that quietly demoted a vise to a raw
    /// solid would change what the collision law sees.
    fixture: Option<FixtureDef>,
    /// The generated path and its warnings, restored with the operation
    /// they belong to (AGENTS.md: deleting frees, undoing must restore
    /// ALL associated resources - in both directions).
    path: Option<Toolpath>,
    /// #819: a plate deleted from the tree lets go of its riders, the
    /// way `DeleteFixture` does.
    riders: crate::plate::mount::Unmounts,
    /// #1094: the raw-stock-only marks naming this model (or ticked on
    /// this stock) die with it and come back with its undo.
    marks: super::stock_source_only::MarkDrops,
}

impl DeleteEntity {
    /// Reads the world now (construction time). The target must exist;
    /// validating here is what keeps undo/redo exact later.
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        Ok(DeleteEntity {
            name: name.to_string(),
            location: model::transform_of(world, &entity).unwrap_or_default(),
            facets: model::facets_of(world, &entity),
            revision: model::rev_of(world, &entity),
            fidelity_vouched: crate::model_vouch::record_of(world, &entity),
            coarse_mm: crate::model_fidelity::chord_of(world, &entity),
            file_tolerance: crate::model_fidelity::file_tolerance_of(world, &entity),
            facet_budget: crate::model_fidelity::budget_of(world, &entity),
            hole: crate::mesh_open::hole_of(world, &entity),
            solids: crate::mesh_solids::of(world, &entity),
            step_source: crate::step_source::of(world, &entity),
            source: crate::model_source::of(world, &entity),
            stock: model::stock_def_of(world, &entity),
            sketch: crate::sketch::sketch_of(world, &entity),
            stock_material: crate::stock_material::of_entity(world, &entity),
            wcs: model::is_wcs(world, &entity),
            wcs_flip: crate::setup_wcs_flip::of_entity(world, &entity),
            tool: model::tool_of(world, &entity),
            tool_number: model::tool_number_of(world, &entity),
            tool_body: crate::tool_body::body_of(world, &entity),
            tool_cutter: crate::tool_cutter::cutter_of(world, &entity),
            operation: model::operation_of(world, &entity),
            operation_meta: crate::program_entry::is_sequenced(world, &entity)
                .then(|| model::operation_meta_of(world, &entity)),
            entry: crate::program_entry::entry_of(world, &entity),
            path: model::toolpath_of(world, &entity),
            fixture: crate::fixture::def_of(world, &entity),
            riders: crate::plate::mount::Unmounts::of(world, name),
            marks: super::stock_source_only::MarkDrops::of(world, name),
        })
    }
}

impl Command for DeleteEntity {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
        self.riders.write(world, true);
        self.marks.write(world, true);
    }

    fn invert(&self, world: &World) {
        self.riders.write(world, false);
        if model::find_by_name(world, &self.name).is_some() {
            return; // already restored (idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.location);
        if let Some(tris) = &self.facets {
            model::set_facets(world, &entity, tris.clone());
        }
        model::set_rev(world, &entity, self.revision);
        crate::model_vouch::set(world, &entity, self.fidelity_vouched.clone());
        crate::model_fidelity::set_with_cause(world, &entity, self.coarse_mm, self.file_tolerance);
        crate::model_fidelity::set_budget(world, &entity, self.facet_budget);
        crate::mesh_open::set(world, &entity, self.hole.as_deref());
        crate::mesh_solids::set(world, &entity, &self.solids);
        crate::step_source::set(world, &entity, self.step_source.as_deref());
        crate::model_source::set(world, &entity, self.source.as_ref());
        if let Some(shape) = &self.stock {
            entity
                .entity_view(world)
                .set(StockDef { shape: shape.clone() });
            crate::stock_material::land_entity(world, &entity, self.stock_material);
        }
        if self.wcs {
            entity.entity_view(world).add(Wcs);
            crate::setup_wcs_flip::set(world, &entity, self.wcs_flip);
        }
        if let Some(sketch) = &self.sketch {
            entity.entity_view(world).set(sketch.clone());
        }
        if let Some(tool) = &self.tool {
            entity
                .entity_view(world)
                .set(ToolDef { tool: tool.clone(), number: self.tool_number });
            crate::tool_body::set_body(world, &entity, self.tool_body.as_ref());
            crate::tool_cutter::set_cutter(world, &entity, self.tool_cutter.as_ref());
        }
        if let Some(operation) = &self.operation {
            entity.entity_view(world).set(operation.clone());
        }
        if let Some(entry) = &self.entry {
            entity.entity_view(world).set(entry.clone());
        }
        if let Some(meta) = &self.operation_meta {
            entity.entity_view(world).set(meta.clone());
        }
        if let Some(path) = &self.path {
            entity.entity_view(world).set(path.clone());
        }
        if let Some(fixture) = &self.fixture {
            entity.entity_view(world).set(fixture.clone());
        }
        self.marks.write(world, false);
    }
}
