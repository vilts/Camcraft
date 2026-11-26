//! The planar pocket's trochoidal door (issue #5 round 2): where the HSM
//! spiral refuses a channel (`pocket_hsm`), the trochoid along its
//! medial line is planned (`adaptive::plan_trochoid`, provenance there),
//! entered at the line's start by the ladder's entry law (`EntryRoom`,
//! a helix narrowed clear of the returns, said) and held to the same
//! laws as the spiral: no crossing, the pocket's budget, the rest. Any
//! law it breaks is named and the Offset rings cut instead. Ours.

use crate::entry::{Entry3D, dive_room};
use crate::geo2d::{Point2, Region};

use super::adaptive::{AdaptiveSpec, boundary_dist, first_crossing, plan_trochoid, trochoid_seed};
use super::face::GenError;
use super::pocket::PocketParams;
use super::pocket_hsm::{EntryRoom, HSM_REST_MAX, Joins, PLAN_TOL, SMOOTH_PER_R, Walks, components, over_budget, triangle_about};

/// The trochoid's passes, notes, entry and joins - or why it is not.
pub(super) type Troch = Result<(Walks, Vec<String>, EntryRoom, Joins), String>;

/// The trochoid for `center`, or why not (said in the fallback note).
pub(super) fn trochoid_passes(center: &Region, p: &PocketParams) -> Result<Troch, GenError> {
    let comps = components(center)?;
    if comps.len() != 1 {
        return Ok(Err(format!("needs one connected area ({} pieces)", comps.len())));
    }
    let helix = p.entry == Entry3D::Helix;
    if !(helix || p.entry == Entry3D::Plunge) {
        return Ok(Err("needs a helix or plunge entry".into()));
    }
    let (seed, hr) = match trochoid_seed(center, if helix { p.helix_radius } else { 0.0 })? {
        Ok(s) => s,
        Err(why) => return Ok(Err(why)),
    };
    let r = p.tool_diameter * 0.5;
    if helix && hr < 1e-3 {
        return Ok(Err("has no helix room at its medial line's start".into()));
    }
    let tess = triangle_about(seed, hr + 0.01)?;
    let Some(seed) = dive_room(&tess, if helix { hr + 1e-3 } else { 0.0 }) else {
        return Ok(Err("has no entry room at its medial line's start".into()));
    };
    let mut notes = Vec::new();
    if helix && hr < p.helix_radius {
        notes.push(format!(
            "entry: a helix of radius {:.3} does not fit behind the channel's medial line - radius {hr:.3} used",
            p.helix_radius
        ));
    }
    let spec = AdaptiveSpec {
        r,
        load: p.optimal_load,
        tol: PLAN_TOL,
        climb: p.climb,
        seed,
        start: Point2::new(seed.x - hr, seed.y),
        bore: r + hr,
        helix_r: hr,
        smooth: r * SMOOTH_PER_R,
    };
    let reach = center.offset(r)?;
    let pl = plan_trochoid(&reach, center, &spec)?;
    if let Some(why) = pl.refused {
        return Ok(Err(why));
    }
    if let Some(x) = first_crossing(&pl.passes) {
        return Ok(Err(format!("would cross itself at ({:.2}, {:.2})", x.x, x.y)));
    }
    let levels = super::equal_steps::cut_levels_for(p.top_z, &p.links.approach,p.bottom_z, p.stepdown)?.len();
    if let Some(why) = over_budget(&pl.travel, levels) {
        return Ok(Err(why));
    }
    let rest = pl.rest / reach.area().max(1e-9);
    if rest > HSM_REST_MAX {
        return Ok(Err(format!("would leave {:.1}% of the area standing", rest * 100.0)));
    }
    notes.extend(pl.notes);
    let room = EntryRoom { seed, clear: boundary_dist(center, seed) - 1e-6, helix_r: hr, tess };
    Ok(Ok((pl.passes, notes, room, pl.joins)))
}
