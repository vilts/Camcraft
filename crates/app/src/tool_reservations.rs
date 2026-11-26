//! Provenance: machine tool reservations from issue #2604 and ordinary CNC tool-table practice.

use flecs_ecs::prelude::{Entity, World};
use crate::{machine::Machine, model, tools::ToolKind};

/// Judge the actual controller selectors, including old saved tool numbers.
pub(crate) fn on_machine(machine: &Machine, name: &str, kind: ToolKind, number: u32) -> Result<(), String> {
    if !crate::tool_numbers::valid(number) {
        return Err(format!("tool '{name}': tool number {number} is outside {}", crate::tool_numbers::RANGE_TEXT));
    }
    if let Some(why) = machine.tooling_refusal(None) {
        return Err(why);
    }
    for reservation in &machine.reserved_tools {
        let selected = reservation.number == Some(number)
            || reservation.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name.trim()));
        if selected && !(reservation.usage == "probe" && kind == ToolKind::Probe) {
            return Err(format!(
                "tool '{name}' (T{number}): {} is reserved for {} on {}",
                reservation.name.as_deref().map_or_else(|| format!("T{number}"), |n| format!("'{n}'")),
                reservation.usage, machine.model
            ));
        }
    }
    Ok(())
}

/// Edits judge every machine that uses the tool; an unused tool takes the active machine.
pub(crate) fn for_tool(world: &World, name: &str, kind: ToolKind, number: u32) -> Result<(), String> {
    let mut machines = Vec::new();
    for (_, entity) in model::named_entities(world) {
        if let Some(op) = model::operation_of(world, &entity).filter(|op| op.tool == name) {
            let machine = crate::machine::of_setup(world, op.setup.as_deref());
            if !machines.contains(&machine) {
                machines.push(machine);
            }
        }
    }
    if machines.is_empty() {
        machines.push(crate::machine::of(world));
    }
    for (_, machine) in machines {
        if let Some(machine) = machine {
            on_machine(&machine, name, kind, number)?;
        }
    }
    Ok(())
}

/// Allocate from the active machine's table. Reserved probe pockets take precedence.
pub(crate) fn allocate(world: &World, name: &str, kind: ToolKind, taken: &[u32]) -> Result<u32, String> {
    let (_, machine) = crate::machine::of(world);
    let mut used: std::collections::HashSet<u32> = taken.iter().copied().collect();
    if let Some(machine) = &machine {
        if let Some(why) = machine.tooling_refusal(None) {
            return Err(why);
        }
        // Refuse a reserved name even if its numerical pocket has not been stated.
        if let Some(r) = machine.reserved_tools.iter().find(|r| {
            r.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name.trim()))
                && !(r.usage == "probe" && kind == ToolKind::Probe)
        }) {
            return Err(format!("tool '{name}': name is reserved for {} on {}", r.usage, machine.model));
        }
        let probe_pockets: Vec<u32> = machine.reserved_tools.iter()
            .filter(|r| r.usage == "probe" && kind == ToolKind::Probe)
            .filter_map(|r| r.number).collect();
        if !probe_pockets.is_empty() {
            return probe_pockets.into_iter().find(|n| !used.contains(n))
                .ok_or_else(|| format!("tool '{name}': every reserved probe pocket is occupied on {}", machine.model));
        }
    }
    if let Some(machine) = &machine {
        used.extend(machine.reserved_tools.iter().filter_map(|r| r.number));
    }
    (1..=crate::tool_numbers::MAX).find(|n| !used.contains(n))
        .ok_or_else(|| format!("tool '{name}': no free tool numbers remain ({})", crate::tool_numbers::RANGE_TEXT))
}

/// Every operation uses its own setup's machine, also across multiple mounts.
pub(crate) fn program_refusal(world: &World, ops: &[(String, Entity)]) -> Option<String> {
    for (name, entity) in ops {
        let Some(op) = model::operation_of(world, entity) else { continue };
        let Some(tool) = model::find_by_name(world, &op.tool).and_then(|e| model::tool_of(world, &e)) else { continue };
        let Some(machine) = crate::machine::of_setup(world, op.setup.as_deref()).1 else { continue };
        let number = match crate::tool_numbers::number_of(world, &op.tool) {
            Some(n) => n,
            None => return Some(format!("{name}: tool '{}' has no tool number", op.tool)),
        };
        if let Err(why) = on_machine(&machine, &op.tool, tool.kind, number) {
            return Some(format!("{name}: {why}"));
        }
    }
    None
}
