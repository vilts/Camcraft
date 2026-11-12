//! ImportModel: parse a mesh file at construction time, spawn a named model
//! entity on apply, remove it on invert.

use std::path::Path;

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::import::{self, step_budget};
use crate::geometry::Mesh;
use crate::model;

pub struct ImportModel {
    pub name: String,
    pub mesh: Mesh,
    /// The chord error of a mesh imported coarser than the STEP importer
    /// promises (#864); `None` at full fidelity. It lands with the mesh.
    pub coarse_mm: Option<f64>,
    /// The STEP facet budget the mesh was made under when it is not the
    /// default (#1054); `None` for the default and for STL/OBJ. It lands
    /// with the mesh, so the model says how to import it again.
    pub facet_budget: Option<u64>,
}

impl ImportModel {
    /// Parse + validate now; a corrupt file is a construction error and
    /// never reaches the history.
    pub fn new(path: &Path) -> Result<Self, CommandError> {
        Self::within(path, None)
    }

    /// [`ImportModel::new`] with a STEP facet budget (#1054); `None` is the
    /// default. A budget outside `step_budget`'s bounds, or one given for a
    /// file that is not STEP (its mesh is the file; nothing would honour
    /// it), is refused by name here, before any parsing.
    pub fn within(path: &Path, facet_budget: Option<u64>) -> Result<Self, CommandError> {
        // IGES needs a translation kernel we do not have; refuse by
        // extension, not by a failed read, so the user gets the honest
        // diagnostic. STEP imports via the truck tessellator (mesh only).
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) if matches!(ext.to_ascii_lowercase().as_str(), "iges" | "igs") => {
                return Err(CommandError::UnsupportedType(ext.to_ascii_uppercase()));
            }
            _ => {}
        }
        let budget = match facet_budget {
            None => import::MAX_FACETS,
            Some(b) => {
                if !matches!(import::format_from_path(path), Ok(import::Format::Step)) {
                    return Err(CommandError::Invalid(format!(
                        "a {} applies to STEP files only, not {}",
                        step_budget::NAME,
                        path.display()
                    )));
                }
                step_budget::check(b).map_err(CommandError::Invalid)?
            }
        };
        let graded = import::load_model_graded_within(path, budget)?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model")
            .to_string();
        let mut cmd = Self::from_mesh(&name, graded.mesh).with_fidelity(graded.coarse_mm);
        cmd.facet_budget = step_budget::recorded(facet_budget);
        Ok(cmd)
    }

    pub fn from_mesh(name: &str, mesh: Mesh) -> Self {
        ImportModel {
            name: name.to_string(),
            mesh,
            coarse_mm: None,
            facet_budget: None,
        }
    }

    /// The same import under another name - mesh and fidelity together.
    #[must_use]
    pub fn renamed(self, name: &str) -> Self {
        ImportModel { name: name.to_string(), ..self }
    }

    /// State the fidelity the mesh was imported at (#864).
    #[must_use]
    pub fn with_fidelity(mut self, coarse_mm: Option<f64>) -> Self {
        self.coarse_mm = coarse_mm;
        self
    }
}

impl Command for ImportModel {
    fn apply(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            // A redo path: the entity was deleted out from under us or this
            // exact apply is replaying; make it idempotent by re-finding.
            // Fresh-conflict protection lives at construction sites, not
            // here, because construction cannot see the world.
            return;
        }
        let entity = model::spawn(world, &self.name);
        model::set_facets(world, &entity, self.mesh.facets.clone());
        // The first geometry landing is revision 1 (M3.1); UpdateModel
        // counts from there. ImportModel only ever spawns - a same-named
        // re-import is routed to UpdateModel by the caller, never here.
        model::set_rev(world, &entity, 1);
        crate::model_fidelity::set(world, &entity, self.coarse_mm);
        crate::model_fidelity::set_budget(world, &entity, self.facet_budget);
    }

    fn invert(&self, world: &World) {
        // Invert is name-keyed, not handle-keyed: after any interleaving of
        // undo/redo the entity to remove is the one that carries this name.
        // Finding nothing (a logic bug elsewhere) must no-op, not crash.
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }
}
