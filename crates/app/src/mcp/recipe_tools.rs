//! The MCP cut recipe tools (#3073): save an operation's settings as a
//! named recipe in the user's recipe library, list the library, and
//! apply a recipe to new geometry through the app's undoable
//! `ApplyCutRecipe` - one History step, so `undo` removes the operation.
//!
//! Provenance: our own tool surface over this repo's cut_recipe module
//! and ApplyCutRecipe command.

use std::path::PathBuf;

use serde_json::{Map, Value, json};

use super::edit::find_op;
use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, opt_str, req_str};
use crate::commands::ApplyCutRecipe;
use crate::cut_recipe::{self, CutRecipe};
use crate::model;
use crate::operation::SrcRef;
use crate::script::Env;

pub(super) const TOOLS: &[Tool] = &[
    Tool {
        name: "save_cut_recipe",
        description: "Save an operation's settings (its kind, tool, every parameter such as depths, stepdown, feeds, tabs, leads and ramps, and the workpiece material in force) as a named cut recipe in the user's recipe library. Not a document change: no undo step. A name already in the library is refused unless replace is true.",
        schema: s_save,
        run: save_cut_recipe,
    },
    Tool {
        name: "list_cut_recipes",
        description: "The cut recipes in the user's recipe library: name, operation kind, tool and material of each.",
        schema: s_list,
        run: list_cut_recipes,
    },
    Tool {
        name: "apply_cut_recipe",
        description: "Add an operation from a saved cut recipe, on the stock, a model or a placed DXF profile, in the active setup. One undo step removes it. Refused by the recipe's name: a recipe not in the library, a tool no longer in the document, a material the table no longer has or one other than the material in force. Every add_operation check applies. Not generated until generate runs.",
        schema: s_apply,
        run: apply_cut_recipe,
    },
];

fn library(a: &Map<String, Value>) -> Result<PathBuf, String> {
    match opt_str(a, "library")? {
        Some(p) if !p.trim().is_empty() => Ok(PathBuf::from(p)),
        _ => cut_recipe::user_dir()
            .map(|d| cut_recipe::library_path(&d))
            .ok_or_else(|| format!("no user folder for the recipe library - set {}", cut_recipe::ENV_VAR)),
    }
}

fn row(r: &CutRecipe) -> Value {
    json!({ "name": r.name, "kind": r.params.kind().info().label, "tool": r.tool, "material": r.material })
}

fn save_cut_recipe(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let key = find_op(env, req_str(a, "operation")?)?;
    let recipe = CutRecipe::capture(&env.world, &key, req_str(a, "name")?)?;
    let replace = match a.get("replace") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("\"replace\" must be true or false, got {other}")),
    };
    let path = library(a)?;
    let out = json!({ "recipe": recipe.name, "library": path.display().to_string(), "saved": row(&recipe) });
    cut_recipe::add(&path, recipe, replace).map_err(|e| e.to_string())?;
    Ok(out)
}

fn list_cut_recipes(_env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let path = library(a)?;
    let lib = cut_recipe::load(&path).map_err(|e| e.to_string())?;
    Ok(json!({ "library": path.display().to_string(), "recipes": lib.iter().map(row).collect::<Vec<_>>() }))
}

fn apply_cut_recipe(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let recipe = cut_recipe::find(&library(a)?, req_str(a, "recipe")?).map_err(|e| e.to_string())?;
    let src = match opt_str(a, "source")? {
        None => SrcRef::Stock,
        Some(s) if s.eq_ignore_ascii_case("stock") => SrcRef::Stock,
        Some(s) => SrcRef::Entity(s.to_owned()),
    };
    let label = recipe.params.kind().info().label;
    let name = match opt_str(a, "name")? {
        Some(n) => n.to_owned(),
        None => (1..)
            .map(|n| format!("{label}{n}"))
            .find(|c| model::find_by_name(&env.world, c).is_none())
            .unwrap_or_else(|| label.to_owned()),
    };
    let cmd = ApplyCutRecipe::new(&env.world, &recipe, &name, src).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "operation": name, "recipe": recipe.name, "kind": label, "status": "never" }))
}

fn lib_prop() -> Value {
    json!({ "type": "string", "description": "The recipe library file (.camrecipes). Omitted: the user library (CAMCRAFT_RECIPES, else the platform data folder)." })
}

fn s_save() -> Value {
    obj(
        json!({
            "operation": { "type": "string", "description": "The operation to save (key or label)." },
            "name": { "type": "string", "description": "The recipe name." },
            "replace": { "type": "boolean", "description": "Replace a recipe of this name. Default false." },
            "library": lib_prop()
        }),
        &["operation", "name"],
    )
}

fn s_list() -> Value {
    obj(json!({ "library": lib_prop() }), &[])
}

fn s_apply() -> Value {
    obj(
        json!({
            "recipe": { "type": "string", "description": "The recipe name (list_cut_recipes)." },
            "source": { "type": "string", "description": "\"stock\" (default), a model name or a DXF profile name." },
            "name": { "type": "string", "description": "The new operation's name. Omitted: the kind's label and the next free number." },
            "library": lib_prop()
        }),
        &["recipe"],
    )
}
