//! The equal-steps mode of the depth ladder (#2012): the same number of levels as the remainder ladder of `levels.rs`, each step the span divided by that count. Provenance: dividing a depth into equal passes at or under the maximum depth of cut is general machining practice (machine-tool handbooks); our own law, no vendor material consulted.

use crate::approach::Approach;

use super::GenError;
use super::levels::cut_levels_under;

/// The stepdown that walks `top` down to `bottom` in equal steps: the
/// span divided by the number of steps the remainder ladder takes with
/// `stepdown` (`top 20, bottom 2, stepdown 5` gives `4.5`, four steps).
/// Never larger than `stepdown` (but for the level law's own `1e-9` of
/// the span). A span of zero or less, or a stepdown the level law
/// refuses (not positive, not finite, more than [`super::MAX_PASSES`]
/// steps), comes back unchanged, so the law's refusal names the user's
/// number.
#[must_use]
pub fn equal_stepdown(top: f64, bottom: f64, stepdown: f64) -> f64 {
    let span = top - bottom;
    if !(span > 0.0 && span.is_finite() && stepdown > 0.0 && stepdown.is_finite()) {
        return stepdown;
    }
    // The remainder ladder's count: its loop stops within `span * 1e-9`
    // of `bottom`, so a span a hair over a whole number of stepdowns
    // takes no sliver step here either.
    let n = ((span - span * 1e-9) / stepdown).ceil().max(1.0);
    let step = span / n;
    if n <= super::MAX_PASSES && step.is_finite() && step > 0.0 { step } else { stepdown }
}

/// The stepdown the ladder of `a` walks between `top` and `bottom`:
/// [`equal_stepdown`] when the approach asks for equal steps, else
/// `stepdown` itself.
#[must_use]
pub fn ladder_stepdown(a: &Approach, top: f64, bottom: f64, stepdown: f64) -> f64 {
    if a.equal_steps {equal_stepdown(top, bottom, stepdown) } else { stepdown }
}

/// The cut levels of an operation whose approach is `a`: the one level
/// law ([`cut_levels_under`], with the approach's stock top) walked at
/// the stepdown of the approach's ladder mode. The default approach is
/// `cut_levels_under` byte for byte.
pub fn cut_levels_for(top: f64, a: &Approach, bottom: f64, stepdown: f64) -> Result<Vec<f64>, GenError> {
    cut_levels_under(top, a.stock_top, bottom, ladder_stepdown(a, top, bottom, stepdown))
}
