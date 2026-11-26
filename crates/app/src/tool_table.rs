//! Document bridge for machine controller tool tables (#2566): routes
//! `.camtools`, PathPilot `.csv`, and Hurco `.TCM` imports and exports
//! through one format-dispatched interface over `camcraft_tools::tool_table`
//! and `crate::tool_library`.
//!
//! Provenance: our own document-layer bridge over `camcraft_tools::tool_table`
//! and `tool_library`; no vendor library source consulted.

use std::path::Path;

use flecs_ecs::prelude::World;

pub use camcraft_tools::tool_table::{ToolTableEntry, ToolTableExportRow, ToolTableFormat, detect_format, parse_unit};

use crate::model;
use crate::tool_body;
use crate::tool_library::{self, LibraryTool};
use crate::tool_numbers;
use crate::tools::Tool;
use crate::units::Unit;

/// Parse a tool library or controller tool table file into [`LibraryTool`] rows.
pub fn parse_import(
    text: &str,
    explicit_format: Option<ToolTableFormat>,
    path: &Path,
    unit: Unit,
    explicit_unit: bool,
) -> Result<(ToolTableFormat, Vec<LibraryTool>), String> {
    let format = explicit_format
        .or_else(|| detect_format(path, Some(text)))
        .unwrap_or(ToolTableFormat::CamTools);
    match format {
        ToolTableFormat::CamTools => {
            if explicit_unit && unit != Unit::Millimeters {
                return Err("camtools format stores millimetres only, not inch".into());
            }
            let lib = tool_library::parse_tool_library(text)?;
            Ok((ToolTableFormat::CamTools, lib))
        }
        ToolTableFormat::PathPilot | ToolTableFormat::Hurco => {
            let entries = camcraft_tools::tool_table::parse_controller_table(format, text, unit)?;
            let lib = entries
                .into_iter()
                .map(|e| LibraryTool { name: e.name, tool: e.tool, body: None, number: Some(e.number), cutter: None })
                .collect();
            Ok((format, lib))
        }
    }
}

/// Export all document tools in `format` (`camtools`, `pathpilot`, or `hurco`).
pub fn export(world: &World, format: ToolTableFormat, unit: Unit) -> Result<String, String> {
    if format == ToolTableFormat::CamTools {
        if unit != Unit::Millimeters {
            return Err("camtools format exports millimetres only, not inch".into());
        }
        return tool_library::export_tool_library(world);
    }
    let assigned = tool_numbers::numbers(world)?;
    if assigned.is_empty() {
        return Err("the document has no tools to export".into());
    }
    struct OwnedRow {
        number: u32,
        name: String,
        tool: Tool,
        gauge_len: Option<f64>,
    }
    let mut items: Vec<OwnedRow> = Vec::with_capacity(assigned.len());
    for (name, number) in assigned {
        let Some(entity) = model::find_by_name(world, &name) else { continue };
        let Some(tool) = model::tool_of(world, &entity) else { continue };
        let (tool, gauge_len) = match tool_body::body_of(world, &entity) {
            Some(b) => {
                let (judged, _) = tool_body::judged(world, &entity, tool)
                    .map_err(|e| format!("tool '{name}': {e}"))?;
                let gauge = b
                    .library
                    .map(|r| r.gauge_len)
                    .or_else(|| (judged.holder_len > 0.0).then_some(judged.holder_len));
                (judged, gauge)
            }
            None => (tool, None),
        };
        items.push(OwnedRow { number, name, tool, gauge_len });
    }
    items.sort_by_key(|r| (r.number, r.name.clone()));
    let rows: Vec<ToolTableExportRow<'_>> = items
        .iter()
        .map(|r| ToolTableExportRow {
            number: r.number,
            name: &r.name,
            tool: &r.tool,
            gauge_len: r.gauge_len,
        })
        .collect();
    camcraft_tools::tool_table::export_controller_table(format, &rows, unit)
}
