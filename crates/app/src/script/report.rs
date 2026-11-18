//! `cam.report()` - the job on one page. Every number is read from
//! the same seams the Setup sheet draws (the tool list's time law,
//! the operation order), never recomputed here: a report that
//! invented its own arithmetic would be a second truth, and the
//! batch lane exists to avoid exactly that.

use super::fnum;
use super::api::ops_view;
use super::Env;

use crate::tool_list;

use serde_json::{json, Value};

pub(crate) fn summary(env: &Env) -> Value {
    let rows = tool_list::program(&env.world);
    let (mut minutes, mut cut, mut rapid) = (0.0f64, 0.0f64, 0.0f64);
    let tools: Vec<Value> = rows
        .iter()
        .map(|r| {
            minutes += r.minutes;
            cut += r.cut_mm;
            rapid += r.rapid_mm;
            json!({
                "tool": r.tool,
                "ops": r.ops,
                "cut_mm": fnum(r.cut_mm),
                "rapid_mm": fnum(r.rapid_mm),
                "minutes": fnum(r.minutes),
                "changes": r.changes,
            })
        })
        .collect();
    json!({
        "document": env.doc_path.as_ref().map(|p| p.display().to_string()),
        "posted": env.posted.as_ref().map(|p| p.display().to_string()),
        "operations": ops_view(env),
        "tools": tools,
        "minutes": fnum(minutes),
        "cut_mm": fnum(cut),
        "rapid_mm": fnum(rapid),
    })
}

/// Build the shared `cam.post` summary fields (`path`, `ops`, `lines`,
/// `not_judged`) and record the first written file on `env.posted`.
fn posted_header(env: &mut Env, items: &[(&std::path::Path, &crate::posting::Posted)]) -> Value {
    let ops: usize = items.iter().map(|(_, p)| p.ops).sum();
    let lines: usize = items.iter().map(|(_, p)| p.lines).sum();
    let first = items.first().copied();
    env.posted = first.map(|(path, _)| path.to_path_buf());
    let mut out = json!({
        "path": first.map(|(path, _)| path.display().to_string()).unwrap_or_default(),
        "ops": ops,
        "lines": lines,
    });
    if let Some(line) = first.and_then(|(_, p)| p.not_judged.as_ref()) {
        out["not_judged"] = json!(line);
    }
    out
}

/// `cam.post`'s answer (#388): one file reads as it always did (path,
/// ops, lines); a program over several mounts adds `files` (every path,
/// program order) and `mounts` (each file's setups, machine, post, ops,
/// lines - and its text when asked), `path` naming the first.
pub(crate) fn posted_view(
    env: &mut Env,
    files: Vec<crate::posting::PostedMount>,
    with_program: bool,
    warning: Option<&str>,
) -> Value {
    let items: Vec<_> = files.iter().map(|f| (f.path.as_path(), &f.posted)).collect();
    let mut out = posted_header(env, &items);
    if let Some(w) = warning {
        out["warning"] = json!(w);
    }
    if files.len() <= 1 {
        if with_program && let Some(f) = files.first() {
            out["program"] = json!(f.posted.program);
        }
        return out;
    }
    out["files"] = json!(files.iter().map(|f| f.path.display().to_string()).collect::<Vec<_>>());
    let mounts: Vec<Value> = files
        .iter()
        .map(|f| {
            let mut m = json!({
                "path": f.path.display().to_string(),
                "setups": f.mount.setups,
                "machine": crate::setup_mount::machine_label(&f.mount.machine),
                "post": crate::posting::post_ref_label(&f.mount.post),
                "ops": f.posted.ops,
                "lines": f.posted.lines,
            });
            if with_program {
                m["program"] = json!(f.posted.program);
            }
            m
        })
        .collect();
    out["mounts"] = Value::Array(mounts);
    out
}

/// `cam.post`'s answer when `per_op` writes one standalone program per
/// operation (#2188): lists every written file and per-operation entry.
pub(crate) fn posted_per_op_view(
    env: &mut Env,
    files: Vec<crate::posting::PostedFile>,
    with_program: bool,
) -> Value {
    let items: Vec<_> = files.iter().map(|f| (f.path.as_path(), &f.posted)).collect();
    let mut out = posted_header(env, &items);
    out["files"] = json!(files.iter().map(|f| f.path.display().to_string()).collect::<Vec<_>>());
    if files.len() == 1 {
        if with_program && let Some(f) = files.first() {
            out["program"] = json!(f.posted.program);
        }
        return out;
    }
    let ops_view: Vec<Value> = files
        .iter()
        .map(|f| {
            let mut row = json!({
                "path": f.path.display().to_string(),
                "operation": f.op,
                "lines": f.posted.lines,
            });
            if with_program {
                row["program"] = json!(f.posted.program);
            }
            row
        })
        .collect();
    out["operations"] = Value::Array(ops_view);
    out
}

/// Split a `cam.post` target path into `(output_dir, program_prefix)`
/// for `post_per_op`; folder paths use the `"program"` prefix inside that folder.
pub(super) fn per_op_target(to: &std::path::Path) -> Result<(&std::path::Path, &str), String> {
    if to.as_os_str().is_empty() {
        return Err("a \"to\" path is required - a program that exists nowhere is not a result".into());
    }
    let s = to.as_os_str().to_string_lossy();
    if to.is_dir() || s.ends_with('/') || s.ends_with('\\') {
        return Ok((to, "program"));
    }
    let dir = to.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(std::path::Path::new("."));
    let program = to.file_stem().and_then(|s| s.to_str()).unwrap_or("program");
    Ok((dir, program))
}
