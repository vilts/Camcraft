//! A modelled tool body read as the stack laws read steel (#368): its
//! revolved envelope - the largest distance from the tool axis the body
//! reaches inside each band along it - and the stack of coaxial bands
//! the collision laws sweep in place of the single holder cylinder.
//! Specced from the revolved-solid reading of a tool assembly in
//! machine-tool handbooks; clipping a triangle to a slab is textbook
//! polygon clipping (Sutherland-Hodgman, public material), and the max
//! of the convex function |(x, y)| over a convex polygon sits at one of
//! its vertices (elementary convex analysis). Our own design; no vendor
//! source or file consulted.
//!
//! Conservative by construction: every triangle is clipped exactly to
//! each band it spans and the band takes the farthest clipped vertex,
//! so a facet crossing a band with no vertex inside it still counts.
//! Bands are then merged greedily, each merge taking the larger radius
//! - a coarser profile can only grow, never shrink.

use crate::tools::{Tool, ToolGeom};

/// One coaxial band of steel: it starts `above` the tip, runs `len`
/// along the axis, `r` from it - the shape the stack laws already
/// read for the shank and the holder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HolderBand {
    pub above: f64,
    pub len: f64,
    pub r: f64,
}

/// What stands above the shank, as the fixture laws sweep it: the bands
/// bottom-up, and the height the spindle housing rides on.
#[derive(Clone, Debug, PartialEq)]
pub struct HolderStack {
    pub bands: Vec<HolderBand>,
    pub top: f64,
    /// #852: the tool's drawn neck over the shank's span, bottom-up
    /// (`neck::neck_bands`); empty - the stated shank cylinder - for
    /// every tool without one. Read through `HolderStack::shank_bands`.
    pub neck: Vec<HolderBand>,
}

impl HolderStack {
    /// Today's holder: one cylinder on the shank's top, the housing on
    /// its top - the very sums the laws always made, so a tool with no
    /// body reads bit-identically.
    #[must_use]
    pub fn cylinder(g: &ToolGeom) -> Self {
        Self {
            bands: vec![HolderBand {
                above: g.flute_len + g.shank_len,
                len: g.holder_len,
                r: g.holder_radius,
            }],
            top: g.flute_len + g.shank_len + g.holder_len,
            neck: Vec::new(),
        }
    }
}

/// Fine bands are cut no thinner than this (mm) before merging.
pub const FINE_BAND: f64 = 0.25;
/// Most fine bands one body is cut into (a taller body cuts coarser).
pub const MAX_FINE_BANDS: usize = 2048;
/// Most bands a profile keeps after merging - the stack laws sweep
/// one capsule per band per interval, so the count is their cost.
pub const MAX_BANDS: usize = 32;
/// Radial pad (mm) on every band: float dust from the clip, never a
/// clearance.
const PAD: f64 = 1e-6;
/// The widest and tallest body that can be meant (mm) - the tool
/// record's own diameter cap, and a height no spindle carries.
pub const MAX_BODY_RADIUS: f64 = 500.0;
pub const MAX_BODY_HEIGHT: f64 = 2000.0;

/// Why a mesh is no tool body. Each refusal names itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeError {
    Empty,
    NonFinite,
    Degenerate,
    Flat,
    OffAxis,
    BelowTip,
    TooLarge,
}

// Hand-rolled Display: the crate takes no dependencies.
impl core::fmt::Display for EnvelopeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Empty => "the tool body has no facets",
            Self::NonFinite => "the tool body has non-finite coordinates",
            Self::Degenerate => "the tool body is degenerate - every facet has zero area",
            Self::Flat => "the tool body has no length along the tool axis - check the axis choice",
            Self::OffAxis => {
                "the tool body does not surround the tool axis - check the axis choice and placement"
            }
            Self::BelowTip => "the tool body reaches below the tool tip - check the tip offset",
            Self::TooLarge => "the tool body is larger than any tool a spindle carries (500 mm from the axis, 2000 mm long)",
        })
    }
}

impl std::error::Error for EnvelopeError {}

/// A body's revolved envelope: contiguous bands bottom-up, each the
/// farthest the body reaches from the axis inside it.
#[derive(Clone, Debug, PartialEq)]
pub struct HolderProfile {
    bands: Vec<HolderBand>,
}

type Tri = [[f64; 3]; 3];

impl HolderProfile {
    /// The envelope of `tris`, given in the tool frame (tip at the
    /// origin, axis +Z toward the spindle). Refuses by name: no facets,
    /// non-finite numbers, all facets zero-area, no axial length, a
    /// body below the tip, one whose XY box misses the axis, or one
    /// past the size caps.
    pub fn envelope(tris: &[Tri]) -> Result<Self, EnvelopeError> {
        if tris.is_empty() {
            return Err(EnvelopeError::Empty);
        }
        if !tris.iter().flatten().flatten().all(|v| v.is_finite()) {
            return Err(EnvelopeError::NonFinite);
        }
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for p in tris.iter().flatten() {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let scale = (0..3).map(|k| hi[k] - lo[k]).fold(0.0_f64, f64::max);
        let area_floor = (scale * 1e-9).powi(2);
        if !tris.iter().any(|t| area2(t) > area_floor) {
            return Err(EnvelopeError::Degenerate);
        }
        let (z0, z1) = (lo[2], hi[2]);
        if z1 - z0 <= 1e-6 {
            return Err(EnvelopeError::Flat);
        }
        if z0 < -1e-9 {
            return Err(EnvelopeError::BelowTip);
        }
        if !(lo[0] < 0.0 && hi[0] > 0.0 && lo[1] < 0.0 && hi[1] > 0.0) {
            return Err(EnvelopeError::OffAxis);
        }
        let rmax = lo.iter().take(2).chain(hi.iter().take(2)).fold(0.0_f64, |a, v| a.max(v.abs()));
        if rmax > MAX_BODY_RADIUS || z1 > MAX_BODY_HEIGHT {
            return Err(EnvelopeError::TooLarge);
        }
        let h = z1 - z0;
        // Both bounded above by the caps checked just now: h <= 2000,
        // so the ceil is a small positive integer and the cast exact.
        let n = ((h / FINE_BAND).ceil().clamp(1.0, MAX_FINE_BANDS as f64)) as usize;
        let step = h / n as f64;
        let edge = |k: usize| if k >= n { z1 } else { z0 + step * k as f64 };
        let mut r = vec![0.0_f64; n];
        for t in tris {
            let tz0 = t[0][2].min(t[1][2]).min(t[2][2]);
            let tz1 = t[0][2].max(t[1][2]).max(t[2][2]);
            let k0 = band_index(tz0, z0, step, n);
            let k1 = band_index(tz1, z0, step, n);
            for (k, rk) in r.iter_mut().enumerate().take(k1 + 1).skip(k0) {
                let reach = clipped_reach(t, edge(k), edge(k + 1));
                if reach > *rk {
                    *rk = reach;
                }
            }
        }
        let fine: Vec<HolderBand> = (0..n)
            .map(|k| HolderBand { above: edge(k), len: edge(k + 1) - edge(k), r: r[k] + PAD })
            .collect();
        Ok(Self { bands: merge(fine, MAX_BANDS) })
    }

    /// A profile from contiguous bands already known exactly (a drawn
    /// spun profile, #93), merged to [`MAX_BANDS`] - merging only grows.
    pub(crate) fn from_bands(bands: Vec<HolderBand>) -> Self {
        Self { bands: merge(bands, MAX_BANDS) }
    }

    #[must_use]
    pub fn bands(&self) -> &[HolderBand] {
        &self.bands
    }

    /// Lowest point of the body above the tip.
    #[must_use]
    pub fn bottom(&self) -> f64 {
        self.bands.first().map_or(0.0, |b| b.above)
    }

    /// Highest point of the body above the tip.
    #[must_use]
    pub fn top(&self) -> f64 {
        self.bands.last().map_or(0.0, |b| b.above + b.len)
    }

    /// Farthest any band of the body reaches over the heights
    /// `[lo, hi]` (overlap inclusive of touching ends); 0 when none.
    #[must_use]
    pub fn reach_over(&self, lo: f64, hi: f64) -> f64 {
        self.bands
            .iter()
            .filter(|b| b.above <= hi && b.above + b.len >= lo)
            .fold(0.0, |a, b| a.max(b.r))
    }

    /// The stack the fixture laws sweep above the shank of `g`: the
    /// body's bands, the shank carried up to the body's foot where the
    /// body starts above the shank's top (the shank runs into the
    /// holder; leaving that gap unjudged would be silent), the housing
    /// on the higher of the two tops.
    #[must_use]
    pub fn stack(&self, g: &ToolGeom) -> HolderStack {
        let shank_top = g.flute_len + g.shank_len;
        let mut bands = Vec::with_capacity(self.bands.len() + 1);
        if self.bottom() > shank_top {
            bands.push(HolderBand { above: shank_top, len: self.bottom() - shank_top, r: g.shank_radius });
        }
        bands.extend_from_slice(&self.bands);
        HolderStack { bands, top: self.top().max(shank_top), neck: Vec::new() }
    }

    /// #828: this body judged no smaller than the stand-in holder
    /// cylinder of `g` (its `holder_radius` x `holder_len` on the
    /// shank's top) - the union of the two, band by band. An ASSUMED
    /// holder (a machine's default, not one the user picked) is judged
    /// so: assuming a holder must never pass a wall the stand-in cylinder
    /// convicts. A gap between the two is the shank running into the
    /// holder, judged at the shank's radius as [`Self::stack`] does.
    /// Non-finite or empty numbers leave the body as it is (the tool
    /// record's own law refuses them first).
    #[must_use]
    pub fn floored(&self, g: &ToolGeom) -> Self {
        let cyl = HolderBand { above: g.flute_len + g.shank_len, len: g.holder_len, r: g.holder_radius };
        let sane = [cyl.above, cyl.len, cyl.r, g.shank_radius].iter().all(|v| v.is_finite());
        if !sane || cyl.len <= 0.0 || cyl.r <= 0.0 || self.bands.is_empty() {
            return self.clone();
        }
        let all: Vec<HolderBand> = self.bands.iter().copied().chain(std::iter::once(cyl)).collect();
        let mut edges: Vec<f64> = all.iter().flat_map(|b| [b.above, b.above + b.len]).collect();
        edges.sort_by(f64::total_cmp);
        edges.dedup_by(|b, a| (*b - *a).abs() <= 1e-12);
        let bands = edges
            .windows(2)
            .filter_map(|w| {
                let (lo, hi) = (w[0], w[1]);
                let mid = 0.5 * (lo + hi);
                let r = all.iter().filter(|b| b.above < mid && mid < b.above + b.len).map(|b| b.r).fold(None, |a: Option<f64>, r| Some(a.map_or(r, |a| a.max(r))));
                (hi - lo > 1e-12).then(|| HolderBand { above: lo, len: hi - lo, r: r.unwrap_or(g.shank_radius) })
            })
            .collect();
        Self::from_bands(bands)
    }

    /// The same tool with its holder replaced by the single cylinder
    /// that encloses this body, for every law that reads one cylinder:
    /// the holder from the shank's top to the body's top at the body's
    /// widest above the shank, and the shank widened to the widest the
    /// body reaches beside it. Never smaller than the body anywhere
    /// over the shank and holder - so it can only over-report. A body
    /// reaching down into the flutes is refused by name (`None`): no
    /// cylinder stack can stand a holder over the cutting edge.
    #[must_use]
    pub fn enclose(&self, t: &Tool) -> Option<Tool> {
        let fl = t.flute_len.max(0.0);
        let shank_top = fl + t.shank_len.max(0.0);
        if self.bottom() < fl - 1e-9 {
            return None;
        }
        let beside = self.reach_over(fl, shank_top);
        let above = self.reach_over(shank_top, f64::INFINITY);
        Some(Tool {
            shank_d: t.shank_d.max(2.0 * beside),
            holder_d: 2.0 * above,
            holder_len: (self.top() - shank_top).max(0.0),
            ..t.clone()
        })
    }
}

/// Twice the facet's area (#369's lathe clearance reads it too).
pub(crate) fn area2(t: &Tri) -> f64 {
    let u = [t[1][0] - t[0][0], t[1][1] - t[0][1], t[1][2] - t[0][2]];
    let v = [t[2][0] - t[0][0], t[2][1] - t[0][1], t[2][2] - t[0][2]];
    let c = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
    (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt()
}

/// The fine band holding height `z`, clamped into `0..n`.
fn band_index(z: f64, z0: f64, step: f64, n: usize) -> usize {
    let f = ((z - z0) / step).floor();
    if f.is_finite() && f > 0.0 {
        (f as usize).min(n - 1)
    } else {
        0
    }
}

/// The farthest from the axis the facet reaches between the planes
/// `z = lo` and `z = hi`: the facet clipped to that slab (a convex
/// polygon), its vertices' radii. 0 when the facet misses the slab.
fn clipped_reach(t: &Tri, lo: f64, hi: f64) -> f64 {
    let poly = clip(&clip(t, lo, true), hi, false);
    poly.iter().fold(0.0, |a, p| a.max(p[0].hypot(p[1])))
}

/// One Sutherland-Hodgman pass: keep the side `z >= c` (`keep_above`)
/// or `z <= c` (#369's lathe clearance clips with it too).
pub(crate) fn clip(poly: &[[f64; 3]], c: f64, keep_above: bool) -> Vec<[f64; 3]> {
    let inside = |p: &[f64; 3]| if keep_above { p[2] >= c } else { p[2] <= c };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for (i, p) in poly.iter().enumerate() {
        let q = &poly[(i + 1) % poly.len()];
        let (pi, qi) = (inside(p), inside(q));
        if pi {
            out.push(*p);
        }
        if pi != qi {
            let d = q[2] - p[2];
            let s = if d == 0.0 { 0.0 } else { ((c - p[2]) / d).clamp(0.0, 1.0) };
            out.push([p[0] + (q[0] - p[0]) * s, p[1] + (q[1] - p[1]) * s, c]);
        }
    }
    out
}

/// Merge adjacent bands until at most `cap` remain: each step joins
/// the pair whose joined cylinder adds the least volume, at the larger
/// radius - so every merge only grows the envelope. Equal neighbours
/// join first for free.
pub(crate) fn merge(mut b: Vec<HolderBand>, cap: usize) -> Vec<HolderBand> {
    let join = |x: &HolderBand, y: &HolderBand| HolderBand { above: x.above, len: (y.above + y.len) - x.above, r: x.r.max(y.r) };
    let cost = |x: &HolderBand, y: &HolderBand| {
        let j = join(x, y);
        j.r * j.r * j.len - x.r * x.r * x.len - y.r * y.r * y.len
    };
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i].r == b[i + 1].r {
            b[i] = join(&b[i], &b[i + 1]);
            b.remove(i + 1);
        } else {
            i += 1;
        }
    }
    while b.len() > cap.max(1) {
        let mut best = (f64::INFINITY, 0);
        for k in 0..b.len() - 1 {
            let c = cost(&b[k], &b[k + 1]);
            if c < best.0 {
                best = (c, k);
            }
        }
        let k = best.1;
        b[k] = join(&b[k], &b[k + 1]);
        b.remove(k + 1);
    }
    b
}

#[cfg(test)]
#[path = "holder_profile_tests.rs"]
mod tests;
