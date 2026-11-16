//! The verifier's one notion of an upright tool: a stated pose whose
//! tilt off +Z is within [`UPRIGHT_TILT_RAD`] is judged exactly as a
//! vertical move (#938). Provenance: small-angle bound of elementary
//! trigonometry (a body of length L tilted by θ moves at most L·sin θ
//! sideways, a disc of radius R dips at most R·sin θ); the tolerance
//! value is ours, justified in docs/decisions/938-tilt-tolerance.md.
//! No vendor material consulted.
//!
//! Why not `camcraft_cl::is_vertical`: that predicate is EXACT on
//! purpose - the stream dump and the post take the prismatic code path
//! on it, bit for bit, so goldens never learn float drift. A verdict is
//! a different question: a pose a rotated setup leaves 1e-10° off the
//! column is upright steel to within a picometre. (The tilted reach
//! laws' enclosing columns dropped a band's bottom by its whole radius
//! then, a cliff on noise; since #1303 they dip by the rim's exact
//! R·sin θ, and the two readings meet at the tolerance.)

/// The largest tilt off +Z (radians) the verifier still judges as an
/// upright tool. At 1e-9 rad a 1 m tool stack moves at most 1e-6 mm
/// sideways and a 100 mm holder disc dips at most 1e-7 mm: four orders
/// below the tightest gouge tolerance a setup states (0.01 mm), and at
/// or under the laws' own kerf margins. Rounding noise of a composed
/// rotation (~1e-16 per component, 1e-10° ≈ 1.7e-12 rad in #938) sits
/// three orders inside it; a tilt anyone commands (0.001° ≈ 1.7e-5
/// rad) sits four orders outside.
pub(crate) const UPRIGHT_TILT_RAD: f64 = 1e-9;

/// True when `axis` points up the Z column to within
/// [`UPRIGHT_TILT_RAD`]. Not required to be unit: the horizontal part is
/// compared against the length, so the test reads the angle itself.
/// A downward or sideways axis, a zero vector or any non-finite
/// component is NOT upright - fail toward the tilted (enclosing)
/// reading, never toward the prismatic one.
#[must_use]
pub(crate) fn is_upright(axis: &[f64; 3]) -> bool {
    if crate::cl::is_vertical(axis) {
        return true; // the exact case keeps its exact answer
    }
    let h = axis[0].hypot(axis[1]);
    let len = h.hypot(axis[2]);
    // sin θ = h / len; θ ≥ sin θ, so `h <= tol·len` admits no tilt
    // above the tolerance. NaN fails every comparison: not upright.
    axis[2] > 0.0 && len.is_finite() && h <= UPRIGHT_TILT_RAD * len
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tilted(deg: f64) -> [f64; 3] {
        let t = deg.to_radians();
        [t.sin(), 0.0, t.cos()]
    }

    #[test]
    fn rounding_noise_is_upright_and_a_real_tilt_is_not() {
        assert!(is_upright(&[0.0, 0.0, 1.0]));
        assert!(is_upright(&tilted(1e-10)));
        assert!(is_upright(&[1e-12, -1e-12, 1.0]));
        // Just above the tolerance: tilted.
        let t = UPRIGHT_TILT_RAD * 1.01;
        assert!(!is_upright(&[t.sin(), 0.0, t.cos()]));
        assert!(!is_upright(&tilted(0.001)));
        // Never upright: end-over-end, sideways, zero, NaN.
        assert!(!is_upright(&[0.0, 0.0, -1.0]));
        assert!(!is_upright(&[1e-12, 0.0, -1.0]));
        assert!(!is_upright(&[1.0, 0.0, 0.0]));
        assert!(!is_upright(&[0.0, 0.0, 0.0]));
        assert!(!is_upright(&[f64::NAN, 0.0, 1.0]));
        assert!(!is_upright(&[1e-12, 0.0, f64::INFINITY]));
    }
}
