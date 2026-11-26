//! ImportModel: parse a mesh file at construction time, spawn a named model
//! entity on apply, remove it on invert.

use std::path::Path;

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::import::{self, native, step_budget};
use crate::geometry::Mesh;
use crate::model;
use crate::units::Unit;

pub struct ImportModel {
    pub name: String,
    pub mesh: Mesh,
    /// The chord error of a mesh imported coarser than the STEP importer
    /// promises (#864); `None` at full fidelity. It lands with the mesh.
    pub coarse_mm: Option<f64>,
    pub file_tolerance: bool,
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
    /// The file the mesh was read from and its SHA-256 (#2463): the link
    /// the UI checks while the project is open. `None` for a mesh handed
    /// in or a browser pick, which have no file to check again.
    pub source: Option<crate::schema::ModelSource>,
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
        Self::within_in(path, facet_budget, None)
    }

    /// [`ImportModel::within`] with the caller's length unit for a STEP
    /// file that states none (#1874). A STEP file that states no unit, with
    /// `None` here, is refused (`ImportError::StepUnitsMissing`): the model
    /// is never read in a guessed unit. A file that states its own unit
    /// keeps it, and the note says that the given unit is not used. A unit
    /// given for a file that is not STEP is refused by name, before any
    /// parsing, as a facet budget is.
    pub fn within_in(path: &Path, facet_budget: Option<u64>, units: Option<Unit>) -> Result<Self, CommandError> {
        let budget = checked_budget(path, facet_budget)?;
        checked_units(path, units)?;
        // #1873: a qualified native file is converted to STEP first. The
        // converter reads the file itself, so its digest is read once more
        // after the conversion (the #2463 two-read link): a file that
        // changes in between records the newer digest, and the next change
        // imports again. An unreadable file records no link.
        if native::route(path) == native::Route::Converted {
            let graded = converted(path, budget, units)?;
            let mut cmd = Self::graded(path, graded, facet_budget);
            cmd.source = crate::model_source::read(path).ok();
            return Ok(cmd);
        }
        // #2689: one read - the link records the digest of the very bytes
        // the parse used, so a file that changes during the import is seen
        // as changed by the next check and imported again.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (graded, sha256) = import::load_model_open_sourced(path, budget, units)?;
            let mut cmd = Self::graded(path, graded, facet_budget);
            cmd.source = Some(crate::schema::ModelSource { path: path.to_string_lossy().into_owned(), sha256 });
            Ok(cmd)
        }
        #[cfg(target_arch = "wasm32")]
        {
            let graded = import::load_model_open_in(path, budget, units)?;
            Ok(Self::graded(path, graded, facet_budget))
        }
    }

    /// [`ImportModel::within`] over bytes already in hand, named by the
    /// file they came from (#31): a browser's file picker hands over a
    /// name and its contents, never a path the importer could open. The
    /// same laws run - IGES and an unknown extension refused by name, the
    /// file-size cap, the STEP budget's bounds, the finite-mesh gate - so
    /// a picked file lands exactly as the same file opened from disk.
    /// A qualified native format (#1873) is refused by name: the external
    /// converter reads a file on disk, and bytes in hand have none.
    pub fn from_bytes(file_name: &str, data: &[u8], facet_budget: Option<u64>) -> Result<Self, CommandError> {
        Self::from_bytes_in(file_name, data, facet_budget, None)
    }

    /// [`ImportModel::from_bytes`] with the caller's length unit for a STEP
    /// file that states none (#1874), as [`ImportModel::within_in`].
    pub fn from_bytes_in(file_name: &str, data: &[u8], facet_budget: Option<u64>, units: Option<Unit>) -> Result<Self, CommandError> {
        let path = Path::new(file_name);
        let budget = checked_budget(path, facet_budget)?;
        checked_units(path, units)?;
        if native::route(path) == native::Route::Converted {
            return Err(CommandError::Needs(no_converter(path)));
        }
        let format = import::format_from_path(path)?;
        if u64::try_from(data.len()).unwrap_or(u64::MAX) > import::MAX_FILE_BYTES {
            return Err(import::ImportError::TooLarge.into());
        }
        let graded = import::parse_model_open_in(data, format, budget, units)?;
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
        cmd.file_tolerance = graded.file_tolerance;
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
            file_tolerance: false,
            facet_budget: None,
            notes: Vec::new(),
            hole: None,
            step_source: None,
            source: None,
        }
    }

    /// The same import under another name - mesh and fidelity together.
    #[must_use]
    pub fn renamed(self, name: &str) -> Self {
        ImportModel { name: name.to_string(), ..self }
    }

    /// Record that only the file tolerance required a coarser chord.
    #[must_use]
    pub fn with_file_tolerance(mut self, file_tolerance: bool) -> Self {
        self.file_tolerance = file_tolerance;
        self
    }

    /// State the fidelity the mesh was imported at (#864).
    #[must_use]
    pub fn with_fidelity(mut self, coarse_mm: Option<f64>) -> Self {
        self.coarse_mm = coarse_mm;
        self.file_tolerance = false;
        self
    }
}

/// A qualified native file (#1873): the external cadmpeg program converts
/// it to STEP, and the STEP importer reads the result.
#[cfg(not(target_arch = "wasm32"))]
fn converted(path: &Path, budget: usize, units: Option<Unit>) -> Result<import::Graded, CommandError> {
    import::cadmpeg::load_open_within(path, budget, &import::cadmpeg::Settings::from_prefs(), units)
}

/// A browser cannot start a program: refuse by name.
#[cfg(target_arch = "wasm32")]
fn converted(path: &Path, _budget: usize, _units: Option<Unit>) -> Result<import::Graded, CommandError> {
    Err(CommandError::Needs(no_converter(path)))
}

/// The refusal of a qualified native file where no converter can run.
fn no_converter(path: &Path) -> String {
    format!(
        "importing .{} needs the desktop app with cadmpeg and a file on disk; export the part to STEP first",
        native::extension(path)
    )
}

/// The facet budget an import runs under, checked before any parsing.
/// A native CAD format needs a translation kernel we do not have. One
/// that the shared table (`import::native`) does not qualify - IGES and
/// `.prt` among them - is refused by its extension, not by a failed read,
/// so the user gets the honest diagnostic. STEP imports via the truck
/// tessellator (mesh only). A budget outside `step_budget`'s bounds, or
/// one given for a file that does not become STEP (its mesh is the file;
/// nothing would honour it), is refused by name.
fn checked_budget(path: &Path, facet_budget: Option<u64>) -> Result<usize, CommandError> {
    let route = native::route(path);
    if route == native::Route::Unqualified {
        return Err(CommandError::UnsupportedType(native::extension(path).to_ascii_uppercase()));
    }
    match facet_budget {
        None => Ok(import::MAX_FACETS),
        Some(b) => {
            let step = route == native::Route::Converted
                || matches!(import::format_from_path(path), Ok(import::Format::Step));
            if !step {
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

/// A length unit applies to STEP text only (#1874): a STEP file, or a
/// native file that is converted to STEP. Elsewhere it is refused by name,
/// before any parsing: an STL or OBJ mesh has no unit to state.
fn checked_units(path: &Path, units: Option<Unit>) -> Result<(), CommandError> {
    let Some(unit) = units else { return Ok(()) };
    let step = native::route(path) == native::Route::Converted
        || matches!(import::format_from_path(path), Ok(import::Format::Step));
    if step {
        return Ok(());
    }
    Err(CommandError::Invalid(format!(
        "a length unit ({}) applies to STEP files only, not {}",
        unit.as_str(),
        path.display()
    )))
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
        crate::mesh_solids::set(world, &entity, self.mesh.solid_ranges());
        crate::model_fidelity::set_with_cause(world, &entity, self.coarse_mm, self.file_tolerance);
        crate::model_fidelity::set_budget(world, &entity, self.facet_budget);
        crate::mesh_open::set(world, &entity, self.hole.as_deref());
        crate::step_source::set(world, &entity, self.step_source.as_deref());
        crate::model_source::set(world, &entity, self.source.as_ref());
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
