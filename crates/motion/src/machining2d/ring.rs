//! Split from `machining2d.rs` by M5.0.1; structure only.

use crate::geo2d::{Point2, Region};

use super::arcs::{Seg, fit_arcs};
/// A closed polyline parametrized by arc length, for tab windows.
pub(super) struct Ring {
    pub(super) pts: Vec<Point2>,
    /// Cumulative length at each vertex; the closing edge runs from
    /// `cum[n-1]` to `len`.
    pub(super) cum: Vec<f64>,
    pub(super) len: f64,
}

impl Ring {
    pub(super) fn new(pts: Vec<Point2>) -> Ring {
        let mut p: Vec<Point2> = Vec::with_capacity(pts.len());
        for q in pts {
            if p.last().is_none_or(|r| *r != q) {
                p.push(q);
            }
        }
        if p.len() > 1 && p.first() == p.last() {
            p.pop();
        }
        let n = p.len();
        let mut cum = vec![0.0; n];
        for i in 1..n {
            cum[i] = cum[i - 1] + dist(p[i - 1], p[i]);
        }
        let len = if n > 1 {
            cum[n - 1] + dist(p[n - 1], p[0])
        } else {
            0.0
        };
        Ring { pts: p, cum, len }
    }

    pub(super) fn at(&self, s: f64) -> Point2 {
        let n = self.pts.len();
        if n == 0 || self.len <= 0.0 {
            return self.pts.first().copied().unwrap_or(Point2::new(0.0, 0.0));
        }
        let sm = s.rem_euclid(self.len);
        let mut i = n - 1;
        while i > 0 && self.cum[i] > sm {
            i -= 1;
        }
        let (a, b) = (self.pts[i], self.pts[(i + 1) % n]);
        let seg = if i + 1 < n { self.cum[i + 1] - self.cum[i] } else { self.len - self.cum[i] };
        let t = if seg > 0.0 { ((sm - self.cum[i]) / seg).clamp(0.0, 1.0) } else { 0.0 };
        Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    }

    /// Vertices of the ring sampled from arc position `a` to `b`
    /// (b - a <= len), endpoints interpolated. Vertex i recurs at
    /// `cum[i] + m*len` for every lap m; the walk emits them ordered by
    /// absolute position, which matters for intervals that wrap the
    /// parametrization seam.
    pub(super) fn arc_between(&self, a: f64, b: f64) -> Vec<Point2> {
        let mut out = vec![self.at(a)];
        let n = self.pts.len();
        let l = self.len;
        if n == 0 || l <= 0.0 || b <= a {
            return out;
        }
        let eps = l * 1e-12;
        // b - a <= len, so each vertex recurs in at most one lap here.
        let m_lo = (a / l).floor() as i64 - 1;
        let m_hi = (b / l).floor() as i64 + 1;
        for m in m_lo..=m_hi {
            for i in 0..n {
                let pos = self.cum[i] + m as f64 * l;
                if pos > a + eps
                    && pos < b - eps
                    && out.last().is_none_or(|q| *q != self.pts[i])
                {
                    out.push(self.pts[i]);
                }
            }
        }
        let e = self.at(b);
        if out.last().is_none_or(|q| *q != e) {
            out.push(e);
        }
        out
    }

    /// Arc intervals to actually cut: the circle minus `count` tab
    /// windows of `width`, centered at equal spacing. Windows wider than
    /// their spacing merge; a fully tabbed ring keeps nothing (the loop
    /// contributes no motion at that level).
    #[cfg(test)]
    pub(super) fn tab_keeps(&self, count: usize, width: f64) -> Vec<(f64, f64)> {
        let l = self.len;
        if count == 0 || l <= 0.0 {
            return vec![(0.0, l.max(0.0))];
        }
        let spacing = l / count as f64;
        let hw = width * 0.5;
        if hw >= spacing * 0.5 {
            return Vec::new();
        }
        let ws: Vec<(f64, f64)> = (0..count).map(|i| ((i as f64 + 0.5) * spacing - hw, (i as f64 + 0.5) * spacing + hw)).collect();
        self.keeps_from_windows(&ws)
    }

    /// The keeps of [`Self::tab_keeps`] for a ring whose start the user
    /// picked (#1473): the same count, width and equal spacing, but
    /// measured from the start, so the first window begins at the pick.
    /// Tab `i` spans `[(i+1)s - width, (i+1)s]`; the last one ends on the
    /// seam itself, so no window crosses it. Tabs as wide as their
    /// spacing leave nothing, as in `tab_keeps`.
    #[cfg(test)]
    pub(super) fn tab_keeps_from_start(&self, count: usize, width: f64) -> Vec<(f64, f64)> {
        let l = self.len;
        if count == 0 || l <= 0.0 {
            return vec![(0.0, l.max(0.0))];
        }
        let spacing = l / count as f64;
        if width >= spacing {
            return Vec::new();
        }
        (0..count).map(|i| (i as f64 * spacing, (i + 1) as f64 * spacing - width)).collect()
    }

    /// The arc intervals left to cut when the windows `ws` (arc position
    /// from, to; any lap) are taken out of the ring. Windows that touch or
    /// overlap merge; keep intervals at both ends of the parametrization
    /// are one arc crossing the seam. A window as long as the ring, or
    /// windows that cover it, keep nothing. Bounded: one pass per window.
    pub(super) fn keeps_from_windows(&self, ws: &[(f64, f64)]) -> Vec<(f64, f64)> {
        let l = self.len;
        if l <= 0.0 || !l.is_finite() {
            return vec![(0.0, l.max(0.0))];
        }
        let eps = l * 1e-12;
        let mut parts: Vec<(f64, f64)> = Vec::with_capacity(ws.len() * 2);
        for &(a, b) in ws {
            let span = b - a;
            if !(span.is_finite() && a.is_finite()) {
                return Vec::new();
            }
            if span >= l {
                return Vec::new();
            }
            let w0 = a.rem_euclid(l);
            let w1 = w0 + span.max(0.0);
            if w1 <= l {
                parts.push((w0, w1));
            } else {
                parts.push((w0, l));
                parts.push((0.0, w1 - l));
            }
        }
        parts.sort_by(|x, y| x.0.total_cmp(&y.0));
        let mut merged: Vec<(f64, f64)> = Vec::with_capacity(parts.len());
        for w in parts {
            match merged.last_mut() {
                Some(last) if w.0 <= last.1 + eps => last.1 = last.1.max(w.1),
                _ => merged.push(w),
            }
        }
        let mut keeps: Vec<(f64, f64)> = Vec::with_capacity(merged.len() + 1);
        let mut cur = 0.0;
        for w in &merged {
            if w.0 > cur + eps {
                keeps.push((cur, w.0));
            }
            cur = f64::max(cur, w.1);
        }
        if cur < l - eps {
            keeps.push((cur, l));
        }
        if keeps.len() > 1
            && keeps[0].0 <= eps
            && keeps[keeps.len() - 1].1 >= l - eps
            && let Some(last) = keeps.pop()
        {
            keeps[0] = (last.0, keeps[0].1 + l);
        }
        keeps
    }
}

/// #2480: the length of one tab window on the tool-centre ring. The
/// cutter disc bites `D/2` past each end of a window, so a window of the
/// tab width alone leaves a bridge of `tab_width - D` on the channel's
/// centre line, and none when the width is `D` or less. The window is
/// therefore the tab width plus the tool diameter: the tab width is the
/// narrowest width of the bridge that stays (general CAM practice). This
/// is the start length only: where the ring turns, or a lead reaches into
/// the tab, `tab_bridge::fit_tabs` grows the window until the true bridge
/// (the closest approach of the cut on the two sides, less `D`) holds.
pub(super) fn tab_window(tab_width: f64, tool_diameter: f64) -> f64 {
    let d = if tool_diameter.is_finite() { tool_diameter.max(0.0) } else { 0.0 };
    tab_width + d
}

pub(super) fn dist(a: Point2, b: Point2) -> f64 {
    ((a.x - b.x) * (a.x - b.x) + (a.y - b.y) * (a.y - b.y)).sqrt()
}

/// How far (mm) a point may sit off a loop edge and still read as on it:
/// [`on_boundary`] and the chord law's vertex touch (`chord_clear`, #1620)
/// share it, so a chord end the one accepts is a touch the other sees.
pub(super) const BOUNDARY_TOL: f64 = 1e-9;

/// True when `p` sits on some loop edge, within a nanometer - exact-zero
/// float tests cannot work here: a computed point on an edge (a chord's
/// midpoint) has round-off of a few ulps against the cross product, and
/// boundary points otherwise read as neither inside nor on. The tolerance
/// is far under the micron quantization of the geometry and far over the
/// cancellation error.
pub fn on_boundary(region: &Region, p: Point2) -> bool {
    const TOL: f64 = BOUNDARY_TOL;
    for l in region.loops() {
        let pts = l.points();
        let n = pts.len();
        for i in 0..n {
            let a = pts[i];
            let b = pts[(i + 1) % n];
            let d = Point2::new(b.x - a.x, b.y - a.y);
            let c = Point2::new(p.x - a.x, p.y - a.y);
            let cross = c.x * d.y - c.y * d.x;
            let dd = d.x * d.x + d.y * d.y;
            if dd == 0.0 || cross * cross > TOL * TOL * dd {
                continue;
            }
            let u = (c.x * d.x + c.y * d.y) / dd;
            if (-TOL..=1.0 + TOL).contains(&u) {
                return true;
            }
        }
    }
    false
}

// ---- pocketing ----

/// The circularity law behind [`PocketStrategy::resolve`]: fit the ring
/// with the arc fitter's own tolerance budget and ask what came back.
/// Arc length must cover at least 0.9 of the total; fitted centers must
/// span no more than `stepover / 4` (a concentric circle family agrees
/// on one center; a rounded rectangle's corner arcs do not).
pub fn ring_is_circular(poly: &[Point2], tol: f64, stepover: f64) -> bool {
    if poly.len() < 3 {
        return false;
    }
    // Loop points are vertex lists whose last edge implicitly wraps to the
    // first; the fitter sees plain sequences, so close the ring explicitly
    // before asking it what shape the whole circuit is.
    let mut closed = poly.to_vec();
    if dist(closed[0], *closed.last().unwrap()) > 0.0 {
        closed.push(closed[0]);
    }
    let segs = fit_arcs(&closed, tol);
    let mut arc_len = 0.0;
    let mut line_len = 0.0;
    let mut centers: Vec<Point2> = Vec::new();
    let mut prev = closed[0];
    for seg in &segs {
        match seg {
            Seg::Line(pts) => {
                for q in pts {
                    line_len += dist(prev, *q);
                    prev = *q;
                }
            }
            Seg::Arc { center, to, .. } => {
                // Chord length for the share: an arc is longer than its
                // chord, but coverage compares like-for-like runs, and
                // the 0.9 margin absorbs the difference.
                arc_len += dist(prev, *to);
                centers.push(*center);
                prev = *to;
            }
        }
    }
    let total = arc_len + line_len;
    if total <= 0.0 || !total.is_finite() {
        return false;
    }
    if arc_len / total < 0.9 {
        return false;
    }
    let mut span = 0.0f64;
    for (i, a) in centers.iter().enumerate() {
        for b in centers[i..].iter() {
            span = span.max(dist(*a, *b));
        }
    }
    span <= stepover * 0.25
}

/// A closed ring's points in travel order, starting at its seam: the
/// stored first point (`seam_last == false`) or the stored last one, then
/// round the ring as stored (`forward`) or the other way. #1555: climb and
/// conventional share each ring's seam and differ only in direction -
/// the seam stays where the pre-#1555 default (climb) put it, the outside
/// ring's first point and the inside ring's last (a tessellated bore's
/// axis-aligned point, where its arc entry posts on one G18/G19 arc).
pub(super) fn walked(pts: &[Point2], forward: bool, seam_last: bool) -> Vec<Point2> {
    let n = pts.len();
    if n == 0 {
        return Vec::new();
    }
    let s = if seam_last { n - 1 } else { 0 };
    (0..n).map(|k| if forward { pts[(s + k) % n] } else { pts[(s + n - k) % n] }).collect()
}
