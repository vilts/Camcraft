//! Waterjet program checks (#1881).
//! Provenance: our own machine-limit checks over cached moves and the issue's XY/optional-Z rules.

use flecs_ecs::prelude::{Entity, World};
use super::{move_extremes, say, slowest_axis, Machine, MachineCheck};
use crate::{model, operation::Family, tools::ToolKind};

pub(super) fn check_waterjet_ops(world: &World, machine: &Machine, ops: &[(String, Entity)]) -> Result<MachineCheck, String> {
    let mut out = MachineCheck::default();
    out.notes.push(format!("{}: no spindle; spindle rpm and power are not judged", machine.model));
    if machine.axis(2).is_none() {
        out.notes.push(format!("{}: no Z axis; Z depth and travel are not judged", machine.model));
    }
    for (name, e) in ops {
        let op = model::operation_of(world, e).ok_or_else(|| format!("{name}: lost its operation"))?;
        if op.params.kind().info().family() != Family::Waterjet {
            say(&mut out, format!("{name}: a waterjet machine admits only waterjet operations"));
        }
        let tool = model::find_by_name(world, &op.tool).and_then(|e| model::tool_of(world, &e))
            .ok_or_else(|| format!("{name}: tool {} is not in the library", op.tool))?;
        if tool.kind != ToolKind::Waterjet {
            say(&mut out, format!("{name}: a waterjet operation requires a waterjet tool"));
        }
        if let Some(why) = tool.torch_refusal() {
            say(&mut out, format!("{name}: {why}"));
        }
        let cache = model::toolpath_of(world, e).ok_or_else(|| format!("{name}: lost its toolpath"))?;
        let (_, origin) = crate::posting::wcs_context(world, op.setup.as_deref());
        for (i, mv) in cache.path.moves.iter().enumerate() {
            if mv.rotary.is_some() || mv.axis.is_some_and(|a| !crate::cl::is_vertical(&a)) {
                say(&mut out, format!("{name}: move {i} tilts or turns; a waterjet has no rotary axis"));
            }
            if let Some(feed) = mv.feed {
                // An arc drives both XY axes even when its endpoints coincide.
                let d = if mv.arc.is_some() { [1.0, 1.0, 0.0] }
                    else { [mv.to.x - mv.from.x, mv.to.y - mv.from.y, 0.0] };
                if let Some(ceil) = machine.feed_ceiling(d) && feed > ceil {
                    say(&mut out, format!("{name}: move {i} feeds {feed:.0} mm/min, {} tops out at {ceil:.0}", slowest_axis(&machine.axes, ceil)));
                }
            }
            for p in move_extremes(mv) {
                for (k, w) in [p.x, p.y, p.z].iter().enumerate() {
                    let Some(ax) = machine.axis(k) else { continue };
                    let Some((lo, hi)) = ax.travel else { continue };
                    let v = (w - origin[k]) * ax.direction.sign();
                    if v < lo || v > hi {
                        say(&mut out, format!("{name}: move {i} reaches {v:.3} in {}, the stroke is {lo:.3}..{hi:.3}", ax.name));
                    }
                }
            }
        }
    }
    Ok(out)
}
