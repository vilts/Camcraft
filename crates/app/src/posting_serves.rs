//! The post-serves-kind door (#1232): a section reaches a post only
//! when that post serves the kind of machine its setup runs on.
//!
//! Provenance: our own rule, read off [`super::post_serves`] (the
//! picker's listing law, M13.5) and made binding at post time. That a
//! milling program (XY plane, tool change, tool spindle) cannot drive a
//! two-axis lathe, nor a cutting program a torch or a wire, is general
//! machining knowledge from the machine-tool handbooks. No vendor
//! material consulted.
//!
//! The law: before any NC text is written, every mount the program
//! posts (whole program, one file per mount, one file per operation)
//! is asked whether its post serves its machine's kind. A shipped post
//! is judged by its registry name ([`super::post_serves`]); a script of
//! the user's own by the dialect its manifest declares (the
//! `capabilities` mask, [`crate::post::dialect::work_voice`]): a lathe
//! takes only a script that declares turning, a mill or a plasma table
//! refuses one that declares turning, and no undeclared user script
//! serves an EDM (the registry's wire posts serve a wire EDM, #533, and
//! its sinker posts a sinker EDM, #542 - each the only door for its
//! kind since the "no post yet" refusal retired). A refusal names the
//! setup, its operations, the post and the machine's kind - nothing is
//! posted.
//!
//! A user's script states the kinds it serves in its manifest
//! (`serves = ["plasma"];`, #1266): it then posts to exactly those
//! kinds. One that states none is accepted only where its dialect
//! proves it (turning serves a lathe and a mill-turn, a non-turning
//! one a mill and a mill-turn). A script declaring `wireEdm = true`
//! serves only a wire EDM (#1335). Other undeclared scripts are refused
//! on a plasma table or EDM, with a request to declare `serves`.
//!
//! And per section (maintainer, 2026-11-12, on a mill-turn): a section
//! that turns the work - a turning op or an on-centre drill, as
//! [`crate::lathe_voice::voice_of`] reads it - posts only through a
//! post whose dialect is turning ([`work_voice`] `WorkSpindle`). On a
//! lathe or a mill-turn a milling post would write it for the TOOL
//! spindle (a turning insert spun at the section's rpm, the facing
//! pass as Y moves): refused by name. Milling and C-axis sections on a
//! mill-turn still post through a milling post.

use flecs_ecs::prelude::World;

use crate::machine::{Kind, Machine};
use crate::post::dialect::{declared_serves, work_voice, WorkVoice};

/// The registry name of a script text, when it is a shipped post's.
fn registry_name(cps: &str) -> Option<&'static str> {
    // #2282: the shipped text is the cached disk read, not a compiled-in copy.
    super::BUILTINS
        .iter()
        .find(|(name, _)| super::builtin_post_text(name).is_ok_and(|text| text == cps))
        .map(|(name, _)| *name)
}

/// Whether the script `cps` serves a machine of `kind`.
fn serves(cps: &str, kind: Kind) -> Result<bool, String> {
    if let Some(name) = registry_name(cps) {
        return Ok(super::post_serves(name, kind));
    }
    if let Some(kinds) = declared_serves(cps).map_err(|e| format!("post engine: {e}"))? {
        return Ok(kinds.contains(&kind));
    }
    if crate::post::js::Script::load(cps).map_err(|e| format!("post engine: {e}"))?.voices_wire_edm() {
        return Ok(kind == Kind::WireEdm);
    }
    let turning = work_voice(cps).map_err(|e| format!("post engine: {e}"))? == WorkVoice::WorkSpindle;
    Ok(match kind {
        Kind::Lathe => turning,
        Kind::MillTurn => true,
        Kind::Mill => !turning,
        // The mask cannot tell a plasma script from a milling one, so an
        // undeclared script is not proven to serve a torch or a wire.
        Kind::Plasma | Kind::WireEdm | Kind::SinkerEdm | Kind::Waterjet => false,
    })
}

/// The operations of `ops` that turn the work, when `kind` has a work
/// spindle and `cps` is not a turning dialect: each would be written
/// for the tool spindle.
fn unturned<'a>(world: &World, ops: &[&'a str], kind: Kind, cps: &str) -> Result<Vec<&'a str>, String> {
    if !matches!(kind, Kind::Lathe | Kind::MillTurn)
        || work_voice(cps).map_err(|e| format!("post engine: {e}"))? == WorkVoice::WorkSpindle
    {
        return Ok(Vec::new());
    }
    Ok(ops
        .iter()
        .copied()
        .filter(|name| crate::model::find_by_name(world, name).is_some_and(|e| crate::lathe_voice::on_work_spindle(world, &e)))
        .collect())
}

/// The refusal for one mount, or `None` when its post serves it:
/// `setups` (effective names, `""` for the implicit setup) and their
/// `ops` (program order) run on `machine` through the script `cps`;
/// `label` names a script that is not a shipped one (its path).
///
/// # Errors
/// A script that does not load, as everywhere.
pub(super) fn refusal(
    world: &World,
    setups: &[String],
    ops: &[&str],
    machine: &(String, Option<Machine>),
    cps: &str,
    label: Option<&str>,
) -> Result<Option<String>, String> {
    let kind = crate::machine::kind(machine.1.as_ref());
    let who = if setups.iter().all(String::is_empty) { "the setup".to_owned() } else { setups.join(", ") };
    let post = match (registry_name(cps), label) {
        (Some(name), _) => format!("post {name}"),
        (None, Some(path)) => format!("post script {path}"),
        (None, None) => "the given post script".to_owned(),
    };
    if serves(cps, kind)? {
        let turned = unturned(world, ops, kind, cps)?;
        if turned.is_empty() {
            return Ok(None);
        }
        return Ok(Some(format!(
            "{who} ({}): {post} is not a turning post - on {}, a {} machine, these sections turn the work, and a \
             milling post would write them for the tool spindle; give the setup a post that serves turning, or move \
             them to a setup that does, or suppress them",
            turned.join(", "),
            crate::setup_mount::machine_label(machine),
            kind.label()
        )));
    }
    Ok(Some(format!(
        "{who} ({}): {post} does not serve {}, a {} machine - it would write a program that machine cannot run; \
         give the setup a post that serves {}, or suppress its operations{}",
        ops.join(", "),
        crate::setup_mount::machine_label(machine),
        kind.label(),
        kind.label(),
        if registry_name(cps).is_some() {
            String::new()
        } else {
            format!(" (a script of your own states the kinds it serves: serves = [\"{}\"];)", kind.word())
        }
    )))
}

#[cfg(test)]
#[path = "posting_serves/tests.rs"]
mod tests;
