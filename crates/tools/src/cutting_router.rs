//! Router chip load by cutter diameter (#3068): the scale that turns a
//! router material's 10 mm feed per tooth into the feed per tooth of a
//! smaller or larger cutter.
//!
//! Provenance: the trend (chip load grows about in proportion to the
//! cutter diameter up to about 6 mm, then more slowly) is the one that
//! public woodworking and router chip-load charts and machining
//! handbooks print. The points are our own conservative numbers, held
//! at the low side of those charts; no vendor table was copied.
//!
//! Pure module: no flecs, no I/O.

/// (cutter diameter mm, chip load / chip load at 10 mm). Linear between
/// the points. Below the first point the scale falls in proportion to
/// the diameter (a small cutter breaks before it burns). Above the last
/// point it holds: a larger cutter gets no more than a 12 mm one.
pub const ROUTER_CURVE: [(f64, f64); 4] = [(3.0, 0.30), (6.0, 0.65), (10.0, 1.0), (12.0, 1.15)];

/// The router chip-load scale at diameter `d` mm. A diameter that is
/// not a finite positive number gives 0 (no feed is better than a
/// guess; the tool's own checks refuse such a cutter first).
#[must_use]
pub fn router_scale(d: f64) -> f64 {
    if !(d.is_finite() && d > 0.0) {
        return 0.0;
    }
    let (d0, s0) = ROUTER_CURVE[0];
    if d <= d0 {
        return s0 * d / d0;
    }
    for pair in ROUTER_CURVE.windows(2) {
        let ((da, sa), (db, sb)) = (pair[0], pair[1]);
        if d <= db {
            return sa + (sb - sa) * (d - da) / (db - da);
        }
    }
    ROUTER_CURVE[ROUTER_CURVE.len() - 1].1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_curve_reads_its_points_and_holds_at_the_ends() {
        for (d, s) in ROUTER_CURVE {
            assert!((router_scale(d) - s).abs() < 1e-12, "{d}");
        }
        assert!((router_scale(10.0) - 1.0).abs() < 1e-12);
        assert!((router_scale(1.5) - 0.15).abs() < 1e-12);
        assert!((router_scale(8.0) - 0.825).abs() < 1e-12);
        assert!((router_scale(25.0) - 1.15).abs() < 1e-12);
        assert_eq!(router_scale(0.0), 0.0);
        assert_eq!(router_scale(f64::NAN), 0.0);
        assert_eq!(router_scale(-3.0), 0.0);
    }

    #[test]
    fn the_curve_never_falls_with_the_diameter() {
        let mut last = 0.0;
        for i in 1..=300 {
            let s = router_scale(f64::from(i) * 0.1);
            assert!(s >= last, "{i}");
            last = s;
        }
    }
}
