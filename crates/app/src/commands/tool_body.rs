//! SetToolBody (#368): attach, replace or clear a tool's modelled body
//! as one undoable command - a milling body along the spindle axis, or
//! (#369) a lathe body in the turning tool frame, with (#396) its
//! optional insert zone. The mesh is read, placed and judged at
//! construction (AGENTS.md: malformed input never enters the history);
//! `apply`/`invert` only land the snapshot. Our own command model, the
//! snapshot-and-restore inverse every sibling uses.

use std::path::Path;

use flecs_ecs::prelude::World;

use camcraft_tools::holder_library;
use camcraft_tools::spun::{SpunPoint, SpunProfile};

use crate::commands::{Command, CommandError};
use crate::geometry::Triangle;
use crate::model;
use crate::schema::ToolBody;
use crate::tool_body;
use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

/// A body as the caller states it: model-frame facets, where they came
/// from, and the numeric placement (`tool_body::place`).
#[derive(Clone, Debug, PartialEq)]
pub struct ToolBodySpec {
    pub source: String,
    pub facets: Vec<Triangle>,
    /// The model direction that is the tool axis, pointing from the tip
    /// toward the spindle.
    pub axis: [f64; 3],
    /// The model origin's height above the tool tip, along that axis.
    pub offset: f64,
}

impl ToolBodySpec {
    /// Read a STEP file and align it automatically by its ISO 13399
    /// coordinate systems (#370). Refuses by name if the file is unreadable,
    /// holds no facets, or if coordinate systems are absent or ambiguous.
    pub fn from_step_auto(path: &Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("I/O error: {e}"))?;
        // #361: the one body-file door (a reduced-fidelity mesh is refused).
        let file = crate::tool_body_file::read(path)?;
        let report = crate::geometry::import::step_csys::read_tool_csys(&data)
            .map_err(|e| format!("coordinate systems unreadable: {e}"))?;
        let (axis, offset) = report.align_tool_body()?;
        Ok(ToolBodySpec { source: file.source, facets: file.facets, axis, offset })
    }
}

/// A lathe-held body as the caller states it (#369): model-frame
/// facets, where they came from, and the turning-frame placement (the
/// model directions that are radial and along the spindle, the model
/// point that is the tip - `camcraft_tools::lathe_holder`).
pub struct LatheBodySpec {
    pub source: String,
    pub facets: Vec<Triangle>,
    pub place: LathePlacement,
    /// #396: the insert plate the mesh models, a box in the turning
    /// tool frame the holder judge leaves out; `None` judges it all.
    pub zone: Option<InsertZone>,
}

/// #793: a built-in library holder drawn for `tool` (named `name` in
/// the refusals) as its spun-profile body, judged against it - what
/// [`SetToolBody::library`] lands on an existing tool and (#828)
/// `AddTool` lands on a new one, so both doors refuse alike.
pub(crate) fn library_body(
    tool: &crate::tools::Tool,
    name: &str,
    entry: &str,
    gauge_len: Option<f64>,
    projection: Option<f64>,
) -> Result<ToolBody, CommandError> {
    let invalid = |e: holder_library::LibraryError| CommandError::Invalid(format!("tool '{name}': holder: {e}"));
    let e = holder_library::find(entry).map_err(invalid)?;
    e.fits_shank(tool.shank_d).map_err(invalid)?;
    let nose = projection.unwrap_or(tool.flute_len + tool.shank_len);
    let points = e.points(nose, gauge_len.unwrap_or(e.gauge_len)).map_err(invalid)?;
    let spun = SpunProfile::new(points).map_err(|e| CommandError::Invalid(format!("tool '{name}': holder profile: {e}")))?;
    let body = tool_body::build_spun(spun);
    tool_body::fits(tool, &body).map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
    Ok(body)
}

/// A body file through the one door (#361): an importer error stays an
/// import error; a mesh the door will not take is refused by name.
fn body_file(path: &Path) -> Result<crate::tool_body_file::BodyFile, CommandError> {
    let graded = crate::geometry::import::load_model_graded(path)?;
    crate::tool_body_file::accept(path, graded).map_err(CommandError::Invalid)
}

/// Set (`Some`) or clear (`None`) a tool's body.
#[derive(Debug)]
pub struct SetToolBody {
    name: String,
    prev: Option<ToolBody>,
    next: Option<ToolBody>,
}

impl SetToolBody {
    /// Place and judge `spec` against tool `name` now; `None` clears.
    pub fn new(world: &World, name: &str, spec: Option<ToolBodySpec>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let next = match spec {
            None => None,
            Some(s) => {
                let placed = tool_body::place(&s.facets, s.axis, s.offset).map_err(CommandError::Invalid)?;
                let body = tool_body::build(&s.source, s.axis, s.offset, placed)
                    .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
                tool_body::fits(&tool, &body).map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
                Some(body)
            }
        };
        Ok(SetToolBody { name: name.to_string(), prev: tool_body::body_of(world, &entity), next })
    }

    /// Set (`Some`) a drawn spun holder profile (#93) as the tool's
    /// body, or clear it (`None`, back to the holder cylinder). The
    /// outline is validated here and refused by name ("holder profile:
    /// point 3 ..."), then judged against the tool like any body.
    pub fn spun(world: &World, name: &str, points: Option<Vec<SpunPoint>>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let next = match points {
            None => None,
            Some(p) => {
                let spun = SpunProfile::new(p)
                    .map_err(|e| CommandError::Invalid(format!("tool '{name}': holder profile: {e}")))?;
                let body = tool_body::build_spun(spun);
                tool_body::fits(&tool, &body).map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
                Some(body)
            }
        };
        Ok(SetToolBody { name: name.to_string(), prev: tool_body::body_of(world, &entity), next })
    }

    /// #793: drop a built-in library holder (`camcraft_tools::
    /// holder_library`, "BT30 ER20") into tool `name`'s holder as a
    /// drawn spun profile - one undo step, editable afterwards like any
    /// drawn profile. `gauge_len` (gauge line to nose face) defaults to
    /// the entry's; `projection` (nose face above the tip) to the
    /// tool's flute plus shank length. Refused by name: no such tool or
    /// entry, a shank the collet or bore cannot take, a gauge length
    /// too short for the flange and nose, and everything a drawn
    /// profile refuses.
    pub fn library(
        world: &World,
        name: &str,
        entry: &str,
        gauge_len: Option<f64>,
        projection: Option<f64>,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let next = Some(library_body(&tool, name, entry, gauge_len, projection)?);
        Ok(SetToolBody { name: name.to_string(), prev: tool_body::body_of(world, &entity), next })
    }

    /// Read a STEP / STL / OBJ through the model importer and place it,
    /// preserving any named STEP coordinate systems on piece 0.
    pub fn from_file(world: &World, name: &str, path: &Path, axis: [f64; 3], offset: f64) -> Result<Self, CommandError> {
        let file = body_file(path)?;
        let raw_csys = crate::tool_body_assembly::read_csys_if_step(path).map_err(CommandError::Invalid)?;
        Self::new_with_csys(world, name, ToolBodySpec { source: file.source, facets: file.facets, axis, offset }, raw_csys)
    }

    /// Read a STEP file, align it automatically by its ISO 13399
    /// coordinate systems (#370), and place it on tool `name` with no
    /// manual offsets, preserving its STEP coordinate systems on piece 0.
    pub fn from_step_auto(world: &World, name: &str, path: &Path) -> Result<Self, CommandError> {
        let spec = ToolBodySpec::from_step_auto(path).map_err(CommandError::Invalid)?;
        let raw_csys = crate::tool_body_assembly::read_csys_if_step(path).map_err(CommandError::Invalid)?;
        Self::new_with_csys(world, name, spec, raw_csys)
    }

    /// Variant of [`Self::new`] that also transforms model-frame CSYS onto piece 0.
    pub fn new_with_csys(
        world: &World,
        name: &str,
        spec: ToolBodySpec,
        raw_csys: Vec<crate::tool_body_assembly::BodyCsys>,
    ) -> Result<Self, CommandError> {
        let axis = spec.axis;
        let offset = spec.offset;
        let mut cmd = Self::new(world, name, Some(spec))?;
        if !raw_csys.is_empty()
            && let Some(ref mut body) = cmd.next
            && let Some(p0) = body.pieces.first_mut()
        {
            p0.csys = crate::tool_body_assembly::transform_csys_spindle(&raw_csys, axis, offset)
                .map_err(CommandError::Invalid)?;
        }
        Ok(cmd)
    }

    /// Place and judge a lathe body (#369) against tool `name` now;
    /// `None` clears. Refused by name: no such tool, no lathe steel, a
    /// placement that is no frame, a mesh that is no body.
    pub fn lathe(world: &World, name: &str, spec: Option<LatheBodySpec>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let invalid = |e: String| CommandError::Invalid(format!("tool '{name}': {e}"));
        let next = match spec {
            None => None,
            Some(s) => {
                crate::tool_body_lathe::fits(&tool, s.zone.as_ref()).map_err(invalid)?;
                let placed = crate::tool_body_lathe::place(&s.facets, &s.place).map_err(invalid)?;
                let body = crate::tool_body_lathe::build(&s.source, s.place, s.zone, placed).map_err(invalid)?;
                tool_body::fits(&tool, &body).map_err(invalid)?;
                Some(body)
            }
        };
        Ok(SetToolBody { name: name.to_string(), prev: tool_body::body_of(world, &entity), next })
    }

    /// Read a STEP / STL / OBJ through the model importer and place it
    /// as a lathe body, preserving any named STEP coordinate systems.
    pub fn lathe_from_file(world: &World, name: &str, path: &Path, place: LathePlacement) -> Result<Self, CommandError> {
        let file = body_file(path)?;
        let raw_csys = crate::tool_body_assembly::read_csys_if_step(path).map_err(CommandError::Invalid)?;
        Self::lathe_with_csys(world, name, LatheBodySpec { source: file.source, facets: file.facets, place, zone: None }, raw_csys)
    }

    /// Variant of [`Self::lathe`] that also transforms model-frame CSYS onto piece 0.
    pub fn lathe_with_csys(
        world: &World,
        name: &str,
        spec: LatheBodySpec,
        raw_csys: Vec<crate::tool_body_assembly::BodyCsys>,
    ) -> Result<Self, CommandError> {
        let place = spec.place;
        let mut cmd = Self::lathe(world, name, Some(spec))?;
        if !raw_csys.is_empty()
            && let Some(ref mut body) = cmd.next
            && let Some(p0) = body.pieces.first_mut()
        {
            p0.csys = crate::tool_body_assembly::transform_csys_lathe(&raw_csys, &place)
                .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        }
        Ok(cmd)
    }

    /// Attach a modular piece (adapter, clamping unit, extension) onto
    /// `name`'s existing body by ISO 13399 CSYS or explicit placement, as
    /// one undoable command (#361 / #1040).
    pub fn attach_piece(
        world: &World,
        name: &str,
        path: &Path,
        place: crate::tool_body_assembly::PiecePlace,
    ) -> Result<Self, CommandError> {
        // Ensure body_file door handles any import vs body refusal consistently.
        let file = body_file(path)?;
        let csys = crate::tool_body_assembly::read_csys_if_step(path).map_err(CommandError::Invalid)?;
        let piece = crate::tool_body_assembly::LoadedPiece { source: file.source, facets: file.facets, csys };
        Self::attach_loaded_piece(world, name, piece, place)
    }

    /// Attach a pre-loaded modular piece onto `name`'s existing body.
    pub fn attach_loaded_piece(
        world: &World,
        name: &str,
        piece: crate::tool_body_assembly::LoadedPiece,
        place: crate::tool_body_assembly::PiecePlace,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let prev = tool_body::body_of(world, &entity);
        let Some(ref cur) = prev else {
            return Err(CommandError::Invalid(format!(
                "tool '{name}' has no body yet - place piece 0 first before attaching modular pieces"
            )));
        };
        let invalid = |e: String| CommandError::Invalid(format!("tool '{name}': {e}"));
        let next = crate::tool_body_assembly::attach_piece(name, cur, piece, &place).map_err(invalid)?;
        tool_body::fits(&tool, &next).map_err(invalid)?;
        Ok(SetToolBody { name: name.to_string(), prev, next: Some(next) })
    }

    /// Remove modular piece `index` from `name`'s multi-piece body as one
    /// undoable command (#361 / #1040).
    pub fn remove_piece(world: &World, name: &str, index: usize) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let prev = tool_body::body_of(world, &entity);
        let Some(ref cur) = prev else {
            return Err(CommandError::Invalid(format!("tool '{name}' has no body to remove a piece from")));
        };
        let invalid = |e: String| CommandError::Invalid(format!("tool '{name}': {e}"));
        let next = crate::tool_body_assembly::remove_piece(name, cur, index).map_err(invalid)?;
        tool_body::fits(&tool, &next).map_err(invalid)?;
        Ok(SetToolBody { name: name.to_string(), prev, next: Some(next) })
    }

    /// Set (`Some`) or clear (`None`) the insert zone (#396) of tool
    /// `name`'s lathe body, keeping its mesh and placement. Refused by
    /// name: no such tool, no lathe body on it, a zone its inserts
    /// could not fill or that holds the whole body.
    pub fn insert_zone(world: &World, name: &str, zone: Option<InsertZone>) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let invalid = |e: String| CommandError::Invalid(format!("tool '{name}': {e}"));
        let prev = tool_body::body_of(world, &entity);
        let Some((source, place, facets)) = prev.as_ref().and_then(|b| match b.frame {
            tool_body::BodyFrame::Lathe { place, .. } => Some((b.source.clone(), place, b.facets.to_vec())),
            tool_body::BodyFrame::Spindle { .. } => None,
        }) else {
            return Err(invalid("it carries no lathe body - an insert zone is part of one".to_owned()));
        };
        crate::tool_body_lathe::fits(&tool, zone.as_ref()).map_err(invalid)?;
        let mut body = crate::tool_body_lathe::build(&source, place, zone, facets).map_err(invalid)?;
        if let Some(ref p) = prev
            && !p.pieces.is_empty()
        {
            body.pieces = p.pieces.clone();
        }
        Ok(SetToolBody { name: name.to_string(), prev, next: Some(body) })
    }
}

impl Command for SetToolBody {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_body::set_body(world, &entity, self.next.as_ref());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            tool_body::set_body(world, &entity, self.prev.as_ref());
        }
    }
}

#[path = "tool_body_reapply.rs"]
mod reapply;
