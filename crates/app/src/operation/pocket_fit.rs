//! The 2D Pocket's tool-fit advice (#3631): an edit-time warning when
//! the generator's own region has no centre position for the tool.
//! Provenance: our own design notes; the predicate is the pocket
//! generator's ("pocket is narrower than the tool"); no vendor material.

use flecs_ecs::prelude::World;

use super::OpParams;
use crate::machining2d::PocketStrategy;
use crate::model;

/// Why the 2D Pocket `op_name` cannot fit its tool, read on the same
/// region that Generate cuts: the job's collected region with the area
/// controls on it (`generate::run_job_arms`). `None` when the tool fits,
/// for every other kind, and whenever this door cannot build exactly
/// the generator's region (a refused collect, a tilted frame, a rotated
/// WCS, rest machining, the retired Adaptive strategy): no estimate
/// stands in for that region, so a legal pocket is never warned.
#[must_use]
pub fn pocket_fit_advice(world: &World, op_name: &str) -> Option<String> {
    let op = model::find_by_name(world, op_name).and_then(|e| model::operation_of(world, &e))?;
    let OpParams::Pocket { stock_to_leave, strategy, rest, .. } = &op.params else {
        return None;
    };
    if rest.is_some() || *strategy == PocketStrategy::Adaptive {
        return None;
    }
    let leave = *stock_to_leave;
    let mut job = super::collect_job(world, &op, op_name).ok()?;
    if job.frame.is_some() || job.wcs_yaw.is_some() || job.sketch_contour.is_some() {
        return None;
    }
    let mut picked = job.picked_region.take();
    let mut section = job.section_z.take();
    let base = super::job::region_for_job(&mut picked, &mut section, &job.src, &job.params, &job.material, None).ok()?;
    let region = match job.area.take() {
        Some(ctx) => super::area::apply_area_full(base, Some(&ctx.ok()?)).ok()?.region,
        None => base,
    };
    let d = job.tool.diameter;
    if crate::machining2d::pocket_tool_fits(&region, d, leave).ok()? {
        return None;
    }
    let len = |v: f64| format!("{} {}", crate::units::len(v, job.unit, 3), job.unit.suffix());
    let walls = if leave > 0.0 { format!(" with {} left on each wall", len(leave)) } else { String::new() };
    Some(format!(
        "{op_name}: the tool '{}' is {} wide and has no position inside the pocket{walls}; Generate will refuse it as narrower than the tool - choose a smaller tool or less stock to leave",
        op.tool,
        len(d),
    ))
}
