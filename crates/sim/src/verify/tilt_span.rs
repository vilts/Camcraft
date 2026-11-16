//! The floor of a tilted band BETWEEN two sampled poses of law (c3)'s
//! mid-turn sweep (#1303 follow-up). Provenance: interval bounds of
//! cos over an angle interval and of a product of two intervals -
//! elementary trigonometry and interval arithmetic; the convexity of
//! `-ln cos` on (-90, 90) degrees (its second derivative is sec²).
//! Our own derivation, written out in
//! docs/decisions/1303-tilt-band-dip.md; no vendor material consulted.
//!
//! **Why a span needs its own floor.** The sweep places the band at
//! sampled poses only. In between, the letters lerp, and the tool axis
//! holds `cos θ = cos A · cos B` (`cl::angles_to_axis`; C turns about
//! Z and leaves the tilt alone). The band's lowest point over the tip
//! is `min(above·cos θ, (above + len)·cos θ) - R·sin θ`
//! ([`super::tilt_band::lean`]).
//! - While every letter A and B of BOTH ends stands inside the CLOSED
//!   [-90, 90] degrees, the whole lerp stays there, `cos A` and `cos B`
//!   stay `>= 0`, and `ln cos A + ln cos B` is concave along it where
//!   it is finite, so `cos θ` takes its least value at an end - by
//!   continuity also when that end stands at exactly 90 (`cos θ = 0`
//!   there, the least it can be). The in-between tilt never exceeds
//!   the larger sampled tilt, and that tilt is at most 90 degrees. With
//!   `above >= 0` and `len >= 0` the bottom `above·cos θ - R·sin θ`
//!   falls as θ grows on [0, 90], so the more tilted sample's own
//!   reading is already the span's lowest: [`span_floor`] says `None`.
//!   The cap was strict before #1404, and a blade-lying swarf link
//!   (B +90 -> -90) read its horizontal end's `-R` floor pinned to the
//!   B 45 pose's footprint, 40 mm under the band there: a false refusal.
//! - Otherwise the in-between axis can stand farther from +Z than both
//!   samples (two samples 100 degrees off +Z whose lerp passes straight
//!   down; or A = B lerping 60 -> 120 degrees, both samples 75.5
//!   degrees off and the midpoint 90). Then the floor is read from the
//!   whole RANGE of `cos θ` the span can reach: never under, whatever
//!   the lerp does between.

/// The range of `cos` over the degree interval between `a0` and `a1`.
/// Non-finite or a full turn or more: the whole [-1, 1].
fn cos_range(a0: f64, a1: f64) -> (f64, f64) {
    // Finite first: `f64::min` drops a NaN, which would read as a point.
    if !(a0.is_finite() && a1.is_finite()) {
        return (-1.0, 1.0);
    }
    let (lo, hi) = (a0.min(a1), a0.max(a1));
    if hi - lo >= 360.0 {
        return (-1.0, 1.0);
    }
    let (c0, c1) = (a0.to_radians().cos(), a1.to_radians().cos());
    let mut mx = c0.max(c1);
    let mut mn = c0.min(c1);
    if (lo / 360.0).ceil() * 360.0 <= hi {
        mx = 1.0; // the interval holds a whole turn's zero: cos peaks at 1
    }
    if ((lo - 180.0) / 360.0).ceil() * 360.0 + 180.0 <= hi {
        mn = -1.0; // and a half turn: cos bottoms at -1
    }
    (mn, mx)
}

/// True when every A and B letter of both ends stands inside the closed
/// [-90, 90] degrees: the span's in-between tilt is then bounded by its
/// more tilted end (module law). Past 90 the lerp can lean further than
/// both ends, and the interval floor reads it.
fn in_cap(from: &[f64; 3], to: &[f64; 3]) -> bool {
    [from[0], from[1], to[0], to[1]].iter().all(|x| x.is_finite() && x.abs() <= 90.0)
}

/// The lowest z over the tip that the band `above..above + len`
/// (radius `r`) can reach at ANY pose between the letter triples
/// `from` and `to` (degrees), or `None` when the two sampled poses'
/// own exact readings already bound every in-between pose (module
/// law). Fails toward the deeper floor: an unreadable input is
/// `-inf`, which no column stands under.
#[must_use]
pub(super) fn span_floor(from: &[f64; 3], to: &[f64; 3], above: f64, r: f64, len: f64) -> Option<f64> {
    if in_cap(from, to) && above >= 0.0 && len >= 0.0 {
        return None;
    }
    let (amin, amax) = cos_range(from[0], to[0]);
    let (bmin, bmax) = cos_range(from[1], to[1]);
    let corners = [amin * bmin, amin * bmax, amax * bmin, amax * bmax];
    let clo = corners.iter().copied().fold(f64::INFINITY, f64::min).clamp(-1.0, 1.0);
    let chi = corners.iter().copied().fold(f64::NEG_INFINITY, f64::max).clamp(-1.0, 1.0);
    // sin θ = sqrt(1 - cos²θ) peaks where cos θ is nearest 0.
    let sin_max = if clo <= 0.0 && chi >= 0.0 { 1.0 } else { (1.0 - (clo * clo).min(chi * chi)).max(0.0).sqrt() };
    let axial = [above * clo, above * chi, (above + len) * clo, (above + len) * chi]
        .iter()
        .copied()
        .fold(f64::INFINITY, f64::min);
    let floor = axial - r * sin_max;
    Some(if floor.is_finite() { floor } else { f64::NEG_INFINITY })
}

#[cfg(test)]
mod tests {
    use super::super::tilt_band::lean;
    use super::*;
    use crate::cl::angles_to_axis;

    /// The band's exact bottom over the tip at the letters `abc`.
    fn bottom(abc: [f64; 3], above: f64, r: f64, len: f64) -> f64 {
        let Some(a) = angles_to_axis(abc) else { return f64::NAN };
        let col = lean(&a, above, r, len);
        col.lo - col.dip
    }

    fn lerp(p: [f64; 3], q: [f64; 3], t: f64) -> [f64; 3] {
        [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t, p[2] + (q[2] - p[2]) * t]
    }

    /// The lowest bottom over a fine lerp of the span.
    fn fine_low(p: [f64; 3], q: [f64; 3], above: f64, r: f64, len: f64) -> f64 {
        (0..=2000).map(|k| bottom(lerp(p, q, f64::from(k) / 2000.0), above, r, len)).fold(f64::INFINITY, f64::min)
    }

    /// Two sampled axes 100 degrees off +Z (A = 100 and A = 260) whose
    /// lerp passes A = 180, the tool straight down: the in-between band
    /// reaches 50 mm under its tip where each sample's exact reading
    /// stands 18.5 mm under it - and so does the whole-radius reading
    /// the sweep used before #1303 (18.7 mm), which was no cover either.
    /// The span floor covers the whole lerp.
    #[test]
    fn a_span_past_a_right_angle_dips_further_than_both_samples_and_the_floor_covers_it() {
        let (above, r, len) = (0.0, 10.0, 50.0);
        let (p, q) = ([100.0, 0.0, 0.0], [260.0, 0.0, 0.0]);
        let (bp, bq) = (bottom(p, above, r, len), bottom(q, above, r, len));
        let low = fine_low(p, q, above, r, len);
        assert!(low < bp.min(bq) - 30.0, "the lerp dips {low} below the samples {bp} / {bq}");
        let floor = span_floor(&p, &q, above, r, len);
        assert!(floor.is_some_and(|f| f <= low + 1e-9), "{floor:?} must cover {low}");
        // Both sampled tilts under 90 degrees is NOT enough: A = B
        // lerping 60 -> 120 holds 75.5 degrees at both ends and 90 at
        // the middle, 2.8 mm deeper on this r 10 band 10 mm up.
        let (p, q) = ([60.0, 60.0, 0.0], [120.0, 120.0, 0.0]);
        let (bp, low) = (bottom(p, 10.0, r, len), fine_low(p, q, 10.0, r, len));
        assert!(low < bp - 2.0, "{low} vs {bp}");
        assert!(span_floor(&p, &q, 10.0, r, len).is_some_and(|f| f <= low + 1e-9));
    }

    /// Inside the cap the samples bound the lerp (the module law, checked
    /// by brute force), and the sweep keeps its exact reading - `None`.
    /// Past it, the floor covers every in-between pose of random spans.
    #[test]
    fn inside_the_cap_the_samples_bound_the_span_and_past_it_the_floor_does() {
        let mut s = 0x1306u64;
        let mut next = || {
            s = s.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((s >> 11) as f64) / ((1u64 << 53) as f64)
        };
        for case in 0..400 {
            let (above, r, len) = (next() * 100.0, 0.01 + next() * 40.0, 0.01 + next() * 120.0);
            let wide = case % 2 == 1;
            let span = if wide { 400.0 } else { 179.8 };
            let mut letter = || (next() - 0.5) * span;
            let (p, q) = ([letter(), letter(), letter()], [letter(), letter(), letter()]);
            let low = fine_low(p, q, above, r, len);
            match span_floor(&p, &q, above, r, len) {
                None => {
                    let sampled = bottom(p, above, r, len).min(bottom(q, above, r, len));
                    assert!(low >= sampled - 1e-9, "case {case}: the lerp {low} under the samples {sampled}");
                }
                Some(f) => assert!(f <= low + 1e-9, "case {case}: floor {f} over the lerp's {low}"),
            }
        }
        // Ends at exactly +-90 are in the closed cap (#1404); the brute
        // lerp still never stands under the samples there.
        for case in 0..400 {
            let (above, r, len) = (next() * 100.0, 0.01 + next() * 40.0, 0.01 + next() * 120.0);
            let mut letter = || match (next() * 4.0) as u32 {
                0 => 90.0,
                1 => -90.0,
                _ => (next() - 0.5) * 180.0,
            };
            let (p, q) = ([letter(), letter(), letter()], [letter(), letter(), letter()]);
            assert_eq!(span_floor(&p, &q, above, r, len), None, "case {case}: {p:?} -> {q:?}");
            let sampled = bottom(p, above, r, len).min(bottom(q, above, r, len));
            let low = fine_low(p, q, above, r, len);
            assert!(low >= sampled - 1e-9, "case {case}: the lerp {low} under the samples {sampled}");
        }
        // Unreadable letters read the deepest floor there is.
        assert_eq!(span_floor(&[f64::NAN, 0.0, 0.0], &[0.0; 3], 0.0, 1.0, 1.0), Some(-1.0 - 1.0));
        assert_eq!(span_floor(&[0.0; 3], &[0.0; 3], f64::NAN, 1.0, 1.0), Some(f64::NEG_INFINITY));
    }
}
