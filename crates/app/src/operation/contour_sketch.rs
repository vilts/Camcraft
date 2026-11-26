//! #1864: 2D Contour resolution, open-chain safety checks, per-chain side/direction overrides,
//! and XY-transformed toolpath generation for DXF sketch profiles.
//!
//! Provenance: clean-room 2D contouring over `geometry::sketch_chains`; closed loops honour per-chain
//! `ChainSide` overrides or operation `Side` (`Auto` reads `ClosedChain::nesting` outer-vs-hole); open
//! chains cut along centerline under `Side::Auto` and refuse any side/control compensation by name.

use std::collections::HashSet;

use flecs_ecs::prelude::World;

use crate::cl::Toolpath;
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::sketch_chains::ChainId;
use crate::machining2d::{self, ContourParams, GenError, Side};

use super::contour_sketch_pick::collect_chains;
use super::wcs_yaw::Rot2;
use super::{AreaCtx, OpParams, SrcRef, apply_area_controls, stitch};

#[derive(Clone, Debug)]
pub(super) struct ResolvedSketchChain {
    pub(super) chain_id: ChainId,
    /// The DXF profile entity that owns the chain; nesting is per profile.
    pub(super) entity: String,
    pub(super) label: String,
    pub(super) closed: bool,
    pub(super) pts: Vec<Point2>,
    pub(super) parent: Option<ChainId>,
    pub(super) eff_side: Option<Side>,
    pub(super) eff_climb: bool,
    pub(super) natural_auto: bool,
    pub(super) open_side_err: Option<String>,
    pub(super) auto_role_err: Option<String>,
}

#[derive(Clone, Debug)]
pub(super) struct SketchContourPlan {
    pub(super) items: Vec<ResolvedSketchChain>,
    pub(super) region_loops: Vec<Loop>,
    pub(super) notes: Vec<String>,
}

impl SketchContourPlan {
    pub(super) fn rotate_xy(&mut self, rot: &Rot2) -> Result<(), GenError> {
        for item in &mut self.items {
            for p in &mut item.pts {
                *p = rot.p2(*p);
            }
        }
        self.region_loops = self
            .region_loops
            .iter()
            .map(|l| rot.loop2(l))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(())
    }

    pub(super) fn open_chains(&self) -> Vec<Vec<Point2>> {
        self.items
            .iter()
            .filter(|i| !i.closed)
            .map(|i| i.pts.clone())
            .collect()
    }

    pub(super) fn region(&self) -> Result<Option<Region>, GenError> {
        if self.region_loops.is_empty() {
            Ok(None)
        } else {
            Ok(Some(Region::new(self.region_loops.clone())?))
        }
    }
}

/// True when `params` is a `2D Contour` and `src` names at least one DXF sketch profile.
#[must_use]
pub(crate) fn is_sketch_contour(world: &World, src: &SrcRef, params: &OpParams) -> bool {
    matches!(params, OpParams::Contour { .. }) && !crate::sketch_model::source_profiles(world, src).is_empty()
}

pub(super) fn resolve_plan(
    world: &World,
    src: &SrcRef,
    params: &OpParams,
) -> Result<SketchContourPlan, GenError> {
    let OpParams::Contour {
        tolerance,
        side,
        climb,
        chain_overrides,
        ..
    } = params
    else {
        return Err(GenError::Invalid("not a 2D Contour operation".into()));
    };
    let items = collect_chains(world, src, *tolerance, *side, *climb, chain_overrides)?;
    let mut region_loops = Vec::new();
    let mut closed_outer = 0usize;
    let mut closed_inside = 0usize;
    let mut open_count = 0usize;
    for item in &items {
        if item.closed {
            region_loops.push(Loop::new(item.pts.clone())?);
            match item.eff_side {
                Some(Side::Inside) => closed_inside += 1,
                _ => closed_outer += 1,
            }
        } else {
            open_count += 1;
        }
    }
    let mut notes = Vec::new();
    if closed_outer + closed_inside > 0 || open_count > 0 {
        notes.push(format!(
            "DXF sketch contour: {closed_outer} outside, {closed_inside} inside, {open_count} open centerline"
        ));
    }
    Ok(SketchContourPlan {
        items,
        region_loops,
        notes,
    })
}

fn validate_plan(plan: &SketchContourPlan, control_comp: bool, comp_name: &str) -> Result<(), GenError> {
    for item in &plan.items {
        if let Some(why) = &item.open_side_err {
            return Err(GenError::Invalid(why.clone()));
        }
        if !item.closed && control_comp {
            return Err(GenError::Invalid(format!(
                "{comp_name} compensation does not ride open sketch chain {} - choose In computer",
                item.label
            )));
        }
        if item.closed
            && item.eff_side.is_none()
            && let Some(why) = &item.auto_role_err
        {
            return Err(GenError::Invalid(why.clone()));
        }
    }
    Ok(())
}

/// Summary notes + early-door refusal for UI property pages (`selection_account` / `selection_memo`).
pub(super) fn account(
    world: &World,
    src: &SrcRef,
    params: &OpParams,
) -> (Vec<String>, Option<String>, Option<bool>) {
    match resolve_plan(world, src, params) {
        Ok(plan) => {
            let comp = params.links().is_some_and(|l| l.approach.compensation.in_control());
            let label = params
                .links()
                .map(|l| l.approach.compensation.label())
                .unwrap_or("Control");
            let refusal = validate_plan(&plan, comp, label).err().map(|e| e.to_string());
            let inside = uniform_inside(&plan);
            (plan.notes, refusal, inside)
        }
        Err(e) => (Vec::new(), Some(e.to_string()), None),
    }
}

fn uniform_inside(plan: &SketchContourPlan) -> Option<bool> {
    let mut closed = plan.items.iter().filter(|i| i.closed);
    let first = closed.next()?.eff_side?;
    let all_same = closed.all(|i| i.eff_side == Some(first));
    all_same.then_some(first == Side::Inside)
}

/// True when all closed chains form an unmodified natural nesting (every Hole's parent is
/// also present, same climb, and natural Auto sides) inside ONE DXF profile. In that case one
/// unified `Region::new` cuts holes Inside and outers Outside together in a single
/// level-by-level contour pass. Loops from separate profiles never group: their Auto side
/// comes from each profile's own nesting, and an even-odd region of the union would turn a
/// loop that lies inside another profile's loop into a hole (#1864 review). The grouped and
/// the per-chain paths must give the same side, so a Reverse-only change never moves it.
fn can_group_natural(closed: &[&ResolvedSketchChain], op_climb: bool) -> bool {
    let Some(first) = closed.first() else {
        return false;
    };
    let ids: HashSet<ChainId> = closed.iter().map(|c| c.chain_id).collect();
    closed.iter().all(|c| {
        c.natural_auto
            && c.entity == first.entity
            && c.eff_climb == op_climb
            && c.parent.is_none_or(|p| ids.contains(&p))
    })
}

fn append_notes(dst: &mut Vec<String>, src: Vec<String>) {
    for n in src {
        if !dst.contains(&n) {
            dst.push(n);
        }
    }
}

/// Generate the `2D Contour` toolpath for a validated `SketchContourPlan` and its verification region.
#[allow(clippy::too_many_arguments)]
pub(super) fn generate_plan(
    plan: SketchContourPlan,
    cp: &ContourParams,
    op_side: Side,
    op_climb: bool,
    mut area: Option<AreaCtx>,
    d: f64,
    comp_name: &str,
    carried_top: &mut dyn FnMut(&Region, &Toolpath) -> Option<f64>,
) -> Result<(Region, Toolpath, Option<f64>), GenError> {
    let control_comp = cp.links.approach.compensation.in_control();
    validate_plan(&plan, control_comp, comp_name)?;
    if plan.region_loops.is_empty() && area.is_some() {
        return Err(GenError::Invalid(
            "the Boundary settings shape an area, and these picks are open sketch chains the cutter rides - reset them, or pick closed chains".into(),
        ));
    }
    // Boundary controls (Exclude Stock, Include, ...) shape closed regions only. An open
    // chain is a centerline the cutter rides, so the controls cannot keep it out of an
    // excluded area: refuse the mix by name instead of cutting through it (#1864 review).
    if area.is_some()
        && let Some(open) = plan.items.iter().find(|i| !i.closed)
    {
        return Err(GenError::Invalid(format!(
            "the Boundary settings shape an area, and {} is an open sketch chain the cutter rides outside those controls - reset the Boundary settings, or pick closed chains only",
            open.label
        )));
    }

    let closed_items: Vec<&ResolvedSketchChain> = plan.items.iter().filter(|i| i.closed).collect();
    let verify_region = if plan.region_loops.is_empty() {
        Region::empty()
    } else {
        Region::new(plan.region_loops.clone())?
    };

    let mut path = Toolpath::new(Vec::new());
    let mut max_carried: Option<f64> = None;
    append_notes(&mut path.notes, plan.notes.clone());

    if !closed_items.is_empty() {
        if can_group_natural(&closed_items, op_climb) {
            let mut g_cp = cp.clone();
            g_cp.side = Side::Outside;
            g_cp.climb = op_climb;
            let mut ctx = area.take();
            if let Some(c) = &mut ctx {
                c.contour_side(false);
            }
            let reg = apply_area_controls(verify_region.clone(), ctx.as_ref())?;
            let mut sub = machining2d::contour(&reg, &g_cp)?;
            if let Some(m) = carried_top(&super::top_stock::contour_reach(&reg, false, d), &sub) {
                g_cp.links.approach.material_top = Some(m);
                max_carried = Some(max_carried.map_or(m, |p| p.max(m)));
                sub = machining2d::contour(&reg, &g_cp)?;
            }
            let notes = std::mem::take(&mut sub.notes);
            path = if path.moves.is_empty() { sub } else { stitch(path, sub) };
            append_notes(&mut path.notes, notes);
        } else {
            // Order holes (Inside or nested children) before outer chains so scraps are freed last.
            let mut ordered = closed_items;
            ordered.sort_by_key(|c| usize::from(c.eff_side != Some(Side::Inside)));
            let area_ctx = area.take();
            for item in ordered {
                let eff_side = item.eff_side.unwrap_or(match op_side {
                    Side::Inside => Side::Inside,
                    _ => Side::Outside,
                });
                let single_raw = Region::new(vec![Loop::new(item.pts.clone())?])?;
                let mut local_ctx = area_ctx.clone();
                if let Some(c) = &mut local_ctx {
                    c.contour_side(eff_side == Side::Inside);
                }
                let reg = apply_area_controls(single_raw, local_ctx.as_ref())?;
                let mut c_cp = cp.clone();
                c_cp.side = eff_side;
                c_cp.climb = item.eff_climb;
                let mut sub = machining2d::contour(&reg, &c_cp)?;
                if let Some(m) = carried_top(
                    &super::top_stock::contour_reach(&reg, eff_side == Side::Inside, d),
                    &sub,
                ) {
                    c_cp.links.approach.material_top = Some(m);
                    max_carried = Some(max_carried.map_or(m, |p| p.max(m)));
                    sub = machining2d::contour(&reg, &c_cp)?;
                }
                let notes = std::mem::take(&mut sub.notes);
                path = if path.moves.is_empty() { sub } else { stitch(path, sub) };
                append_notes(&mut path.notes, notes);
            }
        }
    }

    for item in plan.items.iter().filter(|i| !i.closed) {
        let mut o_cp = cp.clone();
        o_cp.side = Side::Auto;
        o_cp.climb = item.eff_climb;
        let next = machining2d::contour_along_chain(&item.pts, &o_cp)
            .map_err(|e| e.after_moves(&path.moves))?;
        let notes = next.notes.clone();
        path = if path.moves.is_empty() { next } else { stitch(path, next) };
        append_notes(&mut path.notes, notes);
    }

    Ok((verify_region, path, max_carried))
}
