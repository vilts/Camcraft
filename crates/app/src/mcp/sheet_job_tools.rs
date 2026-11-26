//! The MCP sheet-job tool (#3070): set up the active setup as a flat-sheet
//! router job in one call - one `SetSheetJob` run through the session
//! History, so one `undo` puts the stock, the WCS and the record back.
//! Every refusal is the command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's setup command.

use super::setup_tools::{cmd_err, obj};
use super::tools::Tool;
use crate::commands::SetSheetJob;
use crate::script::{fnum, Env};
use crate::sheet_job::{SheetCorner, SheetJob, SheetZero};

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "set_sheet_job",
    description: "Set up the active setup as a flat-sheet router job, as one undo step: the stock becomes a box of the sheet's length (X), width (Y) and thickness; the WCS moves to Z zero at the sheet top or the spoilboard and XY zero on the stated corner (front_left by default). The through-cut allowance (how far a cut may go into the spoilboard, mm) is required and always stated; the post refuses by name a move below the sheet bottom less the allowance, or a rapid across below the sheet top plus the clamp clearance. {\"clear\": true} drops only the record. Sizes in mm.",
    schema: s_sheet_job,
    run: set_sheet_job,
}];

fn num(a: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => n.as_f64().map(Some).ok_or_else(|| format!("\"{key}\" {n} is not a number")),
        Some(other) => Err(format!("\"{key}\" must be a number in mm, got {other}")),
    }
}

fn need(a: &Map<String, Value>, key: &str) -> Result<f64, String> {
    num(a, key)?.ok_or_else(|| format!("\"{key}\" is required (mm)"))
}

fn word<'a>(a: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => Err(format!("\"{key}\" must be a word, got {other}")),
    }
}

/// The job as the tool returns it (and `get_job` shows it).
pub(super) fn job_view(j: &SheetJob) -> Value {
    json!({
        "length": fnum(j.length),
        "width": fnum(j.width),
        "thickness": fnum(j.thickness),
        "z_zero": j.z_zero.word(),
        "xy_zero": j.xy_zero.word(),
        "allowance": fnum(j.allowance),
        "clamp_clearance": fnum(j.clamp_clearance),
    })
}

fn set_sheet_job(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let cmd = if a.get("clear").and_then(Value::as_bool) == Some(true) {
        SetSheetJob::clear(&env.world).map_err(cmd_err)?
    } else {
        let z_zero = match word(a, "z_zero")? {
            None => return Err("\"z_zero\" is required: \"top\" (the sheet's top face) or \"spoilboard\"".into()),
            Some(w) => SheetZero::from_word(w).ok_or_else(|| format!("\"z_zero\" '{w}' is not \"top\" or \"spoilboard\""))?,
        };
        let xy_zero = match word(a, "xy_zero")? {
            None => SheetCorner::FrontLeft,
            Some(w) => SheetCorner::from_word(w).ok_or_else(|| {
                format!("\"xy_zero\" '{w}' is not front_left, front_right, back_left, back_right or centre")
            })?,
        };
        let job = SheetJob {
            length: need(a, "length")?,
            width: need(a, "width")?,
            thickness: need(a, "thickness")?,
            z_zero,
            xy_zero,
            allowance: need(a, "allowance")?,
            clamp_clearance: num(a, "clamp_clearance")?.unwrap_or(0.0),
        };
        SetSheetJob::new(&env.world, job).map_err(cmd_err)?
    };
    let setup = cmd.setup().to_owned();
    env.history.execute(&env.world, cmd);
    let now = crate::sheet_job::of(&env.world, &setup);
    let (_, origin) = crate::posting::wcs_context(&env.world, Some(&setup));
    Ok(json!({
        "setup": setup,
        "sheet_job": now.as_ref().map(job_view),
        "wcs_origin": origin.map(fnum),
    }))
}

fn s_sheet_job() -> Value {
    obj(
        json!({
            "length": { "type": "number", "description": "Sheet size along X, mm." },
            "width": { "type": "number", "description": "Sheet size along Y, mm." },
            "thickness": { "type": "number", "description": "Sheet thickness, mm." },
            "z_zero": { "type": "string", "enum": ["top", "spoilboard"], "description": "Where Z zero is: the sheet's top face, or the spoilboard (the sheet's bottom face)." },
            "xy_zero": { "type": "string", "enum": ["front_left", "front_right", "back_left", "back_right", "centre"], "description": "Where XY zero is on the sheet, seen from the machine front; default front_left." },
            "allowance": { "type": "number", "description": "How far a through cut may go into the spoilboard, mm (0 to 5). Required - never assumed." },
            "clamp_clearance": { "type": "number", "description": "How high clamps or screw heads stand above the sheet, mm; rapids across stay above it. Default 0." },
            "clear": { "type": "boolean", "description": "true drops the setup's sheet-job record only." }
        }),
        &[],
    )
}
