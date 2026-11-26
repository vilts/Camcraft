//! Live corner-relief zone reconstruction for `stock_sim` (#2400).
//!
//! Provenance: re-evaluates inside-corner relief corridors from current
//! operation geometry and parameters so cached files cannot expand verification grants.

use flecs_ecs::prelude::World;

use crate::geometry::slice;
use crate::schema::Operation;
use crate::verify::CornerReliefZone;

use super::corner_relief::{ReliefCheck, plan_and_check};
use super::generate::DepthLaw;
use super::material::FOOTPRINT_SAMPLES;
use super::{OpParams, SrcRef, apply_area_full, collect_job, depth_law_of, region_for_job, req_z};

/// Live corner-relief zones for `stock_sim::simulate_document`, derived from the
/// operation's model geometry and parameters rather than cached file data.
#[must_use]
pub(crate) fn zones_for_op(world: &World, op: &Operation, op_name: &str) -> Vec<CornerReliefZone> {
    let relief_off = match &op.params {
        OpParams::Pocket { corner_relief, .. } | OpParams::Offset2D { corner_relief, .. } => {
            corner_relief.is_off()
        }
        OpParams::Adaptive2D { corner_relief, method, rest, .. } => {
            corner_relief.is_off()
                || *method == crate::machining2d::AdaptiveMethod::Trochoidal
                || rest.is_some()
        }
        _ => true,
    };
    if relief_off || !matches!(op.src, SrcRef::Selection(_)) {
        return Vec::new();
    }
    let Ok(mut job) = collect_job(world, op, op_name) else {
        return Vec::new();
    };
    if job.frame.is_some()
        || !job.part_solid
        || depth_law_of(&job.params, &job.src, job.part_solid) != DepthLaw::Section
    {
        return Vec::new();
    }
    let yaw = job.wcs_yaw.take();
    if let Some(yw) = yaw
        && yw
            .enter(
                &mut job.material,
                &mut job.stock_facets,
                &mut job.stock_aabb,
                &mut job.picked_region,
                &mut job.contour_sel,
                &mut job.sketch_contour,
                &mut job.sketch_mark,
                &mut job.drive_chains,
                &mut job.slot_bound,
                &mut job.prior,
                &mut job.drills_ahead,
                &mut job.keep_parts,
                &mut job.neighbour_parts,
                &mut job.area,
                &mut job.params,
            )
            .is_err()
    {
        return Vec::new();
    }
    let area_ctx = match job.area.take().transpose() {
        Ok(a) => a,
        Err(_) => return Vec::new(),
    };
    let Ok(base) = region_for_job(
        &mut job.picked_region,
        &mut job.section_z,
        &job.src,
        &job.params,
        &job.material,
        None,
    ) else {
        return Vec::new();
    };
    let Ok(applied) = apply_area_full(base, area_ctx.as_ref()) else {
        return Vec::new();
    };
    let (Ok(top_z), Ok(bottom_z)) = (req_z(job.hs.top, "Top of Part Z"), req_z(job.hs.bottom, "Bottom of Part Z")) else {
        return Vec::new();
    };
    let (stock_to_leave, corner_relief) = match &job.params {
        OpParams::Pocket { stock_to_leave, corner_relief, .. }
        | OpParams::Adaptive2D { stock_to_leave, corner_relief, .. }
        | OpParams::Offset2D { stock_to_leave, corner_relief, .. } => (*stock_to_leave, corner_relief),
        _ => return Vec::new(),
    };
    let stock_footprint = job
        .stock_facets
        .as_deref()
        .and_then(|f| slice::project(f, FOOTPRINT_SAMPLES).ok())
        .filter(|r| !r.is_empty());
    let cutter = crate::verify::Cutter::of(&job.tool).with_body(
        &job.tool.geom(),
        &crate::tool_body::stack(&job.tool.geom(), job.body.as_ref()).with_neck(job.neck.clone()),
    );
    let avoid_spec = match job.avoid.take().transpose() {
        Ok(Some((items, cl))) => Some(super::avoid::spec_of(items, cl, &job.params, cutter)),
        Ok(None) => None,
        Err(_) => return Vec::new(),
    };
    let check = ReliefCheck {
        region: &applied.region,
        tool_diameter: job.tool.diameter,
        stock_to_leave,
        top_z,
        bottom_z,
        relief: corner_relief,
        stock_footprint: stock_footprint.as_ref(),
        forbidden: &applied.forbidden,
        avoid: avoid_spec.as_ref(),
        kept_steel: None,
        neighbour_steel: &job.neighbour_parts,
        section_facets: Some((&job.material.facets, &job.material.bodies)),
    };
    let Ok((_, zones)) = plan_and_check(&check) else {
        return Vec::new();
    };
    match yaw {
        Some(yw) => zones.into_iter().map(|z| yw.exit_zone(z)).collect(),
        None => zones,
    }
}
