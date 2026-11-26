//! `edit_tool` (#1062): a partial tool definition applied through the
//! app's undoable `ChangeTool`, so its gates and refusals are the GUI's.
//!
//! Provenance: our own tool surface over this repo's command layer.

use super::edit::{lost, merge};
use super::tools::req_str;
use crate::commands::ChangeTool;
use crate::model;
use crate::project;
use crate::script::Env;

use serde_json::{json, Map, Value};

/// Keys of a tool definition `ChangeTool` does not own: the number is
/// `SetToolNumber`'s, the body, the library holder (#2016) and the
/// cutter outline their own commands.
const NOT_HERE: [&str; 4] = ["number", "body", "holder", "cutter"];

pub(super) fn edit_tool(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(a, "name")?;
    let Some(Value::Object(patch)) = a.get("patch") else {
        return Err("\"patch\" must be an object of tool fields".into());
    };
    let entity = model::find_by_name(&env.world, name).ok_or_else(|| format!("no tool named '{name}'"))?;
    let prev = model::tool_of(&env.world, &entity).ok_or_else(|| format!("'{name}' is not a tool"))?;
    if let Some(k) = NOT_HERE.iter().find(|k| patch.contains_key(**k)) {
        return Err(format!("bad tool patch: \"{k}\" is not edited by edit_tool - tool {name} keeps its {k}"));
    }
    let mut def = project::tool_to_json(&prev);
    merge(&mut def, &Value::Object(patch.clone()));
    let stated = project::tool_from_json(&def).map_err(|e| format!("bad tool patch: {e}"))?;
    let next = settled(stated.clone());
    // #3369: settling writes a form mill's outline and sizes. A value the
    // patch states and settling replaces is refused, never dropped.
    if let (Value::Object(asked), Value::Object(lands)) = (project::tool_to_json(&stated), project::tool_to_json(&next))
        && let Some((path, want, got)) = replaced(patch, &asked, &lands, "")
    {
        let why = if path.starts_with("form.") {
            "the form's preset writes the outline - set \"form.preset\" to null to draw \"form.points\", or edit the preset's numbers"
        } else {
            "the form outline sets it - edit \"form\" (its preset numbers or its drawn points)"
        };
        return Err(format!("bad tool patch: \"{path}\" {want} is not what form mill {name} gets ({got}): {why}"));
    }
    // serde drops keys it does not know; a dropped edit reads as done.
    let Value::Object(back) = project::tool_to_json(&next) else {
        return Err("the patched tool could not be read back".into());
    };
    let mut checked = patch.clone();
    // Written only when false, so a `true` leaves no trace to find.
    if checked.get("center_cutting") == Some(&Value::Bool(true)) {
        checked.remove("center_cutting");
    }
    // #1152: an electrode's outline is written only when it states one,
    // so an empty one (back to a rectangular face) leaves no trace.
    if checked.get("outline").and_then(Value::as_array).is_some_and(Vec::is_empty) {
        checked.remove("outline");
    }
    // #3368: a sharp tip and a zero tip diameter are written as absent.
    if checked.get("tip_form").and_then(Value::as_str) == Some("sharp") {
        checked.remove("tip_form");
    }
    if checked.get("tip_diameter").and_then(Value::as_f64) == Some(0.0) {
        checked.remove("tip_diameter");
    }
    // #3369: a zero pilot length is written as absent.
    if let Some(Value::Object(form)) = checked.get_mut("form")
        && form.get("pilot_len").and_then(Value::as_f64) == Some(0.0)
    {
        form.remove("pilot_len");
    }
    if let Some(path) = lost(&checked, &back, "") {
        return Err(format!("bad tool patch: unknown field {path} - a tool definition has no field \"{path}\""));
    }
    let cmd = ChangeTool::new(&env.world, name, next).map_err(|e| e.to_string())?;
    env.history.execute(&env.world, cmd);
    let now = model::tool_of(&env.world, &entity).map(|t| project::tool_to_json(&t));
    Ok(json!({ "tool": name, "definition": now }))
}

/// #3369: a form mill read from a definition or a patch, its outline
/// written from its preset and its diameter and flute length from the
/// outline - as the editor's Apply writes them.
pub(super) fn settled(mut t: crate::tools::Tool) -> crate::tools::Tool {
    t.settle_form();
    t
}

/// The first value `patch` states that settling replaced: its path, the
/// value asked for and the value the tool would get. `asked` is the
/// patched tool as stated, `lands` the same tool settled. Numbers agree
/// within the form mill's slack.
fn replaced(patch: &Map<String, Value>, asked: &Map<String, Value>, lands: &Map<String, Value>, prefix: &str) -> Option<(String, Value, Value)> {
    for (k, v) in patch {
        let (Some(a), Some(l)) = (asked.get(k), lands.get(k)) else { continue };
        let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        match (v, a, l) {
            (Value::Null, ..) => {}
            (Value::Object(p), Value::Object(a), Value::Object(l)) => {
                if let Some(hit) = replaced(p, a, l, &path) {
                    return Some(hit);
                }
            }
            _ if !agree(a, l) => return Some((path, a.clone(), l.clone())),
            _ => {}
        }
    }
    None
}

fn agree(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(p, q)| agree(p, q)),
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(p), Some(q)) => (p - q).abs() <= camcraft_tools::form_mill::AGREE,
            _ => x == y,
        },
        _ => a == b,
    }
}
