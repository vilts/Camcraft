//! The verbs: every `cam` method as a pure JSON function over the
//! run's document. No JsValue crosses in here - the dispatcher has
//! already spoken to serde (script/mod.rs's bridge law) - and no
//! world write skips a command (the seam law). A verb that could not
//! do its whole job returns `Err(reason)`; the dispatcher records it,
//! and the script sees a catchable `Error` whose text is the reason.

use super::{arg_opt, arg_str, inventory, only_keys, Env};

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
    // #3608: stamped before the read, so a write during the load refuses the next save.
    let stamp = crate::project::DiskStamp::of(&path);
    let notes = crate::project::load_noted(&world, &path).map_err(|e| e.to_string())?;
    env.history = History::default();
    env.world = world;
    env.posted = None;
    env.doc_path = Some(path.clone());
    env.doc_read_only = false;
    env.doc_stamp = stamp;
    let mut out = json!({ "path": path.display().to_string(), "operations": ops_view(env) });
    if !notes.is_empty() {
        out["notes"] = json!(notes);
    }
    Ok(out)
}

/// `cam.save(path?, options?)` - write the document. With no path this saves
/// over the file `cam.load` opened; with neither, it refuses - a
/// document that silently goes nowhere is the silent-wrong answer. A
/// document open read-only is not saved over its own file (#3609).
/// #3608: a save over the file loaded or saved here is refused when another
/// program changed it since, unless `options.overwrite` is true.
pub(crate) fn v_save(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    if args.len() > 2 {
        return Err(format!("save takes a path and one options object, got {} arguments", args.len()));
    }
    let path = match arg_opt(&args, 0, "a project path")? {
        Some(p) => PathBuf::from(p),
        None => env.doc_path.clone().ok_or("no document was loaded to save over - pass a path")?,
    };
    crate::project::refuse_read_only_save(env.doc_path.as_deref(), env.doc_read_only, &path)?;
    super::save_guard::check(env.doc_stamp.as_ref(), &path, args.get(1))?;
    crate::project::save(&env.world, &path).map_err(|e| e.to_string())?;
    env.doc_stamp = crate::project::DiskStamp::of(&path);
    env.doc_path = Some(path.clone());
    env.doc_read_only = false;
    Ok(json!({ "path": path.display().to_string() }))
}

/// `cam.import(path, name?, facetBudget?)` - parse a mesh file and land
/// it as a model entity, by its file stem unless named otherwise (a null
/// name keeps the stem). `facetBudget` is the STEP facet budget (#1054):
/// absent or null is the default; out of bounds, or given for a file that
/// is not STEP, refuses by name. `units` (#1874) is the length unit,
/// `"mm"` or `"in"`, for a STEP file that states none: such a file is
/// refused without it, never read in a guessed unit. A name that
/// already wears a model updates it (the revision counts, the stale
/// marks follow); a name that belongs to setup furniture refuses -
/// an import never overwrites the stock solid by accident.
pub(crate) fn v_import(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let path = PathBuf::from(arg_str(&args, 0, "a mesh path")?);
    // The command's own construction door: parse, validate, and the
    // extension refusals (IGES needs a kernel we do not have) are
    // ImportModel's law, not a second copy here.
    let explicit = crate::geometry::import::step_budget::from_json(args.get(2))?;
    // #1185: no budget given -> a re-import keeps the model's recorded one.
    let given_name = arg_opt(&args, 1, "a model name")?;
    let budget = explicit.or_else(|| crate::model_fidelity::reimport_budget(&env.world, &path, given_name.as_deref()));
    let units = match arg_opt(&args, 3, "a length unit")? {
        None => None,
        Some(u) => Some(crate::units::Unit::parse(&u).ok_or_else(|| format!("the length unit must be \"mm\" or \"in\", got \"{u}\""))?),
    };
    let imp = ImportModel::within_in(&path, budget, units).map_err(|e| e.to_string())?;
    let name = given_name.unwrap_or_else(|| imp.name.clone());
    let notes = imp.notes.clone();
    if model::find_by_name(&env.world, &name).is_some() {
        let cmd = UpdateModel::new(&env.world, &name, imp.mesh).map_err(|e| e.to_string())?.with_fidelity(imp.coarse_mm).with_file_tolerance(imp.file_tolerance).with_hole(imp.hole.clone()).with_budget(imp.facet_budget).with_step_source(imp.step_source).with_source(imp.source);
        env.history.execute(&env.world, cmd);
    } else {
        env.history.execute(&env.world, imp.renamed(&name));
    }
    let entity = model::find_by_name(&env.world, &name);
    let facets = entity.and_then(|e| model::facets_of(&env.world, &e)).map_or(0, |a| a.len());
    let mut out = json!({ "model": name, "facets": facets });
    // #864: a mesh imported coarser than the importer promises says so,
    // with the error it carries; a full-fidelity import adds nothing.
    if let Some((e, chord)) = entity.and_then(|e| crate::model_fidelity::chord_of(&env.world, &e).map(|c| (e, c))) {
        out["reduced_fidelity"] = crate::mesh_open::facts(&env.world, &e, chord);
    }
    if let Some(b) = entity.and_then(|e| crate::model_fidelity::budget_of(&env.world, &e)) {
        out["facet_budget"] = json!(b);
    }
    // #1573 review: what the importer changed (a STEP face flipped to agree
    // with its neighbours) is in the result, by name.
    if !notes.is_empty() {
        out["notes"] = json!(notes);
    }
    Ok(out)
}

/// `cam.exportTools(path, format?, units?)` - write the document's tools
/// as a CamCraft `.camtools` library, a Tormach PathPilot `.csv` tool
/// table, or a Hurco WinMax `.TCM` tool library (#300, #2566).
pub(crate) fn v_export_tools(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() || args.len() > 3 {
        return Err(format!("exportTools takes 1 to 3 arguments, got {}", args.len()));
    }
    let path = PathBuf::from(arg_str(&args, 0, "a tool library path")?);
    let explicit_format = match arg_opt(&args, 1, "format")? {
        None => None,
        Some(f) => crate::tool_table::ToolTableFormat::parse_opt(&f)?,
    };
    let explicit_unit = match arg_opt(&args, 2, "units")? {
        None => None,
        Some(u) => Some(crate::tool_table::parse_unit(&u)?),
    };
    export_tools(env, &path, explicit_format, explicit_unit)
}

/// Shared export helper behind `cam.exportTools` and MCP `export_tool_library`.
pub(crate) fn export_tools(
    env: &Env,
    path: &Path,
    explicit_format: Option<crate::tool_table::ToolTableFormat>,
    explicit_unit: Option<crate::units::Unit>,
) -> Result<Value, String> {
    use crate::tool_table::ToolTableFormat;
    let format = explicit_format
        .or_else(|| crate::tool_table::detect_format(path, None))
        .unwrap_or(ToolTableFormat::CamTools);
    let unit = explicit_unit.unwrap_or_else(|| match format {
        ToolTableFormat::CamTools => crate::units::Unit::Millimeters,
        ToolTableFormat::PathPilot | ToolTableFormat::Hurco => crate::units::of(&env.world),
    });
    let text = crate::tool_table::export(&env.world, format, unit)?;
    let count = model::named_entities(&env.world)
        .iter()
        .filter(|(_, e)| model::tool_of(&env.world, e).is_some())
        .count();
    std::fs::write(path, text)
        .map_err(|e| format!("tool library not written to {}: {e}", path.display()))?;
    Ok(json!({ "path": path.display().to_string(), "tools": count, "format": format.label() }))
}

/// `cam.importTools(path, mode?, holders?, format?, units?)` - read a
/// tool library or machine tool table into the document through
/// `ImportTools`, one undo step (#300, #889, #899, #2566).
pub(crate) fn v_import_tools(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() || args.len() > 5 {
        return Err(format!("importTools takes 1 to 5 arguments, got {}", args.len()));
    }
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
    let explicit_format = match arg_opt(&args, 3, "format")? {
        None => None,
        Some(f) => crate::tool_table::ToolTableFormat::parse_opt(&f)?,
    };
    let explicit_unit = match arg_opt(&args, 4, "units")? {
        None => None,
        Some(u) => Some(crate::tool_table::parse_unit(&u)?),
    };
    import_tools(env, &path, mode, holders, explicit_format, explicit_unit)
}

/// The import behind `cam.importTools` and MCP's `import_tool_library`.
pub(crate) fn import_tools(
    env: &mut Env,
    path: &Path,
    mode: ImportMode,
    holders: bool,
    explicit_format: Option<crate::tool_table::ToolTableFormat>,
    explicit_unit: Option<crate::units::Unit>,
) -> Result<Value, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let unit = explicit_unit.unwrap_or_else(|| crate::units::of(&env.world));
    let (format, library) = crate::tool_table::parse_import(
        &text,
        explicit_format,
        path,
        unit,
        explicit_unit.is_some(),
    )?;
    let cmd = ImportTools::with_options(&env.world, library, mode, holders)
        .map_err(|e| e.to_string())?;
    let report = cmd.report().clone();
    env.history.execute(&env.world, cmd);
    let renamed: Vec<Value> = report
        .renamed
        .iter()
        .map(|(from, to)| json!({ "from": from, "to": to }))
        .collect();
    Ok(json!({
        "path": path.display().to_string(),
        "format": format.label(),
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
/// every operation that is not Ok when reached in program order,
/// including operations made Stale by an earlier generation in this call;
/// named operations regenerate whatever their status, the way
/// the app's Generate button does. Each one rides its own
/// GenerateOperation command: a mid-run refusal names the operation
/// and leaves the generations before it real and undoable, exactly as
/// in the app.
pub(crate) fn v_generate(env: &mut Env, args: Vec<Value>) -> Result<Value, String> {
    let targets = generate_targets(env, &args)?;
    let mut done = Vec::with_capacity(targets.len());
    for (name, entity) in targets {
        if args.is_empty() && crate::operation::status_of(&env.world, &entity) == OpStatus::Ok {
            continue;
        }
        let cmd = GenerateOperation::new(&env.world, &name).map_err(|e| refusal_text(&name, &e))?;
        env.history.execute(&env.world, cmd);
        done.push(op_view(env, &name, &entity, false));
    }
    Ok(Value::Array(done))
}

/// The operations a `generate` call runs, in program order: every one
/// for no names (the caller skips the Ok ones as it reaches them), or
/// each named one. #3035: the MCP server's background generate reads
/// the same list, so the two paths cannot disagree on what runs.
pub(crate) fn generate_targets(env: &Env, args: &[Value]) -> Result<Vec<(String, Entity)>, String> {
    let all = inventory(env);
    Ok(if args.is_empty() {
        all.iter()
            .map(|(n, e, _)| (n.clone(), *e))
            .collect()
    } else {
        let mut out = Vec::with_capacity(args.len());
        for i in 0..args.len() {
            let name = arg_str(args, i, "an operation name")?;
            match all.iter().find(|(n, e, _)| *n == name || model::operation_label(&env.world, n, e) == name) {
                Some((key, e, _)) => out.push((key.clone(), *e)),
                None => return Err(format!("no operation named '{name}'")),
            }
        }
        out
    })
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
    let needs_review = !model::operation_meta_of(&env.world, entity).needs_review.is_empty();
    !suppressed
        && (needs_review
            || model::toolpath_of(&env.world, entity).is_some_and(|t| t.warnings.iter().any(|w| w.blocks_post())))
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
    let mut operations: Vec<Value> = scope.iter().map(|(n, e, _)| op_view(env, n, e, true)).collect();
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
        if !r.warnings.is_empty() {
            g["warnings"] = gate_warnings(env, &r.warnings, &mut operations);
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

/// #3456: the program laws' non-blocking findings (a plasma tip-up or
/// kerf crossing) as data: each joins its op's `warnings` row (when the
/// op is in scope) and the gate's `warnings` list, op named by label.
/// They hold nothing - the law that found them warns, and `ok` follows
/// the problems and `held_by` alone.
fn gate_warnings(env: &Env, warnings: &[posting::GateWarning], operations: &mut [Value]) -> Value {
    let mut out = Vec::with_capacity(warnings.len());
    for w in warnings {
        if let Some(list) = operations
            .iter_mut()
            .find(|o| o["key"] == json!(w.op))
            .and_then(|o| o["warnings"].as_array_mut())
        {
            list.push(json!(w.message));
        }
        let label = model::find_by_name(&env.world, &w.op)
            .map_or_else(|| w.op.clone(), |e| model::operation_label(&env.world, &w.op, &e));
        out.push(json!({ "op": label, "kind": w.kind, "message": w.message }));
    }
    Value::Array(out)
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
    let opts = only_keys(
        args.first().unwrap_or(&Value::Null),
        &["to", "cps", "program", "options"],
    )?;
    let to = match opts.get("to") {
        Some(Value::String(s)) => PathBuf::from(s),
        Some(_) => return Err("\"to\" must be a path string".into()),
        None => return Err("a \"to\" path is required - a program that exists nowhere is not a result".into()),
    };
    // Omitted: each mount's own post (#388); named: that script for all.
    let cps = match opts.get("cps") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => Some(posting::resolve_cps(s)?),
        Some(_) => return Err("\"cps\" must be a built-in name or a .cps path".into()),
    };
    let with_program = match opts.get("program") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err("\"program\" must be true or false".into()),
    };
    let extra = match opts.get("options") {
        None | Some(Value::Null) => None,
        Some(v) => Some(posting::json_map_to_overrides(v)?),
    };
    if posting::post_options(&env.world).per_op {
        let (dir, program) = super::report::per_op_target(&to)?;
        let files = match &cps {
            None => {
                posting::post_per_op_with_script_options(&env.world, dir, program, extra.as_ref())
            }
            Some((post_ref, text)) => posting::post_per_op_for_post_with_options(
                &env.world,
                dir,
                program,
                post_ref,
                text,
                extra.as_ref(),
            ),
        }
        .map_err(|e| e.to_string())?;
        let warning = cps.as_ref().and_then(|(post_ref, _)| {
            let mut setups = Vec::new();
            for file in &files {
                let setup = model::find_by_name(&env.world, &file.op)
                    .and_then(|e| model::operation_of(&env.world, &e))
                    .and_then(|op| op.setup);
                let effective =
                    crate::setup::effective(&env.world, setup.as_deref()).unwrap_or_default();
                let group = vec![effective];
                if !setups.contains(&group) {
                    setups.push(group);
                }
            }
            crate::setup_post_options::unapplied_script_warning(&env.world, &setups, post_ref)
        });
        if let Some(w) = &warning {
            env.logs.push(w.clone());
        }
        let mut out = super::report::posted_per_op_view(env, files, with_program);
        if let Some(w) = warning {
            out["warning"] = json!(w);
        }
        return Ok(out);
    }
    let files = match &cps {
        None => posting::post_mounts_with_script_options(&env.world, &to, extra.as_ref()),
        Some((post_ref, text)) => {
            posting::post_mounts_for_post_with_options(&env.world, &to, post_ref, text, extra.as_ref())
        }
    }
    .map_err(|e| e.to_string())?;
    let warning = cps.as_ref().and_then(|(post_ref, _)| {
        let setups: Vec<Vec<String>> = files.iter().map(|f| f.mount.setups.clone()).collect();
        crate::setup_post_options::unapplied_script_warning(&env.world, &setups, post_ref)
    });
    if let Some(w) = &warning {
        env.logs.push(w.clone());
    }
    Ok(super::report::posted_view(env, files, with_program, warning.as_deref()))
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
pub(crate) fn op_view(env: &Env, name: &str, entity: &Entity, warnings: bool) -> Value {
    let op = model::operation_of(&env.world, entity);
    let status = crate::operation::status_of(&env.world, entity);
    let mut row = json!({
        "name": model::operation_label(&env.world, name, entity),
        "key": name,
        "kind": op.as_ref().map(|o| o.params.kind().info().label).unwrap_or("?"),
        "setup": op.as_ref().and_then(|o| o.setup.clone()),
        "status": st_str(status),
    });
    let tp = model::toolpath_of(&env.world, entity);
    if warnings {
        let warns: Vec<String> = tp
            .as_ref()
            .map(|t| t.warnings.iter().map(|w| w.to_string()).collect())
            .unwrap_or_default();
        row["warnings"] = json!(warns);
        let review = model::operation_meta_of(&env.world, entity).needs_review;
        if !review.is_empty() {
            row["needs_review"] = json!(review);
        }
    }
    if let Some(notes) = tp.filter(|t| !t.path.notes.is_empty()).map(|t| t.path.notes) {
        row["notes"] = json!(notes);
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

