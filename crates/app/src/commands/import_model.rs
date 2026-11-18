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
    /// What the importer changed on the way (`import::Graded::notes`), for
    /// the caller to say: the MCP result and the status line. Not part of
    /// the document; empty for a mesh handed in.
    pub notes: Vec<String>,
    /// The hole of a STEP closed shell that meshes open (#1610,
    /// `import::Graded::open`): the model lands incomplete, every
    /// operation on it refused at the post by name. `None` for a whole mesh.
    pub hole: Option<String>,
    /// Raw STEP text kept on entities imported from STEP (#744).
    pub step_source: Option<String>,
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
        let budget = checked_budget(path, facet_budget)?;
        let graded = import::load_model_open_within(path, budget)?;
        Ok(Self::graded(path, graded, facet_budget))
    }

    /// [`ImportModel::within`] over bytes already in hand, named by the
    /// file they came from (#31): a browser's file picker hands over a
    /// name and its contents, never a path the importer could open. The
    /// same laws run - IGES and an unknown extension refused by name, the
    /// file-size cap, the STEP budget's bounds, the finite-mesh gate - so
    /// a picked file lands exactly as the same file opened from disk.
    pub fn from_bytes(file_name: &str, data: &[u8], facet_budget: Option<u64>) -> Result<Self, CommandError> {
        let path = Path::new(file_name);
        let budget = checked_budget(path, facet_budget)?;
        let format = import::format_from_path(path)?;
        if u64::try_from(data.len()).unwrap_or(u64::MAX) > import::MAX_FILE_BYTES {
            return Err(import::ImportError::TooLarge.into());
        }
        let graded = import::parse_model_open_within(data, format, budget)?;
        Ok(Self::graded(path, graded, facet_budget))
    }

    /// The command for a graded mesh, named by its file's stem.
    fn graded(path: &Path, graded: import::Graded, facet_budget: Option<u64>) -> Self {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("model")
            .to_string();
        // #1610: a mesh with a hole is of unknown fidelity, on every path
        // its chord rides (an update, a fixture, a stock copy).
        let coarse = if graded.open.is_some() { Some(crate::model_fidelity::UNKNOWN_MM) } else { graded.coarse_mm };
        let mut cmd = Self::from_mesh(&name, graded.mesh).with_fidelity(coarse);
        cmd.facet_budget = step_budget::recorded(facet_budget);
        cmd.notes = graded.notes;
        cmd.hole = graded.open;
        cmd.step_source = graded.step_source;
        cmd
    }

    pub fn from_mesh(name: &str, mesh: Mesh) -> Self {
        ImportModel {
            name: name.to_string(),
            mesh,
            coarse_mm: None,
            facet_budget: None,
            notes: Vec::new(),
            hole: None,
            step_source: None,
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

/// The facet budget an import runs under, checked before any parsing.
/// IGES needs a translation kernel we do not have; refuse it by extension,
/// not by a failed read, so the user gets the honest diagnostic. STEP
/// imports via the truck tessellator (mesh only). A budget outside
/// `step_budget`'s bounds, or one given for a file that is not STEP (its
/// mesh is the file; nothing would honour it), is refused by name.
fn checked_budget(path: &Path, facet_budget: Option<u64>) -> Result<usize, CommandError> {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if matches!(ext.to_ascii_lowercase().as_str(), "iges" | "igs") => {
            return Err(CommandError::UnsupportedType(ext.to_ascii_uppercase()));
        }
        _ => {}
    }
    match facet_budget {
        None => Ok(import::MAX_FACETS),
        Some(b) => {
            if !matches!(import::format_from_path(path), Ok(import::Format::Step)) {
                return Err(CommandError::Invalid(format!(
                    "a {} applies to STEP files only, not {}",
                    step_budget::NAME,
                    path.display()
                )));
            }
            step_budget::check(b).map_err(CommandError::Invalid)
        }
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
        crate::mesh_open::set(world, &entity, self.hole.as_deref());
        crate::step_source::set(world, &entity, self.step_source.as_deref());
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
