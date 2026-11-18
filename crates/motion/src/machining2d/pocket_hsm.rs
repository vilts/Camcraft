//! The planar pocket's HSM adaptive door (M11.4 C): where the entry
//! stands the tool, which bore it leaves, and one adaptive plan per
//! connected piece of the center area. The plan itself is `adaptive`
//! (provenance there). For a helix or plunge entry the plan chooses the
//! spot (round 2): the morph spiral wants the entry in the area's
//! kernel near its centroid (`adaptive::preferred_seed`), the fronts at
//! its widest point - and the ladder's entry law
//! (`levels::run_level_walks`, consumed, not changed) is handed that
//! spot as its certified room, so the plan starts exactly where the
//! dive ends. A helix too wide for the area is narrowed to the widest
//! it holds (round 3: at most [`HELIX_SHARE`] of the room, so a
//! channel's growth rings can part from the orbit), and said.

use crate::entry::{Entry3D, dive_room};
use crate::geo2d::{Loop, Point2, Region};

use super::adaptive::{AdaptiveSpec, Return, Travel, boundary_dist, central_point, corner_radius, first_crossing, plan, preferred_seed};
use super::face::GenError;
use super::pocket::{PocketParams, PocketStrategy, plan_passes};

/// Where the plan put the entry: the spot, the radius of the disc
/// about it that is inside the center area, the helix radius the
/// ladder must orbit, and the triangle the ladder's `dive_room` finds
/// the spot in (its centroid, by construction).
#[derive(Clone, Debug)]
pub(crate) struct EntryRoom {
    pub seed: Point2,
    pub clear: f64,
    pub helix_r: f64,
    pub tess: Region,
}

/// The Adaptive pocket's passes: the HSM plan, or - when it would leave
/// more than [`HSM_REST_MAX`] of the reachable stock uncut within twice
/// the load, or run past its budget ([`over_budget`]) - the Offset
/// rings, named in the notes (an incomplete or impractical plan is
/// never posted as if it were the clearing). Returns the strategy that
/// actually planned them and, for the HSM plan, its entry. `what` names
/// the area in the fallback note ("this pocket"; the 3-D ladder's "the
/// level at Z ...", M11.4's per-level port) - never a silent switch.
/// Issue #5: with `trochoid`, a channel the spiral refuses is tried as
/// a trochoid first (spiral -> trochoid -> rings, each fallback named).
pub(crate) fn adaptive_passes(center: &Region, p: &PocketParams, what: &str, trochoid: bool) -> Result<Planned, GenError> {
    let (passes, notes, rest, room, joins, travel, refused) = hsm_passes(center, p)?;
    // A plan that would cross itself is never posted (the verifier's
    // crossing law, asked of the plan first); one refused past its
    // planning budget (issue #13) has no passes to judge.
    let cross = if refused.is_some() { None } else { first_crossing(&passes) };
    let levels = super::levels::cut_levels_under(p.top_z, p.links.approach.stock_top, p.bottom_z, p.stepdown)?.len();
    let budget = refused.or_else(|| over_budget(&travel, levels));
    if rest <= HSM_REST_MAX && cross.is_none() && budget.is_none() {
        return Ok((passes, notes, PocketStrategy::Adaptive, room, joins));
    }
    let s = PocketStrategy::OffsetRings;
    let why = match (cross, budget) {
        (Some(x), _) => format!("for this area crosses itself at ({:.2}, {:.2})", x.x, x.y),
        (None, Some(why)) => why,
        (None, None) => format!("left {:.0}% of the area it could not cut within twice the load", rest * 100.0),
    };
    let troch = if trochoid { super::pocket_troch::trochoid_passes(center, p)? } else { Err("is not tried here".to_owned()) };
    let note = match troch {
        Ok((passes, tnotes, room, joins)) => {
            let mut notes = vec![format!("adaptive: the HSM plan {why} - trochoidal loops cut {what} instead")];
            notes.extend(tnotes);
            return Ok((passes, notes, PocketStrategy::Adaptive, Some(room), joins));
        }
        Err(t) if trochoid => format!("adaptive: the HSM plan {why}; a trochoid {t} - Offset rings cut {what} instead"),
        Err(_) => format!("adaptive: the HSM plan {why} - Offset rings cut {what} instead"),
    };
    let step = crate::load::load_step(p.tool_diameter, p.optimal_load);
    Ok((plan_passes(center, step, p.climb, s)?, vec![note], s, None, Vec::new()))
}

/// Cutter-center walks, one per pass.
pub(super) type Walks = Vec<Vec<Point2>>;

/// The pocket's passes, notes, the strategy that planned them, the HSM
/// plan's entry and its joins.
pub(crate) type Planned = (Walks, Vec<String>, PocketStrategy, Option<EntryRoom>, Joins);

/// The HSM plan's passes, notes, share of stock left, entry, joins,
/// travel and - refused past its planning budget - why.
type HsmPlanned = (Walks, Vec<String>, f64, Option<EntryRoom>, Joins, Travel, Option<String>);

/// The HSM plan's budget (round 4): it may travel at most this many times
/// its cutting length out of cut (floor air and lifted returns) ...
pub const NONCUT_PER_CUT: f64 = 1.5;

/// ... and post at most this many moves over all its levels ...
pub const MOVE_BUDGET: usize = 120_000;

/// ... and reach at most this many passes a level by the ladder's retract
/// hop (round 5): a piece no microlift return reaches costs a full
/// retract, a rapid and a plunge-feed descent each - a plan of many is
/// slower than the rings it replaces.
pub const HOP_BUDGET: usize = 8;

/// Why a plan with `travel` (per level) over `levels` levels is past its
/// budget, or `None`.
pub(super) fn over_budget(t: &Travel, levels: usize) -> Option<String> {
    let moves = t.moves.saturating_mul(levels.max(1));
    if t.air + t.lift > NONCUT_PER_CUT * t.cut {
        return Some(format!(
            "would travel {:.0} mm out of cut for {:.0} mm in cut a level (over {NONCUT_PER_CUT:.1}x)",
            t.air + t.lift,
            t.cut
        ));
    }
    if t.hops > HOP_BUDGET {
        return Some(format!("would reach {} passes a level by retract hops (over {HOP_BUDGET})", t.hops));
    }
    (moves > MOVE_BUDGET).then(|| format!("would post {moves} moves (over {MOVE_BUDGET})"))
}

/// Per pass: the microlift's down ramp it is reached by (`None`: the
/// ladder's own pass link).
pub(crate) type Joins = Vec<Option<Return>>;

/// The contact-angle margin the planner holds (fraction of the
/// optimal load; negative = under it): the plan is read on the
/// measuring stock's own law, and the margin covers the grid phase and
/// the arc fit between the plan and the posted moves.
pub const PLAN_TOL: f64 = 0.05;

/// The share of the reachable stock an HSM plan may leave standing (grid
/// crumbs) before the pocket falls back to Offset rings.
pub const HSM_REST_MAX: f64 = 0.02;

/// A helix takes at most this share of the room about its spot (round
/// 3): in a channel the spiral's first rings need the rest to grow
/// apart from the orbit at the walls (rings microns apart cross on their
/// chords).
pub(super) const HELIX_SHARE: f64 = 0.6;

/// The lead/link arc radius as a fraction of the tool radius.
pub const SMOOTH_PER_R: f64 = 0.5;

/// The connected pieces of `center`: each outer loop with the holes it
/// carries.
pub(super) fn components(center: &Region) -> Result<Vec<Region>, GenError> {
    let (outers, holes): (Vec<&Loop>, Vec<&Loop>) = center.loops().iter().partition(|l| l.area() > 0.0);
    let mut out = Vec::with_capacity(outers.len());
    for o in &outers {
        let mut loops = vec![(*o).clone()];
        for h in &holes {
            if let Some(&p) = h.points().first()
                && Region::new(vec![(*o).clone()])?.contains(p)
            {
                loops.push((*h).clone());
            }
        }
        out.push(Region::new(loops)?);
    }
    Ok(out)
}

/// The triangle whose centroid is `c` and whose inscribed circle has
/// radius `inr`: `dive_room` finds exactly `c` in it.
pub(super) fn triangle_about(c: Point2, inr: f64) -> Result<Region, GenError> {
    let pts = (0..3)
        .map(|k| {
            let a = std::f64::consts::FRAC_PI_2 + k as f64 * std::f64::consts::TAU / 3.0;
            Point2::new(c.x + 2.0 * inr * a.cos(), c.y + 2.0 * inr * a.sin())
        })
        .collect();
    Ok(Region::new(vec![Loop::new(pts)?])?)
}

/// The entry the plan chooses for a helix or plunge: the morph spiral's
/// kernel spot when the area is one star cell, else the roomy spot of
/// least geodesic eccentricity (round 3: where the growth rings finish
/// soonest);
/// the helix narrowed to fit. `None` leaves the ladder's own choice.
pub(crate) fn choose_entry(center: &Region, p: &PocketParams, notes: &mut Vec<String>) -> Result<Option<EntryRoom>, GenError> {
    let helix = p.entry == Entry3D::Helix;
    if !(helix || p.entry == Entry3D::Plunge) {
        return Ok(None);
    }
    let r = p.tool_diameter * 0.5;
    let hr0 = if helix { p.helix_radius } else { 0.0 };
    let pick = preferred_seed(center, corner_radius(center, r), hr0 + 0.02)?;
    let Some(cand) = pick.or_else(|| central_point(center).map(|w| w.0)) else { return Ok(None) };
    let clear = boundary_dist(center, cand);
    let hr = if helix { hr0.min(clear - 0.01).min(HELIX_SHARE * clear) } else { 0.0 };
    if helix && hr < 1e-3 {
        return Ok(None);
    }
    let tess = triangle_about(cand, hr + 0.01)?;
    let Some(seed) = dive_room(&tess, if helix { hr + 1e-3 } else { 0.0 }) else { return Ok(None) };
    if helix && hr < hr0 {
        notes.push(format!(
            "entry: a helix of radius {hr0:.3} does not fit the area - radius {hr:.3}, the widest it holds with room to spiral out, used"
        ));
    }
    Ok(Some(EntryRoom { seed, clear: boundary_dist(center, seed) - 1e-6, helix_r: hr, tess }))
}

/// Cutter-center walks for an Adaptive pocket, the plan's notes, the
/// share of the reachable stock it leaves standing and its entry.
fn hsm_passes(center: &Region, p: &PocketParams) -> Result<HsmPlanned, GenError> {
    let r = p.tool_diameter * 0.5;
    let mut notes = Vec::new();
    let mut passes = Vec::new();
    let mut joins = Vec::new();
    let mut travel = Travel::default();
    let (mut rest, mut area) = (0.0, 0.0);
    let room = choose_entry(center, p, &mut notes)?;
    // Where the entry law stands the tool (`run_level_walks`): the room
    // the plan chose, or the ladder's own - a helix orbits the widest
    // room's center and ends on its west side; every other arm dives
    // where the disc fits.
    let (seed, start, bore, helix_r) = match &room {
        Some(e) => (e.seed, Point2::new(e.seed.x - e.helix_r, e.seed.y), r + e.helix_r, e.helix_r),
        None => {
            let helix = (p.entry == Entry3D::Helix).then(|| dive_room(center, p.helix_radius + 1e-3)).flatten();
            match helix {
                Some(c) => (c, Point2::new(c.x - p.helix_radius, c.y), r + p.helix_radius, p.helix_radius),
                None => {
                    let Some(d) = dive_room(center, 0.0) else {
                        return Err(GenError::Invalid("adaptive: the center area has no room for an entry".into()));
                    };
                    (d, d, r, 0.0)
                }
            }
        }
    };
    let mut comps = components(center)?;
    // The entry's own piece first.
    comps.sort_by_key(|c| !c.contains(seed));
    let first = AdaptiveSpec {
        r,
        load: p.optimal_load,
        tol: PLAN_TOL,
        climb: p.climb,
        seed,
        start,
        bore,
        helix_r,
        smooth: r * SMOOTH_PER_R,
    };
    for (i, comp) in comps.iter().enumerate() {
        let reach = comp.offset(r)?;
        let spec = if i == 0 {
            first
        } else {
            let Some(d) = dive_room(comp, 0.0) else {
                notes.push("adaptive: a piece of the area has no entry room - left uncut".into());
                continue;
            };
            // #2265: the ladder enters this piece's solid start by the
            // asked entry (`ladder_links::pass_link`); the plan counts on
            // the tool's own disc there only, which every entry clears.
            notes.push(if p.entry == Entry3D::Plunge {
                format!("adaptive: a separate piece of the area is entered by an axial dive at ({:.2}, {:.2})", d.x, d.y)
            } else {
                format!("adaptive: a separate piece of the area is entered at ({:.2}, {:.2})", d.x, d.y)
            });
            AdaptiveSpec { seed: d, start: d, bore: r, helix_r: 0.0, ..first }
        };
        let pl = plan(&reach, comp, &spec)?;
        if let Some(why) = pl.refused {
            return Ok((Vec::new(), notes, 1.0, room, Vec::new(), travel, Some(why)));
        }
        rest += pl.rest;
        area += reach.area();
        // Each piece's passes after its first may be microlift-joined.
        joins.extend((0..pl.passes.len()).map(|k| if k > 0 { pl.joins.get(k).cloned().flatten() } else { None }));
        passes.extend(pl.passes);
        let t = pl.travel;
        (travel.cut, travel.air, travel.lift, travel.moves) = (travel.cut + t.cut, travel.air + t.air, travel.lift + t.lift, travel.moves + t.moves);
        travel.hops += t.hops;
        notes.extend(pl.notes);
    }
    Ok((passes, notes, if area > 0.0 { rest / area } else { 0.0 }, room, joins, travel, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round 4's budget: a plan riding air past 1.5x its cut, or posting
    /// past the move budget, is named and handed to the Offset rings.
    #[test]
    fn a_plan_past_its_air_or_move_budget_is_refused_by_name() {
        let ok = Travel { cut: 1000.0, air: 300.0, lift: 900.0, moves: 9000, hops: 2 };
        assert!(over_budget(&ok, 2).is_none());
        let airy = Travel { air: 2000.0, ..ok };
        assert!(over_budget(&airy, 2).is_some_and(|w| w.contains("out of cut")));
        let long = Travel { moves: 70_000, ..ok };
        assert!(over_budget(&long, 2).is_some_and(|w| w.contains("moves")));
        let hoppy = Travel { hops: HOP_BUDGET + 1, ..ok };
        assert!(over_budget(&hoppy, 2).is_some_and(|w| w.contains("retract hops")));
    }
}
