//! The pairing of two closed profiles into rulings (#537, part of
//! #290): which point of the upper profile the wire holds while its
//! lower end stands on a point of the lower profile. The rule is
//! explicit - one piecewise-linear, monotone map between the two rings'
//! arc lengths, fixed by anchors:
//!
//! - **by normalised arc length**: one anchor (each profile's point
//!   nearest the start hole); from it both rings are walked
//!   counter-clockwise at the same fraction of their own length;
//! - **by sync points**: each stated pair is an anchor; between two
//!   neighbouring anchors both spans are walked at the same fraction of
//!   their own length. Two neighbouring anchors may share one point:
//!   that end stays at the cone apex while the other walks its span.
//!
//! A ruling is laid at every vertex of either ring, so both profiles
//! are traced exactly and the wire between two rulings sweeps one
//! bilinear patch. Anchors that come in one order on the lower ring and
//! another on the upper cross, and are refused by name.
//!
//! Provenance: the arc-length (chord-length) parametrisation and the
//! ruled surface `S(u, v) = (1 - v) C0(u) + v C1(u)` between two
//! parametrised curves, as the public CAGD texts state them (Piegl and
//! Tiller, "The NURBS Book", ch. 8 "ruled surfaces"; Farin, "Curves and
//! Surfaces for CAGD"). Our own code; no vendor post, control manual or
//! CAM product behaviour was consulted.

use crate::geo2d::Point2;
use crate::machining2d::GenError;

use super::ring::dist;

/// Two arc positions nearer than this (mm) are one point.
const ARC_EPS: f64 = 1e-9;
/// Two anchors nearer than this (mm) on one ring are the same point.
const SYNC_GAP: f64 = 1e-6;
/// Two feet whose distances differ by less than this (mm) tie.
const TIE: f64 = 1e-6;

fn invalid(why: String) -> GenError {
    GenError::Invalid(format!("wire contour: {why}"))
}

/// A closed counter-clockwise ring with monotone arc coordinates.
/// Normally these are its own lengths; `preimage` can instead use the
/// input's lengths, with repeated points across an excised input span.
pub(super) struct Ring {
    pub(super) pts: Vec<Point2>,
    /// `cum[i]` is the arc coordinate of `pts[i]`, starting at zero.
    pub(super) cum: Vec<f64>,
    pub(super) total: f64,
}

impl Ring {
    /// The ring of `pts`, turned counter-clockwise, coincident
    /// neighbours dropped; `None` when it bounds nothing.
    pub(super) fn new(pts: &[Point2]) -> Option<Self> {
        let mut clean: Vec<Point2> = Vec::with_capacity(pts.len());
        for p in pts {
            if clean.last().is_none_or(|q| dist(*q, *p) > ARC_EPS) {
                clean.push(*p);
            }
        }
        while clean.len() > 1 && clean.first().zip(clean.last()).is_some_and(|(a, b)| dist(*a, *b) <= ARC_EPS) {
            clean.pop();
        }
        let n = clean.len();
        if n < 3 {
            return None;
        }
        let area: f64 = (0..n).map(|i| clean[i].x * clean[(i + 1) % n].y - clean[(i + 1) % n].x * clean[i].y).sum();
        if !area.is_finite() || area == 0.0 {
            return None;
        }
        if area < 0.0 {
            clean.reverse();
        }
        let mut cum = Vec::with_capacity(n);
        let mut total = 0.0;
        for i in 0..n {
            cum.push(total);
            total += dist(clean[i], clean[(i + 1) % n]);
        }
        (total.is_finite() && total > 0.0).then_some(Ring { pts: clean, cum, total })
    }

    /// The ring's point at arc position `a` (any real: it wraps).
    pub(super) fn at(&self, a: f64) -> Point2 {
        let n = self.pts.len();
        let a = a.rem_euclid(self.total);
        let i = self.cum.partition_point(|c| *c <= a).saturating_sub(1).min(n - 1);
        let (p, q) = (self.pts[i], self.pts[(i + 1) % n]);
        let len = self.cum.get(i + 1).copied().unwrap_or(self.total) - self.cum[i];
        let t = if len > 0.0 { ((a - self.cum[i]) / len).clamp(0.0, 1.0) } else { 0.0 };
        Point2::new(p.x + (q.x - p.x) * t, p.y + (q.y - p.y) * t)
    }

    /// The arc position of the ring's nearest point to `q`, and its
    /// distance. The first nearest edge wins a tie; with `corner`, a
    /// point with two nearest feet close together (it faces a mitred
    /// corner of this ring) takes the ring's point midway between them
    /// - the corner itself.
    pub(super) fn nearest(&self, q: Point2, corner: bool) -> (f64, f64) {
        let n = self.pts.len();
        let foot = |i: usize| {
            let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let dd = dx * dx + dy * dy;
            let t = if dd > 0.0 { (((q.x - a.x) * dx + (q.y - a.y) * dy) / dd).clamp(0.0, 1.0) } else { 0.0 };
            let f = Point2::new(a.x + t * dx, a.y + t * dy);
            let span = self.cum.get(i + 1).copied().unwrap_or(self.total) - self.cum[i];
            (self.cum[i] + t * span, dist(f, q))
        };
        let mut best = (0.0, f64::INFINITY);
        for i in 0..n {
            let f = foot(i);
            if f.1 < best.1 {
                best = f;
            }
        }
        if !corner {
            return best;
        }
        let reach = 8.0 * best.1.max(ARC_EPS);
        let (mut lo, mut hi) = (0.0f64, 0.0f64);
        for i in 0..n {
            let (arc, d) = foot(i);
            if d > best.1 + TIE {
                continue;
            }
            // The signed short way round from the best foot.
            let mut sep = (arc - best.0).rem_euclid(self.total);
            if sep > self.total * 0.5 {
                sep -= self.total;
            }
            if sep.abs() <= reach {
                lo = lo.min(sep);
                hi = hi.max(sep);
            }
        }
        ((best.0 + (lo + hi) * 0.5).rem_euclid(self.total), best.1)
    }
}

/// One fixed correspondence: arc positions on the two rings. `no` is
/// the sync pair's number as stated (1-based); 0 is the seam of an
/// arc-length pairing.
#[derive(Clone, Copy, Debug)]
pub(super) struct Anchor {
    pub(super) upper: f64,
    pub(super) lower: f64,
    pub(super) no: usize,
}

/// The rulings `(lower point, upper point)` of the two rings under the
/// anchors, in counter-clockwise order; with `seam` (an arc position on
/// the lower ring) a ruling is laid there and the walk starts from it.
///
/// # Errors
/// Two identical rulings, or anchors whose order on the upper ring
/// is not their order on the lower (crossed sync points) - by name.
pub(super) fn pair(upper: &Ring, lower: &Ring, anchors: &[Anchor], seam: Option<f64>) -> Result<Vec<(Point2, Point2)>, GenError> {
    let mut a: Vec<Anchor> = anchors.to_vec();
    a.sort_by(|p, q| p.lower.total_cmp(&q.lower));
    let m = a.len();
    let Some(first) = a.first().copied() else {
        return Err(invalid("the pairing states no point to start from".into()));
    };
    let same = |p: Anchor, q: Anchor, which: &str| {
        invalid(format!(
            "sync pairs #{} and #{} state the same point of the {which} profile - state distinct rulings and anchors around both closed profiles; a whole-ring fan is ambiguous",
            p.no.min(q.no),
            p.no.max(q.no)
        ))
    };
    // A fan holds one end still while the other walks a stated span.
    // It still needs anchors at different points on both rings: otherwise
    // the return span would ambiguously mean zero or one whole turn.
    if m > 1 && a.iter().all(|p| (p.lower - first.lower).abs() < SYNC_GAP) {
        return Err(same(first, a[1], "lower"));
    }
    if m > 1 && a.iter().all(|p| (p.upper - first.upper).abs() < SYNC_GAP) {
        return Err(same(first, a[1], "upper"));
    }
    let mut off_prev = 0.0;
    for j in 1..m {
        let (p, q) = (a[j - 1], a[j]);
        let mut off = (q.upper - first.upper).rem_euclid(upper.total);
        // A fan may straddle the seam: after advancing, a return to
        // the first upper point is the end of this turn, not its start.
        if off == 0.0 && off_prev > 0.0 {
            off = upper.total;
        }
        let same_lower = q.lower - p.lower < SYNC_GAP;
        let same_upper = (off - off_prev).abs() < SYNC_GAP;
        if same_lower && same_upper {
            return Err(same(p, q, "lower and upper"));
        }
        if off < off_prev {
            return Err(invalid(format!(
                "sync pair #{} crosses sync pair #{}: walking the lower profile they come in one order, on the upper profile in the other - the rulings between them would cross; restate the pairs in matching order",
                q.no, p.no
            )));
        }
        off_prev = off;
    }
    let mut out: Vec<(Point2, Point2)> = Vec::new();
    for j in 0..m {
        let (from, to) = (a[j], a[(j + 1) % m]);
        let (len_l, len_u) = if m == 1 {
            (lower.total, upper.total)
        } else {
            ((to.lower - from.lower).rem_euclid(lower.total), (to.upper - from.upper).rem_euclid(upper.total))
        };
        let mut fr = vec![0.0];
        let mut lay = |arc: f64, a0: f64, total: f64, len: f64| {
            let rel = (arc - a0).rem_euclid(total);
            if rel > ARC_EPS && rel < len - ARC_EPS {
                fr.push(rel / len);
            }
        };
        for c in &lower.cum {
            lay(*c, from.lower, lower.total, len_l);
        }
        for c in &upper.cum {
            lay(*c, from.upper, upper.total, len_u);
        }
        if let Some(s) = seam {
            lay(s, from.lower, lower.total, len_l);
        }
        fr.sort_by(f64::total_cmp);
        let span = len_l.max(len_u);
        let mut last = f64::NEG_INFINITY;
        for f in fr {
            if (f - last) * span > ARC_EPS {
                out.push((lower.at(from.lower + f * len_l), upper.at(from.upper + f * len_u)));
                last = f;
            }
        }
    }
    if let Some(s) = seam {
        let p = lower.at(s);
        let at = (0..out.len()).min_by(|i, j| dist(out[*i].0, p).total_cmp(&dist(out[*j].0, p))).unwrap_or(0);
        out.rotate_left(at);
    }
    Ok(out)
}
