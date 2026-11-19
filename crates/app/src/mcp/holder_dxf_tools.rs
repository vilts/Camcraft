//! MCP holder DXF & STEP half-profile tools (#743): import DXF half-profiles,
//! export a tool's holder body/profile as ASCII DXF, and convert ISO 13399
//! aligned STEP holder bodies to editable conservative spun profiles.
//! Provenance: original MCP tool surface over `SetToolBody` and `tool_profile_dxf`.

use std::path::Path;

use camcraft_geometry::dxf::Unit;
use serde_json::{Map, Value, json};

use super::holders::holder_view;
use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, opt_str, req_str};
use crate::commands::SetToolBody;
use crate::model;
use crate::script::Env;
use crate::tool_body;
use crate::tool_profile_dxf::{tool_body_to_dxf, tool_body_to_spun};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "import_tool_holder_dxf",
        description: "Import a 2D half-profile ASCII DXF (one LINE on layer CENTRELINE plus an open LINE/ARC profile chain wholly on one side) as a tool's revolved holder body. The centreline must be parallel to Y. DXF +Y is tool +Z; z = 0 is the lower end of the centreline, independent of draw direction. One undo step; missing centreline, crossing geometry, closed loops, splines or missing $INSUNITS (unless \"units\" is stated) refuse by name.",
        schema: s_import_dxf,
        run: import_tool_holder_dxf,
    },
    Tool {
        name: "export_tool_holder_dxf",
        description: "Export a document tool's holder body or drawn/converted spun profile to an ASCII DXF half-profile in millimetres (layers CENTRELINE and PROFILE, $INSUNITS = 4).",
        schema: s_export_dxf,
        run: export_tool_holder_dxf,
    },
    Tool {
        name: "convert_tool_step_to_profile",
        description: "Convert an ISO 13399 aligned STEP holder/body to a conservative outer-envelope half-profile on a document tool (non-revolved features are enveloped, never cut away) and optionally write it as ASCII DXF. Refuses STEP files with missing, degenerate or ambiguous ISO 13399 coordinate systems instead of guessing +Z. One undo step.",
        schema: s_convert_step,
        run: convert_tool_step_to_profile,
    },
];

fn parse_unit(s: Option<&str>) -> Result<Option<Unit>, String> {
    let Some(u) = s else {
        return Ok(None);
    };
    match u.trim().to_ascii_lowercase().as_str() {
        "mm" | "millimetre" | "millimetres" | "millimeter" | "millimeters" => Ok(Some(Unit::Millimeters)),
        "in" | "inch" | "inches" => Ok(Some(Unit::Inches)),
        other => Err(format!(
            "\"units\" must be \"mm\" or \"inch\", got \"{other}\""
        )),
    }
}

fn import_tool_holder_dxf(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let tool = req_str(a, "tool")?;
    let path = Path::new(req_str(a, "path")?);
    let unit = parse_unit(opt_str(a, "units")?)?;
    let cmd = SetToolBody::from_dxf(&env.world, tool, path, unit).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "tool": tool, "holder": holder_view(&env.world, tool) }))
}

fn export_tool_holder_dxf(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let tool = req_str(a, "tool")?;
    let out_path = req_str(a, "path")?;
    let Some(entity) = model::find_by_name(&env.world, tool) else {
        return Err(format!("tool \"{tool}\" not found"));
    };
    if model::tool_of(&env.world, &entity).is_none() {
        return Err(format!("\"{tool}\" is not a tool"));
    }
    let Some(body) = tool_body::body_of(&env.world, &entity) else {
        return Err(format!(
            "tool \"{tool}\" has no holder body to export - attach a body, library holder or spun profile first"
        ));
    };
    let spun = tool_body_to_spun(&body)?;
    let dxf = tool_body_to_dxf(&body)?;
    std::fs::write(out_path, &dxf)
        .map_err(|e| format!("failed to write DXF \"{out_path}\": {e}"))?;
    Ok(json!({
        "tool": tool,
        "path": out_path,
        "units": "mm",
        "points": spun.points().len(),
    }))
}

fn convert_tool_step_to_profile(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let tool = req_str(a, "tool")?;
    let step_path = Path::new(req_str(a, "step_path")?);
    let dxf_out = opt_str(a, "dxf_out_path")?;
    let cmd = SetToolBody::from_step_profile(&env.world, tool, step_path).map_err(cmd_err)?;
    if let Some(out) = dxf_out {
        let dxf = cmd.profile_dxf()?;
        std::fs::write(out, dxf)
            .map_err(|e| format!("failed to write DXF \"{out}\": {e}"))?;
    }
    env.history.execute(&env.world, cmd);
    Ok(json!({
        "tool": tool,
        "holder": holder_view(&env.world, tool),
        "dxf_path": dxf_out,
    }))
}

fn s_import_dxf() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name." },
            "path": { "type": "string", "description": "ASCII DXF file containing a LINE on layer CENTRELINE and one open half-profile chain." },
            "units": { "type": "string", "enum": ["mm", "inch"], "description": "Fallback drawing unit when the DXF header omits $INSUNITS." }
        }),
        &["tool", "path"],
    )
}

fn s_export_dxf() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name." },
            "path": { "type": "string", "description": "Destination .dxf path to write (mm)." }
        }),
        &["tool", "path"],
    )
}

fn s_convert_step() -> Value {
    obj(
        json!({
            "tool": { "type": "string", "description": "The document tool's name." },
            "step_path": { "type": "string", "description": "STEP (.step/.stp) holder/body carrying an unambiguous ISO 13399 coordinate system." },
            "dxf_out_path": { "type": "string", "description": "Optional path to also write the conservative half-profile DXF." }
        }),
        &["tool", "step_path"],
    )
}
