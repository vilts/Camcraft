//! The verification gate's stock-sim clause: the program-level material
//! verdict, and - when the sim cannot judge - a NAMED not-judged line.
//! Provenance: our own design (the Prime Directive's "correct, or
//! visibly refused" read onto the M2.3 D gate); nothing from vendor
//! material.
//!
//! ## The law here
//!
//! The material sim (`stock_sim::simulate_document`) refuses programs
//! it cannot model - the turn family's half-plane, rotary sectors,
//! posed (tilted-tool) moves, a rotated stock, a later setup's model
//! stock, no stock at all. Before this clause that refusal was dropped
//! on the floor: the post went ahead and nothing said the material was
//! never judged. Now the refusal is ALWAYS visible - it rides the post
//! result, the status line and the G-code viewer's header as "stock
//! sim did not judge this program: <reason>".
//!
//! Whether it BLOCKS turns on the second judge. Every operation that
//! posts was generated through `operation::generate`, whose verify
//! pass (the depth-aware laws, the envelope scan, the revolved and
//! rotary verdicts) judges each path against the part and caches its
//! convictions as warnings - and any cached warning already blocks the
//! post (all but the scallop note, M13.9: a spacing's surface-quality
//! reading, shown and never binding - `Warning::blocks_post`). So a
//! fresh, stamped op with a silent cache HAS a clean
//! generator-time verdict (a convicted one blocks on its own WARN
//! records, by name), and the sim's silence alone is "unjudged at
//! program level", not "unsafe": it posts, loudly labelled. An op that
//! cannot show that verdict (no stamp, not fresh, or the op list itself
//! unreadable) has no judge at all, and then the not-judged line is a
//! blocking problem - two missing verdicts never add up to a pass.

use crate::model;
use crate::operation::{self, OpStatus};
use flecs_ecs::prelude::*;

/// The prefix every not-judged line carries, said once.
pub const NOT_JUDGED: &str = "stock sim did not judge this program";

/// The stock-sim clause's two possible outputs: a blocking problem
/// (a located gouge, or a not-judged program with an op no generator
/// verdict covers), and the visible-but-non-blocking not-judged line.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SimClause {
    pub problem: Option<String>,
    pub not_judged: Option<String>,
}

/// Run the program sim and read its outcome for the gate. A void
/// raw-stock-only mark (#1094) rides the visible, non-blocking line.
#[must_use]
pub fn sim_clause(world: &World) -> SimClause {
    let mut clause = judge_clause(world);
    if let Some(note) = crate::stock_source::void_note(world) {
        clause.not_judged = Some(match clause.not_judged.take() {
            Some(line) => format!("{line}; note: {note}"),
            None => format!("note: {note}"),
        });
    }
    clause
}

fn judge_clause(world: &World) -> SimClause {
    // #1094: a stock source marked raw stock only, with no other model,
    // leaves nothing judged as the part - refused by name, never passed.
    if let Some(why) = crate::stock_source::nothing_judged(world) {
        return SimClause { problem: Some(why), not_judged: None };
    }
    match crate::stock_sim::simulate_document(world) {
        Ok(sim) if sim.dev.gouge_cells > 0 => {
            let u = crate::units::of(world);
            let at = sim.dev.max_gouge_at.map_or_else(
                || "?".to_owned(),
                |q| {
                    format!(
                        " at ({}, {}, z {})",
                        crate::units::len(q.x, u, 2),
                        crate::units::len(q.y, u, 2),
                        crate::units::len(q.z, u, 2)
                    )
                },
            );
            SimClause {
                problem: Some(format!(
                    "gouge: program cuts below the model in {} cell(s), worst {} {}{at}{}",
                    sim.dev.gouge_cells,
                    crate::units::len(sim.dev.max_gouge, u, 2),
                    u.suffix(),
                    crate::stock_source::gouge_hint(world, sim.dev.max_gouge_at).unwrap_or_default() // #1094
                )),
                not_judged: None,
            }
        }
        Ok(_) => SimClause::default(),
        Err(reason) => {
            let line = format!("{NOT_JUDGED}: {reason}");
            let problem = uncovered(world).map(|why| {
                format!("{line} - and {why}, so nothing judged it; refusing")
            });
            SimClause {
                problem,
                not_judged: Some(line),
            }
        }
    }
}

/// The first op the generator-time verify does NOT cover, or `None`
/// when every op carries a verdict: fresh and stamped (the stamp
/// matching the world as it stands is the proof the cache came out of
/// `generate`'s verify pass against it). A CONVICTED op is covered
/// too - its verdict exists, and its cached warnings block the post by
/// name on their own WARN records; this clause only asks whether a
/// judge spoke at all.
fn uncovered(world: &World) -> Option<String> {
    let ops = match crate::posting::program_ops(world) {
        Ok(ops) => ops,
        Err(e) => return Some(format!("no operation's generator verdict can be read ({e})")),
    };
    for (name, e) in &ops {
        let Some(op) = model::operation_of(world, e) else {
            return Some(format!("{name} lost its operation"));
        };
        if op.stamp.is_none()
            || operation::status_of(world, e) != OpStatus::Ok
            || model::toolpath_of(world, e).is_none()
        {
            return Some(format!("{name} carries no generator-time verify verdict"));
        }
    }
    None
}
