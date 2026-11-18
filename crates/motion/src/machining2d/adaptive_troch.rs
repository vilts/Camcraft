//! 2D Adaptive's trochoidal method (issue #1509): the region is cleared
//! by trochoid loops along its medial line, confined to the region.
//! Trochoidal milling - circular loops whose centre advances along the
//! cut, so the contact angle stays bounded where a straight pass would
//! slot - is Otkur & Lazoglu 2007, "Trochoidal milling", Int. J. Mach.
//! Tools Manuf. 47, and Rauch, Duc & Hascoet 2009, "Improving trochoidal
//! tool paths generation and implementation using process constraints
//! modelling", IJMTM 49; the medial line as the loops' guide is the
//! grassfire skeleton (Blum 1967; Held 1991, "On the Computational
//! Geometry of Pocket Machining"), taken by `adaptive::medial`. The loop
//! layout and the measured-engagement step ladder are `trochoid_slot`'s
//! (#220), the opened bore `trochoid_bore`'s. The lane law here is ours:
//!
//! - the cutter-centre area is the region eroded by the tool radius and
//!   the stock to leave (the pocket's own law);
//! - the lane is that area's medial line, straightened within
//!   [`SPINE_TOL`] of its half-width; its loop band is the least clearance along it, so
//!   every cutter-centre point lies inside the area by construction;
//! - the finished path is then CHECKED against the area, move by move
//!   ([`confined`]) - construction is the plan, the check is the law;
//! - what the lane cannot reach (corners, wider parts) is said in the
//!   notes with its size; past [`REST_MAX`] of the region the method is
//!   refused by name (the region is no single channel).
//!
//! Prime Directive: a region narrower than the tool, one too narrow to
//! loop in, a ring round an island, a region with no single medial
//! line, a lane that would leave most of the region standing and any
//! loop outside the region are each refused by name. No vendor material.

use crate::cl::{MoveKind, PathBuilder, Toolpath, lattice};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::adaptive::{Spine, medial_spine, widest_point};
use super::adaptive_troch_clean::{emit_cleanup, plan_cleanup};
use super::adaptive_troch_lanes::{clearance, narrow_err, plan_all_lanes, rest_share, straightened, trochoid_params};
use super::face::GenError;
use super::pocket::{PocketParams, PocketStrategy};
use super::trochoid_slot::{DEFAULT_MAX_ENGAGEMENT, MIN_BAND_PER_D, TrochoidReturn, emit_level, plan_lane};

/// The name every refusal and note of the method starts with.
const WHO: &str = "2D Adaptive (Trochoidal)";

/// The medial line is straightened within this share of the area's
/// half-width: the sampled line of a straight channel becomes its two
/// ends (its trimmed ends curl toward the corners by less). What the
/// straightening costs in clearance is read on the straightened line.
const SPINE_TOL: f64 = 0.05;

/// The loops stay this far (mm) inside the cutter-centre area's walls.
const WALL_MARGIN: f64 = 0.01;

/// A cutter-centre point this far (mm) outside the area is outside it
/// (the path's own lattice and the area's offset quantisation).
const CONFINE_TOL: f64 = 2e-3;

/// The share of the region the lane may leave out of reach before the
/// method is refused; under it the rest is said in the notes ...
pub const REST_MAX: f64 = 0.25;

/// ... when it is more than this share.
const REST_NOTE: f64 = 0.005;

/// Clear `region` with trochoidal loops (see the module law). `p` is the
/// Trochoidal parameters exposed under 2D Adaptive when `Method = Trochoidal`
/// (#1588): engagement limit, return mode between loops, return feed, and
/// loop width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdaptiveTrochParams {
    /// Engagement limit (degrees of the cutter periphery in contact).
    pub max_engagement: f64,
    /// Return between loops: `None` is Auto (0.3 mm microlift when clearance
    /// exceeds top by at least 0.3 mm, else retract).
    pub ret: Option<TrochoidReturn>,
    /// Return feed (mm/min); `None` uses the cutting `feed`.
    pub return_feed: Option<f64>,
    /// Loop band width (mm); `None` uses all the room along the medial line.
    pub loop_width: Option<f64>,
}

impl Default for AdaptiveTrochParams {
    fn default() -> Self {
        Self { max_engagement: DEFAULT_MAX_ENGAGEMENT, ret: None, return_feed: None, loop_width: None }
    }
}

impl AdaptiveTrochParams {
    /// Named refusal if any trochoidal control is non-finite or out of range.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        let pos = |v: f64| v.is_finite() && v > 0.0;
        if !pos(self.max_engagement) || self.max_engagement > 180.0 {
            return Some("max engagement must be finite and in (0, 180] degrees".into());
        }
        if let Some(TrochoidReturn::Microlift { height }) = self.ret
            && !pos(height)
        {
            return Some("microlift height must be finite and positive".into());
        }
        if self.return_feed.is_some_and(|f| !pos(f)) {
            return Some("return feed must be finite and positive".into());
        }
        if self.loop_width.is_some_and(|w| !pos(w)) {
            return Some("loop width must be finite and positive".into());
        }
        None
    }

    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.refusal().is_none()
    }

    fn validate(&self) -> Result<(), GenError> {
        if let Some(msg) = self.refusal() {
            return Err(GenError::Invalid(format!("{WHO}: {msg}")));
        }
        Ok(())
    }
}

/// 2D Adaptive's own parameter set with default trochoidal controls: the
/// optimal load is the most the loops may advance a turn, the stepover
/// and the entry are not read (the lane bores its own helix at its start).
pub fn adaptive_trochoidal(region: &Region, p: &PocketParams) -> Result<Toolpath, GenError> {
    adaptive_trochoidal_with(region, p, &AdaptiveTrochParams::default())
}

/// 2D Adaptive's trochoidal planner with explicit engagement, return,
/// return-feed, and loop-width controls (#1588).
pub fn adaptive_trochoidal_with(region: &Region, p: &PocketParams, t: &AdaptiveTrochParams) -> Result<Toolpath, GenError> {
    // The Adaptive doors (finite numbers, the load in (0, 50] % of D).
    let p = &PocketParams { strategy: PocketStrategy::Adaptive, ..*p };
    p.validate()?;
    t.validate()?;
    if p.optimal_load < 0.04 {
        return Err(GenError::Invalid(format!(
            "{WHO}: optimal load below 4% of tool diameter is unsupported by cleanup - increase optimal load to at least 4%"
        )));
    }
    if region.is_empty() {
        return Err(GenError::Empty("empty pocket region".into()));
    }
    let r = 0.5 * p.tool_diameter;
    let center = region.offset(-(r + p.stock_to_leave))?;
    if center.is_empty() {
        return Err(GenError::Invalid(format!(
            "{WHO}: the region is narrower than the {:.3} mm tool (with {:.3} mm left on each wall)",
            p.tool_diameter, p.stock_to_leave
        )));
    }
    let planned = plan_all_lanes(region, &center, p, t)?;
    let reach = if p.stock_to_leave > 0.0 { region.offset(-p.stock_to_leave)? } else { region.clone() };
    let (clean, final_cleared) =
        plan_cleanup(&center, &reach, &planned.used_center, &planned.cleared, p.tool_diameter, p.optimal_load, p.climb, t)?;

    let zs = super::levels::cut_levels_under(p.top_z, p.links.approach.stock_top, p.bottom_z, p.stepdown)?;
    let hop = p.retract_z.unwrap_or(p.clearance_z);
    let p3 = |q: Point2, z: f64| Point3::new(lattice(q.x), lattice(q.y), lattice(z));
    let first_entry = planned.layouts[0].entry;
    let mut bld = PathBuilder::approached(p3(first_entry, p.clearance_z));
    bld.push(MoveKind::Rapid, p3(first_entry, p.clearance_z), None);
    // #1559: over stock carried above the top of the cut, the first
    // level's entry starts at its plunge height and feeds down.
    let mut above = p.links.approach.entry_top(p.top_z);
    for (zi, &z) in zs.iter().enumerate() {
        for (li, lay) in planned.layouts.iter().enumerate() {
            if zi > 0 || li > 0 {
                let at = bld.at();
                if (at.z - hop).abs() > 1e-6 {
                    bld.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop), None);
                }
                bld.push(MoveKind::Rapid, p3(lay.entry, hop), None);
            }
            let from = (above + crate::approach::DEFAULT_PLUNGE_HEIGHT).min(hop).max(above);
            bld.push(MoveKind::Rapid, p3(lay.entry, from), None);
            emit_level(&mut bld, &lay.lay, &lay.tp, z, from, true, lay.open).map_err(|e| renamed(e).with_ghost_moves(bld.moves()))?;
        }
        emit_cleanup(&mut bld, &clean, p, z, above, hop);
        above = z;
    }
    let at = bld.at();
    bld.push(MoveKind::Rapid, Point3::new(at.x, at.y, p.clearance_z), None);

    let mut total_loops = 0usize;
    let mut max_deg = 0.0_f64;
    for lay in &planned.layouts {
        total_loops += lay.lay.loops;
        max_deg = max_deg.max(lay.deg);
    }
    if planned.layouts.len() == 1 {
        let lay = &planned.layouts[0];
        bld.note(format!(
            "{WHO}: {} loops of radius {:.3} mm a level at a {:.4} mm step, measured {:.1} deg engagement (limit {:.1} deg)",
            lay.lay.loops, lay.lay.rho, lay.lay.spacing, lay.deg, t.max_engagement
        ));
    } else {
        bld.note(format!(
            "{WHO}: {} lanes ({total_loops} loops a level, radius {:.3} mm), measured {:.1} deg engagement (limit {:.1} deg)",
            planned.layouts.len(),
            planned.layouts[0].lay.rho,
            max_deg,
            t.max_engagement
        ));
    }
    if !clean.is_empty() {
        bld.note(format!("{WHO}: {} corner/wall clean-up pass(es) emitted", clean.len()));
    }
    let rest = rest_share(&reach, &final_cleared);
    if rest > REST_NOTE {
        bld.note(format!(
            "{WHO}: about {:.1} mm2 ({:.0} %) of the region is outside the cutter's reach",
            rest * reach.area(),
            rest * 100.0
        ));
    }
    if p.entry != crate::entry::Entry3D::Helix {
        bld.note(format!("{WHO}: the loops enter by their own bore helix at the lane's start - the Entry setting is not read"));
    }

    let path = bld.finish();
    check_path(&path, &center)?;
    if t.max_engagement != DEFAULT_MAX_ENGAGEMENT {
        let spec = crate::quality::QualitySpec::new(p.tool_diameter);
        let mut stock = crate::quality::stock_for(region, p.top_z, p.bottom_z, p.tool_diameter, &spec)
            .map_err(|e| GenError::Invalid(format!("{WHO}: measuring stock: {e}")))?;
        let quality =
            crate::quality::measure(&path, &mut stock, &spec).map_err(|e| GenError::Invalid(format!("{WHO}: measuring stock: {e}")))?;
        if quality.max_deg > t.max_engagement {
            return Err(GenError::Invalid(format!(
                "{WHO}: cleanup and lanes exceed the {:.1} deg engagement limit (measured {:.1} deg)",
                t.max_engagement, quality.max_deg
            )));
        }
    }
    Ok(path)
}

/// Final production checks apply to lanes and cleanup together.
fn check_path(path: &Toolpath, center: &Region) -> Result<(), GenError> {
    if !path.all_finite() {
        return Err(GenError::Invalid(format!("{WHO}: path left the finite horizon")));
    }
    if let Some(at) = confined(path, center)? {
        return Err(GenError::invalid_at(
            format!("{WHO}: a move at ({:.3}, {:.3}) would leave the region - nothing is cut", at.x, at.y),
            at,
        )
        .with_ghost(path));
    }
    if let Some([i, j]) = crate::verify::crossed_pairs(path, super::ARC_FIT_TOL).first().copied() {
        let at = path.moves.get(i).map_or(Point3::new(0.0, 0.0, 0.0), |m| m.to);
        return Err(GenError::invalid_at(
            format!("{WHO}: the loops would cross themselves (moves {i} and {j}, near ({:.3}, {:.3}))", at.x, at.y),
            at,
        )
        .with_ghost(path));
    }
    Ok(())
}

/// The lane of `center`: its straightened medial line and the loop
/// band's half-width, kept for targeted tests of `cut_lane`.
#[cfg_attr(not(test), allow(dead_code))]
fn lane(region: &Region, center: &Region, p: &PocketParams) -> Result<(Vec<Point2>, f64), GenError> {
    let (d, r) = (p.tool_diameter, 0.5 * p.tool_diameter);
    let other = |why: &str| {
        GenError::Invalid(format!("{WHO}: {why} - the trochoidal method loops along one channel; use the Spiral method for this region"))
    };
    let Some((_, widest)) = widest_point(center) else {
        return Err(narrow_err(0.0, p));
    };
    if widest - WALL_MARGIN <= MIN_BAND_PER_D * d {
        return Err(narrow_err(widest, p));
    }
    let (pts, half) = match medial_spine(center)? {
        Ok((Spine::Open(pts), half)) => (pts, half),
        Ok((Spine::Closed(_), _)) => return Err(other("the region closes round an island")),
        Err(why) => return Err(other(&why)),
    };
    let chain = straightened(&pts, SPINE_TOL * half);
    let b = clearance(center, &chain) - WALL_MARGIN;
    let reach = if p.stock_to_leave > 0.0 { region.offset(-p.stock_to_leave)? } else { region.clone() };
    let swept = Region::sweep(std::slice::from_ref(&chain), r + b)?;
    let rest = rest_share(&reach, &swept);
    if rest > REST_MAX {
        return Err(other(&format!(
            "loops along its medial line would leave {:.0} % of the region standing (at most {:.0} %)",
            rest * 100.0,
            REST_MAX * 100.0
        )));
    }
    Ok((chain, b))
}

/// The lane cut: loops of band half-width `b` along `chain`, planned by
/// the measured-engagement ladder, then held to `center` move by move.
#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn cut_lane(center: &Region, chain: &[Point2], b: f64, p: &PocketParams, t: &AdaptiveTrochParams) -> Result<Toolpath, GenError> {
    let r = 0.5 * p.tool_diameter;
    let tp = trochoid_params(b, p, t);
    let mut path = plan_lane(chain, &tp, true).map_err(renamed)?.path;
    if let Some(at) = confined(&path, center)? {
        return Err(GenError::invalid_at(
            format!("{WHO}: a loop at ({:.3}, {:.3}) would leave the region - nothing is cut", at.x, at.y),
            at,
        )
        .with_ghost(&path));
    }
    for n in &mut path.notes {
        *n = rename(n);
    }
    let reach = center.offset(r)?;
    let b_used = t.loop_width.map_or(b, |w| (0.5 * w).min(b));
    let rest = rest_share(&reach, &Region::sweep(&[chain.to_vec()], r + b_used)?);
    if rest > REST_NOTE {
        path.notes.push(format!(
            "{WHO}: about {:.1} mm2 ({:.0} %) of the region is outside the loops' reach (its corners and wider parts) - a following operation must clear it",
            rest * reach.area(),
            rest * 100.0
        ));
    }
    if p.entry != crate::entry::Entry3D::Helix {
        path.notes.push(format!("{WHO}: the loops enter by their own bore helix at the lane's start - the Entry setting is not read"));
    }
    Ok(path)
}

/// The first cutter-centre point of a cutting move of `path` outside
/// `center` (grown by [`CONFINE_TOL`]), if any.
pub(super) fn confined(path: &Toolpath, center: &Region) -> Result<Option<Point3>, GenError> {
    let grown = center.offset(CONFINE_TOL)?;
    for m in path.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
        for (a, b) in m.segments3(0.01) {
            for q in [a, b] {
                if !grown.contains(Point2::new(q.x, q.y)) {
                    return Ok(Some(q));
                }
            }
        }
    }
    Ok(None)
}

/// The slot planner's words under the method's name.
fn rename(msg: &str) -> String {
    msg.replacen("trochoidal slot:", &format!("{WHO}:"), 1)
}

pub(super) fn renamed(e: GenError) -> GenError {
    match e {
        GenError::Invalid(msg) => GenError::Invalid(rename(&msg)),
        GenError::At { msg, at } => GenError::At { msg: rename(&msg), at },
        GenError::Ghosted { msg, at, ghost } => GenError::Ghosted { msg: rename(&msg), at, ghost },
        other => other,
    }
}

#[cfg(test)]
#[path = "adaptive_troch_tests.rs"]
mod tests;
