//! Multi-level whole-model 2D contouring (#2436): when no specific faces or
//! edges are picked, a part whose cross-section grows outward below Top Z (a
//! chamfer, shoulder or open-ended slot floor) cannot stay on the top plane's
//! rings down to Bottom Z. Each depth band walks the active outline as far as
//! the solid allows (`contour_depth::ladder`), then re-slices the part's
//! overhead silhouette (`slice::material_overhead_of`) at the next level down
//! until Bottom Z is reached; unsliceable planes or collapsed lower bands fail
//! closed naming the stopped level. Provenance: 2.5D Z-level outer-profile
//! continuation over textbook polygonal cross-section + downward shadow union;
//! no vendor material consulted.

use std::ops::Range;

use crate::cl::{PathBuilder, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};

use super::contour::{PreparedBand, finish_contour, prepare_band};
use super::contour_comp;
use super::contour_depth::{Ladder, ladder};
use super::contour_entry::say;
use super::contour_params::{ContourParams, Side};
use super::contour_start;
use super::contour_step::Said;
use super::contour_whole_fence::{Planned, Sink, flush};
use super::contour_whole_reach::{band_sweep, reach_up};
use super::contour_whole_slice::{next_region, whole_region_at};
use super::face::GenError;
use super::fillet_tally::FilletTally;

const SAME_LEVEL: f64 = 1e-6;

fn check_unread(
    lad: &Ladder,
    remaining: &[f64],
    p: &ContourParams,
    b: &PathBuilder,
) -> Result<(), GenError> {
    for (li, why) in lad.unread.iter().enumerate() {
        let Some(why) = why else { continue };
        let k = remaining.iter().position(|z| !lad.walks(li, *z)).unwrap_or(0);
        let z_bad = remaining.get(k).copied().unwrap_or(p.bottom_z);
        let skipped: Vec<String> = remaining[k..].iter().map(|z| format!("{z:.3}")).collect();
        let last = lad.ring(li).last().copied();
        let stopped = last.map_or_else(
            || format!("before first level Z {z_bad:.3}"),
            |z| format!("at Z {z:.3} above Z {z_bad:.3}"),
        );
        let msg = format!(
            "whole-model contour stopped {stopped} (Bottom Z {:.3}) - cannot section deeper ({}): {why}",
            p.bottom_z,
            skipped.join(", ")
        );
        let partial = b.clone().finish();
        if why.contains("open") || why.contains("non-manifold") || why.contains("hole") {
            return Err(GenError::Model(msg));
        }
        return Err(if partial.moves.is_empty() {
            GenError::Invalid(msg)
        } else {
            GenError::Invalid(msg).with_ghost(&partial)
        });
    }
    Ok(())
}

/// First index in `remaining` that needs a fresh region slice below it: either
/// an outer boundary ring ended before that level, or an interior hole ring
/// ended and a narrower hole opened inside it at that level (counterbore).
fn first_split(
    band: &PreparedBand,
    lad: &Ladder,
    remaining: &[f64],
    part: &[Triangle],
    bodies: &[Range<usize>],
    p: &ContourParams,
) -> Result<Option<usize>, GenError> {
    let has_outer = band.loops.iter().any(|l| l.is_ccw());
    let outer_stop = band
        .loops
        .iter()
        .enumerate()
        .filter(|(_, l)| !has_outer || l.is_ccw())
        .filter_map(|(li, _)| remaining.iter().position(|z| !lad.walks(li, *z)))
        .min();
    let limit = outer_stop.unwrap_or(remaining.len());
    let mut hole_stops: Vec<usize> = band
        .loops
        .iter()
        .enumerate()
        .filter(|(_, l)| has_outer && !l.is_ccw())
        .filter_map(|(li, _)| remaining[..limit].iter().position(|z| !lad.walks(li, *z)))
        .collect();
    hole_stops.sort_unstable();
    hole_stops.dedup();
    let mut hole_stop = None;
    for k in hole_stops {
        let z_k = remaining[k];
        let active_holes = band
            .loops
            .iter()
            .enumerate()
            .filter(|(li, l)| !l.is_ccw() && lad.walks(*li, z_k))
            .count();
        let next = whole_region_at(part, bodies, z_k - p.stock_to_leave, z_k)?;
        let d = p.tool_diameter * 0.5 + p.stock_to_leave;
        let next_holes = next.offset(d)?.loops().iter().filter(|l| !l.is_ccw()).count();
        if next_holes > active_holes {
            hole_stop = Some(k);
            break;
        }
    }
    Ok(match (outer_stop, hole_stop) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    })
}

fn trim_ladder(lad: &Ladder, z_cut: f64) -> Ladder {
    let per_ring: Vec<Vec<f64>> = lad
        .per_ring
        .iter()
        .map(|zs| zs.iter().copied().filter(|&z| z > z_cut + SAME_LEVEL).collect())
        .collect();
    let mut zs: Vec<f64> = per_ring.iter().flatten().copied().collect();
    zs.sort_by(|a, b| b.total_cmp(a));
    zs.dedup_by(|a, b| a.to_bits() == b.to_bits());
    Ladder { zs, per_ring, unread: lad.unread.clone() }
}

fn forward_band_notes(
    band: &PreparedBand,
    lad: &Ladder,
    remaining_last: f64,
    continues: bool,
    ring_notes: Vec<String>,
    lead_notes: &mut Vec<String>,
) {
    let mut it = ring_notes.into_iter();
    for (li, l) in band.loops.iter().enumerate() {
        if !lad.walks(li, remaining_last)
            && let Some(n) = it.next()
            && (!continues || !l.is_ccw())
        {
            say(lead_notes, n);
        }
    }
}

/// A band that stops at the next flat level `z_next` and still emits floor
/// passes: an outer ring that stops there and has no floor level above it
/// gets no pass, and the re-slice does not return to it (#2538). Name the
/// uncut Z range of each such ring.
fn note_rings_without_floor(
    band: &PreparedBand,
    lad: &Ladder,
    trimmed: &Ladder,
    z_next: f64,
    ceiling: f64,
    lead_notes: &mut Vec<String>,
) {
    for (li, l) in band.loops.iter().enumerate() {
        if !l.is_ccw() || lad.walks(li, z_next) || !trimmed.ring(li).is_empty() {
            continue;
        }
        let at = band.plan.get(li).and_then(|r| r.first().copied()).unwrap_or(Point2::new(0.0, 0.0));
        say(
            lead_notes,
            format!(
                "whole-model contour: the ring at ({:.3}, {:.3}) has no floor level between Z {z_next:.3} \
                 and Z {ceiling:.3} - that wall is not cut; reduce the stepdown",
                at.x, at.y
            ),
        );
    }
}

fn stopped(msg: String, b: PathBuilder) -> GenError {
    let partial = b.finish();
    if partial.moves.is_empty() {
        GenError::Invalid(msg)
    } else {
        GenError::Invalid(msg).with_ghost(&partial)
    }
}

fn match_pick(
    res: Result<Option<(usize, Point2)>, GenError>,
    matched: &mut bool,
    deferred: &mut Option<GenError>,
) -> Result<Option<(usize, Point2)>, GenError> {
    match res {
        Ok(Some(v)) => {
            *matched = true;
            Ok(Some(v))
        }
        Ok(None) => Ok(None),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("farther than") || msg.contains("no contour ring") {
                *deferred = Some(e);
                Ok(None)
            } else {
                Err(e)
            }
        }
    }
}

/// Whole-model `2D Contour` over `part` / `bodies`: starts with `top_region`
/// and continues deeper bands from the part's overhead outline whenever the
/// outer boundary changes below `Top Z`.
pub fn contour_whole_bodies(
    top_region: &Region,
    part: &[Triangle],
    bodies: &[Range<usize>],
    p: &ContourParams,
) -> Result<Toolpath, GenError> {
    contour_whole_bodies_with(top_region, part, bodies, p, &mut Ok).map(|(path, _)| path)
}

/// [`contour_whole_bodies`] with a caller-supplied per-band region transform
/// (such as boundary controls) and the sequence of regions cut.
pub fn contour_whole_bodies_with(
    top_region: &Region,
    part: &[Triangle],
    bodies: &[Range<usize>],
    p: &ContourParams,
    area_at: &mut dyn FnMut(Region) -> Result<Region, GenError>,
) -> Result<(Toolpath, Vec<Region>), GenError> {
    if p.side == Side::Inside {
        let path = super::contour::contour_on_bodies(top_region, part, bodies, p)?;
        return Ok((path, vec![top_region.clone()]));
    }
    p.validate()?;
    contour_comp::door(p)?;
    if top_region.is_empty() {
        return Err(GenError::Empty("empty footprint".into()));
    }
    let flat = p.ring_levels()?;
    let r = p.tool_diameter * 0.5;
    let mut b = PathBuilder::approached(Point3::new(0.0, 0.0, p.clearance_z));
    let mut said = Said::default();
    let mut tally = FilletTally::default();
    let mut lead_notes = Vec::new();
    let mut walked = Vec::new();
    let mut cover = Region::empty();
    let mut remaining: &[f64] = &flat;
    let mut current_region = top_region.clone();
    let mut ceiling = p.top_z;
    let mut used_fallback = false;
    let mut pick_matched = p.picks.first(crate::path_picks::PickRole::Start).is_none();
    let mut exit_matched = p.picks.first(crate::path_picks::PickRole::Exit).is_none();
    let mut deferred_pick = None;
    let mut deferred_exit = None;
    // #2677: bands are planned first and emitted together, so each lead room
    // can fence the other bands' rings on a shared level (contour_whole_fence).
    // An error that carries the partial path flushes them first.
    let mut planned: Vec<Planned> = Vec::new();
    macro_rules! flush_planned {
        () => {
            flush(&mut b, &mut planned, p, &mut Sink { said: &mut said, lead_notes: &mut lead_notes, tally: &mut tally })?
        };
    }

    let mut mid = false;
    let max_bands = flat.len().saturating_mul(2).saturating_add(part.len().saturating_mul(3)).saturating_add(2);
    let mut band_idx = 0usize;
    while !remaining.is_empty() && band_idx <= max_bands {
        let last_z = remaining[remaining.len() - 1];
        let band = prepare_band(&current_region, p)?;
        let refs: Vec<&crate::geo2d::Loop> = band.loops.iter().collect();
        let pick = match_pick(contour_start::pick_ring(&refs, p), &mut pick_matched, &mut deferred_pick)?;
        let exit = match_pick(contour_start::pick_exit(&refs, p), &mut exit_matched, &mut deferred_exit)?;

        let mut ring_notes = Vec::new();
        let lad = ladder(&band.plan, part, bodies, remaining, ceiling, r, p.stock_to_leave, &mut ring_notes);
        if check_unread(&lad, remaining, p, &b).is_err() {
            flush_planned!();
            check_unread(&lad, remaining, p, &b)?;
        }
        // #2597: a ring that reaches outside every sweep cut so far starts at Top Z.
        let lad = reach_up(&band, lad, &cover, &flat, part, bodies, p)?;

        // An intermediate band (the outline changed between the last cut level
        // and the next flat level) cuts only its floor passes above that level.
        let split = if mid { Some(0) } else { first_split(&band, &lad, remaining, part, bodies, p)? };
        match split {
            Some(0) if mid || (band_idx == 0 && !used_fallback) => {
                // A ring can still have floor passes above the next flat level
                // (#2004 floor search): cut them before the re-slice, or that
                // wall is silently left uncut.
                let trimmed = trim_ladder(&lad, remaining[0]);
                let emitted = trimmed.zs.last().is_some_and(|&z| z < ceiling - SAME_LEVEL);
                if emitted {
                    note_rings_without_floor(&band, &lad, &trimmed, remaining[0], ceiling, &mut lead_notes);
                    forward_band_notes(&band, &lad, last_z, true, ring_notes, &mut lead_notes);
                    walked.push(current_region.clone());
                    cover = cover.union(&band_sweep(&band.center, r)?)?;
                    ceiling = trimmed.zs.last().copied().unwrap_or(ceiling);
                    planned.push(Planned { band, lad: trimmed, pick, exit });
                } else if mid {
                    say(
                        &mut lead_notes,
                        format!(
                            "whole-model contour: the outline changes between Z {:.3} and Z {:.3} but has                              no floor level there - that wall is not cut; reduce the stepdown",
                            remaining[0], ceiling
                        ),
                    );
                }
                used_fallback = true;
                let (raw, next_mid) = next_region(part, bodies, p, ceiling, remaining[0], emitted)?;
                mid = next_mid;
                current_region = area_at(raw)?;
            }
            Some(0) => {
                let msg = format!(
                    "whole-model contour cannot cut Z {:.3} (Bottom Z {:.3}) without gouging ({})",
                    remaining[0],
                    p.bottom_z,
                    ring_notes.join("; ")
                );
                flush_planned!();
                return Err(stopped(msg, b));
            }
            Some(j) => {
                let z_next = remaining[j];
                let trimmed = trim_ladder(&lad, z_next);
                if trimmed.zs.is_empty() {
                    return Err(GenError::Invalid(format!(
                        "whole-model contour produced no cut levels above Z {z_next:.3}"
                    )));
                }
                forward_band_notes(&band, &lad, last_z, true, ring_notes, &mut lead_notes);
                walked.push(current_region.clone());
                cover = cover.union(&band_sweep(&band.center, r)?)?;
                ceiling = trimmed.zs.last().copied().unwrap_or(remaining[j - 1]);
                planned.push(Planned { band, lad: trimmed, pick, exit });
                remaining = &remaining[j..];
                used_fallback = true;
                let (raw, next_mid) = next_region(part, bodies, p, ceiling, remaining[0], true)?;
                mid = next_mid;
                current_region = match area_at(raw) {
                    Ok(r) => r,
                    Err(e) => {
                        let msg = format!(
                            "whole-model contour stopped at Z {ceiling:.3} (Bottom Z {:.3}):                              no machinable region at Z {:.3} ({e})",
                            p.bottom_z, remaining[0]
                        );
                        flush_planned!();
                        return Err(stopped(msg, b));
                    }
                };
            }
            None => {
                forward_band_notes(&band, &lad, last_z, false, ring_notes, &mut lead_notes);
                planned.push(Planned { band, lad, pick, exit });
                walked.push(current_region);
                remaining = &[];
                break;
            }
        }
        band_idx += 1;
    }
    if let Some(z) = remaining.first() {
        let msg = format!(
            "whole-model contour stopped at Z {ceiling:.3} (Bottom Z {:.3}): too many outline changes above Z {z:.3}",
            p.bottom_z
        );
        flush_planned!();
        return Err(stopped(msg, b));
    }
    flush_planned!();

    if !pick_matched
        && let Some(e) = deferred_pick
    {
        return Err(e);
    }
    if !exit_matched
        && let Some(e) = deferred_exit
    {
        return Err(e);
    }
    let path = finish_contour(b, lead_notes, &tally, p)?;
    Ok((path, walked))
}
