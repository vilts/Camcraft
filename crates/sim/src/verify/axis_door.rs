//! The verifier's door for a move's stated tool axis (#1312): a
//! non-finite or zero-length axis is refused by the NonFinite law, and a
//! finite axis that is not unit length is normalised once here, so every
//! law after it reads a unit vector. Provenance: elementary vector
//! algebra (a direction is its vector over its length) and the Prime
//! Directive (a pose the laws cannot read is refused, never passed); no
//! vendor material consulted.

use std::borrow::Cow;

use crate::cl::Toolpath;

/// How far `|axis|²` may stand off 1 before the axis is rescaled. A
/// generator's unit vector carries ~1e-16 of rounding; leaving those
/// bit-for-bit untouched keeps every existing verdict (and golden) as it
/// was, while a scaled axis (#1312's 7x) is far outside it.
const UNIT_TOL: f64 = 1e-12;

/// What the door made of a path: the path every law reads, and the
/// moves whose axis it refused (their NonFinite warnings).
pub(crate) struct AxisDoor<'a> {
    pub(crate) path: Cow<'a, Toolpath>,
    pub(crate) refused: Vec<usize>,
}

/// The unit direction of `a`, or None when it has none (a non-finite
/// component or a zero vector).
fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    if !a.iter().all(|c| c.is_finite()) {
        return None;
    }
    let len = a[0].hypot(a[1]).hypot(a[2]);
    if !(len.is_finite() && len > 0.0) {
        return None;
    }
    if ((len * len) - 1.0).abs() <= UNIT_TOL {
        return Some(a); // already unit: untouched, bit for bit
    }
    let u = [a[0] / len, a[1] / len, a[2] / len];
    u.iter().all(|c| c.is_finite()).then_some(u)
}

/// Pass `path` through the door. Borrowed (no copy) when every stated
/// axis is already a finite unit vector - every generator's path. A
/// refused move keeps its axis as stated: its NonFinite warning blocks
/// the post, and every law's own guard for a pose that is no direction
/// still fires (the radial rapid law convicts one, never reading it as
/// upright) - the door adds a refusal, it never takes a verdict away.
pub(crate) fn axis_door(path: &Toolpath) -> AxisDoor<'_> {
    let mut refused = Vec::new();
    let mut fixed: Option<Toolpath> = None;
    for (i, m) in path.moves.iter().enumerate() {
        let Some(a) = m.axis else { continue };
        match unit(a) {
            None => refused.push(i),
            Some(u) if u == a => {}
            Some(u) => {
                let copy = fixed.get_or_insert_with(|| path.clone());
                if let Some(c) = copy.moves.get_mut(i) {
                    c.axis = Some(u);
                }
            }
        }
    }
    AxisDoor { path: fixed.map_or(Cow::Borrowed(path), Cow::Owned), refused }
}

#[cfg(test)]
mod tests {
    use super::unit;

    #[test]
    fn unit_axes_pass_untouched_and_scaled_ones_are_normalised() {
        assert_eq!(unit([0.0, 0.0, 1.0]), Some([0.0, 0.0, 1.0]));
        assert_eq!(unit([0.0, 0.0, 7.0]), Some([0.0, 0.0, 1.0]));
        let u = unit([3.0, 0.0, 4.0]).unwrap_or_default();
        assert!((u[0] - 0.6).abs() < 1e-15 && (u[2] - 0.8).abs() < 1e-15, "{u:?}");
    }

    #[test]
    fn non_finite_and_zero_axes_have_no_direction() {
        assert_eq!(unit([1e-12, 0.0, f64::INFINITY]), None);
        assert_eq!(unit([f64::NAN, 0.0, 1.0]), None);
        assert_eq!(unit([0.0, 0.0, 0.0]), None);
        assert_eq!(unit([f64::MAX, f64::MAX, 0.0]), None, "length overflows");
    }
}
