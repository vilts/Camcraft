//! Multi-lane decomposition and closed-spine routing for `adaptive_troch` (#1587):
//! single open lanes, closed ring lanes round islands (`layout_closed`),
//! oriented parallel scanline slicing, and residual medial lanes inside
//! leftover pockets. Clean-room decomposition based on Blum (1967) and Held (1991).

use crate::geo2d::{Point2, Region};
use crate::load::load_step;

use super::adaptive::{Spine, medial_spine, widest_point};
use super::adaptive_troch::AdaptiveTrochParams;
use super::adaptive_troch_slice::{candidate_angles, dedup_open, layout_confined, rotate_to_longest_edge, slice_angle};
pub(super) use super::adaptive_troch_slice::{clearance, rest_share, straightened};
use super::face::GenError;
use super::pocket::PocketParams;
use super::pocket_hsm::{SMOOTH_PER_R, components};
use super::trochoid_slot::{DEFAULT_MICROLIFT, LaneLayout, MIN_BAND_PER_D, TrochoidParams, TrochoidReturn, plan_lane_layout};

const SPINE_TOL: f64 = 0.05;
const WALL_MARGIN: f64 = 0.01;
const LANE_GAP: f64 = 0.25;
const REST_MAX: f64 = 0.25;
const WHO: &str = "2D Adaptive (Trochoidal)";

pub(super) struct PlannedLanes {
    pub(super) layouts: Vec<LaneLayout>,
    pub(super) used_center: Region,
    pub(super) cleared: Region,
}

pub(super) fn trochoid_params(b: f64, p: &PocketParams, t: &AdaptiveTrochParams) -> TrochoidParams {
    let r = 0.5 * p.tool_diameter;
    let ret = t.ret.unwrap_or(if p.clearance_z - p.top_z >= DEFAULT_MICROLIFT {
        TrochoidReturn::Microlift { height: DEFAULT_MICROLIFT }
    } else {
        TrochoidReturn::Retract
    });
    TrochoidParams {
        tool_diameter: p.tool_diameter,
        width: 2.0 * (b + r + p.stock_to_leave),
        loop_width: t.loop_width,
        step: load_step(p.tool_diameter, p.optimal_load),
        max_engagement: t.max_engagement,
        ret,
        smoothing: r * SMOOTH_PER_R,
        stock_to_leave: p.stock_to_leave,
        clearance_z: p.clearance_z,
        retract_z: p.retract_z,
        top_z: p.top_z,
        bottom_z: p.bottom_z,
        // #2012: the lane's own ladder walks the pocket's ladder mode.
        stepdown: super::equal_steps::ladder_stepdown(&p.links.approach, p.top_z, p.bottom_z, p.stepdown),
        feed: p.feed,
        return_feed: t.return_feed.unwrap_or(p.feed),
        plunge_feed: p.plunge_feed,
        climb: p.climb,
        stock_top: p.links.approach.stock_top,
    }
}

pub(super) fn narrow_err(clear: f64, p: &PocketParams) -> GenError {
    let (d, r) = (p.tool_diameter, 0.5 * p.tool_diameter);
    GenError::Invalid(format!(
        "{WHO}: the region is {:.3} mm wide where the loops would run, too narrow for a {d:.3} mm tool to loop in (it needs more than {:.3} mm) - use a smaller tool, or cut it as a Slot",
        2.0 * (clear.max(0.0) + r + p.stock_to_leave),
        d * (1.0 + 2.0 * MIN_BAND_PER_D) + 2.0 * (p.stock_to_leave + WALL_MARGIN)
    ))
}

pub(super) fn plan_all_lanes(
    region: &Region,
    center: &Region,
    p: &PocketParams,
    t: &AdaptiveTrochParams,
) -> Result<PlannedLanes, GenError> {
    let d = p.tool_diameter;
    let Some((_, widest)) = widest_point(center) else {
        return Err(narrow_err(0.0, p));
    };
    if widest - WALL_MARGIN <= MIN_BAND_PER_D * d {
        return Err(narrow_err(widest, p));
    }
    let reach = if p.stock_to_leave > 0.0 { region.offset(-p.stock_to_leave)? } else { region.clone() };
    let comps = components(center)?;
    let mut layouts = Vec::new();
    let mut used_center = Region::empty();
    let mut cleared = Region::empty();

    for comp in &comps {
        if widest_point(comp).is_none_or(|(_, w)| w - WALL_MARGIN <= MIN_BAND_PER_D * d) {
            continue;
        }
        if comps.len() == 1 && try_single_fast_path(comp, &reach, p, t, &mut layouts, &mut used_center, &mut cleared)? {
            continue;
        }
        fill_component(comp, center, &reach, p, t, &mut layouts, &mut used_center, &mut cleared)?;
    }
    if layouts.is_empty() {
        return Err(narrow_err(widest, p));
    }
    Ok(PlannedLanes { layouts, used_center, cleared })
}

#[allow(clippy::too_many_arguments)]
fn plan_lane_safe(
    chain: &[Point2],
    b_room: f64,
    loop_w: Option<f64>,
    step_hint: Option<f64>,
    open: bool,
    closed: bool,
    p: &PocketParams,
    t: &AdaptiveTrochParams,
) -> Result<LaneLayout, GenError> {
    let mut tp = TrochoidParams {
        loop_width: match (loop_w, t.loop_width) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        },
        max_engagement: t.max_engagement.min(88.5),
        ..trochoid_params(b_room, p, t)
    };
    if let Some(s) = step_hint {
        tp.step = tp.step.min(s);
    }
    match plan_lane_layout(chain, &tp, open, closed) {
        Ok(lay) => Ok(lay),
        Err(_) => {
            tp.max_engagement = t.max_engagement;
            plan_lane_layout(chain, &tp, open, closed)
        }
    }
}

fn try_single_fast_path(
    comp: &Region,
    reach: &Region,
    p: &PocketParams,
    t: &AdaptiveTrochParams,
    layouts: &mut Vec<LaneLayout>,
    used_center: &mut Region,
    cleared: &mut Region,
) -> Result<bool, GenError> {
    let (d, r) = (p.tool_diameter, 0.5 * p.tool_diameter);
    match medial_spine(comp)? {
        Ok((Spine::Open(pts), half)) => {
            let chain = dedup_open(&straightened(&pts, SPINE_TOL * half));
            if chain.len() < 2 {
                return Ok(false);
            }
            let b = clearance(comp, &chain) - WALL_MARGIN;
            if b <= MIN_BAND_PER_D * d || b > 2.2 * r {
                return Ok(false);
            }
            let b_used = t.loop_width.map_or(b, |w| 0.5 * w);
            let swept = Region::sweep(std::slice::from_ref(&chain), r + b_used)?;
            // An explicit narrow band deliberately leaves stock for cleanup.
            if t.loop_width.is_none() && rest_share(reach, &swept) > REST_MAX {
                return Ok(false);
            }
            let tp = trochoid_params(b, p, t);
            let lay = plan_lane_layout(&chain, &tp, b > r, false).map_err(super::adaptive_troch::renamed)?;
            if layout_confined(&lay, comp, false, p)? {
                *used_center = used_center.union(&Region::sweep(std::slice::from_ref(&chain), lay.b)?)?;
                *cleared = cleared.union(&swept)?;
                layouts.push(lay);
                return Ok(true);
            }
        }
        Ok((Spine::Closed(pts), half)) => {
            let simplified = dedup_open(&straightened(&pts, 0.005 * half));
            let rotated = rotate_to_longest_edge(&simplified);
            let mut chain = dedup_open(&rotated);
            if let Some(&first) = chain.first()
                && chain.last().is_none_or(|&l| (l.x - first.x).hypot(l.y - first.y) > 1e-6)
            {
                chain.push(first);
            }
            if chain.len() < 4 {
                return Ok(false);
            }
            let room_b = clearance(comp, &chain) - WALL_MARGIN;
            let b_max = (room_b - (0.015 * half).max(WALL_MARGIN)).min(1.40 * r);
            for scale in [0.95, 0.84, 0.70, 0.55] {
                let b_loop = t.loop_width.map_or(b_max * scale, |w| (0.5 * w).min(b_max * scale));
                if b_loop <= MIN_BAND_PER_D * d {
                    continue;
                }
                let swept = Region::sweep(std::slice::from_ref(&chain), r + b_loop)?;
                if rest_share(reach, &swept) > REST_MAX {
                    break;
                }
                if let Ok(lay) = plan_lane_safe(&chain, room_b, Some(2.0 * b_loop), None, b_loop > r, true, p, t)
                    && layout_confined(&lay, comp, true, p)?
                {
                    *used_center = used_center.union(&Region::sweep(std::slice::from_ref(&chain), lay.b)?)?;
                    *cleared = cleared.union(&swept)?;
                    layouts.push(lay);
                    return Ok(true);
                }
            }
        }
        Err(_) => {}
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn fill_component(
    comp: &Region,
    center: &Region,
    reach: &Region,
    p: &PocketParams,
    t: &AdaptiveTrochParams,
    layouts: &mut Vec<LaneLayout>,
    used_center: &mut Region,
    cleared: &mut Region,
) -> Result<(), GenError> {
    let (d, r) = (p.tool_diameter, 0.5 * p.tool_diameter);
    for pass in 0..3 {
        let uncut = reach.difference(cleared)?;
        if uncut.offset(-0.025)?.is_empty() {
            break;
        }
        let avail = if used_center.is_empty() { comp.clone() } else { comp.difference(&used_center.offset(LANE_GAP)?)? };
        if avail.is_empty() {
            break;
        }
        let subs = components(&avail)?;
        let min_w = if pass == 0 { MIN_BAND_PER_D * d + WALL_MARGIN } else { (0.35 * r).max(MIN_BAND_PER_D * d + WALL_MARGIN) };
        let min_uncut = if pass == 0 { 0.25 } else { 2.0 };
        let mut added = false;
        for sub in &subs {
            let Some((_, widest)) = widest_point(sub) else {
                continue;
            };
            if widest <= min_w {
                continue;
            }
            if sub.offset(r - 0.02)?.intersect(&uncut)?.area() < min_uncut {
                continue;
            }
            if pass > 0 && sub.loops().len() == 1 && try_residual_medial(sub, center, p, t, layouts, used_center, cleared)? {
                added = true;
                continue;
            }
            if try_parallel_sub(sub, center, p, t, pass > 0, layouts, used_center, cleared)?
                || try_residual_medial(sub, center, p, t, layouts, used_center, cleared)?
            {
                added = true;
            }
        }
        if !added {
            break;
        }
    }
    Ok(())
}

fn try_residual_medial(
    sub: &Region,
    center: &Region,
    p: &PocketParams,
    t: &AdaptiveTrochParams,
    layouts: &mut Vec<LaneLayout>,
    used_center: &mut Region,
    cleared: &mut Region,
) -> Result<bool, GenError> {
    let (d, r) = (p.tool_diameter, 0.5 * p.tool_diameter);
    let Ok(Ok((Spine::Open(pts), half))) = medial_spine(sub) else {
        return Ok(false);
    };
    let chain = dedup_open(&straightened(&pts, SPINE_TOL * half));
    if chain.len() < 2 {
        return Ok(false);
    }
    let margin = (0.025 * half).max(WALL_MARGIN);
    let b = (clearance(sub, &chain) - margin).min(1.8 * r);
    let b = t.loop_width.map_or(b, |w| b.min(0.5 * w));
    if b <= MIN_BAND_PER_D * d {
        return Ok(false);
    }
    let sub_reach = sub.offset(r)?;
    let swept = Region::sweep(std::slice::from_ref(&chain), r + b)?;
    if rest_share(&sub_reach, &swept) > 0.40 {
        return Ok(false);
    }
    let Ok(lay) = plan_lane_safe(&chain, b, None, None, b > r, false, p, t) else {
        return Ok(false);
    };
    if !layout_confined(&lay, center, false, p)? {
        return Ok(false);
    }
    *used_center = used_center.union(&Region::sweep(std::slice::from_ref(&chain), lay.b)?)?;
    *cleared = cleared.union(&swept)?;
    layouts.push(lay);
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn try_parallel_sub(
    sub: &Region,
    center: &Region,
    p: &PocketParams,
    t: &AdaptiveTrochParams,
    allow_short: bool,
    layouts: &mut Vec<LaneLayout>,
    used_center: &mut Region,
    cleared: &mut Region,
) -> Result<bool, GenError> {
    let (d, r) = (p.tool_diameter, 0.5 * p.tool_diameter);
    let Some((_, widest)) = widest_point(sub) else {
        return Ok(false);
    };
    let b_cap = (0.90 * r).min(0.75 * (widest - WALL_MARGIN));
    let b_cap = t.loop_width.map_or(b_cap, |w| b_cap.min(0.5 * w));
    if b_cap <= MIN_BAND_PER_D * d {
        return Ok(false);
    }
    let candidates = candidate_angles(sub);
    let mut best: Option<(f64, usize, f64, Vec<[Point2; 2]>)> = None;
    for theta in candidates {
        let Some((b_use, segs)) = slice_angle(sub, theta, b_cap, r, d, allow_short)? else {
            continue;
        };
        if segs.is_empty() {
            continue;
        }
        let chains: Vec<Vec<Point2>> = segs.iter().map(|s| s.to_vec()).collect();
        let cov = Region::sweep(&chains, r + b_use)?.area();
        if best.as_ref().is_none_or(|(bc, bn, _, _)| cov > bc + 1e-3 || ((cov - bc).abs() <= 1e-3 && segs.len() < *bn)) {
            best = Some((cov, segs.len(), b_use, segs));
        }
    }
    let Some((_, _, b_use, segs)) = best else {
        return Ok(false);
    };
    let mut placed = false;
    let mut step_hint: Option<f64> = None;
    for seg in segs {
        let chain = vec![seg[0], seg[1]];
        if let Ok(lay) = plan_lane_safe(&chain, b_use, None, step_hint, b_use > r, false, p, t)
            && layout_confined(&lay, center, false, p)?
        {
            step_hint = Some(step_hint.map_or(lay.step, |s| s.min(lay.step)));
            *used_center = used_center.union(&Region::sweep(std::slice::from_ref(&chain), lay.b)?)?;
            *cleared = cleared.union(&Region::sweep(std::slice::from_ref(&chain), r + lay.b)?)?;
            layouts.push(lay);
            placed = true;
        }
    }
    Ok(placed)
}
