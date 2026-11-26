//! The MCP `set_stock` tool: a box, a cylinder or a copy of a model as the
//! active setup's stock, upright or turned (#2576: `axis` or `angles_deg`),
//! through the same undoable `SetStock` as the Stock page's Apply.
//!
//! Provenance: our own tool surface over this repo's command layer.

use serde_json::{Map, Value, json};

use super::tools::opt_str;
use crate::commands::SetStock;
use crate::geometry::stock_axis::{StockAxis, stock_axis_of, stock_axis_rot};
use crate::project;
use crate::script::Env;
use crate::stock::StockShape;

/// #2576: the stock's turn from the optional `axis` ("z", "x", "y") or
/// `angles_deg` ({yaw, pitch, roll}); one of them at most.
fn stock_turn(a: &Map<String, Value>) -> Result<[f64; 4], String> {
    let axis = match (opt_str(a, "axis")?, a.get("angles_deg")) {
        (Some(_), Some(_)) => return Err("give \"axis\" or \"angles_deg\", not both".into()),
        (None, None) | (Some("z" | "Z"), None) => StockAxis::Z,
        (Some("x" | "X"), None) => StockAxis::X,
        (Some("y" | "Y"), None) => StockAxis::Y,
        (Some(other), None) => return Err(format!("bad axis \"{other}\": give \"z\", \"x\" or \"y\"")),
        (None, Some(v)) => {
            let ang = |k: &str| match v.get(k) {
                None => Ok(0.0),
                Some(n) => n.as_f64().ok_or_else(|| format!("angles_deg.{k} must be a number")),
            };
            let angles = [ang("yaw")?, ang("pitch")?, ang("roll")?];
            return stock_axis_rot(StockAxis::Custom, angles).ok_or_else(|| "a stock angle is not a finite number".into());
        }
    };
    stock_axis_rot(axis, [0.0; 3]).ok_or_else(|| "bad stock axis".into())
}

pub(super) fn set_stock(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let rot = stock_turn(a)?;
    let given = a.keys().filter(|k| !matches!(k.as_str(), "axis" | "angles_deg")).count();
    if given != 1 {
        return Err("give exactly one of \"box\", \"cylinder\" or \"from_model\"".into());
    }
    let shape = match (a.get("box"), a.get("cylinder"), opt_str(a, "from_model")?) {
        (Some(b), None, None) => project::stock_from_json(&json!({ "box": b })),
        (None, Some(c), None) => project::stock_from_json(&json!({ "cylinder": c })),
        (None, None, Some(m)) => Ok(StockShape::FromModel { source: m.to_owned() }),
        _ => Err("give exactly one stock shape".into()),
    }
    .map_err(|e| format!("bad stock: {e}"))?;
    let at = crate::schema::Location { rot, ..Default::default() };
    let cmd = SetStock::new(&env.world, shape.clone(), at).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    let (axis, angles) = stock_axis_of(rot);
    Ok(json!({ "stock": project::stock_to_json(&shape), "axis": axis.label(), "angles_deg": angles }))
}
