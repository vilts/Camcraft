//! DXF half-profile and ISO-aligned STEP-to-profile constructors for
//! [`SetToolBody`] (#743), using `tool_body::build_spun` so undo/redo and
//! serialization keep working cleanly.
//! Provenance: original command constructors over `crate::tool_profile_dxf`.

use std::path::Path;

use camcraft_geometry::dxf::Unit;
use flecs_ecs::prelude::World;

use crate::commands::{CommandError, SetToolBody};
use crate::geometry::Triangle;
use crate::model;
use crate::tool_body;
use crate::tool_profile_dxf::{
    dxf_to_spun_with_unit, placed_mesh_to_spun_profile, step_to_spun_profile, tool_body_to_dxf,
};

impl SetToolBody {
    /// Parse ASCII DXF bytes representing a centreline + half-profile and
    /// land the validated spun profile on tool `name` as one undo step.
    pub fn from_dxf_bytes(
        world: &World,
        name: &str,
        source: &str,
        bytes: &[u8],
        default_unit: Option<Unit>,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let spun = dxf_to_spun_with_unit(bytes, default_unit)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        let mut body = tool_body::build_spun(spun);
        if !source.trim().is_empty() {
            body.source = source.to_owned();
        }
        tool_body::fits(&tool, &body)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        Ok(SetToolBody {
            name: name.to_string(),
            prev: tool_body::body_of(world, &entity),
            next: Some(body),
        })
    }

    /// Read a DXF file from disk and land its half-profile on tool `name`.
    pub fn from_dxf(
        world: &World,
        name: &str,
        path: &Path,
        default_unit: Option<Unit>,
    ) -> Result<Self, CommandError> {
        let bytes = std::fs::read(path)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': DXF unreadable: {e}")))?;
        let source = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("holder.dxf");
        Self::from_dxf_bytes(world, name, source, &bytes, default_unit)
    }

    /// Convert an ISO 13399 aligned STEP holder into a conservative half-profile
    /// and land it on tool `name` as an editable spun profile (one undo step).
    pub fn from_step_profile(world: &World, name: &str, path: &Path) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let spun = step_to_spun_profile(path)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        let mut body = tool_body::build_spun(spun);
        let file = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("step profile");
        body.source = format!("profile ({file})");
        tool_body::fits(&tool, &body)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        Ok(SetToolBody {
            name: name.to_string(),
            prev: tool_body::body_of(world, &entity),
            next: Some(body),
        })
    }

    /// Convert a placed milling/spindle holder mesh into a conservative
    /// half-profile and land it on tool `name` (one undo step).
    pub fn from_placed_profile(
        world: &World,
        name: &str,
        source: &str,
        facets: &[Triangle],
        axis: [f64; 3],
        offset: f64,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let tool = model::tool_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not a tool")))?;
        let placed = tool_body::place(facets, axis, offset)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        let spun = placed_mesh_to_spun_profile(&placed)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        let mut body = tool_body::build_spun(spun);
        if !source.trim().is_empty() {
            body.source = format!("profile ({source})");
        }
        tool_body::fits(&tool, &body)
            .map_err(|e| CommandError::Invalid(format!("tool '{name}': {e}")))?;
        Ok(SetToolBody {
            name: name.to_string(),
            prev: tool_body::body_of(world, &entity),
            next: Some(body),
        })
    }

    /// Export the pending body as an ASCII DXF half-profile.
    pub fn profile_dxf(&self) -> Result<String, String> {
        match &self.next {
            Some(body) => tool_body_to_dxf(body),
            None => Err(format!("tool '{}' clears its holder body", self.name)),
        }
    }
}
