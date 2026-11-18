//! The MCP cutting-data tools (#732): state the workpiece material (the
//! document's, or #2015 the active setup stock's), read the
//! feeds-and-speeds suggestion for an operation, and apply it. Each
//! write is the very undoable command the pages queue (`SetWorkMaterial`,
//! `SetStockMaterial`, `ApplyCuttingData`), one History step, so `undo`
//! takes it back; a refusal comes back by name.
//!
//! Provenance: our own tool surface over this repo's cutting-data commands.

use super::setup_tools::{cmd_err, obj};
use super::tools::{Tool, req_str};
use crate::commands::{ApplyCuttingData, SetStockMaterial, SetWorkMaterial};
use crate::cutting::{self, Suggestion};
use crate::script::Env;
use crate::tools::WorkMaterial;

use serde_json::{Map, Value, json};

pub(super) const TOOLS: &[Tool] = &[
    Tool { name: "set_work_material", description: "State the document's workpiece material (the Cutting data page's pick): material is aluminium_6061, mild_steel, stainless_304, grey_cast_iron, brass or plastics_pom, or null to clear it. Re-stating the current material, or an unknown word, is refused by name. One undoable step.", schema: s_material, run: set_work_material },
    Tool { name: "set_stock_material", description: "State the workpiece material of the active setup's stock (#2015): a material word as set_work_material takes, or null to clear it. It picks the row of each tool's cutting data the setup's operations are seeded from (the stock's material, else the document's, else the tool's Unknown row): every operation of the setup takes the feed of a row naming the material and the row's stepdown / stepover, in the same undoable step. Tools' spindle rpm follow the document material. Refused by name with no stock, or when the stock already states it.", schema: s_material, run: set_stock_material },
    Tool { name: "suggest_cutting_data", description: "The feeds-and-speeds table's suggestion for an operation's tool on the stated material, as data; writes nothing: material, tool material, cutting speed (m/min), feed per tooth (mm), chip-thinning factor, spindle rpm, feed (mm/min) and the machine clamps applied. Refused by name when no material is stated or the table cannot speak for the tool.", schema: s_op, run: suggest_cutting_data },
    Tool { name: "apply_cutting_data", description: "Apply suggest_cutting_data's numbers: the operation's feed and its tool's spindle speed, as one undoable step (the Cutting data page's Apply). Refused by name when no material is stated, the kind has no milling feed, or the operation already runs the suggestion.", schema: s_op, run: apply_cutting_data },
];

/// The work material as the project file words it (`null` when none).
pub(super) fn material_view(env: &Env) -> Value {
    json!(cutting::material_of(&env.world).map(WorkMaterial::word))
}

fn view(s: &Suggestion) -> Value {
    json!({
        "operation_tool": s.tool,
        "material": s.material.word(),
        "tool_material": s.tool_material.word(),
        "cutting_speed": s.vc,
        "feed_per_tooth": s.fz,
        "thinning_factor": s.thinning,
        "rpm": s.rpm,
        "feed": s.feed,
        "machine_clamps": s.notes,
    })
}

fn set_work_material(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let next = material_arg("set_work_material", args)?;
    let cmd = SetWorkMaterial::new(&env.world, next).map_err(cmd_err)?;
    env.history.execute(&env.world, cmd);
    Ok(json!({ "work_material": material_view(env) }))
}

fn material_arg(tool: &str, args: &Map<String, Value>) -> Result<Option<WorkMaterial>, String> {
    match args.get("material") {
        None => Err(format!("{tool} needs \"material\" (a material word, or null to clear)")),
        Some(Value::Null) => Ok(None),
        Some(Value::String(w)) => WorkMaterial::from_word(w).map(Some).ok_or_else(|| {
            let words: Vec<&str> = WorkMaterial::ALL.iter().map(|m| m.word()).collect();
            format!("unknown workpiece material '{w}' (one of {})", words.join(", "))
        }),
        Some(_) => Err("\"material\" must be a material word or null".to_owned()),
    }
}

fn set_stock_material(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let next = material_arg("set_stock_material", args)?;
    let cmd = SetStockMaterial::new(&env.world, next).map_err(cmd_err)?;
    let (stock, seeded) = (cmd.stock_name().to_owned(), cmd.seeded());
    env.history.execute(&env.world, cmd);
    Ok(json!({ "stock": stock, "stock_material": next.map(WorkMaterial::word), "operations_seeded": seeded }))
}

fn suggest_cutting_data(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(args, "operation")?;
    let s = cutting::suggest(&env.world, name)?;
    let mut out = view(&s);
    out["operation"] = json!(name);
    Ok(out)
}

fn apply_cutting_data(env: &mut Env, args: &Map<String, Value>) -> Result<Value, String> {
    let name = req_str(args, "operation")?;
    let cmd = ApplyCuttingData::new(&env.world, name).map_err(cmd_err)?;
    let mut out = view(cmd.suggestion());
    out["operation"] = json!(name);
    env.history.execute(&env.world, cmd);
    Ok(out)
}

fn s_material() -> Value {
    let words: Vec<&str> = WorkMaterial::ALL.iter().map(|m| m.word()).collect();
    obj(
        json!({ "material": { "type": ["string", "null"], "enum": words,
            "description": "The workpiece material word, or null to clear it." } }),
        &["material"],
    )
}

fn s_op() -> Value {
    obj(json!({ "operation": { "type": "string", "description": "The operation's name." } }), &["operation"])
}
