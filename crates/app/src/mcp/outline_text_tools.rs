//! The MCP outline-text tool (#3072): `insert_outline_text` places a
//! typed string in a TrueType outline font as a profile of closed shapes
//! through the undoable `InsertOutlineText`. One History step; every
//! refusal is the command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's `InsertOutlineText`
//! command; no vendor material consulted.

use super::setup_tools::{cmd_err, obj, opt_nums};
use super::tools::{opt_str, req_str, Tool};
use crate::commands::{InsertOutlineText, OutlineTextOptions};
use crate::script::{dxf, Env};

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "insert_outline_text",
    description: "Place a typed string in a TrueType outline font as a profile of closed shapes on the top face of a setup's stock - the active setup by default. The shapes take a pocket, a V-carve or a cut-out (insert_text gives single-line engraving strokes instead). font is the path of a .ttf file, or an .otf file with TrueType outlines; the profile keeps the shapes, not the font. height is the font's cap height in mm (0.5 to 1000). position [x, y] is the left end of the baseline in mm from the stock origin (default [0, 0]); angle turns the baseline in degrees about the face normal, counterclockwise seen from above (default 0). letter_spacing adds a gap between characters in mm (default 0). The shapes are lines in mm on the layer \"text\"; a letter's hole (O, A) is a hole of its shape. move_dxf_profile moves the profile, and get_job lists it under \"profiles\". One undo step. Refused by name: a font that cannot be read, a font with CFF outlines or a font collection, an empty text, a character the font does not have, outlines that cross or touch, a height or spacing outside the limits, a setup with no stock, an unknown setup, a name in use.",
    schema: s_insert_outline_text,
    run: insert_outline_text,
}];

fn insert_outline_text(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let num = |key: &str| match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v.as_f64().map(Some).ok_or_else(|| format!("\"{key}\" must be a number, got {v}")),
    };
    let [x, y] = opt_nums::<2>(a, "position")?.unwrap_or_default();
    let options = OutlineTextOptions {
        font: req_str(a, "font")?.into(),
        text: req_str(a, "text")?.to_owned(),
        height: num("height")?.ok_or("\"height\" is required")?,
        letter_spacing: num("letter_spacing")?.unwrap_or(0.0),
        x,
        y,
        angle_deg: num("angle")?.unwrap_or(0.0),
        name: opt_str(a, "name")?.map(str::to_owned),
    };
    let cmd = InsertOutlineText::new(&env.world, opt_str(a, "setup")?, &options).map_err(cmd_err)?;
    let name = cmd.name().to_owned();
    env.history.execute(&env.world, cmd);
    let mut out = dxf::profile_view(&env.world, &name).ok_or_else(|| format!("the text profile '{name}' did not land"))?;
    out["text"] = json!(options.text);
    out["height"] = json!(options.height);
    out["summary"] = json!(format!(
        "'{name}' placed on the stock top: \"{}\" at a cap height of {} mm, {} closed shapes",
        options.text, options.height, out["chains"]["closed"],
    ));
    Ok(out)
}

fn s_insert_outline_text() -> Value {
    obj(
        json!({
            "font": { "type": "string", "description": "The TrueType font file: .ttf, or .otf with TrueType outlines." },
            "text": { "type": "string", "description": "The string, on one line." },
            "height": { "type": "number", "description": "The font's cap height in mm (0.5 to 1000)." },
            "position": { "type": "array", "items": {"type": "number"}, "minItems": 2, "maxItems": 2,
                "description": "[x, y] of the left end of the baseline on the stock's top face, in mm from the stock origin (default [0, 0])." },
            "angle": { "type": "number", "description": "Turn of the baseline about the face normal in degrees, counterclockwise seen from above (default 0)." },
            "letter_spacing": { "type": "number", "description": "Extra gap between two characters in mm (default 0; 0 to 1000)." },
            "setup": { "type": "string", "description": "The setup whose stock takes the text; omitted: the active setup." },
            "name": { "type": "string", "description": "The profile's name; omitted: \"Text <string>\", made unique." }
        }),
        &["font", "text", "height"],
    )
}
