//! One NC file per mount (#388): a program whose setups mount different
//! machines or posts is written as one file per [`Mount`], each through
//! its own post for its own machine, instead of being refused.
//!
//! Provenance: our own design (issue #388); a setup being one clamping on
//! one machine, run by that machine's program, is general machine-shop
//! practice - no vendor implementation inspected.
//!
//! The laws:
//! - **One mount, one file - byte for byte as before.** A program whose
//!   setups share one machine and one post (every document before #343,
//!   and every one that never gave a setup its own mount) takes the
//!   single-file post unchanged.
//! - **Each file is that mount posted alone.** A mount's file is its own
//!   ops' streams, joined exactly as the single-file post joins them, run
//!   through its own post with its own machine - so it is byte-identical
//!   to posting those setups with the rest of the program left out.
//! - **Judged once, on the right steel.** The program gate runs once over
//!   the whole program - every machine law reading each op's own mount -
//!   and each file adds its own one-register clause. Every file is judged
//!   before any is written; a refusal anywhere writes nothing.

use super::per_op::{sanitise, write_all};
use super::{
    file_stem, join_streams, script_of, verification_report, viewer_map,
    wcs_slot, Posted,
};
use crate::machine::{check_ops, Machine, MachineCheck};
use crate::model;
use crate::post::{cnc, engine, options::OptionValue};
use std::collections::BTreeMap;
use crate::schema::PostRef;
use crate::setup_mount::{program_mounts, Mount};
use serde_json::Value;
use flecs_ecs::prelude::{Entity, World};
use std::path::{Path, PathBuf};

/// One written file of a per-mount post.
#[derive(Debug)]
pub struct PostedMount {
    /// Where it was written.
    pub path: PathBuf,
    /// The mount it carries: its setups, machine and post.
    pub mount: Mount,
    /// The program as posted, the same evidence a whole-job post gives.
    pub posted: Posted,
}

/// The post a declared setup context runs (#388): its own, else the
/// document's - what an operation of that setup posts through.
#[must_use]
pub fn post_of_setup(world: &World, declared: Option<&str>) -> PostRef {
    crate::setup_mount::post_of(world, declared)
}

/// [`super::machine_value`] for a declared setup context's machine (#388).
#[must_use]
pub fn machine_value_of(world: &World, declared: Option<&str>) -> Value {
    machine_value_for(&crate::machine::of_setup(world, declared))
}

/// A mount's machine in the post grammar: the mirror, or the default
/// VMC where it says no machine.
#[must_use]
pub fn machine_value_for(machine: &(String, Option<Machine>)) -> Value {
    match &machine.1 {
        Some(m) => crate::post::machine::mirror(m),
        None => super::default_machine(),
    }
}

/// [`machine_value_for`] as a post reads it (#2319): where the mount says
/// no machine, the default machine file must read - a missing or
/// malformed file refuses the post by its name, never posts a null machine.
pub(super) fn posted_machine_for(machine: &(String, Option<Machine>)) -> Result<Value, String> {
    match &machine.1 {
        Some(m) => Ok(crate::post::machine::mirror(m)),
        None => super::default_machine_checked().map_err(|why| format!("post blocked - {why}")),
    }
}

/// The active setup's mount - what a program with nothing to post reads
/// (its refusal is the post's own, "no operations to post").
pub(super) fn active(world: &World) -> Mount {
    Mount {
        setups: vec![crate::setup::active(world).unwrap_or_default()],
        machine: crate::machine::of(world),
        post: super::post_of(world),
    }
}

/// Refuse combined-program posting when Output is "One file per operation" (#2188):
/// setup-change stops are suppressed for per-op files, so a combined program
/// would cross setup boundaries without a stop.
pub(super) fn refuse_per_op(world: &World, surface: &str) -> Result<(), String> {
    if crate::setup_post_options::in_force(world).per_op {
        return Err(format!(
            "Output is set to One file per operation - {surface} cannot hold per-operation programs; \
             post one file per operation, or switch Output to One program"
        ));
    }
    Ok(())
}

/// The program's one mount, or a refusal naming every mount it spans:
/// one file holds one post for one machine. `by_post` false groups by
/// machine alone (a caller handing its own script).
pub(super) fn single(world: &World, by_post: bool) -> Result<Mount, String> {
    refuse_per_op(world, "one combined file")?;
    let mounts = program_mounts(world, by_post);
    // #1532: "One file per setup" asks a file per setup - one file
    // cannot hold two, so the single-file post refuses and says how.
    let per_setup = super::setup_change::split(world, mounts.clone());
    if mounts.len() <= 1 && per_setup.len() > 1 {
        let names: Vec<String> = per_setup.iter().map(Mount::label).collect();
        return Err(format!(
            "Between setups is set to One file per setup, and the program holds {} setups ({}) - \
             post it as one file per setup, or choose another Between setups",
            per_setup.len(),
            names.join("; ")
        ));
    }
    if mounts.len() > 1 {
        let names: Vec<String> = mounts.iter().map(Mount::label).collect();
        return Err(format!(
            "the program's setups run on {} mounts ({}) - one file holds one post for one machine; \
             post the program as one file per mount",
            mounts.len(),
            names.join("; ")
        ));
    }
    Ok(mounts.into_iter().next().unwrap_or_else(|| active(world)))
}

/// Whether operation entity `e` belongs to a mount of `setups`.
pub(super) fn member(world: &World, e: &Entity, setups: &[String]) -> bool {
    model::operation_of(world, e)
        .map(|op| crate::setup::effective(world, op.setup.as_deref()).unwrap_or_default())
        .is_some_and(|name| setups.contains(&name))
}

/// Each machine of the program against the ops its setups run (#388):
/// one entry per mount with a machine, program order - the limit check
/// the gate and the status line's machine vote read. A machine-less
/// mount checks nothing (the unstated-limit law); with nothing to post
/// the active setup's machine is read, and the program's refusal is its
/// answer.
pub fn machine_checks(world: &World) -> Vec<(Machine, Result<MachineCheck, String>)> {
    let mut mounts = program_mounts(world, false);
    if mounts.is_empty() {
        mounts.push(active(world));
    }
    let program = super::program_ops(world);
    mounts
        .into_iter()
        .filter_map(|mount| {
            let m = mount.machine.1?;
            let ops = program.clone().map(|all| {
                all.into_iter()
                    .filter(|(_, e)| member(world, e, &mount.setups))
                    .collect::<Vec<_>>()
            });
            let check = ops.and_then(|ops| check_ops(world, &m, &ops));
            Some((m, check))
        })
        .collect()
}

/// The limit clause of the program gate (M2.3 D): [`machine_checks`],
/// worded as the single-machine gate always worded it, so a one-mount
/// program reads exactly as before. A mount with no machine is judged
/// against the default machine's stated limits (#2321).
pub(super) fn machine_problems(world: &World) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (m, check) in machine_checks(world) {
        // #374: an inverted axis the post cannot translate.
        out.extend(m.inverted_post_refusal());
        let line = match check {
            Ok(c) if !c.is_ok() => {
                let more = if c.more > 0 { format!(" (+{} more)", c.more) } else { String::new() };
                format!("machine {}: {} limit problem(s): {}{more}", m.model, c.problems.len(), c.problems.join("; "))
            }
            Ok(_) => continue,
            Err(err) => format!("machine: {err}"),
        };
        if !out.contains(&line) {
            out.push(line);
        }
    }
    out.extend(super::default_mch::default_machine_problems(world));
    out
}

/// Post the program as one file per mount, through each mount's own
/// post. A one-mount program writes `out_path` exactly as
/// [`super::post_setup`] does; several mounts write
/// `<stem>_<nn>_<setups>.<ext>` beside it, in program order, each in
/// its post's own extension.
pub fn post_mounts(world: &World, out_path: &Path) -> Result<Vec<PostedMount>, String> {
    post_mounts_with(world, out_path, None, None)
}

/// [`post_mounts`] with temporary per-run script option overrides (#258).
pub fn post_mounts_with_script_options(
    world: &World,
    out_path: &Path,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Vec<PostedMount>, String> {
    post_mounts_with(world, out_path, None, extra)
}

/// [`post_mounts`] with one script for every mount (a caller's own
/// `.cps`): the mounts are then grouped by machine alone.
pub fn post_mounts_script(world: &World, out_path: &Path, cps: &str) -> Result<Vec<PostedMount>, String> {
    post_mounts_with(world, out_path, Some((None, cps)), None)
}

/// [`post_mounts_script`] with temporary per-run script option overrides (#258).
pub fn post_mounts_script_with_options(
    world: &World,
    out_path: &Path,
    cps: &str,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Vec<PostedMount>, String> {
    post_mounts_with(world, out_path, Some((None, cps)), extra)
}

/// [`post_mounts`] for a resolved [`PostRef`] with temporary per-run script option overrides (#258, #2190).
pub fn post_mounts_for_post_with_options(
    world: &World,
    out_path: &Path,
    post: &PostRef,
    cps: &str,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Vec<PostedMount>, String> {
    post_mounts_with(world, out_path, Some((Some(post), cps)), extra)
}

fn post_mounts_with(
    world: &World,
    out_path: &Path,
    explicit: Option<(Option<&PostRef>, &str)>,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Vec<PostedMount>, String> {
    refuse_per_op(world, "one file per mount")?;
    // #1532: "One file per setup" writes each setup as its own mount's file.
    let mounts = super::setup_change::split(world, program_mounts(world, explicit.is_none()));
    if mounts.len() > 1 && match explicit {
        Some((Some(post), _)) => super::agie::is_native(post),
        Some((None, _)) => false,
        None => mounts.iter().any(|mount| super::agie::is_native(&mount.post)),
    } {
        return Err("A native wire EDM job requires a single mount and setup".into());
    }
    if mounts.len() <= 1 {
        let posted = match explicit {
            None => super::post_setup_with_script_options(world, out_path, extra)?,
            Some((Some(post), c)) => super::post_script_for_post_with_options(world, out_path, post, c, extra)?,
            Some((None, c)) => super::post_script_with_script_options(world, out_path, c, extra)?,
        };
        let mount = mounts.into_iter().next().unwrap_or_else(|| active(world));
        return Ok(vec![PostedMount { path: out_path.to_path_buf(), mount, posted }]);
    }
    let started = crate::clock::Instant::now();
    let posted = post_each(world, out_path, explicit, mounts, extra);
    match &posted {
        Ok(files) => tracing::info!(
            files = files.len(),
            ms = crate::logging::elapsed_ms(started),
            "NC programs written, one per mount"
        ),
        Err(why) => tracing::warn!("NC programs not written: {why}"),
    }
    posted
}

fn post_each(
    world: &World,
    out_path: &Path,
    explicit: Option<(Option<&PostRef>, &str)>,
    mounts: Vec<Mount>,
    extra: Option<&BTreeMap<String, OptionValue>>,
) -> Result<Vec<PostedMount>, String> {
    // Every script first: a post that cannot be read refuses before
    // anything is judged, as the single-file post does.
    let mut scripts: Vec<(Option<String>, String)> = Vec::with_capacity(mounts.len());
    for m in &mounts {
        scripts.push(match explicit {
            Some((post, c)) => (post.map(super::post_ref_label), c.to_owned()),
            None => script_of(&m.post).map(|(label, text)| (Some(label), text))?,
        });
    }
    let streams = super::op_streams_by(world, explicit.is_none())?;
    // #1190: each mount's own script opens only what a lathe may run;
    // #1232: and writes only for a machine kind it serves.
    let mut unserved = Vec::new();
    for (m, (label, cps)) in mounts.iter().zip(&scripts) {
        let own: Vec<&str> = streams
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| model::find_by_name(world, name).is_some_and(|e| member(world, &e, &m.setups)))
            .collect();
        crate::lathe_voice::at_post(world, own.iter().copied(), cps, label.as_deref())?;
        unserved.extend(super::serves::refusal(world, &m.setups, &own, &m.machine, cps, label.as_deref())?);
    }
    // #1174: an explicit script is the dialect every op posts through;
    // otherwise each op's own mount's post (the scripts read above).
    let mut report = match explicit {
        Some((_, c)) => {
            let voice = crate::post::dialect::work_voice(c).map_err(|e| format!("post engine: {e}"))?;
            crate::posting::verification_report_by(world, &|_| voice)
        }
        None => verification_report(world),
    };
    report.problems.splice(0..0, unserved);
    for m in &mounts {
        report.problems.extend(wcs_slot::shared_word_problems_in(world, Some(&m.setups)));
    }
    if !report.problems.is_empty() {
        return Err(format!("post blocked - {}", report.problems.join("; ")));
    }
    let dir = out_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let program = out_path.file_stem().and_then(|s| s.to_str()).unwrap_or("program");
    let explicit_post = explicit.and_then(|(p, _)| p);
    // #258: each mount's file is a whole program of its own, numbered as
    // the single-file post numbers it (the document's number, or O0001).
    let id = super::document_program_id(world, 1)?;
    let mut files = Vec::with_capacity(mounts.len());
    for (k, (mount, (_, cps))) in mounts.into_iter().zip(scripts).enumerate() {
        let own: Vec<(String, Vec<cnc::Record>)> = streams
            .iter()
            .filter(|(name, _)| model::find_by_name(world, name).is_some_and(|e| member(world, &e, &mount.setups)))
            .cloned()
            .collect();
        let records = join_streams(own);
        let mount_post = explicit_post.or_else(|| explicit.is_none().then_some(&mount.post));
        let mount_setup = mount.setups.first().map(String::as_str);
        let options = super::program_options_for_script(
            world,
            &cps,
            mount_post,
            mount_setup,
            extra,
        )?;
        let mch = posted_machine_for(&mount.machine).map_err(|why| format!("{}: {why}", mount.label()))?;
        let out = engine::run_program_with(&cps, &mch, &cnc::serialize(&records), &options, &id)
            .map_err(|e| format!("{}: post engine: {e}", mount.label()))?;
        if !out.warnings.is_empty() {
            return Err(format!(
                "post blocked - {}: {} warning(s): {}",
                mount.label(),
                out.warnings.len(),
                out.warnings.join("; ")
            ));
        }
        let ext = sanitise(&engine::extension_of(&cps).map_err(|e| format!("post engine: {e}"))?, "nc");
        let path = dir.join(format!("{}.{ext}", file_stem(program, k + 1, &mount.setups.join("_"))));
        let ops = records.iter().filter(|r| matches!(r, cnc::Record::Section { begin: true })).count();
        let line_pos = viewer_map(&records, &out);
        files.push(PostedMount {
            path,
            mount,
            posted: Posted {
                bundle_files: Vec::new(),
                controller_review: None,
                ops,
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
