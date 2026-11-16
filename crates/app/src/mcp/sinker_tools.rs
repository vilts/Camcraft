//! The MCP tool for a sinker EDM electrode generated from a cavity's
//! picked faces (#547): `electrode_from_faces`. It runs the app's
//! undoable `ElectrodeFromFaces`, so one `undo` removes the tool, and
//! every refusal - a face set that is no single cavity, a
//! garbled number, a taken name - comes back as its text.
//!
//! Provenance: our own tool surface over this repo's command layer.

use super::pick::{pick_list_schema, picks_of};
use super::tools::{opt_str, req_str, Tool};
use crate::commands::ElectrodeFromFaces;
use crate::operation::sinker_faces::{condition_undersize, undersize_warning};
use crate::script::{fnum, Env};
use crate::setup;
use crate::sinker_electrode::ElectrodeSpec;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "electrode_from_faces", description: "Generate a sinker EDM electrode from the picked faces of a part's cavity - its level floor and every vertical wall standing on it (list_features ids) - and add it as a document tool. The electrode is the cavity's outline offset inward by the undersize (stated in mm, or read from a machine EDM condition by id) plus an optional orbit radius, and stands a stated stub above the cavity's mouth. Answers with the electrode and the burn point, cavity size and heights a Sinker Burn of this cavity states. An axis-aligned rectangular cavity yields a rectangular electrode (shape \"rectangle\"); any other outline of planar vertical walls yields a body electrode (shape \"outline\") whose record carries its section, as [x, y] points in mm about the burn point. A face set that is no single cavity is refused by name. One undo step.", schema: s_electrode, run: electrode_from_faces },
];

/// A required finite number of mm.
fn req_mm(a: &Map<String, Value>, key: &str) -> Result<f64, String> {
    opt_mm(a, key)?.ok_or_else(|| format!("\"{key}\" is required (mm)"))
}

fn opt_mm(a: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|x| x.is_finite())
            .map(Some)
            .ok_or_else(|| format!("\"{key}\" must be a finite number of mm, got {v}")),
    }
}

fn electrode_from_faces(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let picks = picks_of(env, a.get("picks"), "picks")?;
    let condition = opt_str(a, "condition")?;
    let setup_name = opt_str(a, "setup")?;
    if let Some(s) = setup_name
        && !setup::exists(&env.world, s)
    {
        return Err(format!("no setup named '{s}'"));
    }
    // The undersize is stated, or read from the machine's condition -
    // never both, never neither (absent is not zero).
    let undersize = match (opt_mm(a, "undersize")?, condition) {
        (Some(u), None) => u,
        (None, Some(id)) => condition_undersize(&env.world, setup_name, id)?,
        (Some(_), Some(_)) => return Err("give \"undersize\" or \"condition\", not both".into()),
        (None, None) => return Err("give the electrode's \"undersize\" (mm per side) or the EDM \"condition\" to read it from".into()),
    };
    if setup_name.is_some() && condition.is_none() {
        return Err("\"setup\" goes with \"condition\" (the setup whose machine states it)".into());
    }
    let spec = ElectrodeSpec { undersize, orbit: opt_mm(a, "orbit")?, stub: req_mm(a, "stub")?, pickup: req_mm(a, "pickup")? };
    let cmd = ElectrodeFromFaces::new(&env.world, name, &picks, &spec).map_err(|e| e.to_string())?;
    let e = cmd.electrode().clone();
    env.history.execute(&env.world, cmd);
    Ok(json!({
        "tool": name,
        "electrode": {
            "x": fnum(e.face.0), "y": fnum(e.face.1), "length": fnum(e.length), "pickup": fnum(e.pickup),
            // #1152: a body electrode states its section about the burn point.
            "shape": if e.body { "outline" } else { "rectangle" },
            "outline": e.body.then(|| e.tool().outline.iter().map(|p| json!([fnum(p[0]), fnum(p[1])])).collect::<Vec<_>>()),
        },
        "undersize": fnum(undersize),
        "condition": condition,
        "orbit": fnum(e.orbit),
        "inset": fnum(e.inset),
        "cavity": {
            "x": fnum(e.cavity.0), "y": fnum(e.cavity.1),
            "center": [fnum(e.center.0), fnum(e.center.1)],
            "floor_z": fnum(e.floor_z), "top_z": fnum(e.top_z), "depth": fnum(e.depth),
        },
        // The fields a Sinker Burn of this cavity states (add_operation params).
        "burn": {
            "x": fnum(e.center.0), "y": fnum(e.center.1),
            "cavity_x": fnum(e.cavity.0), "cavity_y": fnum(e.cavity.1),
            "depth": fnum(e.depth),
            "top_z": { "reference": "fixed", "offset": fnum(e.top_z) },
            "floor_z": { "reference": "fixed", "offset": fnum(e.floor_z) },
        },
        "frame": "world",
        "note": (e.orbit > 0.0).then(|| format!(
            "made {:.3} mm smaller per side for an orbit of that radius: burn it as a single Sinker Burn with an orbit of that radius (#544) - a rough or finish takes no orbit until the size law counts it (#1355), and judges this electrode as the wrong size for its condition",
            e.orbit
        )),
        "warning": undersize_warning(undersize),
    }))
}

fn s_electrode() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string", "description": "Name for the new electrode tool." },
            "picks": pick_list_schema("The cavity's faces: its level floor and every vertical wall standing on it (face:0x… ids from list_features, all on one model)."),
            "undersize": { "type": "number", "description": "The electrode's undersize per side, mm (0 or more). Give this or condition." },
            "condition": { "type": "string", "description": "A machine EDM condition id: its undersize sizes the electrode. Give this or undersize." },
            "setup": { "type": "string", "description": "With condition: the setup whose machine states it (default the active setup)." },
            "orbit": { "type": "number", "description": "Optional orbit radius, mm: the electrode is made that much smaller again per side." },
            "stub": { "type": "number", "description": "How far the electrode stands above the cavity's mouth, mm (its holder stub; more than 0)." },
            "pickup": { "type": "number", "description": "The pickup offset, mm: from the electrode's top face up to the reference the control measures it from (0 or more)." }
        },
        "required": ["name", "picks", "stub", "pickup"],
        "additionalProperties": false,
    })
}
