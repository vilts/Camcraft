//! B-spline span selection, de Boor blossom restriction, and piecewise C^0
//! jump bounds for spline/revolved continuous deviation proofs (M12.1, #1029).
//! Provenance: our own implementation from standard CAGD algorithms (de Boor
//! blossom restriction, Farin ch. 10; Cox-de Boor / Piegl & Tiller *The NURBS
//! Book* derivative jump formula across multiplicity-`degree` knots).

use solidcore::eval::plan::{MAX_SPLINE_DEGREE, span_of};

use crate::P3;
use crate::mesh_bound::{norm, scale, sub};

pub(crate) const W: usize = MAX_SPLINE_DEGREE + 1;

pub(crate) enum SpanRange {
    Single(usize),
    Multi(Vec<usize>),
}

impl SpanRange {
    pub(crate) fn as_slice(&self) -> &[usize] {
        match self {
            SpanRange::Single(s) => std::slice::from_ref(s),
            SpanRange::Multi(v) => v.as_slice(),
        }
    }
}

pub(crate) fn spans_and_kinks(
    k: &[f64],
    p: usize,
    n: usize,
    a: f64,
    b: f64,
    is_closed: bool,
) -> Option<(SpanRange, Vec<usize>, bool, f64, f64)> {
    if !(a.is_finite() && b.is_finite() && a <= b) {
        return None;
    }
    if p == 0 || p > MAX_SPLINE_DEGREE || n <= p || k.len() != n + p + 1 {
        return None;
    }
    let (lo, hi) = (*k.get(p)?, *k.get(n)?);
    if !hi.is_finite() || !lo.is_finite() || hi <= lo {
        return None;
    }
    let (wa, wb, wraps) = if is_closed {
        let per = hi - lo;
        if b - a >= per {
            (lo, hi, true)
        } else {
            let x = a - ((a - lo) / per).floor() * per;
            let wb = x + (b - a);
            (x, wb, wb > hi)
        }
    } else {
        (a.max(lo), b.min(hi), false)
    };
    if !wraps && wa >= lo && wb <= hi {
        let s = span_of(k, p, n, wa);
        if k[s] < k[s + 1] && wb <= k[s + 1] {
            return Some((SpanRange::Single(s), Vec::new(), false, wa, wb));
        }
    }
    let inside = |x: f64| (x > wa && x < wb) || (wraps && x > lo && x < wb - (hi - lo));
    let mut kinks = Vec::new();
    let mut idx = p + 1;
    while idx < n {
        let xi = k[idx];
        let start = idx;
        while idx < n && k[idx] == xi {
            idx += 1;
        }
        let mult = idx - start;
        if inside(xi) {
            if mult > p {
                return None;
            }
            if mult == p {
                kinks.push(start);
            }
        }
    }
    let mut spans = Vec::new();
    for s in p..n {
        if !k[s].is_finite() || !k[s + 1].is_finite() || k[s] >= k[s + 1] {
            continue;
        }
        let hit = if b <= lo && !is_closed {
            spans.is_empty()
        } else if a >= hi && !is_closed {
            true
        } else {
            (k[s + 1] > wa && k[s] < wb) || (wraps && k[s + 1] > lo && k[s] < wb - (hi - lo))
        };
        if hit {
            if a >= hi && !is_closed {
                spans.clear();
            }
            spans.push(s);
            if b <= lo && !is_closed {
                break;
            }
        }
    }
    if spans.is_empty() {
        spans.push(span_of(k, p, n, 0.5 * (wa + wb)));
    }
    Some((SpanRange::Multi(spans), kinks, wraps, wa, wb))
}

fn lerp4(x: [f64; 4], y: [f64; 4], t: f64) -> [f64; 4] {
    let u = 1.0 - t;
    [0, 1, 2, 3].map(|k| u * x[k] + t * y[k])
}

fn safe_ratio(num: f64, den: f64) -> f64 {
    if den > 0.0 { (num / den).clamp(0.0, 1.0) } else { 0.0 }
}

/// Clamps and pads `[wa, wb]` within span `[k_lo, k_hi]` to at least `1e-3`
/// of span width so Bezier restriction stays well-conditioned.
pub(crate) fn safe_sub_interval(
    k_lo: f64,
    k_hi: f64,
    wa: f64,
    wb: f64,
    dom_lo: f64,
    dom_hi: f64,
    wraps: bool,
) -> (f64, f64) {
    let span_w = k_hi - k_lo;
    let per = dom_hi - dom_lo;
    let hit1 = wb > dom_lo && wa < dom_hi && k_lo < wb && k_hi > wa;
    let hit2 = wraps && wb - per > dom_lo && k_lo < wb - per && k_hi > dom_lo;
    let (a0, b0) = match (hit1, hit2) {
        (true, false) => (wa.max(k_lo), wb.min(k_hi)),
        (false, true) => (dom_lo.max(k_lo), (wb - per).min(k_hi)),
        _ => (k_lo, k_hi),
    };
    let (a0, b0) = if a0 <= b0 { (a0, b0) } else { (k_lo, k_hi) };
    let min_w = (1e-3 * span_w).min(span_w);
    if b0 - a0 < min_w {
        let mid = 0.5 * (a0 + b0);
        let a = (mid - 0.5 * min_w).max(k_lo);
        let b = (a + min_w).min(k_hi);
        ((b - min_w).max(k_lo), b)
    } else {
        (a0, b0)
    }
}

/// Restricts one univariate B-spline span `s` onto Bezier control points over `[a, b]`.
pub(crate) fn blossom_restrict_1d(knots: &[f64], p: usize, s: usize, work: &mut [[f64; 4]], a: f64, b: f64) {
    let mut left = [[0.0_f64; 4]; W];
    left[p] = work[p];
    for r in 1..=p {
        for j in 0..=p - r {
            let l = knots[s - p + r + j];
            let hi = knots[s + 1 + j];
            work[j] = lerp4(work[j], work[j + 1], safe_ratio(a - l, hi - l));
        }
        left[p - r] = work[p - r];
    }
    work[0] = left[0];
    for (r, dst) in work.iter_mut().enumerate().take(p + 1).skip(1) {
        for j in 0..=p - r {
            let hi = knots[s + 1 + j];
            left[j] = lerp4(left[j], left[j + 1], safe_ratio(b - a, hi - a));
        }
        *dst = left[0];
    }
}

pub(crate) fn cart(h: [f64; 4]) -> Option<(P3, f64)> {
    (h[3].is_finite() && h[3] > 0.0 && h[0].is_finite() && h[1].is_finite() && h[2].is_finite())
        .then(|| ([h[0] / h[3], h[1] / h[3], h[2] / h[3]], h[3]))
}

pub(crate) fn span_box_stats(count: usize, mut at: impl FnMut(usize) -> [f64; 4]) -> Option<(P3, f64, f64, bool)> {
    if count == 0 {
        return None;
    }
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let (mut w_min, mut w_max) = (f64::INFINITY, 0.0_f64);
    for idx in 0..count {
        let (p, w) = cart(at(idx))?;
        w_min = w_min.min(w);
        w_max = w_max.max(w);
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    if !w_min.is_finite() || w_min <= 0.0 {
        return None;
    }
    let p0 = [0, 1, 2].map(|d| 0.5 * (lo[d] + hi[d]));
    let mut r_span = 0.0_f64;
    for idx in 0..count {
        r_span = r_span.max(norm(sub(cart(at(idx))?.0, p0)));
    }
    Some((p0, r_span, w_min, (w_max - w_min) <= 1e-13 * w_max))
}

pub(crate) fn h_shift(h: [f64; 4], p0: P3) -> [f64; 4] {
    [h[0] - p0[0] * h[3], h[1] - p0[1] * h[3], h[2] - p0[2] * h[3], h[3]]
}

pub(crate) fn h_diff(a: [f64; 4], b: [f64; 4], s: f64) -> [f64; 4] {
    [0, 1, 2, 3].map(|k| (a[k] - b[k]) * s)
}

pub(crate) struct SplineDir<'a> {
    pub(crate) ctrl: &'a [[f64; 4]],
    pub(crate) stride: usize,
    pub(crate) knots: &'a [f64],
    pub(crate) p: usize,
    pub(crate) n: usize,
}

pub(crate) fn jump_bound(
    dir: &SplineDir<'_>,
    kink: Option<usize>,
    other_indices: &[usize],
    other_step: usize,
) -> Option<f64> {
    let &SplineDir { ctrl, stride, knots, p, n } = dir;
    let (ip, icl, icr, inx, dl, dr) = match kink {
        Some(s) => (s - 2, s - 1, s - 1, s, *knots.get(s)? - *knots.get(s - 1)?, *knots.get(s + p)? - *knots.get(s)?),
        None => (n - 2, n - 1, 0, 1, knots[n] - knots[n - 1], knots[p + 1] - knots[p]),
    };
    if !(dl > 0.0 && dr > 0.0) {
        return None;
    }
    let pf = p as f64;
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let (mut w_min, mut d_max, mut dw_sum) = (f64::INFINITY, 0.0_f64, 0.0_f64);
    for &j in other_indices {
        let base = j * other_step;
        let ((pp, wp), (pl, wl), (pr, wr), (pn, wn)) = (
            cart(ctrl[base + ip * stride])?,
            cart(ctrl[base + icl * stride])?,
            cart(ctrl[base + icr * stride])?,
            cart(ctrl[base + inx * stride])?,
        );
        if norm(sub(pl, pr)) > 1e-9 || (wl - wr).abs() > 1e-9 * wl.max(wr) {
            return None;
        }
        w_min = w_min.min(wl).min(wr);
        for q in [pl, pr] {
            for d in 0..3 {
                lo[d] = lo[d].min(q[d]);
                hi[d] = hi[d].max(q[d]);
            }
        }
        d_max = d_max.max(norm(sub(scale(sub(pn, pr), pf * wn / dr), scale(sub(pl, pp), pf * wp / dl))));
        dw_sum = dw_sum.max((pf * (wn - wr) / dr).abs() + (pf * (wl - wp) / dl).abs());
    }
    let mid = [0, 1, 2].map(|d| 0.5 * (lo[d] + hi[d]));
    let mut r_row = 0.0_f64;
    for &j in other_indices {
        let base = j * other_step;
        r_row = r_row
            .max(norm(sub(cart(ctrl[base + icl * stride])?.0, mid)))
            .max(norm(sub(cart(ctrl[base + icr * stride])?.0, mid)));
    }
    Some((d_max + 2.0 * dw_sum * r_row) / w_min)
}

pub(crate) fn collect_indices(spans: &[usize], p: usize) -> Vec<usize> {
    let mut idx: Vec<usize> = spans.iter().flat_map(|&sp| sp - p..=sp).collect();
    idx.sort_unstable();
    idx.dedup();
    idx
}
