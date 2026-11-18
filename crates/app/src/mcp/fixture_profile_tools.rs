//! MCP tools for spun and extruded DXF/profile fixtures (#744).
//! Provenance: clean-room MCP surface over `InsertProfileFixture`,
//! `SetFixtureProfile`, `DeriveFixtureProfile`, and `FixtureProfile` DXF I/O.

use std::path::Path;

use serde_json::{Map, Value, json};

use super::setup_tools::{cmd_err, obj, opt_nums, point, setup_or_active};
use super::tools::{Tool, opt_str, req_str};
use crate::commands::{DeriveFixtureProfile, InsertProfileFixture, SetFixtureProfile};
use crate::fixture;
use crate::fixture_profile::{ExtrudeAxis, FixtureProfile};
use crate::geometry::IDENTITY_Q;
use crate::model;
use crate::schema::Location;
use crate::script::Env;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "import_dxf_fixture",
        description: "Import a spun (turning) or extruded (milling) fixture from an ASCII DXF file path or inline DXF text. Spun reads CENTRELINE + half-profile; extruded reads a closed profile + depth (from depth arg or DXF DEPTH annotation/layer) extruded along X/Y/Z. One undoable step.",
        schema: s_import_dxf,
        run: import_dxf_fixture,
    },
    Tool {
        name: "set_fixture_profile",
        description: "Replace a profile fixture's spun or extruded profile geometry (regenerating facets immediately in one undoable step). Refuses STEP fixtures that keep STEP as their source.",
        schema: s_set_profile,
        run: set_fixture_profile,
    },
    Tool {
        name: "derive_fixture_profile",
        description: "Derive a reference spun or extruded profile from an imported STEP fixture's solid mesh while keeping its STEP source and facets intact. Refused by name for a fixture without a STEP source (a DXF/profile fixture: edit its profile with set_fixture_profile). One undoable step.",
        schema: s_derive_profile,
        run: derive_fixture_profile,
    },
    Tool {
        name: "get_fixture_profile",
        description: "Read a fixture's profile definition (spun or extruded), whether it keeps a STEP source, and its facet count.",
        schema: s_fixture_name,
        run: get_fixture_profile,
    },
    Tool {
        name: "export_fixture_dxf",
        description: "Export a profile fixture's spun or extruded outline as ASCII DXF text (optionally writing to path).",
        schema: s_export_dxf,
        run: export_fixture_dxf,
    },
];

fn opt_f64(a: &Map<String, Value>, key: &str) -> Result<Option<f64>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| format!("\"{key}\" must be a number")),
    }
}

fn fixture_view(env: &Env, name: &str) -> Result<Value, String> {
    let e = model::find_by_name(&env.world, name).ok_or_else(|| format!("no fixture named '{name}'"))?;
    let def = fixture::def_of(&env.world, &e).ok_or_else(|| format!("'{name}' is not a fixture"))?;
    let facets = model::facets_of(&env.world, &e).map_or(0, |f| f.len());
    let has_step_source = crate::step_source::of(&env.world, &e).is_some();
    Ok(json!({
        "fixture": name,
        "setup": def.setup,
        "facets": facets,
        "has_step_source": has_step_source,
        "profile": def.profile,
    }))
}

fn import_dxf_fixture(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let kind = req_str(a, "kind")?;
    let path_arg = opt_str(a, "path")?;
    let dxf_arg = opt_str(a, "dxf")?;
    let bytes: Vec<u8> = match (path_arg, dxf_arg) {
        (Some(p), None) => std::fs::read(p).map_err(|e| format!("cannot read '{p}': {e}"))?,
        (None, Some(text)) => text.as_bytes().to_vec(),
        (Some(_), Some(_)) => return Err("give either \"path\" or \"dxf\", not both".into()),
        (None, None) => return Err("either \"path\" or \"dxf\" is required".into()),
    };
    let default_name = path_arg
        .and_then(|p| Path::new(p).file_stem().and_then(|s| s.to_str()))
        .unwrap_or("Fixture");
    let name = opt_str(a, "name")?.unwrap_or(default_name);
    let setup = setup_or_active(env, a)?;
    let pos = opt_nums::<3>(a, "position")?.map_or(camcraft_geometry::geometry::Point3::ZERO, point);
    let rot = opt_nums::<4>(a, "rotation")?.unwrap_or(IDENTITY_Q);
    let loc = Location { pos, rot };
    let cmd = match kind.trim().to_ascii_lowercase().as_str() {
        "spun" => InsertProfileFixture::from_dxf_spun(&env.world, name, &bytes, setup.as_deref(), loc),
        "extruded" => {
            let axis = match opt_str(a, "axis")? {
                Some(ax) => ExtrudeAxis::parse(ax)?,
                None => ExtrudeAxis::Z,
            };
            let depth = opt_f64(a, "depth")?;
            InsertProfileFixture::from_dxf_extruded(&env.world, name, &bytes, setup.as_deref(), axis, depth, loc)
        }
        other => return Err(format!("unknown DXF fixture kind '{other}' (expected \"spun\" or \"extruded\")")),
    }
    .map_err(cmd_err)?;
    let landed = cmd.name().to_owned();
    env.history.execute(&env.world, cmd);
    fixture_view(env, &landed)
}

fn set_fixture_profile(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let prof_val = a
        .get("profile")
        .ok_or_else(|| "\"profile\" object is required".to_owned())?;
    let profile: FixtureProfile =
        serde_json::from_value(prof_val.clone()).map_err(|e| format!("invalid profile: {e}"))?;
    let cmd = SetFixtureProfile::new(&env.world, name, profile).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    fixture_view(env, name)
}

fn derive_fixture_profile(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let kind = req_str(a, "kind")?;
    let cmd = match kind.trim().to_ascii_lowercase().as_str() {
        "spun" => DeriveFixtureProfile::spun(&env.world, name),
        "extruded" => {
            let axis = match opt_str(a, "axis")? {
                Some(ax) => ExtrudeAxis::parse(ax)?,
                None => ExtrudeAxis::Z,
            };
            DeriveFixtureProfile::extruded(&env.world, name, axis)
        }
        other => return Err(format!("unknown profile kind '{other}' (expected \"spun\" or \"extruded\")")),
    }
    .map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    fixture_view(env, name)
}

fn get_fixture_profile(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    fixture_view(env, req_str(a, "name")?)
}

fn export_fixture_dxf(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let e = model::find_by_name(&env.world, name).ok_or_else(|| format!("no fixture named '{name}'"))?;
    let prof = fixture::def_of(&env.world, &e)
        .ok_or_else(|| format!("'{name}' is not a fixture"))?
        .profile
        .ok_or_else(|| format!("fixture '{name}' has no editable profile to export"))?;
    let dxf = prof.to_dxf()?;
    if let Some(p) = opt_str(a, "path")? {
        std::fs::write(p, dxf.as_bytes()).map_err(|err| format!("cannot write '{p}': {err}"))?;
    }
    Ok(json!({ "fixture": name, "kind": prof.kind_label(), "dxf": dxf }))
}

fn s_fixture_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "Fixture entity name." } }), &["name"])
}

fn s_import_dxf() -> Value {
    obj(
        json!({
            "kind": { "type": "string", "enum": ["spun", "extruded"], "description": "Spun (turning half-profile + CENTRELINE) or extruded (closed 2D profile)." },
            "path": { "type": "string", "description": "Path to ASCII DXF file." },
            "dxf": { "type": "string", "description": "Inline ASCII DXF text (alternative to path)." },
            "name": { "type": "string", "description": "Optional base name for the fixture model." },
            "axis": { "type": "string", "enum": ["x", "y", "z"], "description": "Extrusion axis for extruded kind (default z)." },
            "depth": { "type": "number", "description": "Extrusion depth in mm (overrides DXF DEPTH annotation)." },
            "setup": { "type": "string", "description": "Target setup name; defaults to active setup." },
            "position": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3, "description": "Origin [x, y, z] mm." },
            "rotation": { "type": "array", "items": { "type": "number" }, "minItems": 4, "maxItems": 4, "description": "Unit quaternion [x, y, z, w]." }
        }),
        &["kind"],
    )
}

fn s_set_profile() -> Value {
    let spun_pt = json!({
        "type": "object",
        "properties": {
            "z": { "type": "number" },
            "r": { "type": "number" },
            "bulge": { "type": "number" }
        },
        "required": ["z", "r"],
        "additionalProperties": false
    });
    let ext_pt = json!({
        "type": "object",
        "properties": {
            "u": { "type": "number" },
            "v": { "type": "number" },
            "bulge": { "type": "number" }
        },
        "required": ["u", "v"],
        "additionalProperties": false
    });
    let spun_schema = json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string", "enum": ["spun"] },
            "points": { "type": "array", "items": spun_pt, "minItems": 2 }
        },
        "required": ["kind", "points"],
        "additionalProperties": false
    });
    let ext_schema = json!({
        "type": "object",
        "properties": {
            "kind": { "type": "string", "enum": ["extruded"] },
            "axis": { "type": "string", "enum": ["x", "y", "z"] },
            "depth": { "type": "number" },
            "points": { "type": "array", "items": ext_pt, "minItems": 3 }
        },
        "required": ["kind", "axis", "depth", "points"],
        "additionalProperties": false
    });
    obj(
        json!({
            "name": { "type": "string", "description": "Profile fixture name." },
            "profile": { "oneOf": [spun_schema, ext_schema], "description": "Replacement spun or extruded profile definition." }
        }),
        &["name", "profile"],
    )
}

fn s_derive_profile() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Fixture entity name." },
            "kind": { "type": "string", "enum": ["spun", "extruded"], "description": "Profile kind to derive." },
            "axis": { "type": "string", "enum": ["x", "y", "z"], "description": "Projection/extrusion axis when kind is extruded (default z)." }
        }),
        &["name", "kind"],
    )
}

fn s_export_dxf() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Profile fixture name." },
            "path": { "type": "string", "description": "Optional destination path to write the DXF text." }
        }),
        &["name"],
    )
}
