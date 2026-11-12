//! The verbs: every `cam` method as a pure JSON function over the
//! run's document. No JsValue crosses in here - the dispatcher has
//! already spoken to serde (script/mod.rs's bridge law) - and no
//! world write skips a command (the seam law). A verb that could not
//! do its whole job returns `Err(reason)`; the dispatcher records it,
//! and the script sees a catchable `Error` whose text is the reason.

use super::{arg_opt, arg_str, builtin_cps, inventory, only_keys, Env};

use crate::commands::{GenerateOperation, History, ImportMode, ImportModel, ImportTools, UpdateModel};
use crate::model;
use crate::operation::OpStatus;
use crate::posting;

use flecs_ecs::prelude::Entity;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// `cam.load(path)` - open a project file into this run's world. The
/// loader replaces the document and clears the history (M1.2), and
/// the run owns that law here exactly as the app's menu does.
pub(crate) fn v_load(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let path = PathBuf::from(arg_str(&args, 0, "a project path")?);
    // Load in isolation: refusal leaves the current document and its undo/redo intact.
    let world = crate::world::new_world();
    crate::project::load(&world, &path).map_err(|e| e.to_string())?;
    env.history = History::default();
    env.world = world;
    env.posted = None;
    env.doc_path = Some(path.clone());
    Ok(json!({ "path": path.display().to_string(), "operations": ops_view(env) }))
}

/// `cam.save(path?)` - write the document. With no path this saves
/// over the file `cam.load` opened; with neither, it refuses - a
/// document that silently goes nowhere is the silent-wrong answer.
pub(crate) fn v_save(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    if args.len() > 1 {
        return Err(format!("save takes at most one path, got {}", args.len()));
    }
    let path = match arg_opt(&args, 0, "a project path")? {
        Some(p) => PathBuf::from(p),
        None => env.doc_path.clone().ok_or("no document was loaded to save over - pass a path")?,
    };
    crate::project::save(&env.world, &path).map_err(|e| e.to_string())?;
    env.doc_path = Some(path.clone());
    Ok(json!({ "path": path.display().to_string() }))
}

/// `cam.import(path, name?, facetBudget?)` - parse a mesh file and land
/// it as a model entity, by its file stem unless named otherwise (a null
/// name keeps the stem). `facetBudget` is the STEP facet budget (#1054):
/// absent or null is the default; out of bounds, or given for a file that
/// is not STEP, refuses by name. A name that
/// already wears a model updates it (the revision counts, the stale
/// marks follow); a name that belongs to setup furniture refuses -
/// an import never overwrites the stock solid by accident.
pub(crate) fn v_import(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let path = PathBuf::from(arg_str(&args, 0, "a mesh path")?);
    // The command's own construction door: parse, validate, and the
    // extension refusals (IGES needs a kernel we do not have) are
    // ImportModel's law, not a second copy here.
    let budget = crate::geometry::import::step_budget::from_json(args.get(2))?;
    let imp = ImportModel::within(&path, budget).map_err(|e| e.to_string())?;
    let name = arg_opt(&args, 1, "a model name")?.unwrap_or_else(|| imp.name.clone());
    if model::find_by_name(&env.world, &name).is_some() {
        let cmd = UpdateModel::new(&env.world, &name, imp.mesh).map_err(|e| e.to_string())?.with_fidelity(imp.coarse_mm).with_budget(imp.facet_budget);
        env.history.execute(&env.world, cmd);
    } else {
        env.history.execute(&env.world, imp.renamed(&name));
    }
    let entity = model::find_by_name(&env.world, &name);
    let facets = entity.and_then(|e| model::facets_of(&env.world, &e)).map_or(0, |a| a.len());
    let mut out = json!({ "model": name, "facets": facets });
    // #864: a mesh imported coarser than the importer promises says so,
    // with the error it carries; a full-fidelity import adds nothing.
    if let Some(chord) = entity.and_then(|e| crate::model_fidelity::chord_of(&env.world, &e)) {
        out["reduced_fidelity"] = crate::model_fidelity::facts(&env.world, chord);
    }
    if let Some(b) = entity.and_then(|e| crate::model_fidelity::budget_of(&env.world, &e)) {
        out["facet_budget"] = json!(b);
    }
    Ok(out)
}

/// `cam.exportTools(path)` - write the document's tools as a tool
/// library file, the very text the Tool library page's Export writes
/// (`tool_library::export_tool_library`, #300). A document with no
/// tools refuses, and so does a path that cannot be written.
pub(crate) fn v_export_tools(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let path = PathBuf::from(arg_str(&args, 0, "a tool library path")?);
    let text = crate::tool_library::export_tool_library(&env.world)?;
    let count = model::named_entities(&env.world).iter().filter(|(_, e)| model::tool_of(&env.world, e).is_some()).count();
    std::fs::write(&path, text).map_err(|e| format!("tool library not written to {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string(), "tools": count }))
}

/// `cam.importTools(path, mode?)` - read a tool library file into the
/// document through `ImportTools`, the GUI's own command: the whole
/// file is checked first (a malformed or insane file refuses by name),
/// an identical tool is skipped, a clashing name arrives as "Name (2)",
/// and the import is one undo step. #899: `mode` is `add` (default),
/// `update` (same-named tools take the file's definition in place) or
/// `replace` (update, and tools the file lacks are removed unless an
/// operation uses them).
/// #889: `holders` (default true) takes the file's tool bodies.
pub(crate) fn v_import_tools(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let path = PathBuf::from(arg_str(&args, 0, "a tool library path")?);
    let mode = match args.get(1) {
        None | Some(Value::Null) => ImportMode::Add,
        Some(v) => ImportMode::parse(v.as_str().ok_or("import mode must be a string")?)?,
    };
    let holders = match args.get(2) {
        None | Some(Value::Null) => true,
        Some(Value::Bool(b)) => *b,
        Some(other) => return Err(format!("holders must be true or false, got {other}")),
    };
    import_tools(env, &path, mode, holders)
}

/// The import behind `cam.importTools` and MCP's `import_tool_library`
/// (#889: `holders` takes the file's tool bodies, or leaves them out).
pub(crate) fn import_tools(env: &mut Env, path: &Path, mode: ImportMode, holders: bool) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let library = crate::tool_library::parse_tool_library(&text)?;
    let cmd = ImportTools::with_options(&env.world, library, mode, holders).map_err(|e| e.to_string())?;
    let report = cmd.report().clone();
    env.history.execute(&env.world, cmd);
    let renamed: Vec<Value> = report.renamed.iter().map(|(from, to)| json!({ "from": from, "to": to })).collect();
    Ok(json!({
        "path": path.display().to_string(),
        "added": report.added,
        "renamed": renamed,
        "skipped": report.skipped,
        "updated": report.updated,
        "replaced": report.replaced,
        "removed": report.removed,
        "mode": mode.label(),
        "holders": report.holders,
        "holders_left_out": report.holders_left_out,
        "summary": report.summary(),
    }))
}

/// `cam.operations()` - the tree's own order, one row per operation.
pub(crate) fn v_operations(env: &mut Env, _args: Vec<Value>) -> Result<Value, String> {
    Ok(ops_view(env))
}

/// `cam.generate(...names)` - bring the job current. No names runs
/// every operation that is not already Ok (the Never and Stale set -
/// what RegenerateAll stages, plus the first generation it leaves
/// alone); named operations regenerate whatever their status, the way
/// the app's Generate button does. Each one rides its own
/// GenerateOperation command: a mid-run refusal names the operation
/// and leaves the generations before it real and undoable, exactly as
/// in the app.
pub(crate) fn v_generate(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let all = inventory(env);
    let targets: Vec<(String, Entity)> = if args.is_empty() {
        all.iter()
            .filter(|(_, _, s)| *s != OpStatus::Ok)
            .map(|(n, e, _)| (n.clone(), *e))
            .collect()
    } else {
        let mut out = Vec::with_capacity(args.len());
        for i in 0..args.len() {
            let name = arg_str(&args, i, "an operation name")?;
            match all.iter().find(|(n, e, _)| *n == name || model::operation_label(&env.world, n, e) == name) {
                Some((key, e, _)) => out.push((key.clone(), *e)),
                None => return Err(format!("no operation named '{name}'")),
            }
        }
        out
    };
    let mut done = Vec::with_capacity(targets.len());
    for (name, entity) in targets {
        let cmd = GenerateOperation::new(&env.world, &name).map_err(|e| refusal_text(&name, &e))?;
        env.history.execute(&env.world, cmd);
        done.push(op_view(env, &name, &entity, false));
    }
    Ok(Value::Array(done))
}

/// A refused generation as the script's (and MCP's) error: the refusal,
/// plus - when the generator handed back the moves it had built (#261
/// part 3) - how many there were. The moves themselves stay out: they are
/// the GUI's display-only ghost, never kept, never a toolpath.
pub fn refusal_text(name: &str, e: &crate::commands::CommandError) -> String {
    match e.ghost() {
        Some(g) => format!(
            "{name} - {e} [a partial path of {} move(s){} was built before the refusal; it is not kept]",
            g.len(),
            if g.truncated() { ", capped" } else { "" }
        ),
        None => format!("{name} - {e}"),
    }
}

/// Does this operation hold the post with its own warnings? The rule
/// `posting::stream_records` applies when it writes the program: a
/// suppressed op is not in the program, and every cached warning that
/// `blocks_post()` becomes a `WARN` record, any one of which refuses the
/// post (a scallop or probe-search note never does). #722: `ok` read only
/// generation status, so it said `true` for a program the post refused.
fn holds_post(env: &Env, entity: &Entity) -> bool {
    let suppressed = model::operation_of(&env.world, entity).is_some_and(|op| op.suppressed);
    !suppressed
        && model::toolpath_of(&env.world, entity).is_some_and(|t| t.warnings.iter().any(|w| w.blocks_post()))
}

/// `cam.verify(...names)` - what the post would say, without writing:
/// per-operation status and cached warnings, and the program-level
/// gate over the whole document (gouge, machine limits, the probe
/// verdict) once everything in scope is fresh. While any operation is
/// stale or never generated the gate is SKIPPED and says why - its
/// verdict would speak of paths older than the inputs, and a stale
/// answer about metal is worse than none. `ok` is the post's whole
/// verdict: every op in scope fresh, the gate passed, and no op in
/// scope holding the post with its own warnings (named under
/// `held_by`).
pub(crate) fn v_verify(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let all = inventory(env);
    let scope: Vec<(String, Entity, OpStatus)> = if args.is_empty() {
        all.clone()
    } else {
        let mut out = Vec::with_capacity(args.len());
        for i in 0..args.len() {
            let name = arg_str(&args, i, "an operation name")?;
            match all.iter().find(|(n, e, _)| *n == name || model::operation_label(&env.world, n, e) == name) {
                Some(row) => out.push(row.clone()),
                None => return Err(format!("no operation named '{name}'")),
            }
        }
        out
    };
    let operations: Vec<Value> = scope.iter().map(|(n, e, _)| op_view(env, n, e, true)).collect();
    // The gate judges the PROGRAM, which is the whole document - so it
    // runs (or not) on the document's freshness, whatever slice the
    // caller named. A judgment that could not run never reads as passed.
    let fresh = all.iter().all(|(_, _, s)| *s == OpStatus::Ok);
    let gate = if fresh {
        // The not-judged line rides beside the problems: a gate that
        // passed without the material sim is said so, never silently.
        let r = posting::verification_report(&env.world);
        let mut g = json!({ "ok": r.problems.is_empty(), "problems": r.problems });
        if let Some(line) = r.not_judged {
            g["not_judged"] = json!(line);
        }
        g
    } else {
        json!({ "ok": false, "problems": [], "skipped": "an operation is stale or never generated - regenerate before judging the program" })
    };
    let ops_ok = operations.iter().all(|o| o["status"] == json!("ok"));
    let held_by: Vec<String> = scope
        .iter()
        .filter(|(_, e, _)| holds_post(env, e))
        .map(|(n, e, _)| model::operation_label(&env.world, n, e))
        .collect();
    let ok = ops_ok && fresh && gate["ok"] == json!(true) && held_by.is_empty();
    Ok(json!({ "ok": ok, "operations": operations, "gate": gate, "held_by": held_by }))
}

/// `cam.check()` - the whole document's `verify`, the way the post
/// gate reads it: one judgment, spelled two ways, never two code paths.
pub(crate) fn v_check(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    v_verify(env, args)
}

/// `cam.post({to, cps?, program?})` - translate the program and write
/// the file, through the app's own `post_script` (validation, the
/// verification gate, the engine, the all-or-nothing write - nothing
/// is re-implemented here). `cps` picks a built-in by name or a user
/// `.cps` by path; omitted means the document's own post.
/// `program: true` also hands the text back to the script.
pub(crate) fn v_post(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let opts = only_keys(args.first().unwrap_or(&Value::Null), &["to", "cps", "program"])?;
    let to = match opts.get("to") {
        Some(Value::String(s)) => PathBuf::from(s),
        Some(_) => return Err("\"to\" must be a path string".into()),
        None => return Err("a \"to\" path is required - a program that exists nowhere is not a result".into()),
    };
    // Omitted: each mount's own post (#388); named: that script for all.
    let cps = match opts.get("cps") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            if let Some(text) = builtin_cps(s) {
                Some(text.to_owned())
            } else if Path::new(s).extension().is_some_and(|e| e.eq_ignore_ascii_case("cps")) {
                Some(std::fs::read_to_string(s).map_err(|e| format!("cannot read post script {s}: {e}"))?)
            } else {
                let names: Vec<&str> = crate::posting::BUILTINS.iter().map(|(n, _)| *n).collect();
                return Err(format!("unknown built-in post '{s}' - names: {}; or give a .cps file path", names.join(", ")));
            }
        }
        Some(_) => return Err("\"cps\" must be a built-in name or a .cps path".into()),
    };
    let with_program = match opts.get("program") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("\"program\" must be true or false".into()),
    };
    let files = match &cps {
        None => posting::post_mounts(&env.world, &to),
        Some(text) => posting::post_mounts_script(&env.world, &to, text),
    }
    .map_err(|e| e.to_string())?;
    Ok(super::report::posted_view(env, files, with_program))
}

/// `cam.report()` - the job on one page: the tree, the tool list, the
/// totals, and what this run opened and wrote.
pub(crate) fn v_report(env: &mut Env, _args: Vec<Value>) -> Result<Value, String> {
    Ok(super::report::summary(env))
}

/// `cam.result()` - the value the previous verb returned. The one
/// push into the sandbox the host cannot avoid: shapes too large to
/// chain, parked rather than re-run.
pub(crate) fn v_result(env: &mut Env, _args: Vec<Value>) -> Result<Value, String> {
    Ok(env.last_value.clone())
}

/// `cam.log(...)` - the run's console: strings verbatim, everything
/// else as JSON text (js.rs's serialization law, so a logged object
/// reads the same here and in the envelope).
pub(crate) fn v_log(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    for v in args {
        let text = match v {
            Value::String(s) => s,
            other => other.to_string(),
        };
        env.logs.push(text);
    }
    Ok(Value::Null)
}

/// Every operation of the document, in program order.
pub(crate) fn ops_view(env: &Env) -> Value {
    Value::Array(
        inventory(env)
            .iter()
            .map(|(n, e, _)| op_view(env, n, e, false))
            .collect(),
    )
}

/// One operation as the script sees it: the tree row, plus the cached
/// path's warnings when the caller is asking for a verdict.
fn op_view(env: &Env, name: &str, entity: &Entity, warnings: bool) -> Value {
    let op = model::operation_of(&env.world, entity);
    let status = crate::operation::status_of(&env.world, entity);
    let mut row = json!({
        "name": model::operation_label(&env.world, name, entity),
        "key": name,
        "kind": op.as_ref().map(|o| o.params.kind().info().label).unwrap_or("?"),
        "setup": op.as_ref().and_then(|o| o.setup.clone()),
        "status": st_str(status),
    });
    if warnings {
        let warns: Vec<String> = model::toolpath_of(&env.world, entity)
            .map(|t| t.warnings.iter().map(|w| w.to_string()).collect())
            .unwrap_or_default();
        row["warnings"] = json!(warns);
    }
    row
}

pub(crate) fn st_str(s: OpStatus) -> &'static str {
    match s {
        OpStatus::Ok => "ok",
        OpStatus::Stale => "stale",
        OpStatus::Never => "never",
    }
}

