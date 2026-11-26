//! Saved cut recipes (#3073): an operation's settings - its kind and
//! every parameter (depths, stepdown, stepover, feeds, tabs, leads,
//! ramps), its tool by name and the workpiece material it was cut in -
//! stored under a name, so a repeat job applies them to new geometry in
//! one step (`commands::ApplyCutRecipe`). The geometry is not part of a
//! recipe: the caller names the new source when it applies one.
//!
//! The library is one `recipes.camrecipes` file in a small
//! `camcraft-recipes/1` envelope, the fixture library's shape: a strict
//! reader that refuses the whole file by entry name, and a writer whose
//! save -> load -> save is byte-stable. Parameters are written in the
//! exact form a `.camproj` holds them (`project::ParamsDto`).
//!
//! Provenance: our own format and domain model; no vendor recipe,
//! template or wording was consulted (#3066 clean-room boundary).

use flecs_ecs::prelude::World;
use serde_json::{Value, json};

use crate::model;
use crate::operation::OpParams;
use crate::project;
use crate::tools::WorkMaterial;

mod store;

pub use store::{ENV_VAR, LIBRARY_FILE, LibraryError, add, find, library_path, load, user_dir, user_dir_for};

/// The envelope's `format` tag.
pub const FORMAT: &str = "camcraft-recipes";
/// The envelope's `version`; a reader refuses any other.
pub const VERSION: u64 = 1;

/// One saved recipe.
#[derive(Clone, Debug, PartialEq)]
pub struct CutRecipe {
    /// The user's handle for the recipe; unique in a library.
    pub name: String,
    /// The document tool the recipe cuts with, by name (empty for a kind
    /// that mounts no tool).
    pub tool: String,
    /// The workpiece material word (`WorkMaterial::word`) in force when
    /// it was saved; `None` = no material was stated.
    pub material: Option<String>,
    /// The operation kind and all its parameters.
    pub params: OpParams,
}

impl CutRecipe {
    /// The recipe of operation `operation` (its key) under `name`.
    /// Refused by name: an empty name, or no such operation.
    pub fn capture(world: &World, operation: &str, name: &str) -> Result<Self, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("a cut recipe needs a name".into());
        }
        let op = model::find_by_name(world, operation)
            .and_then(|e| model::operation_of(world, &e))
            .ok_or_else(|| format!("no operation named '{operation}'"))?;
        let material = crate::stock_material::in_force(world, op.setup.as_deref()).map(|m| m.word().to_owned());
        Ok(Self { name: name.to_owned(), tool: op.tool, material, params: op.params })
    }

    /// The recipe's material as the table knows it. A word the table no
    /// longer has is refused with the recipe's name and the word.
    pub fn material(&self) -> Result<Option<WorkMaterial>, String> {
        match &self.material {
            None => Ok(None),
            Some(w) => WorkMaterial::from_word(w)
                .map(Some)
                .ok_or_else(|| format!("cut recipe '{}': its material '{w}' is not a known workpiece material", self.name)),
        }
    }
}

fn entry_to_json(r: &CutRecipe) -> Result<Value, String> {
    let params = project::params_to_json(&r.params);
    if params.is_null() {
        return Err(format!("cut recipe '{}': its parameters cannot be written", r.name));
    }
    Ok(json!({ "name": r.name, "tool": r.tool, "material": r.material, "params": params }))
}

fn entry_from_json(v: &Value, index: usize) -> Result<CutRecipe, String> {
    let name = v
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| format!("cut recipe {index} has no name"))?
        .to_owned();
    let tool = match v.get("tool") {
        Some(Value::String(t)) => t.clone(),
        _ => return Err(format!("cut recipe '{name}' has no tool")),
    };
    let material = match v.get("material") {
        None | Some(Value::Null) => None,
        Some(Value::String(m)) => Some(m.clone()),
        Some(_) => return Err(format!("cut recipe '{name}': material is not a word")),
    };
    let params = v.get("params").ok_or_else(|| format!("cut recipe '{name}' has no params"))?;
    let params = project::params_from_stored_json(params).map_err(|e| format!("cut recipe '{name}': {e}"))?;
    Ok(CutRecipe { name, tool, material, params })
}

/// The library text for `recipes`. Two recipes with one name are refused.
pub fn to_text(recipes: &[CutRecipe]) -> Result<String, String> {
    let mut entries = Vec::with_capacity(recipes.len());
    for (i, r) in recipes.iter().enumerate() {
        if recipes[..i].iter().any(|o| o.name == r.name) {
            return Err(format!("cut recipe '{}' is saved twice", r.name));
        }
        entries.push(entry_to_json(r)?);
    }
    let doc = json!({ "format": FORMAT, "version": VERSION, "recipes": entries });
    serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())
}

/// Read a library text. A wrong envelope, a malformed entry or a name
/// used twice refuses the whole file, by entry name.
pub fn parse(text: &str) -> Result<Vec<CutRecipe>, String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("not a cut recipe library: {e}"))?;
    if doc.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(format!("not a cut recipe library (format is not '{FORMAT}')"));
    }
    match doc.get("version").and_then(Value::as_u64) {
        Some(VERSION) => {}
        other => return Err(format!("cut recipe library version {other:?} is not {VERSION}")),
    }
    let list = doc.get("recipes").and_then(Value::as_array).ok_or("cut recipe library has no recipes list")?;
    let mut out: Vec<CutRecipe> = Vec::with_capacity(list.len());
    for (i, v) in list.iter().enumerate() {
        let r = entry_from_json(v, i)?;
        if out.iter().any(|o| o.name == r.name) {
            return Err(format!("cut recipe '{}' is in the library twice", r.name));
        }
        out.push(r);
    }
    Ok(out)
}
