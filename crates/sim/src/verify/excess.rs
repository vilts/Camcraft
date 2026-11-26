//! What the laws do with a model imported coarser than promised (#864, #904): the one tolerance test every law and the program gate read, and the two readings of it a [`VerifyContext`] offers its laws. Provenance: our own design (maintainer decision on #864) - a facet of a convex wall is a chord of the true surface and stands inside it by its sagitta (elementary geometry); no vendor material consulted. Split from `types.rs` by #904, structure only.

use super::types::VerifyContext;
use crate::geo2d::{Loop, Point2};

/// How near a vertex must stand to a declared circle, mm, for its loop
/// to be read as that rim: a region snaps its vertices to the 1 um
/// lattice (up to 0.71 um off), and a model facet that met even the
/// importer's promise stands thousands of times farther.
const RIM_MATCH: f64 = 2e-6;

impl VerifyContext<'_> {
    /// A gouge tolerance, tightened for a model coarser than the
    /// importer promises (#864): `tol` less [`Self::model_excess`], while
    /// that stays positive. At or below zero it would convict every
    /// exactly finished flat face - a misleading verdict - so `tol` comes
    /// back unchanged and the program gate refuses the operation by name
    /// ([`excess_floor_for`] selects the recorded cause).
    #[must_use]
    pub fn tightened(&self, tol: f64) -> f64 {
        excess_floor_for(tol, self.model_excess, self.file_tolerance).unwrap_or(tol)
    }

    /// How far the collision laws grow a shank or holder band on a model
    /// coarser than the importer promises (#904): the whole excess while
    /// the gouge tolerance is still judged, nothing at full fidelity and
    /// nothing past the bound (the program gate refuses there) -
    /// [`band_growth`]. A wall the band clears by less than this may
    /// stand that much nearer than its facets show.
    #[must_use]
    pub fn band_growth(&self) -> f64 {
        if !self.file_tolerance && strict_floor(self.gouge_tol, self.model_excess).is_none() {
            return 0.0;
        }
        band_growth(self.gouge_tol, self.model_excess)
    }

    /// The tolerance law (b) reads the region loop `l` at (#1057):
    /// [`Self::tightened`] for a loop the model's facets drew, the plain
    /// `gouge_tol` for a rim the operation declares
    /// ([`VerifyContext::rims`]). The excess says how far a coarse
    /// model's facets stand inside its true surface; it says nothing
    /// about a circle drawn from a hole's own centre and radius. That
    /// polygon stands inside its circle by up to the rim builder's sag
    /// budget (half the tolerance), which only the untightened
    /// tolerance still covers once the excess passes the other half -
    /// the threshold a matched drill used to be convicted at. The
    /// tool against the part itself stays the solid law's, tightened.
    #[must_use]
    pub fn loop_tol(&self, l: &Loop) -> f64 {
        if self.is_rim(l) { self.gouge_tol } else { self.tightened(self.gouge_tol) }
    }

    /// Whether `l` is one of the declared rims ([`VerifyContext::rims`]).
    #[must_use]
    pub fn is_rim(&self, l: &Loop) -> bool {
        self.rims.iter().any(|&(c, r)| traces(l, c, r))
    }
}

/// The chord error the importer promises, mm: half the program verdict
/// tolerance (app's `step::CHORD_CAP` is the same number). A model's
/// EXCESS is its kept chord error less this.
pub const PROMISED_CHORD: f64 = crate::stock_sim::DEV_TOL * 0.5;

/// The coarsest chord error a model may carry and still be judged, mm
/// (#2996). A constant: no file, budget or vouch can raise it.
pub const WIDE_CHORD_LIMIT: f64 = 0.02;

/// The excess at [`WIDE_CHORD_LIMIT`], with float dust allowed for: a
/// chord recorded at exactly the limit is inside it.
const WIDE_EXCESS_LIMIT: f64 = WIDE_CHORD_LIMIT - PROMISED_CHORD + 1e-9;

/// `tol` less what a model's `excess` chord error takes from it, while
/// that is an honest tolerance: `Some(tol)` untouched when there is no
/// excess, `None` once nothing positive is left - where the caller must
/// refuse rather than judge. A non-finite or negative excess is no excess.
///
/// What it takes (#864, widened by #2996 only when `file_tolerance` is true):
/// - an excess up to [`PROMISED_CHORD`] (a chord up to `DEV_TOL`): all
///   of it, so the real surface is held to the same bound as at full
///   fidelity - #864's law, unchanged;
/// - a larger excess, up to a chord of [`WIDE_CHORD_LIMIT`]: only
///   [`PROMISED_CHORD`]. The tolerance is widened by how far the chord
///   passes `DEV_TOL`: the true-surface bound is the mesh allowance
///   plus the chord. The mesh allowance is capped at `PROMISED_CHORD`
///   even when a law (such as plasma) has a larger normal tolerance;
/// - past the limit: all of it again (#864), which leaves no gouge
///   tolerance, so the operation is refused by name.
///
/// Collision bands never read this widening: they grow by the whole
/// excess ([`band_growth`]).
#[must_use]
pub fn excess_floor_for(tol: f64, excess: f64, file_tolerance: bool) -> Option<f64> {
    if !(excess.is_finite() && excess > 0.0) {
        return Some(tol);
    }
    let widened = file_tolerance && excess > PROMISED_CHORD && excess <= WIDE_EXCESS_LIMIT;
    let taken = if widened { PROMISED_CHORD } else { excess };
    let left = tol - taken;
    if !(left.is_finite() && left > 0.0) {
        return None;
    }
    Some(if widened { left.min(PROMISED_CHORD) } else { left })
}

/// Unrecorded and budget-coarsened models keep the original strict law.
#[must_use]
pub fn excess_floor(tol: f64, excess: f64) -> Option<f64> {
    strict_floor(tol, excess)
}

/// #864's law without the #2996 widening: `tol` less the WHOLE `excess`
/// while that stays positive, `None` past it. For a reader that guards a
/// body other than the cutting edge as well (a turning holder or bar, a
/// rotary shank, a probe stylus): a wall may stand the whole excess
/// nearer than its facets show, so such a reader never gets the
/// widening. A non-finite or negative excess is no excess.
#[must_use]
pub fn strict_floor(tol: f64, excess: f64) -> Option<f64> {
    if !(excess.is_finite() && excess > 0.0) {
        return Some(tol);
    }
    let left = tol - excess;
    (left.is_finite() && left > 0.0).then_some(left)
}

/// How far a collision band grows on a model `excess` coarser than
/// promised (#904, #2996): the whole excess while a law held at `tol` is
/// still judged ([`excess_floor`] leaves it positive), `0.0` at full
/// fidelity and past that bound (the program gate refuses there). A
/// wall may stand the whole excess nearer than its facets show, so the
/// widening of the gouge tolerance is never given to a band.
#[must_use]
pub fn band_growth(tol: f64, excess: f64) -> f64 {
    if excess.is_finite() && excess > 0.0 && excess_floor_for(tol, excess, true).is_some() { excess } else { 0.0 }
}

/// Whether every vertex of `l` (eight at least, as every rim has) stands
/// on the circle of centre `c` and radius `r`: the loop is that rim.
fn traces(l: &Loop, c: Point2, r: f64) -> bool {
    let pts = l.points();
    r.is_finite()
        && r > 0.0
        && pts.len() >= 8
        && pts.iter().all(|p| ((p.x - c.x).hypot(p.y - c.y) - r).abs() <= RIM_MATCH)
}
