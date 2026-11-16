//! Between setups (#1532): where one program runs from one setup into
//! the next, the operator has a part to flip or re-clamp, so the post
//! stops there by default - an M0 (or M1, by choice) after the setup's
//! last operation, with a comment naming the next setup - or writes one
//! file per setup. Running straight on is the user's explicit choice.
//!
//! Provenance: our own design (issue #1532); a setup being one clamping,
//! and a program stop (M0, RS-274 / ISO 6983) being how a program waits
//! for the operator, is general machine-shop practice - no vendor
//! implementation inspected.
//!
//! The laws:
//! - **The stop rides the setup's last operation.** It folds into that
//!   operation's after-action (`PARAM AfterOperation`), so the dialect
//!   writes it where it writes an operator's own stop: after the spindle
//!   and coolant are off. A `PARAM SetupChange` with the next setup's
//!   name rides beside it, for the comment.
//! - **Independent of "Skip pauses and homes".** That switch mutes the
//!   operations' own stops for a proven program; a re-clamp is not one of
//!   them - the part still has to be turned over.
//! - **One word per gap.** An operation that ends with Go home at a setup
//!   change is refused by name: the post writes one after-action there,
//!   and a trip home is not a pause.
//! - **Per mount.** A boundary is between two setups of one file's
//!   program (one mount: one machine, and one post unless an explicit
//!   script posts them all); the last operation of a mount gets no stop,
//!   whatever setup another mount's file runs next.

use std::collections::HashMap;

use flecs_ecs::prelude::{Entity, World};

use crate::model;
use crate::post::{cnc::Record, AfterOperation};
use crate::schema::BetweenSetups;
use serde_json::json;

/// The stop the program's between-setups choice writes at a setup
/// change, `None` when it writes none (run straight on, one file per
/// setup, or one file per operation).
fn stop_of(world: &World) -> Option<AfterOperation> {
    let o = crate::setup_post_options::in_force(world);
    if o.per_op {
        return None;
    }
    match o.between_setups {
        BetweenSetups::Pause => Some(AfterOperation::ProgramStop),
        BetweenSetups::OptionalStop => Some(AfterOperation::OptionalStop),
        BetweenSetups::Continue | BetweenSetups::FilePerSetup => None,
    }
}

/// Fold the setup-change stops into `afters` (the program's posted
/// after-actions, `program_entry::posted_afters`). Returns, per operation
/// that ends a setup, the name of the setup its mount runs next - what
/// [`ask`] writes into its stream. `by_post` groups the mounts as the
/// post that reads the streams does (`setup_mount::program_mounts`). A
/// Go home at a setup change is refused by name.
pub(super) fn fold(
    world: &World,
    ops: &[(String, Entity)],
    afters: &mut HashMap<String, AfterOperation>,
    by_post: bool,
) -> Result<HashMap<String, String>, String> {
    let mut out = HashMap::new();
    let Some(stop) = stop_of(world) else {
        return Ok(out);
    };
    // Each op's effective setup and the mount (one file) it posts in,
    // program order.
    let mounts = crate::setup_mount::program_mounts(world, by_post);
    let placed: Vec<(String, Option<usize>)> = ops
        .iter()
        .map(|(_, e)| {
            let setup = model::operation_of(world, e)
                .and_then(|op| crate::setup::effective(world, op.setup.as_deref()))
                .unwrap_or_default();
            let mount = mounts.iter().position(|m| m.setups.contains(&setup));
            (setup, mount)
        })
        .collect();
    for (i, ((name, e), (setup, mount))) in ops.iter().zip(&placed).enumerate() {
        // The next op of the same mount's program.
        let rest = placed.get(i.saturating_add(1)..).unwrap_or_default();
        let Some((next, _)) = rest.iter().find(|(_, m)| m == mount) else {
            continue;
        };
        if next == setup {
            continue;
        }
        let now = afters.get(name).copied().unwrap_or_default();
        let merged = match (now, stop) {
            (AfterOperation::GoHome, _) => {
                let label = model::operation_label(world, name, e);
                return Err(format!(
                    "{label} ends with Go home, and the program changes to setup '{next}' after it, which needs a pause for the re-clamp - \
                     the post writes one after-action there: set the operation to Pause (M0), or choose Run straight on or One file per setup under Between setups"
                ));
            }
            (AfterOperation::ProgramStop, _) | (_, AfterOperation::ProgramStop) => AfterOperation::ProgramStop,
            _ => stop,
        };
        afters.insert(name.clone(), merged);
        out.insert(name.clone(), next.clone());
    }
    Ok(out)
}

/// Write the setup change into the stream of the operation `name` that
/// ends a setup: `PARAM SetupChange "<next setup>"`, just ahead of its
/// `PARAM AfterOperation` (the engine reads the pair), else ahead of its
/// section.
pub(super) fn ask(changes: &HashMap<String, String>, name: &str, records: &mut Vec<Record>) {
    let Some(next) = changes.get(name) else {
        return;
    };
    let at = records
        .iter()
        .position(|r| matches!(r, Record::Param { name, .. } if name == "AfterOperation"))
        .or_else(|| records.iter().position(|r| matches!(r, Record::Section { begin: true })))
        .unwrap_or(records.len());
    records.insert(at, Record::Param { name: "SetupChange".into(), value: json!(next) });
}

/// The program's mounts split one per setup when the between-setups
/// choice writes one file per setup (and the post is not one file per
/// operation already); unchanged otherwise.
#[must_use]
pub(super) fn split(world: &World, mounts: Vec<crate::setup_mount::Mount>) -> Vec<crate::setup_mount::Mount> {
    let o = crate::setup_post_options::in_force(world);
    if o.per_op || o.between_setups != BetweenSetups::FilePerSetup {
        return mounts;
    }
    mounts
        .into_iter()
        .flat_map(|m| {
            let (machine, post) = (m.machine, m.post);
            m.setups
                .into_iter()
                .map(move |s| crate::setup_mount::Mount { setups: vec![s], machine: machine.clone(), post: post.clone() })
        })
        .collect()
}
