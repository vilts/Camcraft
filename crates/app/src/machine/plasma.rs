//! The machine check on a plasma table (#669): what a program asks a
//! torch gantry, judged against what the 1300PL-class machine states.
//!
//! Provenance: our own rules, specced from Tormach's public 1300PL
//! Operator's Manual UM10720 v1223A - §2 (X/Y cut, Z is the torch
//! lifter; feed ceilings per axis), §6.3 (the control owns every Z
//! move: probe, pierce height, cut height) - and Tormach's public
//! PathPilot Post Processor Guidelines (no M3/M4/M5 on plasma).
//! Behaviour only, our own words.
//!
//! What is judged: XY travel (arc bulges included, the mill check's
//! own extremes), XY feed against the slowest driven X/Y ceiling, the
//! changer's count, and the refusals by name - an op that is not a
//! plasma op, a spindle word, a tilted or turned move. What is NOT:
//! Z depth and Z travel (the lifter is the control's; the plasma
//! verification laws, #672, judge the through-cut at the post gate -
//! `posting::plasma_clause` - and the generator's kerf and cover laws
//! at generation) and rpm / spindle power (there is no spindle). Those
//! are SKIPPED with a named note on the check, never failed and never
//! silent.

use flecs_ecs::prelude::{Entity, World};

use super::{move_extremes, say, slowest_axis, Machine, MachineCheck};
use crate::model;
use crate::operation::Family;

/// Is this op one a plasma table cuts? The plasma family's (#670 - the
/// Plasma Profile). The match is exhaustive on purpose - a new family
/// must answer this question.
fn cuts_plasma(op: &crate::schema::Operation) -> bool {
    match op.params.kind().info().family() {
        Family::Plasma => true,
        Family::Mill | Family::Turn | Family::WireEdm | Family::Sinker => false,
    }
}

/// The family's work, in the words the refusal names.
fn family_word(f: Family) -> &'static str {
    f.work()
}

/// [`super::check_ops`] for a machine of kind plasma.
pub(super) fn check_plasma_ops(
    world: &World,
    machine: &Machine,
    ops: &[(String, Entity)],
) -> Result<MachineCheck, String> {
    let mut out = MachineCheck::default();
    out.notes.push(format!(
        "{}: Z is the torch lifter - cutting depth and Z travel are not judged here (the control sets pierce and cut height); the through-cut is judged by the plasma laws at the post gate (cut order, cut crossing, kerf crossing, tip-up) and by the kerf and cover laws at generation - the 3D stock sim does not run for a torch",
        machine.model
    ));
    out.notes.push(format!(
        "{}: a plasma table has no spindle - rpm and spindle power are not judged",
        machine.model
    ));

    if let Some(slots) = machine.tool_count {
        let mut names: Vec<String> = Vec::new();
        for (_, e) in ops {
            if let Some(op) = model::operation_of(world, e)
                && !names.contains(&op.tool)
            {
                names.push(op.tool.clone());
            }
        }
        if names.len() > slots {
            say(&mut out, format!("program calls {} tools, {} carries {slots} torch", names.len(), machine.model));
        }
    }

    for (name, e) in ops {
        let op = model::operation_of(world, e).ok_or_else(|| format!("{name}: lost its operation"))?;
        if !cuts_plasma(&op) {
            let kind = op.params.kind();
            say(
                &mut out,
                format!(
                    "{name}: a {kind} operation is {} work - a plasma table cuts only plasma operations",
                    family_word(kind.info().family())
                ),
            );
        }
        let tool_entity = model::find_by_name(world, &op.tool)
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        let tool = model::tool_of(world, &tool_entity)
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        // The spindle word: today every op posts its tool's rpm, so a
        // tool stating one asks the program for an S/M3 a torch has not.
        if tool.rpm > 0.0 {
            say(
                &mut out,
                format!(
                    "{name}: tool {} states {:.0} rpm - a plasma table has no spindle to turn",
                    op.tool, tool.rpm
                ),
            );
        }
        let cache = model::toolpath_of(world, e).ok_or_else(|| format!("{name}: lost its toolpath"))?;
        let (_, origin) = crate::posting::wcs_context(world, op.setup.as_deref());
        for (i, mv) in cache.path.moves.iter().enumerate() {
            if mv.rotary.is_some() || mv.axis.is_some_and(|a| !crate::cl::is_vertical(&a)) {
                say(&mut out, format!("{name}: move {i} tilts or turns the tool - {} states no rotary axis", machine.model));
            }
            // Feed: the X/Y share of the move only - a Z component is
            // the lifter's, which the control drives, not the program.
            if let Some(feed) = mv.feed {
                let d = [mv.to.x - mv.from.x, mv.to.y - mv.from.y, 0.0];
                if let Some(ceil) = machine.feed_ceiling(d)
                    && feed > ceil
                {
                    say(&mut out, format!(
                        "{name}: move {i} feeds {feed:.0} mm/min, {} tops out at {ceil:.0}",
                        slowest_axis(&machine.axes, ceil)
                    ));
                }
            }
            for p in move_extremes(mv) {
                for (k, w) in [p.x, p.y].iter().enumerate() {
                    let Some(ax) = machine.axis(k) else { continue };
                    let Some((lo, hi)) = ax.travel else { continue };
                    let v = (w - origin[k]) * ax.direction.sign();
                    if v < lo || v > hi {
                        say(&mut out, format!(
                            "{name}: move {i} reaches {v:.3} in {}, the stroke is {lo:.3}..{hi:.3}",
                            ax.name
                        ));
                    }
                }
            }
        }
    }
    Ok(out)
}
