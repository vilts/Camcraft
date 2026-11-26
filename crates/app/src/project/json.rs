//! The file DTOs as plain JSON values, for the doors that speak JSON but
//! are not a file (the MCP server, #128): operation parameters, tools and
//! stock read and written in exactly the shape a `.camproj` holds them,
//! so an agent's patch and a saved project can never disagree on a name.
//!
//! Provenance: our own persistence contract (this module's siblings),
//! re-exposed; no new schema. Clean-room, public sources only.

use super::{OpParams, ParamsDto, StockDto, StockShape, Tool, ToolDto};

use serde_json::Value;

/// An operation's parameters as the file writes them: one-key object,
/// the kind's snake_case tag, then the fields (`{"face": {...}}`).
#[must_use]
pub fn params_to_json(p: &OpParams) -> Value {
    serde_json::to_value(ParamsDto::from_params(p)).unwrap_or(Value::Null)
}

/// The stored door (#3073): a tagged parameter object exactly as this
/// build's `params_to_json` wrote it into a file (a cut recipe), read
/// back with no live-edit rewrites.
pub fn params_from_stored_json(v: &Value) -> Result<OpParams, String> {
    let dto: ParamsDto = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok(dto.to_params())
}

/// The reverse door: a tagged parameter object back to the domain type.
/// Shape errors (a missing field, a string where a number goes) are the
/// refusal text; sanity (finite, positive) stays the commands' law.
pub fn params_from_json(v: &Value) -> Result<OpParams, String> {
    // #329: a height naming its own work-offset slot is refused, never
    // ignored - the slot is the setup's (`SetSetupWorkOffset`).
    let mut slots = Vec::new();
    super::wcs_migrate::height_slots(v, &mut slots);
    if !slots.is_empty() {
        return Err("a work_offset height names no slot - the setup's work offset is the \
                    one every height reads; pick it on the setup"
            .into());
    }
    // This door creates or edits live parameters, rather than loading a file.
    // A newly supplied tilt uses the current basis; a persisted version in
    // an existing operation survives a partial edit unchanged.
    let mut live = v.clone();
    if let Some(kinds) = live.as_object_mut() {
        for fields in kinds.values_mut() {
            if let Some(tilt) = fields.get_mut("tilt").and_then(Value::as_object_mut) {
                tilt.entry("basis_version").or_insert(Value::from(1));
            }
        }
    }
    let dto: ParamsDto = serde_json::from_value(live).map_err(|e| e.to_string())?;
    Ok(dto.to_params())
}

#[must_use]
pub fn tool_to_json(t: &Tool) -> Value {
    serde_json::to_value(ToolDto::from_tool(t)).unwrap_or(Value::Null)
}

pub fn tool_from_json(v: &Value) -> Result<Tool, String> {
    let dto: ToolDto = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok(dto.to_tool())
}

#[must_use]
pub fn stock_to_json(s: &StockShape) -> Value {
    serde_json::to_value(StockDto::from_shape(s)).unwrap_or(Value::Null)
}

pub fn stock_from_json(v: &Value) -> Result<StockShape, String> {
    let dto: StockDto = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
    Ok(dto.to_shape())
}
