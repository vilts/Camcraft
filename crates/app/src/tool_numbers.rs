//! Tool numbers (#221): the number a tool posts as (`T..` / `H..`) and the
//! clash law that keeps two tools from posting as one.
//!
//! Provenance: our own rule. Tool-table numbering - a T word naming the
//! tool and an H word naming its length offset, both from the control's
//! table - is general CNC practice (RS-274's T word, ISO 6983); no vendor
//! implementation inspected.
//!
//! A tool receives its own number at creation or import (`ToolDef::number`,
//! lowest unused in `1..=MAX`), and legacy pre-#2442 files freeze unnumbered
//! tools to their 1-based name-sorted list position on load, so adding,
//! renaming, or deleting a tool never shifts any other tool's number. A
//! program whose tools on one machine post the same number is still refused
//! by name: on a machine with a tool changer, `T5 M6` for two different
//! tools loads one of them for both, with the other's length offset.

use flecs_ecs::prelude::World;

use crate::model;

/// The highest tool number accepted. Controls differ (a Tormach table runs
/// 1-1000, others further); the post for a given control refuses past its
/// own table - this is only the sanity bound every control shares.
pub const MAX: u32 = 99_999;

/// How the accepted range reads in a refusal.
pub const RANGE_TEXT: &str = "1-99999";

/// Whether `n` can be a tool number at all (`0` is "no tool" on most
/// controls, so it never names one).
#[must_use]
pub fn valid(n: u32) -> bool {
    (1..=MAX).contains(&n)
}

/// Lowest unused tool number in `1..=MAX` not present in `taken`.
#[must_use]
pub fn next_free_from(taken: &[u32]) -> Option<u32> {
    let used: std::collections::HashSet<u32> = taken.iter().copied().collect();
    (1..=MAX).find(|n| !used.contains(n))
}

/// Lowest unused tool number in `1..=MAX` across all tools in the document.
/// `None` also when a tool has no number and none is free for it: then no
/// number is free for a new one either.
#[must_use]
pub fn next_free(world: &World) -> Option<u32> {
    let taken: Vec<u32> = numbers(world).ok()?.into_iter().map(|(_, n)| n).collect();
    next_free_from(&taken)
}

/// Every tool in document order, with the number it posts as.
///
/// # Errors
/// A tool with no number of its own when none of `1..=MAX` is free for it
/// (see [`assign`]).
pub fn numbers(world: &World) -> Result<Vec<(String, u32)>, String> {
    let tools: Vec<(String, Option<u32>)> = model::named_entities(world)
        .into_iter()
        .filter(|(_, e)| model::is_tool(world, e))
        .map(|(name, e)| (name, model::tool_number_of(world, &e)))
        .collect();
    assign(tools)
}

/// The number each tool posts as: its own, or - for a tool with none (a
/// fixture; files and commands always give one, #2442) - the lowest number
/// no tool has yet, in the given order.
///
/// # Errors
/// An unnumbered tool when every number in `1..=MAX` is taken, by name
/// (#2517): it has no number to post as, and must never post as a made-up one.
pub fn assign(tools: Vec<(String, Option<u32>)>) -> Result<Vec<(String, u32)>, String> {
    let mut taken: Vec<u32> = tools.iter().filter_map(|(_, n)| *n).collect();
    tools
        .into_iter()
        .map(|(name, explicit)| {
            let number = match explicit {
                Some(n) => n,
                None => {
                    let free = next_free_from(&taken).ok_or_else(|| {
                        format!("tool '{name}' has no tool number and none of {RANGE_TEXT} is free - give it its own tool number")
                    })?;
                    taken.push(free);
                    free
                }
            };
            Ok((name, number))
        })
        .collect()
}

/// The number the named tool posts as, if it is a tool.
#[must_use]
pub fn number_of(world: &World, tool: &str) -> Option<u32> {
    numbers(world).ok()?.into_iter().find(|(n, _)| n == tool).map(|(_, t)| t)
}

/// The clash law for one program: among the tools it uses (`used`, names,
/// repeats allowed), two different tools posting the same number is a
/// refusal naming both. `None` when every tool posts a number of its own.
/// A tool that cannot get a number at all is that refusal too ([`assign`]).
#[must_use]
pub fn clash(world: &World, used: &[String]) -> Option<String> {
    let all = match numbers(world) {
        Ok(all) => all,
        Err(why) => return Some(why),
    };
    let mut seen: Vec<(&str, u32)> = Vec::new();
    for name in used {
        let Some((_, number)) = all.iter().find(|(n, _)| n == name) else {
            continue; // not a tool: the post names that refusal itself
        };
        if let Some((other, _)) = seen.iter().find(|(o, t)| *t == *number && *o != name.as_str()) {
            return Some(format!(
                "tools '{other}' and '{name}' would both post as T{number} - give one of them its own tool number"
            ));
        }
        if !seen.iter().any(|(o, _)| *o == name.as_str()) {
            seen.push((name.as_str(), *number));
        }
    }
    None
}

/// The clash law per machine (#438): `ops` (program operations as
/// `(name, entity)`) are grouped by the machine their setup mounts -
/// `setup_mount::program_mounts` grouped by machine alone - and each
/// group's tools are judged on their own. Two tools sharing a number on
/// two different machines are two tool tables and never clash; on one
/// machine they clash even across two posts (one carousel). A program on
/// one machine refuses with the exact #221 message; a multi-machine one
/// prefixes the mount it found the clash on.
#[must_use]
pub fn clash_per_machine(world: &World, ops: &[(String, flecs_ecs::prelude::Entity)]) -> Option<String> {
    if let Some(why) = crate::tool_reservations::program_refusal(world, ops) {
        return Some(why);
    }
    let mounts = crate::setup_mount::program_mounts(world, false);
    let setup_of = |e: &flecs_ecs::prelude::Entity| {
        model::operation_of(world, e).map(|op| {
            (crate::setup::effective(world, op.setup.as_deref()).unwrap_or_default(), op.tool)
        })
    };
    if mounts.len() <= 1 {
        let used: Vec<String> = ops.iter().filter_map(|(_, e)| setup_of(e).map(|(_, t)| t)).collect();
        return clash(world, &used);
    }
    for m in &mounts {
        let used: Vec<String> = ops
            .iter()
            .filter_map(|(_, e)| setup_of(e))
            .filter(|(s, _)| m.setups.contains(s))
            .map(|(_, t)| t)
            .collect();
        if let Some(msg) = clash(world, &used) {
            return Some(format!("{}: {msg}", m.label()));
        }
    }
    None
}
