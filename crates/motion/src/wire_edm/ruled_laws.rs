//! The laws of a ruled (4-axis) wire cut (#537), run before the path
//! leaves the generator, over the rulings `(lower point, upper point)`:
//!
//! - **Fold law.** Between two consecutive rulings the lower end moves
//!   by `a` and the upper end by `b`; the wire's section at the height
//!   fraction `v` moves by `(1 - v) a + v b`. Where `a . b < 0` that
//!   section stops advancing at `v = |a|^2 / (|a|^2 - a . b)`, strictly
//!   inside the part height: the surface folds back on itself there
//!   (two rulings meeting in a point is the anti-parallel case).
//!   Refused. Ends at right angles (`a . b = 0`) turn back only on the
//!   plane itself - no fold, a quarter-turn twist the kerf law reads.
//! - **Crossing law.** The wire's section at the quarter heights must
//!   be a simple ring: rulings far apart on the profiles that cross in
//!   space make it cross itself. Refused.
//! - **Ruled kerf law.** At 33 heights from the lower plane to the
//!   upper, every point of the wire's section must stand off the stated
//!   ruled surface's section by the kerf across the leaning wire -
//!   `k sqrt(1 + (t . u)^2)` toward the surface in direction `u`, `t`
//!   the lean per unit height (the ellipse a leaning cylinder cuts from
//!   a plane). The law returns the worst ratio of need to clearance;
//!   the generator widens its offset by it (see `ruled`).
//!
//! Provenance: the bilinear patch between two rulings and its plane
//! sections (Piegl and Tiller, "The NURBS Book", ch. 8), and the public
//! flank-machining result that a straight cylinder cannot touch a
//! twisted (non-developable) ruled surface along a whole ruling - the
//! tangent plane turns along it (do Carmo, "Differential Geometry of
//! Curves and Surfaces", s. 3-5; Liu, "Five-axis NC cylindrical milling
//! of sculptured surfaces", CAD 27(12), 1995). Our own code.

use crate::geo2d::Point2;
use crate::geometry::Point3;
use crate::machining2d::GenError;

use super::ring::dist;

/// The height steps the ruled kerf law reads: both planes and 31 between
/// (the shortfall between two read heights stays under 0.02 um for a
/// 0.05 mm widening - a parabola in the height).
const HEIGHTS: u32 = 32;
/// The most rulings one cut may lay (the crossing law is quadratic).
pub(super) const MAX_RULINGS: usize = 4000;

pub(super) type Ruling = (Point2, Point2);

fn lerp(p: Point2, q: Point2, v: f64) -> Point2 {
    Point2::new(p.x + (q.x - p.x) * v, p.y + (q.y - p.y) * v)
}

/// The fold law over the closed ring of rulings; `rule` and `hint` are
/// the pairing's words for the refusal.
pub(super) fn folds(r: &[Ruling], z0: f64, h: f64, rule: &str, hint: &str) -> Result<(), GenError> {
    let n = r.len();
    for i in 0..n {
        let ((l0, u0), (l1, u1)) = (r[i], r[(i + 1) % n]);
        let (a, b) = ((l1.x - l0.x, l1.y - l0.y), (u1.x - u0.x, u1.y - u0.y));
        let (aa, bb, ab) = (a.0 * a.0 + a.1 * a.1, b.0 * b.0 + b.1 * b.1, a.0 * b.0 + a.1 * b.1);
        // A ruling end that stands still makes no fold of its own.
        if aa.sqrt() < 1e-9 || bb.sqrt() < 1e-9 || ab >= -1e-9 * aa.sqrt() * bb.sqrt() {
            continue;
        }
        let v = aa / (aa - ab);
        let turn = (ab / (aa.sqrt() * bb.sqrt())).clamp(-1.0, 1.0).acos().to_degrees();
        return Err(GenError::invalid_at(
            format!(
                "wire contour: the pairing ({rule}) folds near X{:.3} Y{:.3}: the upper profile runs {turn:.1} degrees against the lower there, so consecutive rulings cross within the part height (the wire's section turns back at Z{:.3}) - {hint}",
                l0.x,
                l0.y,
                z0 + v * h
            ),
            Point3::new(l0.x, l0.y, z0),
        ));
    }
    Ok(())
}

fn cross(o: Point2, a: Point2, b: Point2) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// The crossing law: the wire's section at each quarter height is a
/// simple ring.
pub(super) fn crossing(r: &[Ruling], z0: f64, h: f64) -> Result<(), GenError> {
    let n = r.len();
    for v in [0.25, 0.5, 0.75] {
        let s: Vec<Point2> = r.iter().map(|(l, u)| lerp(*l, *u, v)).collect();
        for i in 0..n {
            let (p, q) = (s[i], s[(i + 1) % n]);
            // Segments i and j share no vertex: j runs past i's
            // neighbours on both sides.
            for j in i + 2..n {
                if i == 0 && j == n - 1 {
                    continue;
                }
                let (c, d) = (s[j], s[(j + 1) % n]);
                let (d1, d2, d3, d4) = (cross(c, d, p), cross(c, d, q), cross(p, q, c), cross(p, q, d));
                if d1 * d2 < 0.0 && d3 * d4 < 0.0 {
                    return Err(GenError::invalid_at(
                        format!(
                            "wire contour: the paired surface crosses itself at Z{:.3} near X{:.3} Y{:.3} - rulings from different parts of the profiles cross within the part height; state sync points that keep the profiles aligned",
                            z0 + v * h,
                            p.x,
                            p.y
                        ),
                        Point3::new(p.x, p.y, z0 + v * h),
                    ));
                }
            }
        }
    }
    Ok(())
}

/// The nearest point of segment a-b to `q`: (parameter, point).
fn foot(q: Point2, a: Point2, b: Point2) -> (f64, Point2) {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let dd = dx * dx + dy * dy;
    let t = if dd > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
    (t, Point2::new(a.x + t * dx, a.y + t * dy))
}

/// What the ruled kerf law read.
pub(super) struct Clearance {
    /// The worst need / clearance over the profile cut (1 = just clear).
    pub(super) ratio: f64,
    /// Where it was read.
    pub(super) at: Point3,
    /// The worst shortfall (mm) of a lead move, and its spot.
    pub(super) lead: Option<(f64, Point3)>,
}

/// The ruled kerf law: the wire rulings `wire` (walked from the lead's
/// foot), threaded upright at `start`, against the stated surface's
/// rulings `product`, for kerf offset `k`, lower plane `z0` and height
/// `h`.
pub(super) fn clearance(product: &[Ruling], wire: &[Ruling], start: Point2, k: f64, z0: f64, h: f64) -> Clearance {
    let mut out = Clearance { ratio: 0.0, at: Point3::new(start.x, start.y, z0), lead: None };
    let Some(first) = wire.first().copied() else {
        return out;
    };
    // The walk: the start hole, every ruling, the first again, the hole.
    let walk: Vec<Ruling> =
        std::iter::once((start, start)).chain(wire.iter().copied()).chain([first, (start, start)]).collect();
    let lean: Vec<(f64, f64)> = walk.iter().map(|(l, u)| ((u.x - l.x) / h, (u.y - l.y) / h)).collect();
    let last = walk.len() - 2;
    for j in 0..=HEIGHTS {
        let v = f64::from(j) / f64::from(HEIGHTS);
        let z = z0 + v * h;
        let prod: Vec<Point2> = product.iter().map(|(l, u)| lerp(*l, *u, v)).collect();
        let w: Vec<Point2> = walk.iter().map(|(l, u)| lerp(*l, *u, v)).collect();
        let m = prod.len();
        // One reading: the wire's point `wp` on walk segment `seg` at
        // parameter `s`, against the surface's point `pp`.
        let mut read = |seg: usize, s: f64, wp: Point2, pp: Point2| {
            let d = dist(wp, pp);
            let (ta, tb) = (lean[seg], lean[seg + 1]);
            let t = (ta.0 + s * (tb.0 - ta.0), ta.1 + s * (tb.1 - ta.1));
            let tu = if d > 0.0 { (t.0 * (pp.x - wp.x) + t.1 * (pp.y - wp.y)) / d } else { t.0.hypot(t.1) };
            let need = k * tu.mul_add(tu, 1.0).sqrt();
            let spot = Point3::new(wp.x, wp.y, z);
            if seg == 0 || seg == last {
                let short = need - d;
                if short > super::PATH_TOL && out.lead.is_none_or(|(worst, _)| short > worst) {
                    out.lead = Some((short, spot));
                }
            } else {
                let ratio = if d > 0.0 { need / d } else { f64::INFINITY };
                if ratio.is_nan() || ratio > out.ratio {
                    out.ratio = if ratio.is_nan() { f64::INFINITY } else { ratio };
                    out.at = spot;
                }
            }
        };
        // Every wire vertex and mid-point against the surface's section.
        for seg in 0..w.len() - 1 {
            for s in [0.0, 0.5] {
                let wp = lerp(w[seg], w[seg + 1], s);
                let mut best = (f64::INFINITY, wp);
                for e in 0..m {
                    let (_, f) = foot(wp, prod[e], prod[(e + 1) % m]);
                    let d = dist(wp, f);
                    if d < best.0 {
                        best = (d, f);
                    }
                }
                read(seg, s, wp, best.1);
            }
        }
        // Every vertex of the surface's section against the wire.
        for pp in &prod {
            let mut best = (f64::INFINITY, 0usize, 0.0, *pp);
            for seg in 0..w.len() - 1 {
                let (s, f) = foot(*pp, w[seg], w[seg + 1]);
                let d = dist(*pp, f);
                if d < best.0 {
                    best = (d, seg, s, f);
                }
            }
            read(best.1, best.2, best.3, *pp);
        }
    }
    out
}
