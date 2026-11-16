//! The one enclosure of a tilted tool band, shared by every column law
//! that reads a leaning stack: reach (c), its mid-turn sweep (c3), the
//! fixture band law (c2) and the tilted rapid (d2) (#1303). Provenance:
//! the extent of a right circular cylinder along a direction it is
//! tilted against - elementary solid geometry (a disc of radius R whose
//! normal stands θ off the vertical reaches R·sin θ above and below its
//! centre, R or less sideways). Our own derivation, written out in
//! docs/decisions/1303-tilt-band-dip.md; no vendor material consulted.
//!
//! **The band.** A band is the cylinder of radius `r` about the tool
//! axis `a` (unit, tilted θ off +Z) between the axial stations `above`
//! and `above + len` over the tip. Its points are
//! `tip + s·a + ρ·w`, `s` in the span, `0 <= ρ <= r`, `w` any unit
//! vector square to `a`.
//!
//! **The column that holds it.**
//! - Sideways: `s·a` stands within `len/2 · sin θ` of the mid-station's
//!   foot `mid·(ax, ay)`, and `ρ·w` adds `r` or less, so the band
//!   lies inside the vertical column of radius `r + len/2 · sin θ`
//!   about that foot ([`Lean::off`], [`Lean::r`]).
//! - In z: `w` is square to `a`, so its z part is at most `sin θ`
//!   (write +Z as `cos θ·a + sin θ·h`, `h` unit and square to `a`:
//!   `w·Z = sin θ·(w·h)`), and the rim point with `w = -h` reaches it.
//!   The band's lowest point is therefore EXACTLY
//!   `min(above·az, (above + len)·az) - r·sin θ` and its highest
//!   `max(..) + r·sin θ` ([`Lean::lo`], [`Lean::hi`], [`Lean::dip`]).
//!
//! Until #1303 the z planes stood off by the whole `r`: never wrong,
//! but a band a hair off the column read `r` lower than upright, so a
//! holder clear of the stock at 1e-9 rad convicted at 2e-9. With the
//! exact dip the reading meets the upright one as θ goes to 0.
//!
//! **Never under.** [`rim_sine`] never returns less than the sine of
//! the axis's true tilt and never more than 1 (the old reading); what
//! it cannot read - a zero or non-finite axis - gets the whole radius.

/// A leaning band's enclosing vertical column, relative to the tip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Lean {
    /// XY of the column's centre off the tip: the mid-station's foot.
    pub off: (f64, f64),
    /// The column's radius: the band's own plus the half-length's
    /// horizontal run.
    pub r: f64,
    /// The lower of the two end stations' heights over the tip.
    pub lo: f64,
    /// The higher of the two.
    pub hi: f64,
    /// How far the rim stands below `lo` and above `hi`: `r·sin θ`.
    /// The band spans `[lo - dip, hi + dip]` and no further.
    pub dip: f64,
}

/// The sine of `a`'s tilt off the Z line, as the enclosure reads it:
/// never less than the true sine, never more than 1. `a` is a unit
/// axis by the stream's contract and then `hypot(ax, ay)` IS the sine;
/// the laws place the band with the raw components, so an axis off
/// unit length gets the larger of the raw and the normalised reading,
/// and a zero or non-finite axis the whole 1 (the reading before
/// #1303). Fail toward the deeper dip, never the shallower.
#[must_use]
pub(super) fn rim_sine(a: &[f64; 3]) -> f64 {
    let h = a[0].hypot(a[1]);
    let len = h.hypot(a[2]);
    if !(len.is_finite() && len > 0.0) {
        return 1.0;
    }
    let s = h.max(h / len);
    if s.is_finite() { s.min(1.0) } else { 1.0 }
}

/// The rim dip of a disc of radius `r` square to `a`: `r·sin θ`, never
/// less (see [`rim_sine`]) and never more than `r`.
#[must_use]
pub(super) fn rim_dip(a: &[f64; 3], r: f64) -> f64 {
    r * rim_sine(a)
}

/// The column enclosing the band `above..above + len` (radius `r`)
/// on the leaning axis `a` - the module law. The laws add the tip's
/// own place: centre `tip.xy + off`, bottom `tip.z + lo - dip`, top
/// `tip.z + hi + dip`.
#[must_use]
pub(super) fn lean(a: &[f64; 3], above: f64, r: f64, len: f64) -> Lean {
    let mid = above + len * 0.5;
    let spread = (len * 0.5) * (a[0] * a[0] + a[1] * a[1]).sqrt();
    let (za, zb) = (above * a[2], (above + len) * a[2]);
    Lean { off: (mid * a[0], mid * a[1]), r: r + spread, lo: za.min(zb), hi: za.max(zb), dip: rim_dip(a, r) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small deterministic generator (no dependency): uniform in 0..1.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> f64 {
            self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1u64 << 53) as f64)
        }
        fn in_range(&mut self, lo: f64, hi: f64) -> f64 {
            lo + (hi - lo) * self.next()
        }
    }

    fn axis(theta: f64, az: f64) -> [f64; 3] {
        [theta.sin() * az.cos(), theta.sin() * az.sin(), theta.cos()]
    }

    /// Two unit vectors square to `a` and to each other.
    fn basis(a: [f64; 3]) -> ([f64; 3], [f64; 3]) {
        let pick = if a[2].abs() < 0.9 { [0.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0] };
        let cross = |p: [f64; 3], q: [f64; 3]| [p[1] * q[2] - p[2] * q[1], p[2] * q[0] - p[0] * q[2], p[0] * q[1] - p[1] * q[0]];
        let unit = |v: [f64; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            [v[0] / l, v[1] / l, v[2] / l]
        };
        let u = unit(cross(a, pick));
        (u, cross(a, u))
    }

    /// Property: for random tilt (0..180 degrees - a tool pointing down
    /// included), azimuth, radius and span, every sampled point of the
    /// tilted band stands inside the column, and the column's bottom
    /// and top are REACHED by the band (the dip is exact, not slack).
    #[test]
    fn the_column_holds_every_sampled_point_of_the_tilted_band_and_its_planes_are_reached() {
        const EPS: f64 = 1e-9;
        let mut g = Rng(0x1303);
        for case in 0..4000 {
            // Small tilts are where the old cliff stood: a third of the
            // cases live below a milliradian.
            let theta = match case % 3 {
                0 => g.in_range(0.0, 1e-3),
                1 => g.in_range(0.0, std::f64::consts::FRAC_PI_2),
                _ => g.in_range(0.0, std::f64::consts::PI),
            };
            let a = axis(theta, g.in_range(0.0, std::f64::consts::TAU));
            let (r, above, len) = (g.in_range(0.01, 80.0), g.in_range(0.0, 200.0), g.in_range(0.01, 150.0));
            let col = lean(&a, above, r, len);
            let (u, v) = basis(a);
            assert!(col.dip <= r && col.dip >= r * theta.sin() - EPS, "dip {} of r {r} at {theta}", col.dip);
            let (mut z_min, mut z_max) = (f64::INFINITY, f64::NEG_INFINITY);
            for si in 0..=8 {
                let s = above + len * f64::from(si) / 8.0;
                for ri in 0..=2 {
                    let rho = r * f64::from(ri) / 2.0;
                    for pi in 0..48 {
                        let phi = std::f64::consts::TAU * f64::from(pi) / 48.0;
                        let (c, sn) = (phi.cos(), phi.sin());
                        let p = [
                            s * a[0] + rho * (c * u[0] + sn * v[0]),
                            s * a[1] + rho * (c * u[1] + sn * v[1]),
                            s * a[2] + rho * (c * u[2] + sn * v[2]),
                        ];
                        let d = (p[0] - col.off.0).hypot(p[1] - col.off.1);
                        assert!(d <= col.r + EPS, "case {case}: {d} outside the column radius {}", col.r);
                        assert!(p[2] >= col.lo - col.dip - EPS, "case {case}: z {} under the bottom {}", p[2], col.lo - col.dip);
                        assert!(p[2] <= col.hi + col.dip + EPS, "case {case}: z {} over the top {}", p[2], col.hi + col.dip);
                        z_min = z_min.min(p[2]);
                        z_max = z_max.max(p[2]);
                    }
                }
            }
            // The exact lowest and highest rim points (straight down
            // the tilt): the planes are the band's own, no slack left.
            let low = col.lo - r * theta.sin();
            assert!((col.lo - col.dip - low).abs() <= EPS && z_min >= low - EPS, "case {case}");
            // 48 rim stations stand within r·(1 - cos(π/48)) of it.
            let slack = r * (1.0 - (std::f64::consts::PI / 48.0).cos()) + EPS;
            assert!(z_min <= low + slack, "case {case}: the bottom {low} is reached, sampled {z_min}");
            assert!(z_max >= col.hi + col.dip - slack, "case {case}: the top is reached, sampled {z_max}");
        }
    }

    #[test]
    fn the_dip_meets_upright_at_no_tilt_and_the_whole_radius_at_a_right_angle() {
        assert_eq!(rim_dip(&[0.0, 0.0, 1.0], 12.0), 0.0);
        assert_eq!(rim_dip(&[1.0, 0.0, 0.0], 12.0), 12.0);
        assert_eq!(rim_dip(&[0.0, 0.0, -1.0], 12.0), 0.0); // end over end: the disc is level again
        let t = 2e-9f64;
        assert!((rim_dip(&axis(t, 0.3), 12.0) - 12.0 * t).abs() < 1e-15);
        // Monotone in the tilt up to a right angle.
        let mut last = 0.0;
        for k in 0..=900 {
            let d = rim_dip(&axis(f64::from(k).to_radians() / 10.0, 1.1), 7.0);
            assert!(d >= last - 1e-12, "dip falls at {k}");
            last = d;
        }
    }

    /// What the enclosure cannot read gets the whole radius, and an
    /// axis off unit length never reads shallower than its true tilt.
    #[test]
    fn an_unreadable_axis_keeps_the_whole_radius_and_a_scaled_one_never_reads_shallow() {
        for bad in [[0.0, 0.0, 0.0], [f64::NAN, 0.0, 1.0], [0.0, f64::INFINITY, 1.0], [0.0, 0.0, f64::NAN], [1e200, 1e200, 1e200]] {
            assert_eq!(rim_dip(&bad, 5.0), 5.0, "{bad:?}");
        }
        let t = 0.2f64;
        for k in [0.25, 0.5, 0.999, 1.0, 1.001, 3.0] {
            let a = axis(t, 0.7).map(|c| c * k);
            let d = rim_dip(&a, 5.0);
            assert!(d >= 5.0 * t.sin() - 1e-12 && d <= 5.0, "scale {k}: {d}");
        }
    }
}

#[cfg(test)]
mod consumers {
    //! The fixture band law's column (c2, [`super::super::stack_span`])
    //! read directly: its public entry also asks the posed capsule
    //! sweep, whose end caps are a different enclosure.
    use super::super::stack_span::{band_struck, pieces};
    use crate::cl::{Move, MoveKind};
    use crate::fixture::FixtureBox;
    use crate::geometry::Point3;

    fn tilt(deg: f64) -> [f64; 3] {
        [deg.to_radians().sin(), 0.0, deg.to_radians().cos()]
    }

    /// An r 15 holder 30 mm up the axis over a level feed at z 0, a
    /// clamp beside the cut whose top stands 0.5 mm under the holder's
    /// upright bottom. The band's low point drops 15·sin θ (the rim)
    /// plus 30·(1 - cos θ) (its foot): 0.0026 mm at 0.01 degrees -
    /// clear, where the whole radius convicted on any tilt - 0.46 mm
    /// at 1.7 degrees, still clear, and 0.57 mm at 2.1, into the
    /// clamp. Monotone between: once struck, struck.
    #[test]
    fn the_fixture_band_column_dips_by_the_rim_and_strikes_only_when_it_reaches() {
        let clamp = [FixtureBox { min: Point3::new(6.0, -8.0, -10.0), max: Point3::new(14.0, 8.0, 29.5) }];
        let m = Move::new(MoveKind::Feed, Point3::new(0.0, 0.0, 0.0), Point3::new(2.0, 0.0, 0.0), Some(500.0));
        let pcs = pieces(&m, 0.01, 0.0);
        let struck = |lean: Option<[f64; 3]>| band_struck(&pcs, lean, 30.0, 15.0, 30.0, &clamp);
        assert!(!struck(None), "upright, the holder stands over the clamp");
        let mut seen = false;
        for deg in [1e-7, 1e-4, 0.01, 0.1, 1.0, 1.7, 2.1, 3.0, 10.0, 30.0, 60.0] {
            let hit = struck(Some(tilt(deg)));
            assert!(hit || !seen, "the verdict falls back at {deg} degrees");
            assert_eq!(hit, deg >= 2.1, "{deg} degrees");
            seen = hit;
        }
    }
}
