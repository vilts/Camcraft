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
    /// ([`excess_floor`] is the one test both sides read).
    #[must_use]
    pub fn tightened(&self, tol: f64) -> f64 {
        excess_floor(tol, self.model_excess).unwrap_or(tol)
    }

    /// How far the collision laws grow a shank or holder band on a model
    /// coarser than the importer promises (#904): what
    /// [`Self::tightened`] takes out of the gouge tolerance - the excess
    /// while it leaves that tolerance positive, nothing at full fidelity
    /// and nothing past the bound (the program gate refuses there). A
    /// wall the band clears by less than this may stand that much nearer
    /// than its facets show.
    #[must_use]
    pub fn band_growth(&self) -> f64 {
        (self.gouge_tol - self.tightened(self.gouge_tol)).max(0.0)
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

/// `tol` less a model's `excess` chord error, while that is an honest
/// tolerance (#864): `Some(tol)` untouched when there is no excess,
/// `Some(tol - excess)` while it stays positive, `None` once the excess
/// has used the whole tolerance - where the caller must refuse rather
/// than judge. A non-finite or negative excess is no excess.
#[must_use]
pub fn excess_floor(tol: f64, excess: f64) -> Option<f64> {
    if !(excess.is_finite() && excess > 0.0) {
        return Some(tol);
    }
    let left = tol - excess;
    (left.is_finite() && left > 0.0).then_some(left)
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
