//! Provenance: our own swept orthonormal caps and analytic line/circle intersections, from elementary Euclidean geometry.

use crate::geometry::Point3;

/// Local outward bisector and cross-edge direction, one frame per segment.
/// Together with the segment tangent these must be orthonormal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BreakFrame {
    pub outward: [f64; 3],
    pub across: [f64; 3],
}

/// Finished cross-section in bisector coordinates. Width is the full
/// cross-edge span; depth is the maximum inward distance from the edge.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BreakProfile {
    Flat,
    ConcaveBall { radius: f64 },
    ConvexRound { radius: f64 },
}

/// A finite cap swept along a polyline. The actual part clips the cap:
/// at across coordinate s, only 0 <= inward <= profile_depth(s) is
/// declared. End planes stop at the selected segment endpoints.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeBreakVolume {
    pub width: f64,
    pub frames: Vec<BreakFrame>,
    pub profile: BreakProfile,
}

impl EdgeBreakVolume {
    pub(super) fn rotate(&mut self, frame: &super::TiltedFrame) {
        let rotate = |v: [f64; 3]| std::array::from_fn(|i| {
            v[0] * frame.u[i] + v[1] * frame.v[i] + v[2] * frame.axis[i]
        });
        for f in &mut self.frames {
            f.outward = rotate(f.outward);
            f.across = rotate(f.across);
        }
    }

    pub(super) fn is_sane(&self, edge: &[Point3], depth: f64) -> bool {
        if !self.width.is_finite() || self.width <= 0.0 || depth > self.width
            || edge.len() < 2 || self.frames.len() != edge.len() - 1
        {
            return false;
        }
        if let Some(r) = self.radius()
            && (!r.is_finite() || r < self.width / 2.0 || r <= 0.0 || sag(self.width / 2.0, r) > depth)
        {
            return false;
        }
        edge.windows(2).zip(&self.frames).all(|(seg, f)| {
            let delta = seg[1] - seg[0];
            let len = delta.x.hypot(delta.y).hypot(delta.z);
            if !len.is_finite() || len <= 0.0 || !(len + self.width + depth).is_finite() {
                return false;
            }
            let tangent = [delta.x / len, delta.y / len, delta.z / len];
            unit(f.outward) && unit(f.across)
                && dot(f.outward, f.across).abs() <= 1e-9
                && dot(f.outward, tangent).abs() <= 1e-9
                && dot(f.across, tangent).abs() <= 1e-9
        })
    }

    fn radius(&self) -> Option<f64> {
        match self.profile {
            BreakProfile::Flat => None,
            BreakProfile::ConcaveBall { radius } | BreakProfile::ConvexRound { radius } => Some(radius),
        }
    }

    fn profile_depth(&self, s: f64, depth: f64) -> f64 {
        match self.profile {
            BreakProfile::Flat => depth,
            BreakProfile::ConcaveBall { radius } => depth - sag(s, radius),
            BreakProfile::ConvexRound { radius } => depth - sag(self.width / 2.0, radius) + sag(s, radius),
        }
    }

    pub(super) fn floor_below(&self, edge: &[Point3], depth: f64, top: Point3) -> Option<f64> {
        let mut spans = Vec::new();
        for (seg, f) in edge.windows(2).zip(&self.frames) {
            self.column_spans(seg[0], seg[1], *f, depth, top, &mut spans);
        }
        // Each span is used at its certain part only: it joins where even
        // its latest possible start is within GAP of material already
        // credited, and it credits down to its earliest possible end.
        // GAP is a fixed length, never a multiple of a coordinate or a
        // dimension: translating the part or widening the cap must not
        // turn a real gap into a joined span.
        let mut bottom: f64 = 0.0;
        let mut joined = vec![false; spans.len()];
        loop {
            let before = bottom;
            for (span, joined) in spans.iter().zip(&mut joined) {
                if !*joined && span.start <= bottom + GAP {
                    *joined = true;
                    bottom = bottom.max(span.end);
                }
            }
            if bottom <= before {
                break;
            }
        }
        let floor = top.z - bottom;
        (bottom > 0.0 && floor.is_finite()).then_some(floor)
    }

    fn column_spans(&self, a: Point3, b: Point3, f: BreakFrame, depth: f64, top: Point3, spans: &mut Vec<Span>) {
        let delta = b - a;
        let len = delta.x.hypot(delta.y).hypot(delta.z);
        let tangent = [delta.x / len, delta.y / len, delta.z / len];
        let q = top - a;
        let q = [q.x, q.y, q.z];
        let (s, ds) = (dot(q, f.across), -f.across[2]);
        let (d, dd) = (-dot(q, f.outward), f.outward[2]);
        let half = self.width / 2.0;
        // Each end plane is measured from its own endpoint, so a column
        // at a polyline vertex meets that plane without roundoff.
        let p = top - b;
        let p = [p.x, p.y, p.z];
        let (Some(from), Some(to), Some(cross), Some(inward)) = (
            slab(dot(q, tangent), reach(q, tangent), -tangent[2], 0.0, f64::INFINITY),
            slab(dot(p, tangent), reach(p, tangent), -tangent[2], f64::NEG_INFINITY, 0.0),
            slab(s, reach(q, f.across), ds, -half, half),
            slab(d, reach(q, f.outward), dd, 0.0, depth),
        ) else {
            return;
        };
        let slabs = [from, to, cross, inward];
        let bounds = slabs.iter().fold((0.0_f64, len + self.width + depth), |b, s| (b.0.max(s.lo), b.1.min(s.hi)));
        if !(bounds.0.is_finite() && bounds.1.is_finite() && bounds.1 > bounds.0) {
            return;
        }
        // How far roundoff can move each end of the interval: the error
        // of the plane that sets it, or of one that nearly does.
        let late = slabs.iter().fold(0.0_f64, |w, s| w.max(s.lo + s.lo_error - bounds.0));
        let early = slabs.iter().fold(0.0_f64, |w, s| w.max(bounds.1 - (s.hi - s.hi_error)));
        let mut cuts = vec![bounds.0, bounds.1];
        let speed = ds.hypot(dd);
        if let Some(radius) = self.radius().filter(|_| speed > 0.0) {
            let (base, sign) = match self.profile {
                BreakProfile::ConcaveBall { .. } => (depth, 1.0),
                _ => (depth - sag(half, radius), -1.0),
            };
            // Expand the circle about its surface, not its distant
            // centre: subtracting R from a shallow sag loses the cut.
            // Solve in cross-section distance, not column height:
            // squaring tiny world-Z rates can underflow to zero.
            let (rs, rd) = (ds / speed, dd / speed);
            let e = d - base;
            let aa = (rs * rs + rd * rd) / radius;
            let bb = 2.0 * (s / radius * rs + e / radius * rd + sign * rd);
            let cc = e * (e / radius + 2.0 * sign) + s * (s / radius);
            if !(aa.is_finite() && bb.is_finite() && cc.is_finite()) {
                return; // ill-conditioned records grant no credit
            }
            let mut roots = Vec::new();
            if !circle_roots(aa, bb, cc, (bounds.0 * speed, bounds.1 * speed), &mut roots) {
                return;
            }
            cuts.extend(roots.into_iter().map(|distance| distance / speed));
        }
        cuts.sort_by(f64::total_cmp);
        // A profile crossing is solved from the same offsets as the
        // planes, so it is no more certain than they are.
        let wobble = |h: f64| {
            let root = late.max(early) + ROUND * h.abs();
            if h == bounds.0 { late } else if h == bounds.1 { early } else { root }
        };
        // Pieces that share a crossing are one span.
        let mut run: Option<(f64, f64)> = None;
        let mut close = |run: Option<(f64, f64)>| {
            if let Some((lo, hi)) = run {
                let span = Span { start: lo + wobble(lo), end: hi - wobble(hi) };
                if span.end > span.start {
                    spans.push(span);
                }
            }
        };
        for pair in cuts.windows(2) {
            let (lo, hi) = (pair[0], pair[1]);
            if hi <= lo {
                continue;
            }
            let mid = lo + (hi - lo) / 2.0;
            if d + dd * mid <= self.profile_depth(s + ds * mid, depth) {
                run = Some((run.map_or(lo, |r| r.0), hi));
            } else {
                close(run.take());
            }
        }
        close(run);
    }
}

/// The certain part of one column interval inside a cap: its latest
/// possible start and its earliest possible end, as depths below the top.
struct Span {
    start: f64,
    end: f64,
}

fn unit(v: [f64; 3]) -> bool {
    v.iter().all(|x| x.is_finite()) && (dot(v, v) - 1.0).abs() <= 1e-9
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Stable circle sag, including R much larger than the break width.
fn sag(s: f64, radius: f64) -> f64 {
    let t = (s.abs() / radius).min(1.0);
    s.abs() * t / (1.0 + ((1.0 - t) * (1.0 + t)).sqrt())
}

/// The largest real gap, in mm, that a connection may bridge, and the
/// largest roundoff an interval end may carry before its segment is
/// refused. An absolute length: it does not grow with world position,
/// width or depth.
const GAP: f64 = 1e-9;

/// Roundoff of a three-term dot product of a rounded difference, with margin.
const ROUND: f64 = 16.0 * f64::EPSILON;

/// Size of the terms summed in `dot(q, v)`: what its roundoff scales with.
/// `q` is the offset from the edge to the column, so this is local.
fn reach(q: [f64; 3], v: [f64; 3]) -> f64 {
    (q[0] * v[0]).abs() + (q[1] * v[1]).abs() + (q[2] * v[2]).abs()
}

/// Column heights inside one slab, each end with a bound on its roundoff.
struct Slab {
    lo: f64,
    hi: f64,
    lo_error: f64,
    hi_error: f64,
}

/// Heights where `min <= value + rate * h <= max`. `None` outside the
/// slab, or where the answer is too uncertain to grant credit.
fn slab(value: f64, reach: f64, rate: f64, min: f64, max: f64) -> Option<Slab> {
    if !value.is_finite() || !reach.is_finite() {
        return None;
    }
    if rate == 0.0 {
        let inside = value >= min && value <= max && ROUND * reach <= GAP;
        return inside.then_some(Slab { lo: f64::NEG_INFINITY, hi: f64::INFINITY, lo_error: 0.0, hi_error: 0.0 });
    }
    // An infinite plane is no bound: it has no crossing to be unsure of.
    let end = |plane: f64| {
        let error = if plane.is_finite() { ROUND * (reach + plane.abs()) / rate.abs() } else { 0.0 };
        ((plane - value) / rate, error)
    };
    let (mut lo, mut hi) = (end(min), end(max));
    if lo.0 > hi.0 {
        std::mem::swap(&mut lo, &mut hi);
    }
    let known = !lo.0.is_nan() && !hi.0.is_nan() && lo.1.is_finite() && hi.1.is_finite();
    known.then_some(Slab { lo: lo.0, hi: hi.0, lo_error: lo.1, hi_error: hi.1 })
}

fn circle_roots(a: f64, b: f64, c: f64, bounds: (f64, f64), cuts: &mut Vec<f64>) -> bool {
    // Roots are unchanged by a common scale. Normalize before products:
    // finite, tiny coefficients can otherwise lose a positive discriminant.
    let scale = a.abs().max(b.abs()).max(c.abs());
    if !scale.is_finite() {
        return false;
    }
    if scale == 0.0 {
        return true;
    }
    let (aa, bb, cc) = (a / scale, b / scale, c / scale);
    if (a != 0.0 && aa == 0.0) || (b != 0.0 && bb == 0.0) || (c != 0.0 && cc == 0.0) {
        return false; // lost coefficients cannot safely grant credit
    }
    let (a, b, c) = (aa, bb, cc);
    let mut take = |h: f64| {
        if h.is_finite() && h > bounds.0 && h < bounds.1 {
            cuts.push(h);
        }
    };
    if a == 0.0 {
        if b != 0.0 {
            take(-c / b);
        }
        return true;
    }
    let square = b * b;
    let product = 4.0 * a * c;
    if (b != 0.0 && square == 0.0) || (a != 0.0 && c != 0.0 && product == 0.0) {
        return false;
    }
    let disc = square - product;
    if !disc.is_finite() {
        return false;
    }
    if disc < 0.0 {
        return true;
    }
    let q = -0.5 * (b + disc.sqrt().copysign(b));
    if q != 0.0 {
        take(q / a);
        take(c / q);
    } else {
        take(-b / (2.0 * a));
    }
    true
}
