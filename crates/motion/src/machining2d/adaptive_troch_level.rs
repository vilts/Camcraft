//! One level of 3D Adaptive's trochoidal method (issue #1591): the lanes
//! and clean-up peels of `adaptive_troch` (#1509, #1587) planned on a
//! cutter-centre area the caller already has, and posted at one plane.
//! Trochoidal milling is Otkur & Lazoglu 2007, "Trochoidal milling",
//! Int. J. Mach. Tools Manuf. 47, and Rauch, Duc & Hascoet 2009, IJMTM
//! 49; the lane decomposition is `adaptive_troch_lanes`' (Blum 1967;
//! Held 1991). The law here is ours:
//!
//! - the caller's area is the level's own (the footprint less the part
//!   above the plane, eroded by the tool radius and the stock to leave),
//!   so a loop inside it is inside the level area and beside the model;
//! - a bore or a clean-up plunge starts over the material's PROVED top:
//!   the caller says which plane a disc is cleared down to; a bore
//!   starts on that plane (the 3-D ladder's own law for a level under a
//!   cut one, so two levels' helices share no depth), or one plunge
//!   height over the stock top where no plane is proved;
//! - the posted level is CHECKED against the area, move by move, and
//!   against the crossing law - its own moves and every move posted
//!   before it ([`TrochLevel::check`]).
//!
//! Prime Directive: every refusal is by name and nothing is posted on a
//! refusal. No vendor material.

use crate::cl::{Move, MoveKind, PathBuilder, Toolpath, lattice};
use crate::geo2d::{Point2, Region};
use crate::geometry::Point3;

use super::adaptive_troch::{AdaptiveTrochParams, confined};
use super::adaptive_troch_clean::{CleanChain, emit_cleanup, plan_cleanup};
use super::adaptive_troch_lanes::{plan_all_lanes, rest_share};
use super::face::GenError;
use super::pocket::{PocketParams, PocketStrategy};
use super::trochoid_bore::helix_radius;
use super::trochoid_level::emit_level;
use super::trochoid_slot::LaneLayout;

/// The name every refusal and note of the method starts with.
pub(crate) const WHO_3D: &str = "3D Adaptive (Trochoidal)";

/// The planners' own names, said under the method's.
const OLD: [&str; 2] = ["2D Adaptive (Trochoidal):", "trochoidal slot:"];

/// A disc this far (mm) short of its radius inside a cleared area is
/// inside it: the path's own lattice and the area's offset quantisation
/// (`adaptive_troch`'s confinement tolerance).
const PROOF_TOL: f64 = 2e-3;

/// A level leaves this share of its reach standing before it is said.
const REST_NOTE: f64 = 0.005;

/// One planned level: its lanes, its clean-up peels and what they clear.
pub(crate) struct TrochLevel {
    layouts: Vec<LaneLayout>,
    clean: Vec<CleanChain>,
    cleared: Region,
    reach_area: f64,
    rest: f64,
}

/// A planner's message under the 3D method's name.
fn rename(msg: &str) -> String {
    for old in OLD {
        if msg.contains(old) {
            return msg.replacen(old, &format!("{WHO_3D}:"), 1);
        }
    }
    msg.to_owned()
}

/// What a refusal says, without the generator's prefix: the text a
/// fallback note quotes.
pub(crate) fn refusal_text(e: &GenError) -> String {
    match e {
        GenError::Invalid(m) | GenError::Empty(m) | GenError::Model(m) => rename(m),
        GenError::At { msg, .. } | GenError::Ghosted { msg, .. } => rename(msg),
        GenError::Geo(g) => g.to_string(),
    }
}

/// Plan one level on the cutter-centre area `center`. `p` carries the
/// level's plane as both its top and its bottom.
pub(crate) fn plan_troch_level(center: &Region, p: &PocketParams, t: &AdaptiveTrochParams) -> Result<TrochLevel, GenError> {
    let p = &PocketParams { strategy: PocketStrategy::Adaptive, ..*p };
    p.validate()?;
    if let Some(msg) = t.refusal() {
        return Err(GenError::Invalid(format!("{WHO_3D}: {msg}")));
    }
    if p.optimal_load < 0.04 {
        return Err(GenError::Invalid(format!(
            "{WHO_3D}: optimal load below 4% of tool diameter is unsupported by cleanup - increase optimal load to at least 4%"
        )));
    }
    if center.is_empty() {
        return Err(GenError::Empty("empty level area".into()));
    }
    let r = 0.5 * p.tool_diameter;
    // What the cutter can reach from this area, and the wall it stands
    // off: the area grown back by the radius (and the stock to leave).
    let reach = center.offset(r)?;
    let region = if p.stock_to_leave > 0.0 { center.offset(r + p.stock_to_leave)? } else { reach.clone() };
    let planned = plan_all_lanes(&region, center, p, t)?;
    let (clean, cleared) =
        plan_cleanup(center, &reach, &planned.used_center, &planned.cleared, p.tool_diameter, p.optimal_load, p.climb, t)?;
    let rest = rest_share(&reach, &cleared);
    Ok(TrochLevel { layouts: planned.layouts, clean, cleared, reach_area: reach.area(), rest })
}

impl TrochLevel {
    /// The area the level's cuts sweep clear, down to its plane.
    pub(crate) fn cleared(&self) -> &Region {
        &self.cleared
    }

    /// Loops on the level, and the worst engagement its lanes measured.
    pub(crate) fn loops(&self) -> (usize, f64) {
        self.layouts.iter().fold((0, 0.0_f64), |(n, deg), l| (n + l.lay.loops, deg.max(l.deg)))
    }

    /// The stock the level leaves standing in its reach, when it is
    /// worth saying: mm2 and the share.
    pub(crate) fn rest(&self) -> Option<(f64, f64)> {
        (self.rest > REST_NOTE).then_some((self.rest * self.reach_area, self.rest))
    }

    /// Post the level at plane `z`, travelling at `hop` or above.
    /// `cleared(c, radius)` is the plane the disc of `radius` about `c`
    /// is proved cleared down to, if any; `stock_top` is how high stock
    /// can stand where nothing is proved.
    pub(crate) fn emit(
        &self,
        b: &mut PathBuilder,
        p: &PocketParams,
        z: f64,
        hop: f64,
        stock_top: f64,
        cleared: &dyn Fn(Point2, f64) -> Option<f64>,
    ) -> Result<(), GenError> {
        let r = 0.5 * p.tool_diameter;
        let p3 = |q: Point2, zz: f64| Point3::new(lattice(q.x), lattice(q.y), lattice(zz));
        let near = |m: f64| (m + crate::approach::DEFAULT_PLUNGE_HEIGHT).min(hop).max(m);
        for lay in &self.layouts {
            let at = b.at();
            if at.z < hop - 1e-6 {
                b.push(MoveKind::Rapid, Point3::new(at.x, at.y, hop), None);
            }
            let tz = b.at().z;
            b.push(MoveKind::Rapid, p3(lay.entry, tz), None);
            // On its way down the bore sweeps its helix's disc; the
            // opening to the band is a cut on the plane, like the loops.
            // The disc is read [`PROOF_TOL`] short: two levels with one
            // lane lay the disc's rim on the cleared area's own rim.
            let swept = helix_radius(&lay.lay, &lay.tp, lay.open) + r;
            let from = match cleared(lay.lay.c0, swept - PROOF_TOL) {
                Some(zk) => zk.max(z),
                None => near(stock_top.max(z)),
            };
            b.push(MoveKind::Rapid, p3(lay.entry, from), None);
            emit_level(b, &lay.lay, &lay.tp, z, from, true, lay.open).map_err(|e| GenError::Invalid(refusal_text(&e)))?;
        }
        for ch in &self.clean {
            let Some(&q) = ch.pts.first() else { continue };
            emit_cleanup(b, std::slice::from_ref(ch), p, z, cleared(q, r - PROOF_TOL).unwrap_or(stock_top).max(z), hop);
        }
        Ok(())
    }

    /// The posted level's law. `moves` is the whole path, the level its
    /// moves from `n0` on: every cutting move of the level inside
    /// `center`, and none of them crossing another move of the path (its
    /// own, or one posted before it). `Err` names the spot.
    pub(crate) fn check(moves: &[Move], n0: usize, center: &Region) -> Result<(), GenError> {
        let level = Toolpath::new(moves.get(n0..).unwrap_or(&[]).to_vec());
        if !level.all_finite() {
            return Err(GenError::Invalid(format!("{WHO_3D}: path left the finite horizon")));
        }
        if let Some(at) = confined(&level, center)? {
            return Err(GenError::invalid_at(
                format!("{WHO_3D}: a move at ({:.3}, {:.3}) would leave the level area", at.x, at.y),
                at,
            ));
        }
        let path = Toolpath::new(moves.to_vec());
        if let Some([i, j]) = crate::verify::crossed_pairs(&path, super::ARC_FIT_TOL).into_iter().find(|[_, j]| *j >= n0) {
            let at = path.moves.get(j).map_or(Point3::new(0.0, 0.0, 0.0), |m| m.to);
            let what = if i >= n0 { "themselves" } else { "a cut posted before them" };
            return Err(GenError::invalid_at(
                format!("{WHO_3D}: the loops would cross {what} (moves {i} and {j}, near ({:.3}, {:.3}))", at.x, at.y),
                at,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "adaptive_troch_level_tests.rs"]
mod tests;
