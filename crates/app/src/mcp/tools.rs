//! The MCP tool table (#128): name, description, JSON input schema and
//! the Rust handler, one row each - `tools/list` and `tools/call` read
//! the same rows, so a listed tool always runs and a running one is
//! always listed (the script host's VERBS law, at the MCP door).
//!
//! Provenance: our own tool surface over this repo's commands and the
//! script verbs; schemas are JSON Schema as the MCP spec carries them.

use super::{area, do_not_touch, edit, features, holders, job, pick, pockets, render, tool_delete, tool_edit};
use crate::commands::ImportMode;
use crate::script::{api, Env};

use serde_json::{json, Map, Value};

pub(super) type Handler = fn(&mut Env, &Map<String, Value>) -> Result<Value, String>;

pub(super) struct Tool {
    pub(super) name: &'static str,
    pub(super) description: &'static str,
    pub(super) schema: fn() -> Value,
    pub(super) run: Handler,
}

const TOOLS: &[Tool] = &[
    Tool { name: "open_project", description: "Open a .camproj project file, replacing the current document and clearing undo history. With read_only: true, save_project does not write over this file; give it a different path to write a copy, which also ends read-only.", schema: s_open_project, run: open_project },
    Tool { name: "save_project", description: "Save the document as a .camproj file. Without a path, saves over the file last opened or saved. If that file on disk changed after this session opened or saved it, the save is refused by name; overwrite: true replaces the file. Refused for the file of a project open read-only: give a different path.", schema: s_save, run: save_project },
    Tool { name: "import_model", description: "Import a mesh model (STL, STEP or OBJ) through the app's importers. A name already used by a model updates that model. A STEP model too large to import at full detail, or one whose file's edges lie off their surfaces, answers with reduced_fidelity: how closely its surfaces are approximated, in mm - gouge checks against it are stricter, and its operations are refused at the post when it is too coarse to check; import it again with a larger facet_budget, or export the file again with a tighter tolerance, to clear that. A STEP closed shell that meshes open imports incomplete: reduced_fidelity then says incomplete and names the hole, and every operation on the model is refused at the post. notes lists what the importer changed, e.g. a STEP face wound against its neighbours that was flipped, by name, or a closed STEP solid wound inward that was turned outward; for a STEP file it also states the length unit the model was read in. A STEP file that states no length unit is refused until units gives one.", schema: s_import, run: import_model },
    Tool { name: "get_job", description: "The whole document as JSON: setups (each with model_reference: a solid part, or the virtual reference of its placed DXF profiles), stock, work offsets, models (one imported at reduced fidelity carries reduced_fidelity), DXF profiles, tools, and operations with their parameters and status (one on a DXF profile says what it checks against). With summary, a short form for a small context window: no machine definition, and each operation lists only the fields changed from its defaults.", schema: super::compact::s_get_job, run: job::get_job },
    Tool { name: "get_selection", description: "Read currently selected entities, active setup, active operation, viewport pick mode (model, face, edge, vertex), and active geometry picks.", schema: s_empty, run: super::selection::get_selection },
    Tool { name: "list_operation_types", description: "Every operation kind with its parameter fields, JSON types and default values, and the tool kinds it accepts. kind or section lists only those kinds.", schema: super::compact::s_list_operation_types, run: job::list_operation_types },
    Tool { name: "list_tool_catalog", description: "The built-in tool catalog: names and definitions add_tool can copy into the document.", schema: s_empty, run: job::list_tool_catalog },
    Tool { name: "list_holder_library", description: "The built-in holder library: BT30, BT40 and CAT40 holders with ER collet or Weldon side-lock noses, and standalone ER11-ER40 chucks - names set_tool_holder takes, with their dimensions and the shanks they clamp.", schema: s_empty, run: holders::list_holder_library },
    Tool { name: "set_tool_holder", description: "Set a document tool's holder: from the holder library by name (optional gauge_len and projection, mm; the tool stores the reference to the entry, shown as holder.library, not a copy of its outline), from a drawn spun profile (points; a custom holder), or back to the holder cylinder (clear). One undo step; an unknown entry, a shank the holder cannot take or a bad profile is refused by name.", schema: holders::s_set_tool_holder, run: holders::set_tool_holder },
    Tool { name: "add_tool", description: "Add a tool to the document, from the catalog by name or from a full definition (the shape get_job shows). Assigns the lowest unused tool number in 1-99999 that the mounted machine does not reserve (a probe takes the machine's reserved probe pocket; a cutter on a reserved number or name is refused) and starts with the machine's default holder unless \"holder\" says otherwise; one undo removes the tool and its holder. A form mill (kind \"form_mill\") states \"form\": a \"preset\" (kind roundover, cove, ogee, v_groove or bead, with its numbers) or drawn \"points\" [z, r, arc] from the tip up, and \"pilot_len\" (the non-cutting bearing at the tip); the preset writes the points, and the outline sets diameter and flute_len. No operation cuts with a form mill yet.", schema: s_add_tool, run: edit::add_tool },
    Tool { name: "edit_tool", description: "Edit a document tool: \"patch\" is a partial tool definition (the shape add_tool's definition and get_job show, JSON merge patch - null removes an optional key). It runs the app's undoable ChangeTool, one undo step, so every gate applies (a retype that strands an operation, bad cutting_data rows, catalog values) and refuses by tool name. An unknown key is refused by name. Operations using the tool go stale. A form mill (kind \"form_mill\") states \"form\": a \"preset\" (kind roundover, cove, ogee, v_groove or bead, with its numbers) or drawn \"points\" [z, r, arc] from the tip up, and \"pilot_len\" (the non-cutting bearing at the tip); the preset writes the points, and the outline sets diameter and flute_len. No operation cuts with a form mill yet.", schema: s_edit_tool, run: tool_edit::edit_tool },
    Tool { name: "delete_tool", description: "Delete a document tool through the app's undoable DeleteTool, one undo step (undo restores it with its number, holder body and cutter). Refused by tool name, changing nothing: an unknown name, an entity that is not a tool, or a tool an operation still uses (give that operation another tool with set_tool first).", schema: s_name_only_tool, run: tool_delete::delete_tool },
    Tool { name: "export_tool_library", description: "Write the document's tools to a CamCraft library (.camtools), a Tormach PathPilot tool table (.csv), or a Hurco WinMax tool library (.TCM), detected from path or chosen with format. units (mm or inch) applies to controller tables and defaults to document units. Refuses a document with no tools, an unwriteable path, or a tool that cannot fit the target table.", schema: s_library, run: export_tool_library },
    Tool { name: "import_tool_library", description: "Import a CamCraft library (.camtools/.json), Tormach PathPilot tool table (.csv), or Hurco WinMax tool library (.TCM) as one undo step. format (default auto) overrides detection; units (mm or inch) applies to controller tables and defaults to document units. mode add (default): skips an identical tool, renames a clashing name; update: updates matching names in place; replace: removes absent tools unless used. holders (default true) takes holders from .camtools files. Malformed rows, unsupported Hurco tool types, or tool-number clashes refuse by name.", schema: s_import_library, run: import_tool_library },
    Tool { name: "set_stock", description: "Set the active setup's stock: a box, a cylinder, or a copy of a model, through the undoable SetStock. Optional axis (z, x or y) or angles_deg {yaw, pitch, roll} turns it; the simulation refuses a stock tilted off the axes, by name.", schema: s_stock, run: super::stock_tools::set_stock },
    Tool { name: "add_operation", description: "Add an operation of a kind (see list_operation_types) with a tool, on the stock, a model or a placed DXF profile (2D milling, plasma, waterjet and wire EDM kinds only; a kind that needs a solid model refuses a profile by name), seeded with the kind's defaults and an optional params patch. Not generated until generate runs.", schema: s_add_op, run: edit::add_operation },
    Tool { name: "edit_operation", description: "Patch an operation's parameters: the given fields replace the current ones (JSON merge patch over the kind's fields; null clears an optional field). One undo step.", schema: s_edit_op, run: edit::edit_operation },
    Tool { name: "detect_holes", description: "Fill a Drill, Bore or Thread operation's hole list from its source model's holes (the app's Detect holes button); a Thread operation whose params say external: true gets the model's upright round bosses instead. A Bore operation leaves out holes wider than its tool's ring clears without a core (3 x tool diameter + 2 x stock to leave) and names each in `skipped`. A Drill operation keeps a hole wider than its Drill-kind drill (a pilot is valid) and names each in `undersize`; its toolpath then carries a `drill_undersize` warning per such hole that does not hold the post. One undo step.", schema: s_name, run: edit::detect_holes },
    Tool { name: "detect_pockets", description: "A model's recognised pockets and slots (closed or open pockets, straight slots with open, round or square ends): boundary, floor and top Z, depth, tightest inside corner, slot width and open sides. With create, adds the operation that machines one of them (a Pocket over its floor, or a Slot along its centreline). One undo step.", schema: pockets::s_detect_pockets, run: pockets::detect_pockets },
    Tool { name: "delete_operation", description: "Delete an operation and its cached toolpath.", schema: s_name, run: edit::delete_operation },
    Tool { name: "set_tool", description: "Assign a document tool to an operation (the tool must suit the operation's kind).", schema: s_set_tool, run: edit::set_tool },
    Tool { name: "list_features", description: "A model's, fixture's, or DXF profile's pickable features with stable ids: faces, edge chains, welded smooth faces and vertices on models, or sketch chains and layers on DXF profiles, in world coordinates.", schema: pick::s_list_features, run: features::list_features },
    Tool { name: "get_operation_geometry", description: "What an operation machines: its source (stock, a model, a DXF profile, or picked features), per-chain overrides, its area controls, and what the picks resolve to.", schema: pick::s_op, run: pick::get_operation_geometry },
    Tool { name: "set_operation_geometry", description: "Set the features an operation machines (faces, edge chains, loops, or DXF sketch chains and layers with optional per-chain side and direction overrides) by list_features ids. One undo step; undo restores the previous source.", schema: pick::s_set_geometry, run: pick::set_operation_geometry },
    Tool { name: "set_operation_area", description: "Replace an operation's area controls (containment, offset, outer_offset, corner, extension, extra include/exclude boundaries, open boundary). One undo step.", schema: area::s_set_area, run: area::set_operation_area },
    Tool { name: "get_operation_do_not_touch", description: "Read an operation's 'Do not touch' protected faces/edges and standoff clearance (mm).", schema: pick::s_op, run: do_not_touch::get_operation_do_not_touch },
    Tool { name: "set_operation_do_not_touch", description: "Set protected faces or edge chains (plus standoff clearance, mm) that 2D Pocket, Trace, 2D Offset, 2D Adaptive, 3D Pocket, Face, 3D Finish and Engrave paths must stay clear of across flutes, shank, and holder. Refuses unsupported operation kinds, vertex picks, and overlap with the machining selection. One undo step.", schema: do_not_touch::s_set_do_not_touch, run: do_not_touch::set_operation_do_not_touch },
    Tool { name: "undo", description: "Undo the last document change.", schema: s_empty, run: edit::undo },
    Tool { name: "redo", description: "Redo the last undone change.", schema: s_empty, run: edit::redo },
    Tool { name: "generate", description: "Generate toolpaths: one named operation (always regenerated) or, with no name, every operation that is not current when reached in program order, including operations made stale by earlier generations in this call. A refusal stops the call and names the operation.", schema: s_opt_name, run: generate },
    Tool { name: "verify", description: "Verify the job without posting: per-operation status and findings (op, move index, kind, message, whether it blocks the post) plus the program-level post gate. \"ok\" is true only when the post would write: every operation fresh, the gate passed, and no operation's own warning blocking the post (those operations are listed under \"held_by\").", schema: s_opt_name, run: job::verify },
    Tool { name: "post", description: "Post the program to an NC file through a post processor (a built-in name or a .cps path; default the document's own). Returns the written files, or refuses with the reasons. between_setups chooses what the program does at a setup change (pause by default).", schema: s_post, run: post },
    Tool { name: "render_stock", description: "Render the program's simulated stock as a PNG (an image content block), painted by deviation class (clean, rest, gouge, unjudged), optionally with the toolpath over it; plus rest and gouge volume and the bounds of the stock and the rest. Generate first.", schema: render::schema, run: render::render_stock },
];

/// Every row: this table, then the setup furniture's (#181), then the
/// part placement (#716), ..., then operation names and order (#45), then the DXF profiles (#1862).
pub(super) fn all() -> impl Iterator<Item = &'static Tool> {
    TOOLS
        .iter()
        .chain(super::setup_tools::TOOLS)
        .chain(super::fixture_tools::TOOLS)
        .chain(super::fixture_turn_tools::TOOLS)
        .chain(super::fixture_profile_tools::TOOLS)
        .chain(super::align_tools::TOOLS)
        .chain(super::wcs_circle::TOOLS)
        .chain(super::coolant_tools::TOOLS)
        .chain(super::wcs_tools::TOOLS)
        .chain(super::wcs_flip_tools::TOOLS)
        .chain(super::model_tools::TOOLS)
        .chain(super::model_link_tools::TOOLS)
        .chain(super::vouch_tools::TOOLS)
        .chain(super::vise_tools::TOOLS)
        .chain(super::accessory_tools::TOOLS)
        .chain(super::cutter_tools::TOOLS)
        .chain(super::cutting_tools::TOOLS)
        .chain(super::body_tools::TOOLS)
        .chain(super::plate_tools::TOOLS)
        .chain(super::modvise_tools::TOOLS)
        .chain(super::units_tools::TOOLS)
        .chain(super::stock_fit_tools::TOOLS)
        .chain(super::box_pick::TOOLS)
        .chain(super::rotary_tools::TOOLS)
        .chain(super::thread_infeed_tools::TOOLS)
        .chain(super::workholding_tools::TOOLS)
        .chain(super::sheet_job_tools::TOOLS)
        .chain(super::order_tools::TOOLS)
        .chain(super::sinker_tools::TOOLS)
        .chain(super::machine_tools::TOOLS)
        .chain(super::tool_number_tools::TOOLS)
        .chain(super::agie_inspection::TOOLS)
        .chain(super::post_options_tools::TOOLS)
        .chain(super::sketch_tools::TOOLS)
        .chain(super::svg_tools::TOOLS)
        .chain(super::outline_text_tools::TOOLS)
        .chain(super::holder_dxf_tools::TOOLS)
        .chain(super::recipe_tools::TOOLS)
}

/// The `tools/list` rows.
pub(super) fn list() -> Vec<Value> {
    all()
        .map(|t| json!({ "name": t.name, "description": t.description, "inputSchema": (t.schema)() }))
        .collect()
}

/// Run one tool by name. An unknown name is a refusal like any other.
pub(crate) fn call(env: &mut Env, name: &str, args: &Value) -> Result<Value, String> {
    let Some(tool) = all().find(|t| t.name == name) else {
        let names: Vec<&str> = all().map(|t| t.name).collect();
        return Err(format!("unknown tool '{name}' - tools: {}", names.join(", ")));
    };
    let empty = Map::new();
    let map = args.as_object().unwrap_or(&empty);
    // Typos are refused, never ignored: an argument the schema does not
    // name would otherwise read as "done as asked".
    let schema = (tool.schema)();
    let known = schema["properties"].as_object();
    for key in map.keys() {
        if known.is_none_or(|k| !k.contains_key(key)) {
            return Err(format!("{name}: unknown argument \"{key}\""));
        }
    }
    (tool.run)(env, map).map_err(|e| format!("{name}: {e}"))
}

// ---- argument readers -------------------------------------------------

pub(super) fn req_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    match a.get(key) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s),
        None | Some(Value::Null) => Err(format!("\"{key}\" is required")),
        Some(other) => Err(format!("\"{key}\" must be a non-empty string, got {other}")),
    }
}

pub(super) fn opt_str<'a>(a: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match a.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(other) => Err(format!("\"{key}\" must be a string, got {other}")),
    }
}

// ---- handlers that are a script verb, re-dressed ----------------------

fn open_project(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let path = req_str(a, "path")?;
    let read_only = super::compact::flag(a, "read_only")?;
    let mut out = api::v_load(env, vec![json!(path)])?;
    // #3609: `api::v_save` refuses a save over this file while read-only.
    env.doc_read_only = read_only;
    if read_only {
        out["read_only"] = json!(true);
    }
    Ok(out)
}

fn save_project(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let mut args = vec![opt_str(a, "path")?.map_or(Value::Null, |p| json!(p))];
    // #3608: `overwrite` rides to cam.save as its options object.
    if let Some(overwrite) = a.get("overwrite") {
        args.push(json!({ "overwrite": overwrite }));
    }
    api::v_save(env, args)
}

fn import_model(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let name = opt_str(a, "name")?.map_or(Value::Null, |n| json!(n));
    let budget = a.get("facet_budget").cloned().unwrap_or(Value::Null);
    let units = opt_str(a, "units")?.map_or(Value::Null, |u| json!(u));
    api::v_import(env, vec![json!(req_str(a, "path")?), name, budget, units])
}

fn export_tool_library(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let path = std::path::PathBuf::from(req_str(a, "path")?);
    let format = match opt_str(a, "format")? {
        None => None,
        Some(f) => crate::tool_table::ToolTableFormat::parse_opt(f)?,
    };
    let unit = match opt_str(a, "units")? {
        None => None,
        Some(u) => Some(crate::tool_table::parse_unit(u)?),
    };
    api::export_tools(env, &path, format, unit)
}

fn import_tool_library(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let path = std::path::PathBuf::from(req_str(a, "path")?);
    let mode = match opt_str(a, "mode")? {
        None => ImportMode::Add,
        Some(m) => ImportMode::parse(m)?,
    };
    let holders = match a.get("holders") {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("\"holders\" must be true or false, got {other}")),
    };
    let format = match opt_str(a, "format")? {
        None => None,
        Some(f) => crate::tool_table::ToolTableFormat::parse_opt(f)?,
    };
    let unit = match opt_str(a, "units")? {
        None => None,
        Some(u) => Some(crate::tool_table::parse_unit(u)?),
    };
    api::import_tools(env, &path, mode, holders, format, unit)
}

fn generate(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let args = opt_str(a, "name")?.map(|n| vec![json!(n)]).unwrap_or_default();
    let ops = api::v_generate(env, args)?;
    Ok(json!({ "generated": ops }))
}

fn post(env: &mut Env, a: &Map<String, Value>) -> Result<Value, String> {
    let to = req_str(a, "output")?;
    // #1532: the between-setups choice, set first as its own undo step.
    if let Some(word) = opt_str(a, "between_setups")? {
        set_between_setups(env, word)?;
    }
    let mut opts = json!({ "to": to });
    if let Some(p) = opt_str(a, "post")? {
        opts["cps"] = json!(p);
    }
    let mut out = api::v_post(env, vec![opts])?;
    // #388: a program over several mounts names its files itself;
    // otherwise what v_post itself reports as written, not the path asked for.
    if out.get("files").is_none() {
        out["files"] = json!([out.get("path").cloned().unwrap_or_else(|| json!(to))]);
    }
    Ok(out)
}

/// Set the program's between-setups choice (#1532) where the program
/// reads it: the active setup's own post options when it owns them, else
/// the document's - through the undoable command, one undo step; the
/// current choice again changes nothing.
fn set_between_setups(env: &mut Env, word: &str) -> Result<(), String> {
    use crate::schema::{BetweenSetups, PostOptions};
    let want = BetweenSetups::from_word(word).ok_or_else(|| {
        let words: Vec<&str> = BetweenSetups::ALL.iter().map(|b| b.word()).collect();
        format!("unknown between_setups '{word}' - one of: {}", words.join(", "))
    })?;
    let active = crate::setup::active(&env.world);
    if let Some(name) = active.as_deref()
        && let Some(own) = crate::setup_post_options::own(&env.world, Some(name))
    {
        if own.between_setups != want {
            let next = PostOptions { between_setups: want, ..own };
            let cmd = crate::commands::SetSetupPostOptions::new(&env.world, name, Some(next)).map_err(|e| e.to_string())?;
            env.history.execute(&env.world, cmd);
        }
        return Ok(());
    }
    let doc = crate::setup_post_options::document(&env.world);
    if doc.between_setups != want {
        let next = PostOptions { between_setups: want, ..doc };
        let cmd = crate::commands::SetPostOptions::new(&env.world, next).map_err(|e| e.to_string())?;
        env.history.execute(&env.world, cmd);
    }
    Ok(())
}

// ---- input schemas ----------------------------------------------------

fn obj(props: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": props, "required": required, "additionalProperties": false })
}

fn s_empty() -> Value {
    obj(json!({}), &[])
}

fn s_open_project() -> Value {
    obj(json!({ "path": { "type": "string", "description": "File path." }, "read_only": { "type": "boolean", "description": "Open read-only: save_project does not write over this file (default false)." } }), &["path"])
}

fn s_save() -> Value {
    obj(
        json!({
            "path": { "type": "string", "description": "File path (optional)." },
            "overwrite": { "type": "boolean", "description": "Replace the file even if the file on disk changed after this session opened or saved it. Without this, that save is refused." }
        }),
        &[],
    )
}

fn s_library() -> Value {
    obj(
        json!({
            "path": { "type": "string", "description": "Output path (.camtools, PathPilot .csv, or Hurco WinMax .TCM)." },
            "format": { "type": "string", "enum": ["auto", "camtools", "pathpilot", "hurco"], "description": "File format; default auto (from extension)." },
            "units": { "type": "string", "enum": ["mm", "inch"], "description": "Length units for PathPilot .csv and Hurco .TCM; default document units." }
        }),
        &["path"],
    )
}

fn s_import_library() -> Value {
    obj(
        json!({
            "path": { "type": "string", "description": "A tool library or controller tool table file (.camtools, .json, PathPilot .csv, or Hurco .TCM)." },
            "mode": { "type": "string", "enum": ["add", "update", "replace"], "description": "What to do with a tool whose name the document has; default add." },
            "holders": { "type": "boolean", "description": "Take each tool's holder body from the file (default true); false keeps only the holder cylinder for new tools and leaves existing tools' bodies as they are." },
            "format": { "type": "string", "enum": ["auto", "camtools", "pathpilot", "hurco"], "description": "File format; default auto (from extension or file header)." },
            "units": { "type": "string", "enum": ["mm", "inch"], "description": "Length units for PathPilot .csv and Hurco .TCM; default document units." }
        }),
        &["path"],
    )
}

fn s_import() -> Value {
    obj(
        json!({
            "path": { "type": "string", "description": "An .stl, .step/.stp, .obj or Solidcraft .sc/.solidcraft file (a Solidcraft design imports its cached final body)." },
            "name": { "type": "string", "description": "Model name; defaults to the file stem." },
            "facet_budget": { "type": "integer", "minimum": crate::geometry::import::step_budget::MIN, "maximum": crate::geometry::import::step_budget::CEILING, "description": "STEP facet budget: the most triangles a STEP model may be meshed into (default 30000000). Raise it to import a large model at full detail; STEP files only." },
            "units": { "type": "string", "enum": ["mm", "in"], "description": "The length unit of a STEP file that states none. Such a file is refused without it: the model is never read in a guessed unit. A file that states its own unit keeps it, and notes says so. STEP files only." }
        }),
        &["path"],
    )
}

fn s_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "Operation name (key or label)." } }), &["name"])
}

fn s_opt_name() -> Value {
    obj(json!({ "name": { "type": "string", "description": "Operation name; omit for all." } }), &[])
}

fn s_add_tool() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "Tool name in the document; defaults to the catalog name." },
            "catalog": { "type": "string", "description": "A list_tool_catalog name to copy." },
            "definition": { "type": "object", "description": "A full tool definition (get_job's tool shape). Optional catalog keys part_number, vendor, coating, helix_deg (0-90) and notes ride it; unusable ones refuse the add by tool name. Optional cutting_data: [{material, rpm, feed, stepdown?, stepover?}] per workpiece material word (aluminium_6061, mild_steel, stainless_304, grey_cast_iron, brass, plastics_pom, softwood, hardwood, mdf, plywood, acrylic, hdpe) or unknown (the tool's default row; added from its own rpm/feed when absent), feed mm/min, stepdown/stepover mm; a material twice, a non-positive rpm/feed/step or a stepover wider than the cutter refuses the add by tool name. The tool runs its row for the document material (else its unknown row); add_operation seeds a milling op's feed from a row naming the material in force (the active setup stock's, else the document's) and its stepdown/stepover from that row, each missing one from the unknown row." },
            "holder": { "type": "string", "description": "A built-in library holder for it (\"BT30 ER20\", or a taper's Weldon pick \"BT40 Weldon\": the bore for the tool's shank), or \"none\" for no holder body. Omitted: the active setup's machine's default holder (tooling.default_holder), when it states one and the holder takes the tool. The reply names the holder it landed with (holder, holder_from) or why the default was left off (note)." }
        }),
        &[],
    )
}

fn s_name_only_tool() -> Value {
    obj(json!({ "name": { "type": "string", "description": "The document tool's name (get_job's tools[].name)." } }), &["name"])
}

fn s_edit_tool() -> Value {
    obj(
        json!({
            "name": { "type": "string", "description": "The document tool's name (get_job's tools[].name)." },
            "patch": { "type": "object", "description": "Fields of the tool definition to change (geometry, kind, rpm, feed, flutes, part_number, vendor, coating, helix_deg, notes, cutting_data, ...). Number, body and cutter are not editable here." }
        }),
        &["name", "patch"],
    )
}

fn s_stock() -> Value {
    obj(
        json!({
            "box": { "type": "object", "description": "{dx, dy, dz} in mm; centered in XY, z from 0 to dz.",
                "properties": { "dx": {"type": "number"}, "dy": {"type": "number"}, "dz": {"type": "number"} },
                "required": ["dx", "dy", "dz"] },
            "cylinder": { "type": "object", "description": "{diameter, height} in mm.",
                "properties": { "diameter": {"type": "number"}, "height": {"type": "number"} },
                "required": ["diameter", "height"] },
            "from_model": { "type": "string", "description": "A model name whose mesh becomes the stock." },
            "axis": { "type": "string", "enum": ["z", "x", "y"],
                "description": "The direction of the cylinder axis or the box height (default z). x and y lay the stock on its side." },
            "angles_deg": { "type": "object", "description": "A custom turn in degrees instead of axis: {yaw, pitch, roll}, each 0 when absent.",
                "properties": { "yaw": {"type": "number"}, "pitch": {"type": "number"}, "roll": {"type": "number"} } }
        }),
        &[],
    )
}

fn s_add_op() -> Value {
    obj(
        json!({
            "kind": { "type": "string", "description": "Operation kind: label or key from list_operation_types." },
            "tool": { "type": "string", "description": "A document tool name (add_tool first). Required, except for a kind that mounts no tool (list_operation_types says \"mounts_tool\": false): omit it there; naming one is refused." },
            "name": { "type": "string", "description": "Operation name; defaults to the kind's label plus a number." },
            "source": { "type": "string", "description": "\"stock\" (default), a model name, or a placed DXF profile's name." },
            "params": { "type": "object", "description": "Fields to change from the kind's defaults." }
        }),
        &["kind"],
    )
}

fn s_edit_op() -> Value {
    // #3324 / #3339: the start / exit picks, named, with the kinds that take them.
    let ring: Vec<&str> = crate::operation::KindInfo::ALL.iter().filter(|k| k.pick() == crate::operation::PickCapability::Ring).map(|k| k.label).collect();
    // #3331: the turning kinds whose pick is a point of the lathe (Z, X) plane.
    let lathe: Vec<&str> = crate::operation::KindInfo::ALL.iter().filter(|k| k.pick() == crate::operation::PickCapability::Meridian).map(|k| k.label).collect();
    let params = format!("Fields to change (see get_job for the current ones). Start / exit picks: picks {{ start: [[x, y, z?], ...], exit: [...] }} in model mm, null or {{}} clears; taken by {} (Wire Contour: start is the threading point; exit ends an open run at its nearest end, or leaves a closed profile there; Face: the start is the corner of the first raster row, the exit is the side where the last row ends, refused when the row count cannot reach it; Sinker Burn: the angles round the burn position at which its orbit starts and ends; Sinker Side Burn: the end of its side move, the burn position or the wall end, at which the move starts and ends; Drill, Bore and Thread: a point on a hole of the list makes it the first or the last hole) and by {} (a point in the lathe (Z, X) plane: z is required). Each kind's start_exit in list_operation_types says if it takes a pick.", ring.join(", "), lathe.join(", "));
    obj(
        json!({
            "name": { "type": "string", "description": "Operation name (key or label)." },
            "params": { "type": "object", "description": params }
        }),
        &["name", "params"],
    )
}

fn s_set_tool() -> Value {
    obj(
        json!({
            "operation": { "type": "string", "description": "Operation name (key or label)." },
            "tool": { "type": "string", "description": "A document tool name." }
        }),
        &["operation", "tool"],
    )
}

fn s_post() -> Value {
    // #2374: every registry post, so an agent learns each name from the list.
    let names: Vec<&str> = crate::posting::BUILTINS.iter().map(|(n, _)| *n).collect();
    let post_doc = format!("A built-in post name ({}) or a .cps path.", names.join(", "));
    obj(
        json!({
            "output": { "type": "string", "description": "The .nc file to write." },
            "post": { "type": "string", "description": post_doc },
            "between_setups": {
                "type": "string",
                "enum": ["pause", "optional-stop", "continue", "file-per-setup"],
                "description": "What the program does where it changes setup (the part re-clamped): pause (M0, the default), optional-stop (M1), continue (run straight on), or file-per-setup (one NC file per setup). Set before posting as its own undo step; omitted keeps the document's choice."
            }
        }),
        &["output"],
    )
}
