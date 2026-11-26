//! Per-level "Do not touch" pass planning, crossing-free verification, and rest-emission dispatch (#3345).
//! Provenance: our own per-Z-level keep-out adapter over `pocket_avoid`, `rest2d`, `arc_cross`, and `pocket_emit`; no vendor source consulted.

use std::collections::HashSet;

use camcraft_sim::verify::AvoidSpec;

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::GenError;
use super::arc_cross::{CROSS_ROUNDS, crossing_arcs, own_keys, refuse};
use super::arc_proof::ProofTally;
use super::arcs::Seg;
use super::corner_relief::{CornerRelief, ResolvedRelief, capsule_region, plan_corner_reliefs};
use super::level_rings::ring_plan;
use super::levels::{Cert, Motion, WalkState, walk_one_level};
use super::pass_start::StartSide;
use super::pocket::{PocketParams, PocketStrategy, non_sliver_center, pocket_over_with_relief};
use super::pocket_avoid::{AvoidSource, pocket_avoid_over};
use super::pocket_emit::{PocketWalks, emit_pocket_passes};
use super::pocket_hsm::EntryRoom;
use super::pocket_hsm_start::adaptive_passes_from;
use super::pocket_zigzag::rest_passes;
use super::rest2d::{RELIEF_START_TOL, RestPlan};

pub(super) type LevelPlan =
    (Vec<Vec<Point2>>, PocketStrategy, Option<EntryRoom>, super::pocket_hsm::Joins, Region);

pub(super) fn check_reliefs_avoid(
    reliefs: &[ResolvedRelief],
    ko: &Region,
    ko_entry: &Region,
    avoid: &AvoidSpec,
) -> Result<(), GenError> {
    for r in reliefs {
        if let Ok(cap) = capsule_region(r.start, r.end, r.radius) {
            let hits_cut = !ko.is_empty() && !cap.intersect(ko).map(|i| i.is_empty()).unwrap_or(false);
            let hits_entry = !ko_entry.is_empty() && !cap.intersect(ko_entry).map(|i| i.is_empty()).unwrap_or(false);
            if hits_cut || hits_entry {
                return Err(GenError::Invalid(format!(
                    "corner relief at ({:.3}, {:.3}) enters protected 'Do not touch' zone ({})",
                    r.corner.x,
                    r.corner.y,
                    avoid.names_list()
                )));
            }
        }
    }
    Ok(())
}

pub(super) fn plan_avoid_level(
    src: &AvoidSource<'_>,
    level_center: &Region,
    p: &PocketParams,
    strategy: PocketStrategy,
    z: f64,
    notes: &mut Vec<String>,
) -> Result<Option<LevelPlan>, GenError> {
    match src {
        AvoidSource::Plain { region, side, .. } => {
            if strategy == PocketStrategy::Adaptive {
                let what = format!("level at Z {z:.2}");
                let (ps, anotes, act, rm, js) = adaptive_passes_from(region, level_center, p, &what, side)?;
                notes.extend(anotes);
                Ok(Some((ps, act, rm, js, level_center.clone())))
            } else if matches!(strategy, PocketStrategy::Zigzag | PocketStrategy::Auto) {
                Ok(Some((
                    rest_passes(level_center, p.tiling_step(), p.climb)?,
                    strategy,
                    None,
                    Vec::new(),
                    level_center.clone(),
                )))
            } else {
                // #3744: Auto round a part cuts the one ring set from one side.
                let side = &super::ring_side::resolve(level_center, strategy, p.tool_diameter * 0.5, side)?;
                let (ps, pnotes) = ring_plan(region, level_center, p, strategy, side)?;
                notes.extend(pnotes.into_iter().filter(|n| !notes.contains(n)).collect::<Vec<_>>());
                Ok(Some((ps, strategy, None, Vec::new(), level_center.clone())))
            }
        }
        AvoidSource::Rest(plan) => {
            if strategy == PocketStrategy::Zigzag {
                Ok(Some((
                    rest_passes(level_center, p.tiling_step(), p.climb)?,
                    strategy,
                    None,
                    Vec::new(),
                    level_center.clone(),
                )))
            } else {
                let level_plan = RestPlan {
                    rest: plan.rest.clone(),
                    center: level_center.clone(),
                    previous_fits: true,
                };
                match super::rest_rings::rest_passes(&level_plan, p.tool_diameter, p.tiling_step(), p.climb) {
                    Ok((mut ps, cut_center)) => {
                        super::rest_anchor::anchor_first_pass(&mut ps, &level_plan, p);
                        Ok(Some((ps, strategy, None, Vec::new(), cut_center)))
                    }
                    Err(GenError::Empty(_)) => Ok(None),
                    Err(e) => Err(e),
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn enforce_level_crossing_free(
    b: &PathBuilder,
    ws: &WalkState,
    z: f64,
    passes: &[Vec<Point2>],
    runs: &mut [Vec<Seg>],
    motion: &Motion,
    cert_entry: &Cert<'_>,
    cert_links: &Cert<'_>,
    prev_center: Option<&Region>,
    proof: &mut ProofTally,
) {
    let empty_blends = HashSet::new();
    for round in 0..=CROSS_ROUNDS {
        let keys = own_keys(runs);
        if keys.is_empty() {
            return;
        }
        let mut try_b = b.clone();
        let mut try_ws = ws.clone();
        walk_one_level(
            &mut try_b,
            z,
            passes,
            Some(runs),
            motion,
            Some(cert_entry),
            Some(cert_links),
            prev_center,
            &mut try_ws,
        );
        let hit = crossing_arcs(&try_b.finish(), &keys, &empty_blends);
        if hit.is_empty() {
            return;
        }
        if round == CROSS_ROUNDS {
            refuse(passes, runs, &keys, proof);
            return;
        }
        refuse(passes, runs, &hit, proof);
    }
}

/// Emit rest-machining pocket passes with optional "Do not touch" protection (#2033, #3345).
pub(super) fn emit_rest_avoid(
    region: &Region,
    p: &PocketParams,
    plan: &RestPlan,
    mut notes: Vec<String>,
    relief: &CornerRelief,
    avoid: Option<&AvoidSpec>,
) -> Result<Toolpath, GenError> {
    relief.validate()?;
    let off = p.tool_diameter * 0.5 + p.stock_to_leave;
    let auto_side = StartSide::default();
    if !plan.previous_fits {
        let cr = plan_corner_reliefs(region, &plan.center, off, relief)?;
        if let Some(spec) = avoid.filter(|a| !a.is_empty()) {
            return pocket_avoid_over(
                &plan.center,
                p,
                notes,
                true,
                &cr.reliefs,
                spec,
                AvoidSource::Plain { region, side: &auto_side, holes: &[] },
            );
        }
        return pocket_over_with_relief(
            region,
            &plan.center,
            p,
            notes,
            true,
            &[],
            &cr.reliefs,
            &auto_side,
        );
    }
    let corner_reliefs: Vec<ResolvedRelief> = if relief.is_off() {
        Vec::new()
    } else {
        let full_center = non_sliver_center(region, off)?;
        let full_plan = plan_corner_reliefs(region, &full_center, off, relief)?;
        let reach = plan.center.offset(RELIEF_START_TOL)?;
        let (kept, dropped): (Vec<_>, Vec<_>) = full_plan.reliefs.into_iter().partition(|r| reach.contains(r.start));
        for r in &dropped {
            notes.push(format!(
                "corner relief at ({:.3}, {:.3}) is not cut: the rest area does not reach that corner",
                r.corner.x, r.corner.y
            ));
        }
        kept
    };
    if let Some(spec) = avoid.filter(|a| !a.is_empty()) {
        return pocket_avoid_over(
            &plan.center,
            p,
            notes,
            true,
            &corner_reliefs,
            spec,
            AvoidSource::Rest(plan),
        );
    }
    let (mut passes, cut_center) = super::rest_rings::rest_passes(plan, p.tool_diameter, p.tiling_step(), p.climb)?;
    if p.strategy == PocketStrategy::Zigzag {
        passes = rest_passes(&plan.center, p.tiling_step(), p.climb)?;
    }
    super::rest_anchor::anchor_first_pass(&mut passes, plan, p);
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    notes.into_iter().for_each(|n| b.note(n));
    emit_pocket_passes(
        b,
        p,
        PocketWalks {
            center: &plan.center,
            links_center: &cut_center,
            strategy: p.strategy,
            passes,
            room: None,
            joins: Vec::new(),
            hold_crossings: true,
            holes: &[],
            corner_reliefs: &corner_reliefs,
            cleared_links: false,
            ring_links: None,
            air: None,
        },
    )
}
