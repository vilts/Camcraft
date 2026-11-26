//! The MCP SVG-profile tool (#1880): `import_svg` places an SVG drawing on the stock of a
//! setup through the undoable `ImportSvg`. The profile is the same entity as a DXF profile:
//! `move_dxf_profile` moves it and `get_job` lists it under "profiles". Every refusal is the
//! command's own text, returned as a tool error.
//!
//! Provenance: our own tool surface over this repo's `ImportSvg` command; no vendor material
//! consulted.

use super::setup_tools::obj;
use super::tools::{opt_str, req_str, Tool};
use crate::script::{dxf, Env};

use serde_json::{json, Map, Value};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "import_svg",
    description: "Place an SVG drawing (path, rect, circle, ellipse, line, polyline, polygon; groups with transforms; use of an element in the same file) as a profile on the top face of a setup's stock, at the stock origin - the active setup by default. Like import_dxf: set_stock first, a setup needs no model, move_dxf_profile moves the profile and get_job lists it under \"profiles\". The file's width and height (mm, cm, in, pt, pc or px at 96 to the inch) and viewBox give the size; Y is flipped so the drawing looks as it does in the editor. A file that states no physical size is refused by name until units (\"mm\" or \"in\", the size of one drawing unit) is given (never guessed); a file that states its size refuses units. Circular arcs stay arcs; Beziers and other ellipses become lines within 0.005 mm. Top-level groups are layers (Inkscape label, else id). Text, images, clip paths, masks, styles and references to other files are skipped and counted by kind; fill and stroke are not read. One undo step. Refused by name: a setup with no stock, an unknown setup, a file that is not .svg, malformed XML or path data, a zero or negative size, a degenerate arc, a file larger than the import budget, a drawing with nothing to place, a name in use.",
    schema: s_import_svg,
    run: import_svg,
}];

fn import_svg(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let mut options = Map::new();
    for key in ["setup", "units", "name"] {
        if let Some(v) = opt_str(a, key)? {
            options.insert(key.to_owned(), json!(v));
        }
    }
    if let Some(v) = a.get("join_tol").filter(|v| !v.is_null()) {
        options.insert("join_tol".to_owned(), v.clone());
    }
    dxf::v_import_svg(env, vec![json!(req_str(a, "path")?), Value::Object(options)])
}

fn s_import_svg() -> Value {
    obj(
        json!({
            "path": { "type": "string", "description": "The .svg file (plain SVG, not .svgz)." },
            "setup": { "type": "string", "description": "The setup whose stock takes the profile; omitted: the active setup." },
            "units": { "type": "string", "enum": ["mm", "in"], "description": "The size of one drawing unit, for a file that states no physical size only." },
            "join_tol": { "type": "number", "description": "Chain ends closer than this join, in mm (default 0.01; 0.00001 to 10)." },
            "name": { "type": "string", "description": "The profile's name; omitted: the file's stem, made unique." }
        }),
        &["path"],
    )
}
