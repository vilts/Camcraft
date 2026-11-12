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
    file_stem, join_streams, op_streams, options_value, post_options, script_of, verification_report, viewer_map,
    wcs_slot, Posted,
};
use crate::machine::{check_ops, Machine, MachineCheck};
use crate::model;
use crate::post::{cnc, engine};
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

/// The active setup's mount - what a program with nothing to post reads
/// (its refusal is the post's own, "no operations to post").
fn active(world: &World) -> Mount {
    Mount {
        setups: vec![crate::setup::active(world).unwrap_or_default()],
        machine: crate::machine::of(world),
        post: super::post_of(world),
    }
}

/// The program's one mount, or a refusal naming every mount it spans:
/// one file holds one post for one machine. `by_post` false groups by
/// machine alone (a caller handing its own script).
pub(super) fn single(world: &World, by_post: bool) -> Result<Mount, String> {
    let mounts = program_mounts(world, by_post);
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
fn member(world: &World, e: &Entity, setups: &[String]) -> bool {
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
/// program reads exactly as before.
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
    out
}

/// Post the program as one file per mount, through each mount's own
/// post. A one-mount program writes `out_path` exactly as
/// [`super::post_setup`] does; several mounts write
/// `<stem>_<nn>_<setups>.<ext>` beside it, in program order, each in
/// its post's own extension.
pub fn post_mounts(world: &World, out_path: &Path) -> Result<Vec<PostedMount>, String> {
    post_mounts_with(world, out_path, None)
}

/// [`post_mounts`] with one script for every mount (a caller's own
/// `.cps`): the mounts are then grouped by machine alone.
pub fn post_mounts_script(world: &World, out_path: &Path, cps: &str) -> Result<Vec<PostedMount>, String> {
    post_mounts_with(world, out_path, Some(cps))
}

fn post_mounts_with(world: &World, out_path: &Path, cps: Option<&str>) -> Result<Vec<PostedMount>, String> {
    let mounts = program_mounts(world, cps.is_none());
    if mounts.len() <= 1 {
        let posted = match cps {
            None => super::post_setup(world, out_path)?,
            Some(c) => super::post_script(world, out_path, c)?,
        };
        let mount = mounts.into_iter().next().unwrap_or_else(|| active(world));
        return Ok(vec![PostedMount { path: out_path.to_path_buf(), mount, posted }]);
    }
    let started = crate::clock::Instant::now();
    let posted = post_each(world, out_path, cps, mounts);
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

fn post_each(world: &World, out_path: &Path, cps: Option<&str>, mounts: Vec<Mount>) -> Result<Vec<PostedMount>, String> {
    // Every script first: a post that cannot be read refuses before
    // anything is judged, as the single-file post does.
    let mut scripts: Vec<String> = Vec::with_capacity(mounts.len());
    for m in &mounts {
        scripts.push(match cps {
            Some(c) => c.to_owned(),
            None => script_of(&m.post)?.1,
        });
    }
    let streams = op_streams(world)?;
    // #1190: each mount's own script opens only what a lathe may run.
    for (m, cps) in mounts.iter().zip(&scripts) {
        let own = streams
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| model::find_by_name(world, name).is_some_and(|e| member(world, &e, &m.setups)));
        crate::lathe_voice::at_post(world, own, cps)?;
    }
    // #1174: an explicit script is the dialect every op posts through;
    // otherwise each op's own mount's post (the scripts read above).
    let mut report = match cps {
        Some(c) => {
            let voice = crate::post::dialect::work_voice(c).map_err(|e| format!("post engine: {e}"))?;
            crate::posting::verification_report_by(world, &|_| voice)
        }
        None => verification_report(world),
    };
    for m in &mounts {
        report.problems.extend(wcs_slot::shared_word_problems_in(world, Some(&m.setups)));
    }
    if !report.problems.is_empty() {
        return Err(format!("post blocked - {}", report.problems.join("; ")));
    }
    let dir = out_path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let program = out_path.file_stem().and_then(|s| s.to_str()).unwrap_or("program");
    let options = options_value(post_options(world));
    let mut files = Vec::with_capacity(mounts.len());
    for (k, (mount, cps)) in mounts.into_iter().zip(scripts).enumerate() {
        let own: Vec<(String, Vec<cnc::Record>)> = streams
            .iter()
            .filter(|(name, _)| model::find_by_name(world, name).is_some_and(|e| member(world, &e, &mount.setups)))
            .cloned()
            .collect();
        let records = join_streams(own);
        let out = engine::run_with(&cps, &machine_value_for(&mount.machine), &cnc::serialize(&records), &options)
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
