//! The MCP DXF-profile tools (#1862): `import_dxf` places a DXF drawing
//! on the stock of a setup through the undoable `ImportDxf`, and
//! `move_dxf_profile` moves it on the stock face through `MoveSketch` -
//! the commands the Stock page uses. `insert_text` (#2679) places a typed
//! string as a profile of engraving strokes through `InsertText`. Each is
//! one History step, and every refusal is the command's own text,
//! returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's `ImportDxf`,
//! `MoveSketch` and `InsertText` commands; no vendor material consulted.

use super::setup_tools::{cmd_err, obj};
use super::tools::{opt_str, req_str, Tool};
use crate::commands::MoveSketch;
use crate::model;
use crate::script::{dxf, text, Env};
use crate::sketch;

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "import_dxf",
        description: "Place a DXF drawing (ASCII DXF: LINE, ARC, CIRCLE, LWPOLYLINE, POLYLINE) as a profile on the top face of a setup's stock, at the stock origin - the active setup by default. A profile is reference geometry for cutting, not a model: set_stock first, a setup needs no model. units \"mm\" or \"in\" wins over the file's $INSUNITS; a file that states no unit is refused by name until units is given (never guessed). The reply states the units used, the layers, the closed, open and rejected chains, and the skipped entities by kind. One undo step. Refused by name: a setup with no stock, an unknown setup, a file that is not .dxf, a malformed or binary DXF, a drawing with nothing to place, a name in use.",
        schema: s_import_dxf,
        run: import_dxf,
    },
    Tool {
        name: "move_dxf_profile",
        description: "Move a placed DXF profile on the top face of its setup's stock: position [x, y] in mm from the stock origin, and an optional angle in degrees about the face normal, counterclockwise seen from above (omitted keeps the current one). A profile has no Z and no tilt: it stays on the face and moves with the stock. One undo step. Refused by name: an unknown name, a name that is not a DXF profile, a placement that is not finite.",
        schema: s_move_dxf_profile,
        run: move_dxf_profile,
    },
    Tool {
        name: "insert_text",
        description: "Place a typed string as a profile of single-line engraving strokes on the top face of a setup's stock - the active setup by default. The strokes are tool-centreline paths in mm on the layer \"text\", made from CamCraft's own stroke font: capital letters, digits, the space and - _ . : / # + = ( ) only. height is the cap height in mm (0.5 to 1000). position [x, y] is the left end of the baseline in mm from the stock origin (default [0, 0]); angle turns the baseline in degrees about the face normal, counterclockwise seen from above (default 0). letter_spacing adds a gap between characters in mm (default 0). The result is a profile like one from import_dxf: move_dxf_profile moves it, and get_job lists it under \"profiles\". One undo step. Refused by name: an empty text, a character with no glyph (a lower-case letter is not changed to a capital), a height or spacing outside the limits, a setup with no stock, an unknown setup, a name in use.",
        schema: s_insert_text,
        run: insert_text,
    },
];

fn insert_text(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    text::v_insert_text(env, vec![Value::String(req_str(a, "text")?.to_owned()), Value::Object(a.clone())])
}

fn s_insert_text() -> Value {
    obj(
        json!({
            "text": { "type": "string", "description": "The string to engrave, on one line: capital letters, digits, the space and - _ . : / # + = ( )." },
            "height": { "type": "number", "description": "The cap height in mm (0.5 to 1000)." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2,
                "description": "[x, y] of the left end of the baseline on the stock's top face, in mm from the stock origin (default [0, 0])." },
            "angle": { "type": "number", "description": "Turn of the baseline about the face normal in degrees, counterclockwise seen from above (default 0)." },
            "letter_spacing": { "type": "number", "description": "Extra gap between two characters in mm (default 0; 0 to 1000)." },
            "setup": { "type": "string", "description": "The setup whose stock takes the text; omitted: the active setup." },
            "name": { "type": "string", "description": "The profile's name; omitted: \"Text <string>\", made unique." }
        }),
        &["text", "height"],
    )
}

fn import_dxf(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let mut options = Map::new();
    for key in ["setup", "units", "name"] {
        if let Some(v) = opt_str(a, key)? {
            options.insert(key.to_owned(), json!(v));
        }
    }
    if let Some(v) = a.get("join_tol").filter(|v| !v.is_null()) {
        options.insert("join_tol".to_owned(), v.clone());
    }
    dxf::v_import_dxf(env, vec![json!(req_str(a, "path")?), Value::Object(options)])
}

fn s_import_dxf() -> Value {
    obj(
        json!({
            "path": { "type": "string", "description": "The .dxf file." },
            "setup": { "type": "string", "description": "The setup whose stock takes the profile; omitted: the active setup." },
            "units": { "type": "string", "enum": ["mm", "in"], "description": "The drawing's unit. Required when the file states none; wins over the file's $INSUNITS." },
            "join_tol": { "type": "number", "description": "Chain ends closer than this join, in mm (default 0.01; 0.00001 to 10)." },
            "name": { "type": "string", "description": "The profile's name; omitted: the file's stem, made unique." }
        }),
        &["path"],
    )
}

fn move_dxf_profile(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let [x, y] = super::setup_tools::opt_nums::<2>(a, "position")?.ok_or("\"position\" is required")?;
    let angle = match a.get("angle") {
        None | Some(Value::Null) => model::find_by_name(&env.world, name)
            .and_then(|e| model::transform_of(&env.world, &e))
            .and_then(sketch::face_placement)
            .map_or(0.0, |(_, _, angle)| angle),
        Some(v) => v.as_f64().ok_or_else(|| format!("\"angle\" must be a number in degrees, got {v}"))?,
    };
    let cmd = MoveSketch::new(&env.world, name, x, y, angle).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    dxf::profile_view(&env.world, name).ok_or_else(|| format!("'{name}' is not a DXF profile"))
}

fn s_move_dxf_profile() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "A placed DXF profile's name (get_job lists them under \"profiles\")." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2,
                "description": "[x, y] on the stock's top face, in mm from the stock origin." },
            "angle": { "type": "number", "description": "Turn about the face normal in degrees, counterclockwise seen from above; omitted keeps the current angle." }
        }),
        &["name", "position"],
    )
}

/// The `get_job` rows: every placed profile, and for each setup what
/// stands for its model ("solid", "virtual" with its profiles, or none).
pub(super) fn profiles_view(env: &Env) -> Vec<Value> {
    sketch::sketches(&env.world).iter().filter_map(|(name, _)| dxf::profile_view(&env.world, name)).collect()
}

/// What stands for a setup's model: a part (`"solid"`), the virtual
/// model reference of its placed profiles (`"virtual"`), or nothing.
pub(super) fn model_reference(env: &Env, setup: Option<&str>) -> Value {
    let w = &env.world;
    match crate::sketch_model::of(w, setup) {
        Some(v) => json!({ "kind": "virtual", "profiles": v.profiles, "solid": false }),
        None if crate::sketch_model::has_part(w) => json!({ "kind": "solid", "solid": true }),
        None => Value::Null,
    }
}
