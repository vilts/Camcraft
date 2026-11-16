//! The MCP stock-fit tool (#1538): the Stock page's "Fit to Models" -
//! box, cylinder or per-side allowances - over the active setup's PARTS,
//! never its fixtures. It builds the very undoable `SetStock` the GUI's
//! Fit button runs (`stock_fit::fit_command`), one History step.
//!
//! Provenance: our own tool surface over this repo's stock_fit rule.

use serde_json::{Map, Value, json};

use super::setup_tools::{obj, opt_nums};
use super::tools::{Tool, opt_str};
use crate::project;
use crate::script::Env;
use crate::stock_fit::{self, FitKind};

pub(super) const TOOLS: &[Tool] = &[Tool {
    name: "fit_stock",
    description: "Fit the active setup's stock to its part models, as the Stock page's Fit to Models does: a box (`shape` \"box\", the default) or a Z cylinder snug about the parts (\"cylinder\") with `margin` mm on every side (default 2), or a box with `sides` = [-X, +X, -Y, +Y, -Z, +Z] allowances in mm. Only the parts are measured - vises, accessories, plates and tagged fixtures are left out. The stock keeps its rotation. One undoable step; refused by name with no models, a negative allowance or a flat part.",
    schema: s_fit_stock,
    run: fit_stock,
}];

fn fit_stock(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let margin = match a.get("margin") {
        None | Some(Value::Null) => 2.0,
        Some(v) => v.as_f64().filter(|m| m.is_finite()).ok_or("\"margin\" must be a finite number")?,
    };
    let sides = opt_nums::<6>(a, "sides")?;
    let kind = match (opt_str(a, "shape")?, sides) {
        (None | Some("box"), Some(s)) => FitKind::Sides(s),
        (_, Some(_)) => return Err("\"sides\" fits a box only".into()),
        (None | Some("box"), None) => FitKind::Box(margin),
        (Some("cylinder"), None) => FitKind::Cylinder(margin),
        (Some(other), None) => return Err(format!("\"shape\" must be \"box\" or \"cylinder\", not \"{other}\"")),
    };
    let (cmd, shape) = stock_fit::fit_command(&env.world, kind)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "stock": project::stock_to_json(&shape) }))
}

fn s_fit_stock() -> Value {
    obj(
        json!({
            "shape": { "type": "string", "enum": ["box", "cylinder"], "description": "The stock shape to fit (default box)." },
            "margin": { "type": "number", "description": "Allowance in mm on every side (default 2)." },
            "sides": { "type": "array", "items": {"type": "number"}, "minItems": 6, "maxItems": 6,
                "description": "Per-side allowances in mm: [-X, +X, -Y, +Y, -Z, +Z]; a box fit (replaces margin)." }
        }),
        &[],
    )
}
