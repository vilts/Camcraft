//! 2D Offset / Pocket path planning with per-level "Do not touch" keep-outs (#2055).
//! Provenance: our own per-Z-level region difference over `levels` and `verify::avoid`;
//! no vendor material consulted.

use camcraft_sim::verify::{AVOID_PAD, AvoidSpec};

use crate::cl::{MoveKind, PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::GenError;
use super::arc_proof::ProofTally;
use super::arcs::{ARC_FIT_TOL, Seg, fit_arcs};
use super::corner_relief::{CornerRelief, ResolvedRelief, plan_corner_reliefs};
use super::corner_relief_spur::inject_relief_spurs;
use super::equal_steps::cut_levels_for;
use super::fillet::{Legal, fillet_corners};
use super::fillet_tally::{FilletTally, corner_arc_radius, fillet_floor};
use super::levels::{Cert, Motion, WalkState, walk_one_level};
use super::pass_start::{PassStart, StartSide};
use super::pocket::{PocketParams, PocketStrategy, non_sliver_center};
use super::pocket_avoid_level::{check_reliefs_avoid, enforce_level_crossing_free, plan_avoid_level};
use super::predrilled::PreHole;
use super::rest2d::RestPlan;

pub(super) enum AvoidSource<'a> {
    Plain { region: &'a Region, side: &'a StartSide, holes: &'a [PreHole] },
    Rest(&'a RestPlan),
}

/// Plan a 2D pocket/offset path whose tool assembly stays clear of `avoid`.
pub fn pocket_avoid(
    region: &Region,
    p: &PocketParams,
    avoid: &AvoidSpec,
) -> Result<Toolpath, GenError> {
    pocket_avoid_with_relief(region, p, avoid, &CornerRelief::default())
}

/// [`pocket_avoid`] with optional inside-corner reliefs (#2400).
pub fn pocket_avoid_with_relief(
    region: &Region,
    p: &PocketParams,
    avoid: &AvoidSpec,
    relief: &CornerRelief,
) -> Result<Toolpath, GenError> {
    pocket_avoid_started(region, p, avoid, relief, PassStart::Auto)
}

/// [`pocket_avoid_with_relief`] with proved pre-drilled entry holes (#1449, #3345).
pub fn pocket_avoid_entered_with_relief(
    region: &Region,
    p: &PocketParams,
    holes: &[PreHole],
    avoid: &AvoidSpec,
    relief: &CornerRelief,
) -> Result<Toolpath, GenError> {
    pocket_avoid_entered_started(region, p, holes, avoid, relief, PassStart::Auto)
}

/// [`pocket_avoid_with_relief`] with the side each level starts on
/// (#3316); [`PassStart::Auto`] is that function, byte for byte.
pub fn pocket_avoid_started(
    region: &Region,
    p: &PocketParams,
    avoid: &AvoidSpec,
    relief: &CornerRelief,
    start: PassStart,
) -> Result<Toolpath, GenError> {
    pocket_avoid_started_on(region, p, avoid, relief, &StartSide::from(start))
}

/// [`pocket_avoid_started`] with the stock known (#3316): each level's
/// keep-out edges are walls, cut after its rings.
pub fn pocket_avoid_started_on(
    region: &Region,
    p: &PocketParams,
    avoid: &AvoidSpec,
    relief: &CornerRelief,
    side: &StartSide,
) -> Result<Toolpath, GenError> {
    pocket_avoid_entered_on(region, p, &[], avoid, relief, side)
}

/// [`pocket_avoid_entered_with_relief`] with the side each level starts on
/// (#3316, #3345).
pub fn pocket_avoid_entered_started(
    region: &Region,
    p: &PocketParams,
    holes: &[PreHole],
    avoid: &AvoidSpec,
    relief: &CornerRelief,
    start: PassStart,
) -> Result<Toolpath, GenError> {
    pocket_avoid_entered_on(region, p, holes, avoid, relief, &StartSide::from(start))
}

fn pocket_avoid_entered_on(
    region: &Region,
    p: &PocketParams,
    holes: &[PreHole],
    avoid: &AvoidSpec,
    relief: &CornerRelief,
    side: &StartSide,
) -> Result<Toolpath, GenError> {
    if avoid.is_empty() {
        return super::pocket_started_on(region, p, holes, relief, side);
    }
    p.validate()?;
    relief.validate()?;
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let off = p.tool_diameter * 0.5 + p.stock_to_leave;
    let base_center = non_sliver_center(region, off)?;
    if base_center.is_empty() {
        return Err(GenError::Invalid("pocket is narrower than the tool".into()));
    }
    let relief_plan = plan_corner_reliefs(region, &base_center, off, relief)?;
    pocket_avoid_over(
        &base_center,
        p,
        Vec::new(),
        false,
        &relief_plan.reliefs,
        avoid,
        AvoidSource::Plain { region, side, holes },
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn pocket_avoid_over(
    base_center: &Region,
    p: &PocketParams,
    initial_notes: Vec<String>,
    hold_crossings: bool,
    reliefs: &[ResolvedRelief],
    avoid: &AvoidSpec,
    src: AvoidSource<'_>,
) -> Result<Toolpath, GenError> {
    let strategy = match (&src, p.strategy) {
        (AvoidSource::Plain { region, .. }, PocketStrategy::Auto) => {
            let outer = region
                .loops()
                .iter()
                .max_by(|a, b| a.area().abs().total_cmp(&b.area().abs()))
                .map(|l| l.points())
                .unwrap_or(&[]);
            p.strategy.resolve(outer, p.stepover, p.tool_diameter, ARC_FIT_TOL)
        }
        (_, s) => s,
    };
    let holes: &[PreHole] = match &src {
        AvoidSource::Plain { holes, .. } => holes,
        AvoidSource::Rest(_) => &[],
    };
    // #3744: Auto round a part resolves to Inside or Outside of the one
    // ring set (`ring_side::resolve`), so it links as a start side does.
    let cleared_links = matches!(&src, AvoidSource::Plain { side, .. } if side.start != PassStart::Auto || super::ring_side::resolves(strategy, side));
    let zs = cut_levels_for(p.top_z, &p.links.approach, p.bottom_z, p.stepdown)?;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    for n in initial_notes {
        b.note(n);
    }
    let mut ws = WalkState::new(p.links.approach.entry_top(p.top_z));
    let mut prev_center: Option<Region> = None;
    let mut blocked_levels: Vec<f64> = Vec::new();
    let mut notes: Vec<String> = Vec::new();
    // #3249: a relief that gets no spur is noted once, not once per level.
    let mut noted_relief_misses: Vec<Point2> = Vec::new();
    let mut tally = FilletTally::default();
    let mut proof = ProofTally::default();

    for &z in &zs {
        let ko = avoid.keepout_region(z, z, AVOID_PAD).map_err(GenError::Invalid)?;
        // Check overhead protected obstacles across the full descent/final-lift envelope up to clearance_z.
        let top_above = p.clearance_z.max(z);
        let ko_entry = avoid
            .keepout_region(z, top_above, AVOID_PAD)
            .map_err(GenError::Invalid)?;
        check_reliefs_avoid(reliefs, &ko, &ko_entry, avoid)?;
        let level_center = if ko.is_empty() { base_center.clone() } else { base_center.difference(&ko)? };
        let level_center = if ko_entry.is_empty() { level_center } else { level_center.difference(&ko_entry)? };
        if level_center.is_empty() {
            blocked_levels.push(z);
            continue;
        }
        let Some((passes, actual, room, joins, links_center)) =
            plan_avoid_level(&src, &level_center, p, strategy, z, &mut notes)?
        else {
            blocked_levels.push(z);
            continue;
        };
        let fit = match actual {
            PocketStrategy::Waterline | PocketStrategy::OffsetRings => Some(ARC_FIT_TOL),
            PocketStrategy::Zigzag | PocketStrategy::Auto | PocketStrategy::Adaptive => None,
        };
        let passes = if p.links.approach.arc_xy && room.is_none() && actual != PocketStrategy::Adaptive {
            super::descent_xy::reseat_first(passes)
        } else {
            passes
        };
        let mut passes = super::predrilled::reseat(
            passes,
            holes,
            &level_center,
            p.links.approach.predrilled && room.is_none(),
        );
        if passes.iter().all(|ps| ps.len() < 2) {
            blocked_levels.push(z);
            continue;
        }
        let mut runs: Option<Vec<Vec<Seg>>> = if p.blend_turns && actual != PocketStrategy::Adaptive {
            let legal = Legal { region: &level_center, inside: true, air_left: None };
            let rho = corner_arc_radius(p.blend_radius, p.tool_diameter).map_err(GenError::Invalid)?;
            let r = passes
                .iter()
                .map(|pass| {
                    let Some((first, rest)) = pass.split_first() else {
                        return Vec::new();
                    };
                    let walk = match fit {
                        Some(tol) => fit_arcs(pass, tol),
                        None => vec![Seg::Line(rest.to_vec())],
                    };
                    let (walk, t) = fillet_corners(&walk, *first, rho, ARC_FIT_TOL, &legal);
                    tally.add(t);
                    walk
                })
                .collect();
            Some(r)
        } else {
            fit.map(|tol| {
                passes
                    .iter()
                    .map(|ps| {
                        let segs = fit_arcs(ps, tol);
                        proof.arcs += segs.iter().filter(|s| matches!(s, Seg::Arc { .. })).count();
                        segs
                    })
                    .collect()
            })
        };
        let hr = room.as_ref().map_or(p.helix_radius, |rm| p.helix_radius.min(rm.helix_r));
        // #3810: the start side's air room, less this level's fixtures and
        // clamps over the whole descent.
        let air = match &src {
            AvoidSource::Plain { side, .. } => {
                let resolved = super::ring_side::resolve(&level_center, actual, p.tool_diameter * 0.5, side)?;
                let room = super::air_entry::AirRoom::for_side(&resolved, actual, &level_center, p.tool_diameter * 0.5, p.links.approach.approach_distance);
                let room = room.and_then(|r| r.without(&ko.union(&ko_entry).ok()?));
                // #3884: a selected Arc on an Outside ring pass with no air
                // room is refused by name.
                let rings = matches!(actual, PocketStrategy::Waterline | PocketStrategy::OffsetRings);
                super::entry_warn::refuse_outside_arc(side.start, p.entry, rings, room.is_some())?;
                // #3748: the user's Inside or Outside with a lead-in other
                // than Arc: used as selected, named (not gated on the air).
                if rings
                    && p.strategy != PocketStrategy::Adaptive
                    && let Some(n) = super::pass_start::entry_advice_note(side.start, p.entry)
                {
                    b.note_once(&n);
                }
                room
            }
            AvoidSource::Rest(_) => None,
        };
        let level_motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed)
            .with_entry(p.entry, hr)
            .with_arc_radius(p.tool_diameter)
            .with_links(p.links, hr)
            .with_top(p.links.approach.entry_top(p.top_z))
            .with_xy_arc(p.links.approach.arc_xy)
            .with_microlift(joins)
            .with_holes(holes)
            .with_solid_restart()
            .with_cleared_links(cleared_links)
            .with_ring_links(match &src {
                AvoidSource::Plain { side, .. } if cleared_links => {
                    super::ring_links::for_side(side, actual, p.tool_diameter * 0.5, Some(ko.union(&ko_entry)?))?
                }
                _ => None,
            })
            .with_air(air);
        let cert_entry = room.as_ref().map(|rm| Cert::Region(&rm.tess));
        let cert_center = Cert::Region(&level_center);
        let cert_links = Cert::Region(&links_center);
        let hold_fitted = !p.blend_turns
            && (hold_crossings || matches!(actual, PocketStrategy::Waterline | PocketStrategy::OffsetRings));
        if hold_fitted && let Some(ref mut r) = runs {
            enforce_level_crossing_free(
                &b,
                &ws,
                z,
                &passes,
                r,
                &level_motion,
                cert_entry.as_ref().unwrap_or(&cert_center),
                &cert_links,
                prev_center.as_ref(),
                &mut proof,
            );
        }
        if !reliefs.is_empty() {
            let mut misses = Vec::new();
            inject_relief_spurs(&mut passes, &mut runs, &level_center, reliefs, &mut misses);
            for m in misses {
                if noted_relief_misses.contains(&m) {
                    continue;
                }
                noted_relief_misses.push(m);
                b.note(format!(
                    "corner relief at ({:.3}, {:.3}) is not cut: no pass vertex reaches the relief spur",
                    m.x, m.y
                ));
            }
        }
        walk_one_level(
            &mut b,
            z,
            &passes,
            runs.as_deref(),
            &level_motion,
            Some(cert_entry.as_ref().unwrap_or(&cert_center)),
            Some(&cert_links),
            prev_center.as_ref(),
            &mut ws,
        );
        prev_center = Some(links_center);
    }

    if ws.last_cut_z.is_none() {
        return Err(GenError::Invalid(format!(
            "cannot cut clear of protected 'Do not touch' {}: no safe cutting room remains",
            avoid.names_list()
        )));
    }
    for n in notes {
        b.note(n);
    }
    for n in tally.notes("pass", fillet_floor(ARC_FIT_TOL)) {
        b.note(n);
    }
    if let Some(n) = proof.note() {
        b.note(n);
    }
    let end_motion = Motion::new(p.clearance_z, p.retract_z, p.feed, p.plunge_feed)
        .with_entry(p.entry, p.helix_radius)
        .with_arc_radius(p.tool_diameter)
        .with_links(p.links, p.helix_radius)
        .with_top(p.links.approach.entry_top(p.top_z))
        .with_holes(holes);
    ws.finish(&mut b, &end_motion);
    let cur = b.at();
    b.push(MoveKind::Rapid, Point3::new(cur.x, cur.y, p.clearance_z), None);
    if !blocked_levels.is_empty() {
        b.note(format!(
            "Do not touch ({}) blocked {} cut level(s); remaining levels stayed clear",
            avoid.names_list(),
            blocked_levels.len()
        ));
    }
    let mut path = b.finish();
    let _ = camcraft_sim::verify::elevate_rapid_links(&mut path, avoid, None, AVOID_PAD);
    if let Some(why) = crate::approach::refusal(&path) {
        return Err(GenError::Invalid(why).with_ghost(&path));
    }
    if !path.all_finite() {
        return Err(GenError::Invalid("path left the finite horizon".into()));
    }
    Ok(path)
}
