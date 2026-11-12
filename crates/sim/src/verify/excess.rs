//! What the laws do with a model imported coarser than promised (#864, #904): the one tolerance test every law and the program gate read, and the two readings of it a [`VerifyContext`] offers its laws. Provenance: our own design (maintainer decision on #864) - a facet of a convex wall is a chord of the true surface and stands inside it by its sagitta (elementary geometry); no vendor material consulted. Split from `types.rs` by #904, structure only.

use super::types::VerifyContext;

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
