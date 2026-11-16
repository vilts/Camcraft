//! One NC file per operation (#111): the program cut into standalone
//! programs, one per operation, so an operator can rerun or prove out
//! a single op on its own.
//!
//! Provenance: our own design (issue #111, RECOMMENDED.md); the gate is
//! `posting`'s own, and each file's shape (header, tool change, safe
//! start, end) is the dialect's whole-program shape from the shipped
//! `.cps` scripts - public RS-274 / ISO 6983 behaviour only, per the
//! Clean-Room Protocol.
//!
//! The law: every file is judged before any is written. The program
//! gate (freshness, tools, gouge, limits, fixtures, probe) runs once
//! over the same op list, each op's own post must come back with no
//! counted warning, and only then are the files written - into a
//! scratch folder first, then moved into place. A refusal anywhere
//! writes nothing: never a folder with ops 1 and 3 but not 2.

use super::{
    machine_value_of, op_streams, script_of, script_source, viewer_map,
    Posted,
};
use crate::post::{cnc, engine};
use flecs_ecs::prelude::World;
use std::path::{Path, PathBuf};

/// One written program of a per-operation post.
#[derive(Debug)]
pub struct PostedFile {
    /// Where it was written.
    pub path: PathBuf,
    /// The operation it carries (entity name).
    pub op: String,
    /// The program as posted, the same evidence a whole-job post gives.
    pub posted: Posted,
}

/// A post script as a per-op post resolves it: its label (a post
/// file's path, for a refusal to name; `None` for a caller's own text)
/// and its text.
type Script = (Option<String>, String);

/// Longest sanitised operation name in a file name: a file system
/// limit is never the reason a post fails.
const NAME_MAX: usize = 48;

/// A name cut to the portable file-name characters (ASCII letters,
/// digits, `-`, `_`; everything else becomes `_`, runs collapse, ends
/// are trimmed). Empty after that is `fallback`.
pub(super) fn sanitise(name: &str, fallback: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' };
        if c == '_' && out.ends_with('_') {
            continue;
        }
        out.push(c);
    }
    let cut: String = out.trim_matches('_').chars().take(NAME_MAX).collect();
    let cut = cut.trim_end_matches('_');
    if cut.is_empty() { fallback.to_owned() } else { cut.to_owned() }
}

/// The file stem of operation `index` (1-based, program order):
/// `<program>_<nn>_<op>`, both names sanitised, `nn` at least two
/// digits so a folder listing sorts in program order.
#[must_use]
pub fn file_stem(program: &str, index: usize, op_name: &str) -> String {
    format!("{}_{index:02}_{}", sanitise(program, "program"), sanitise(op_name, "op"))
}

/// Post every program operation into `dir` as its own standalone
/// program, each through its own setup's post for its own setup's
/// machine (#388). See [`post_per_op_script`].
pub fn post_per_op(world: &World, dir: &Path, program: &str) -> Result<Vec<PostedFile>, String> {
    // Every mount's script first: a post that cannot be read refuses
    // before anything is judged.
    let mounts = crate::setup_mount::program_mounts(world, true);
    if mounts.is_empty() {
        script_source(world)?;
    }
    let mut scripts: Vec<(Vec<String>, Script)> = Vec::with_capacity(mounts.len());
    for m in mounts {
        let (label, text) = script_of(&m.post)?;
        scripts.push((m.setups, (Some(label), text)));
    }
    post_per_op_with(world, dir, program, |setup| {
        scripts.iter().find(|(s, _)| s.contains(&setup.to_owned())).map(|(_, t)| t.clone())
    })
}

/// [`post_per_op`] at a given script. Suppressed ops write no file
/// (`program_ops` leaves them out). Each op's stream posts alone, as
/// program number 1, 2, 3... (the O-word a numbering dialect writes),
/// so each file opens with the header and units, calls its own tool,
/// states its work offset, spindle and coolant, and ends with the
/// dialect's spindle stop, coolant off, home and program end.
pub fn post_per_op_script(
    world: &World,
    dir: &Path,
    program: &str,
    cps: &str,
) -> Result<Vec<PostedFile>, String> {
    post_per_op_with(world, dir, program, |_| Some((None, cps.to_owned())))
}

/// The per-op post with each op's script chosen by its effective setup
/// name (`""` for the implicit setup) with its label (a post file's
/// path, for a refusal to name); the machine is always the op's own
/// setup's (#388).
fn post_per_op_with(
    world: &World,
    dir: &Path,
    program: &str,
    script: impl Fn(&str) -> Option<Script>,
) -> Result<Vec<PostedFile>, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let streams = op_streams(world)?;
    // Each op's setup and script, resolved once - and the lathe door
    // (#1190) before anything else is judged: a turning dialect opens
    // only what a lathe may run.
    let mut resolved = Vec::with_capacity(streams.len());
    // #1232: each setup's ops, for the post-serves-kind door below.
    let mut by_setup: Vec<(String, Vec<&str>, Option<String>, String)> = Vec::new();
    for (name, _) in &streams {
        let setup = crate::model::find_by_name(world, name)
            .and_then(|e| crate::model::operation_of(world, &e))
            .and_then(|op| op.setup);
        let effective = crate::setup::effective(world, setup.as_deref()).unwrap_or_default();
        let (label, cps) = script(&effective).ok_or_else(|| format!("{name}: no post resolved for its setup"))?;
        crate::lathe_voice::at_post(world, [name.as_str()], &cps, label.as_deref())?;
        match by_setup.iter_mut().find(|(s, ..)| *s == effective) {
            Some((_, ops, ..)) => ops.push(name.as_str()),
            None => by_setup.push((effective, vec![name.as_str()], label, cps.clone())),
        }
        resolved.push((setup, cps));
    }
    let mut unserved = Vec::new();
    for (setup, ops, label, cps) in &by_setup {
        let machine = crate::setup_mount::machine_of(world, Some(setup.as_str()).filter(|s| !s.is_empty()));
        unserved.extend(super::serves::refusal(world, std::slice::from_ref(setup), ops, &machine, cps, label.as_deref())?);
    }
    // #1174: the workholding clause reads each op's own script's dialect
    // (keyed by effective setup name, as `script` is); unreadable reads
    // as undeclared - the stricter reading.
    let voice = |declared: Option<&str>| {
        let setup = crate::setup::effective(world, declared).unwrap_or_default();
        script(&setup).map_or(crate::post::dialect::WorkVoice::Undeclared, |(_, c)| {
            crate::post::dialect::work_voice(&c).unwrap_or(crate::post::dialect::WorkVoice::Undeclared)
        })
    };
    let mut report = crate::posting::verification_report_by(world, &voice);
    report.problems.splice(0..0, unserved);
    if !report.problems.is_empty() {
        return Err(format!("post blocked - {}", report.problems.join("; ")));
    }
    let options = super::program_options(world);
    let mut files = Vec::with_capacity(streams.len());
    for (k, ((name, records), (setup, cps))) in streams.iter().zip(resolved).enumerate() {
        let index = k + 1;
        let id = super::document_program_id(world, index)?;
        let ext = engine::extension_of(&cps).map_err(|e| format!("post engine: {e}"))?;
        let ext = sanitise(&ext, "nc");
        let mch = machine_value_of(world, setup.as_deref());
        let out = engine::run_program_with(&cps, &mch, &cnc::serialize(records), &options, &id)
            .map_err(|e| format!("{name}: post engine: {e}"))?;
        if !out.warnings.is_empty() {
            return Err(format!(
                "post blocked - {name}: {} warning(s): {}",
                out.warnings.len(),
                out.warnings.join("; ")
            ));
        }
        let line_pos = viewer_map(records, &out);
        files.push(PostedFile {
            path: dir.join(format!("{}.{ext}", file_stem(program, index, name))),
            op: name.clone(),
            posted: Posted {
                ops: 1,
                lines: out.program.lines().count(),
                program: out.program,
                line_pos,
                blocks: out.blocks,
                not_judged: report.not_judged.clone(),
            },
        });
    }
    let texts: Vec<(&Path, &str)> = files.iter().map(|f| (f.path.as_path(), f.posted.program.as_str())).collect();
    write_all(dir, program, &texts)?;
    Ok(files)
}

/// Write every file or none: all into a scratch folder beside the
/// targets, then each renamed into place. A failed rename takes back
/// the ones already moved, so a refusal leaves no partial set.
pub(super) fn write_all(dir: &Path, program: &str, files: &[(&Path, &str)]) -> Result<(), String> {
    let scratch = dir.join(format!(".{}.camcraft-post-{}", sanitise(program, "program"), std::process::id()));
    if scratch.exists() {
        std::fs::remove_dir_all(&scratch).map_err(|e| format!("cannot clear {}: {e}", scratch.display()))?;
    }
    std::fs::create_dir(&scratch).map_err(|e| format!("cannot write into {}: {e}", dir.display()))?;
    let staged = files.iter().try_for_each(|(path, text)| {
        let name = path.file_name().ok_or_else(|| format!("{}: no file name", path.display()))?;
        std::fs::write(scratch.join(name), text.as_bytes()).map_err(|e| format!("cannot write {}: {e}", path.display()))
    });
    if let Err(err) = staged {
        let _ = std::fs::remove_dir_all(&scratch);
        return Err(err);
    }
    let mut moved: Vec<&Path> = Vec::with_capacity(files.len());
    for (path, _) in files {
        let Some(name) = path.file_name() else { continue };
        if let Err(e) = std::fs::rename(scratch.join(name), path) {
            for done in &moved {
                let _ = std::fs::remove_file(done); // never a partial set
            }
            let _ = std::fs::remove_dir_all(&scratch);
            return Err(format!("cannot write {}: {e}", path.display()));
        }
        moved.push(path);
    }
    // Every file is in place: an empty scratch folder left behind is
    // untidy, not a wrong program - never a refusal after the write.
    let _ = std::fs::remove_dir(&scratch);
    Ok(())
}
