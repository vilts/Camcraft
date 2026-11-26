//! Corner-relief planning, boundary/wall safety checks, and stock-sim grant (#2400).
//!
//! Provenance: clean-room app guard over `machining2d::pocket_relief_plan`,
//! `geometry::slice::material_above_of`, `geometry::slice::material_overhead_of`,
//! and `sim::verify::CornerReliefZone`. No vendor source was consulted.

use std::ops::Range;

use crate::geo2d::{Point2, Region};
use crate::geometry::{Triangle, slice};
use crate::machining2d::{CornerRelief, GenError, ResolvedRelief, capsule_region, pocket_relief_plan};
use crate::verify::{AvoidSpec, CornerReliefZone};

use super::generate::DepthLaw;
use super::material::FOOTPRINT_SAMPLES;
use super::{OpParams, SrcRef, depth_law_of};

pub(crate) use super::corner_relief_sim::zones_for_op;

const BOUNDARY_TOL: f64 = 1e-3;
const WALL_EXPAND_TOL: f64 = 5e-3;
const SLICE_NUDGE: f64 = 1e-3;

/// Inputs needed to plan and verify inside-corner reliefs for a 2D pocket/clearing op.
pub(super) struct ReliefCheck<'a> {
    pub region: &'a Region,
    pub tool_diameter: f64,
    pub stock_to_leave: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub relief: &'a CornerRelief,
    pub stock_footprint: Option<&'a Region>,
    pub forbidden: &'a [(&'static str, Region)],
    pub avoid: Option<&'a AvoidSpec>,
    pub kept_steel: Option<&'a [Triangle]>,
    pub neighbour_steel: &'a [Triangle],
    pub section_facets: Option<(&'a [Triangle], &'a [Range<usize>])>,
}

fn slice_err(e: slice::SliceError) -> GenError {
    GenError::Invalid(format!("corner relief check could not slice geometry: {e}"))
}

fn seg_pt_dist(p: Point2, a: Point2, b: Point2) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd <= 1e-18 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / dd).clamp(0.0, 1.0)
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

fn seg_seg_dist(a0: Point2, a1: Point2, b0: Point2, b1: Point2) -> f64 {
    let ccw = |p: Point2, q: Point2, r: Point2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2, d3, d4) = (ccw(a0, a1, b0), ccw(a0, a1, b1), ccw(b0, b1, a0), ccw(b0, b1, a1));
    if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
        return 0.0;
    }
    seg_pt_dist(a0, b0, b1)
        .min(seg_pt_dist(a1, b0, b1))
        .min(seg_pt_dist(b0, a0, a1))
        .min(seg_pt_dist(b1, a0, a1))
}

fn seg_min_dist_to_region(a: Point2, b: Point2, region: &Region) -> f64 {
    let mut best = f64::INFINITY;
    for lp in region.loops() {
        let pts = lp.points();
        for i in 0..pts.len() {
            best = best.min(seg_seg_dist(a, b, pts[i], pts[(i + 1) % pts.len()]));
        }
    }
    best
}

fn section_at(facets: &[Triangle], bodies: &[Range<usize>], z: f64, hi: f64) -> Result<Region, GenError> {
    match slice::material_above_of(facets, bodies, z) {
        Ok(r) => Ok(r),
        Err(first) => {
            let z2 = (z + SLICE_NUDGE).min(hi);
            slice::material_above_of(facets, bodies, z2).map_err(|_| slice_err(first))
        }
    }
}

fn overhead_at(facets: &[Triangle], z: f64, hi: f64) -> Result<Region, GenError> {
    match slice::material_overhead_of(facets, &[], z) {
        Ok(r) => Ok(r),
        Err(first) => {
            let z2 = (z + SLICE_NUDGE).min(hi);
            slice::material_overhead_of(facets, &[], z2).map_err(|_| slice_err(first))
        }
    }
}

fn check_avoid(rel: &ResolvedRelief, keepout: &Region, labels: &str) -> Result<(), GenError> {
    let seg_band = capsule_region(rel.start, rel.end, BOUNDARY_TOL)?;
    if keepout.contains(rel.start) || keepout.contains(rel.end) || !keepout.intersect(&seg_band)?.is_empty() {
        return Err(GenError::Invalid(format!(
            "corner relief at ({:.3}, {:.3}) violates 'Do not touch' clearance around {}",
            rel.corner.x, rel.corner.y, labels
        )));
    }
    Ok(())
}

fn check_steel(rel: &ResolvedRelief, cap: &Region, wall: &Region, what: &str) -> Result<(), GenError> {
    if !wall.is_empty() && cap.intersect(wall)?.area().abs() > BOUNDARY_TOL {
        return Err(GenError::Invalid(format!(
            "corner relief at ({:.3}, {:.3}) cuts into {what}",
            rel.corner.x, rel.corner.y
        )));
    }
    Ok(())
}

/// Plan corner reliefs, enforce stock/boundary/avoid/keep/thin-wall safety guards,
/// and return the relieved pocket region plus bounded verification zones.
pub(super) fn plan_and_check(input: &ReliefCheck<'_>) -> Result<(Region, Vec<CornerReliefZone>), GenError> {
    if input.relief.is_off() {
        return Ok((input.region.clone(), Vec::new()));
    }
    let plan = pocket_relief_plan(input.region, input.tool_diameter, input.stock_to_leave, input.relief)?;
    if plan.reliefs.is_empty() {
        return Ok((plan.relieved_region, Vec::new()));
    }
    let r_tool = input.tool_diameter * 0.5;
    let span = input.top_z - input.bottom_z;
    let z_lo = input.bottom_z + (span * 0.5).min(0.05);
    let z_mid = input.bottom_z + span * 0.5;
    let z_hi = input.top_z - (span * 0.5).min(0.05);

    let keepout = match input.avoid {
        Some(spec) if !spec.is_empty() => Some((
            spec.keepout_region(input.bottom_z, input.top_z, 0.0).map_err(GenError::Invalid)?,
            spec.names_list(),
        )),
        _ => None,
    };
    let kept_wall = match input.kept_steel {
        Some(tris) if !tris.is_empty() && span > 0.0 => Some(overhead_at(tris, z_lo, input.top_z)?),
        _ => None,
    };
    let neighbour_wall = if !input.neighbour_steel.is_empty() && span > 0.0 {
        Some(overhead_at(input.neighbour_steel, z_lo, input.top_z)?)
    } else {
        None
    };
    let part_slices: Vec<(Region, Region)> = match input.section_facets {
        Some((facets, bodies)) if !facets.is_empty() && span > 2.0 * SLICE_NUDGE => {
            let mut out = Vec::new();
            for z in [z_lo, z_mid, z_hi] {
                let sec = section_at(facets, bodies, z, input.top_z)?;
                if !sec.is_empty() {
                    let expanded = sec.offset_sharp(WALL_EXPAND_TOL)?;
                    out.push((sec, expanded));
                }
            }
            out
        }
        _ => Vec::new(),
    };
    let stock_expanded = match input.stock_footprint {
        Some(s) => Some((s, s.offset_sharp(0.01)?)),
        None => None,
    };

    let mut zones = Vec::with_capacity(plan.reliefs.len());
    for rel in &plan.reliefs {
        let cap_tool = capsule_region(rel.start, rel.end, r_tool)?;
        for (label, forb) in input.forbidden {
            if cap_tool.intersect(forb)?.area().abs() > BOUNDARY_TOL {
                return Err(GenError::Invalid(format!(
                    "corner relief at ({:.3}, {:.3}) crosses {label}",
                    rel.corner.x, rel.corner.y
                )));
            }
        }
        if let Some((stock, stock_exp)) = &stock_expanded {
            let out_edge = !stock.contains(rel.start)
                || !stock.contains(rel.end)
                || seg_min_dist_to_region(rel.start, rel.end, stock) < r_tool - BOUNDARY_TOL
                || cap_tool.difference(stock_exp)?.area().abs() > BOUNDARY_TOL;
            if out_edge {
                return Err(GenError::Invalid(format!(
                    "corner relief at ({:.3}, {:.3}) exits the stock boundary",
                    rel.corner.x, rel.corner.y
                )));
            }
        }
        if let Some((ko, labels)) = &keepout {
            check_avoid(rel, ko, labels)?;
        }
        if let Some(wall) = &kept_wall {
            check_steel(rel, &cap_tool, wall, "the kept part")?;
        }
        if let Some(wall) = &neighbour_wall {
            check_steel(rel, &cap_tool, wall, "a neighbouring part")?;
        }
        if !part_slices.is_empty() {
            let ear_tool = cap_tool.difference(input.region)?;
            for (sec, sec_exp) in &part_slices {
                if !ear_tool.intersect(sec)?.is_empty()
                    && ear_tool.difference(sec_exp)?.area().abs() > BOUNDARY_TOL
                {
                    return Err(GenError::Invalid(format!(
                        "corner relief at ({:.3}, {:.3}) punches through a thin part wall",
                        rel.corner.x, rel.corner.y
                    )));
                }
            }
        }
        zones.push(CornerReliefZone {
            corner: rel.corner,
            start: rel.start,
            end: rel.end,
            radius: r_tool,
            z_bottom: input.bottom_z,
            z_top: input.top_z,
        });
    }
    Ok((plan.relieved_region, zones))
}

/// Operation-level inputs from `generate::run_job_arms` shared by Pocket, Adaptive2D, and Offset2D.
pub(super) struct JobReliefCtx<'a> {
    pub tool_diameter: f64,
    pub stock_to_leave: f64,
    pub top_z: f64,
    pub bottom_z: f64,
    pub relief: &'a CornerRelief,
    pub stock_region: Option<&'a Region>,
    pub stock_facets: Option<&'a [Triangle]>,
    pub src: &'a SrcRef,
    pub params: &'a OpParams,
    pub material: &'a super::Material,
    pub m_gen: &'a super::Material,
    pub frame: Option<&'a crate::cl::TiltedFrame>,
    pub face_band: Option<&'a (Vec<(&'static str, Region)>, f64)>,
    pub avoid: Option<&'a AvoidSpec>,
    pub part_solid: bool,
    pub around_part: bool,
    pub keep_parts: &'a [Triangle],
    pub neighbour_parts: &'a [Triangle],
}

/// Resolve operation frame inputs and run [`plan_and_check`] when corner relief is active.
pub(super) fn check_op_relief(
    region: &Region,
    ctx: &JobReliefCtx<'_>,
) -> Result<(Region, Vec<CornerReliefZone>), GenError> {
    if ctx.relief.is_off() {
        return Ok((region.clone(), Vec::new()));
    }
    let stock_local = ctx.frame.and_then(|f| {
        super::material_source::judged_stock(ctx.stock_facets, ctx.src, ctx.params, ctx.material)
            .map(|t| f.local_facets(t))
            .and_then(|t| slice::project(&t, FOOTPRINT_SAMPLES).ok())
            .filter(|r| !r.is_empty())
    });
    let stock_footprint = if ctx.frame.is_some() { stock_local.as_ref() } else { ctx.stock_region };
    let law = match depth_law_of(ctx.params, ctx.src, ctx.part_solid) {
        DepthLaw::Keep if !ctx.around_part => DepthLaw::None,
        other => other,
    };
    let keep_local = ctx
        .frame
        .filter(|_| !ctx.keep_parts.is_empty() && law != DepthLaw::Keep)
        .map(|f| f.local_facets(ctx.keep_parts));
    let kept_steel = if law == DepthLaw::Keep {
        Some(ctx.m_gen.facets.as_slice())
    } else if let Some(local) = keep_local.as_deref() {
        Some(local)
    } else if !ctx.keep_parts.is_empty() {
        Some(ctx.keep_parts)
    } else {
        None
    };
    let neighbour_local = ctx
        .frame
        .filter(|_| !ctx.neighbour_parts.is_empty())
        .map(|f| f.local_facets(ctx.neighbour_parts));
    let neighbour_steel = neighbour_local.as_deref().unwrap_or(ctx.neighbour_parts);
    let forbidden = ctx.face_band.map(|(f, _)| f.as_slice()).unwrap_or(&[]);
    let section_facets = (law == DepthLaw::Section)
        .then_some((ctx.m_gen.facets.as_slice(), ctx.m_gen.bodies.as_slice()));
    plan_and_check(&ReliefCheck {
        region,
        tool_diameter: ctx.tool_diameter,
        stock_to_leave: ctx.stock_to_leave,
        top_z: ctx.top_z,
        bottom_z: ctx.bottom_z,
        relief: ctx.relief,
        stock_footprint,
        forbidden,
        avoid: ctx.avoid,
        kept_steel,
        neighbour_steel,
        section_facets,
    })
}
