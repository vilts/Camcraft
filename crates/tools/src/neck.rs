//! A necked tool's shank as the stack laws judge it (#852): the drawn
//! cutter outline (`cutter_profile`, #93 slice 2) read over the shank's
//! span, flute top to stickout, as stepped coaxial bands - but only where
//! it is narrower than the stated shank cylinder, never wider. A band
//! stack is the stepped-cylinder reading of a solid of revolution
//! (machine-tool handbooks); our own design, no vendor source consulted.
//!
//! The laws here:
//! - **No drawn neck, no change.** A tool with no drawn cutter, one that
//!   no longer agrees with the numbers, or one that narrows nowhere
//!   carries no neck: the shank is the one stated cylinder, the very
//!   band every law always read, so its verdicts are bit-identical.
//! - **Never wider than the shank, never narrower than the tool.** Each
//!   band is the drawn outline's own band over its span (never smaller
//!   than the outline, `SpunProfile::bands`), capped at the stated shank
//!   radius; a gap is filled at the shank radius; merging only grows.
//! - **Never narrower than a body beside it.** A holder body reaching
//!   down beside the shank widens the neck band it stands beside.
//! - **Refused shapes fall back.** A neck that does not run contiguously
//!   from the flute top to the stickout, or is non-finite, negative,
//!   too many bands or wider than the shank, is not this tool's: the
//!   stated cylinder stands, which can only over-report.

use crate::cutter_profile;
use crate::holder_profile::{HolderBand, HolderProfile, HolderStack, MAX_BANDS, merge};
use crate::spun::SpunProfile;
use crate::tools::{Tool, ToolGeom};

/// Float dust on the span checks (mm).
const EPS: f64 = 1e-6;

/// The neck `tool`'s drawn cutter `spun` states over the shank's span,
/// bottom-up, or empty - the stated shank stands - when the outline no
/// longer agrees with the numbers (`cutter_profile::agrees`), the tool
/// has no shank span, or the outline is nowhere narrower than the
/// shank.
#[must_use]
pub fn neck_bands(tool: &Tool, spun: &SpunProfile) -> Vec<HolderBand> {
    if cutter_profile::agrees(tool, spun).is_err() {
        return Vec::new();
    }
    let g = tool.geom();
    let (fl, top, sr) = (g.flute_len, g.flute_len + g.shank_len, g.shank_radius);
    if !([fl, top, sr].iter().all(|v| v.is_finite()) && fl >= 0.0 && top - fl > EPS && sr > 0.0) {
        return Vec::new();
    }
    let mut out: Vec<HolderBand> = Vec::new();
    let mut z = fl;
    for b in spun.bands() {
        if !(b.above.is_finite() && b.len.is_finite() && b.r.is_finite()) {
            return Vec::new();
        }
        let (lo, hi) = (b.above.max(fl), (b.above + b.len).min(top));
        if hi - lo <= EPS {
            continue;
        }
        // A span the drawn bands leave (the agreement slack at either
        // end) is judged at the stated shank.
        if lo > z + EPS {
            out.push(HolderBand { above: z, len: lo - z, r: sr });
        }
        let lo = lo.max(z);
        if hi - lo > EPS {
            out.push(HolderBand { above: lo, len: hi - lo, r: b.r.min(sr) });
            z = hi;
        }
    }
    if top > z + EPS {
        out.push(HolderBand { above: z, len: top - z, r: sr });
    }
    // The last band ends exactly on the stickout the laws stack on.
    if let Some(last) = out.last_mut() {
        last.len = top - last.above;
    }
    let out = merge(out, MAX_BANDS);
    if out.iter().all(|b| b.r >= sr) { Vec::new() } else { out }
}

/// The neck over `stored`'s shank span widened band-by-band by any
/// bands of `body` reaching strictly beside the shank (#2199).
///
/// Leaves `neck` untouched when `body` is absent or never enters the
/// interior of `(flute_len, flute_len + shank_len)` (a holder starting
/// at or past `shank_top` is bit-identical). Otherwise partitions
/// `[fl, top]` across the band edges of `neck` (or `stored`'s stated
/// shank) and `body`, taking the wider radius on every sub-span.
#[must_use]
pub fn shank_with_body(stored: &ToolGeom, body: Option<&HolderProfile>, neck: Vec<HolderBand>) -> Vec<HolderBand> {
    let Some(body) = body else { return neck };
    let (fl, top, sr) = (stored.flute_len, stored.flute_len + stored.shank_len, stored.shank_radius);
    if !([fl, top, sr].iter().all(|v| v.is_finite()) && fl >= 0.0 && top - fl > EPS && sr >= 0.0) {
        return neck;
    }
    let reach = |bands: &[HolderBand], lo: f64, hi: f64| {
        bands.iter().filter(|b| b.above < hi - 1e-9 && b.above + b.len > lo + 1e-9).fold(0.0_f64, |m, b| m.max(b.r))
    };
    let max_body = reach(body.bands(), fl, top);
    if max_body <= 0.0 {
        return neck;
    }
    let judged_sr = sr.max(max_body);
    let base = if neck.is_empty() {
        vec![HolderBand { above: fl, len: top - fl, r: sr }]
    } else {
        HolderStack { neck: neck.clone(), ..HolderStack::cylinder(stored) }.shank_bands(stored)
    };
    let mut cuts: Vec<f64> = Vec::with_capacity((base.len() + body.bands().len()) * 2);
    for b in base.iter().chain(body.bands().iter()) {
        for e in [b.above, b.above + b.len] {
            if e.is_finite() && e > fl + EPS && e < top - EPS {
                cuts.push(e);
            }
        }
    }
    cuts.sort_by(f64::total_cmp);
    let mut z = fl;
    let mut out: Vec<HolderBand> = Vec::with_capacity(cuts.len() + 1);
    for c in cuts.into_iter().chain(std::iter::once(top)) {
        if c - z <= EPS {
            continue;
        }
        let r = reach(&base, z, c).max(reach(body.bands(), z, c));
        out.push(HolderBand { above: z, len: c - z, r });
        z = c;
    }
    let mut out = merge(out, MAX_BANDS);
    if let Some(last) = out.last_mut() {
        last.len = top - last.above;
    }
    if out.iter().all(|b| b.r >= judged_sr - 1e-12) { Vec::new() } else { out }
}

impl HolderStack {
    /// This stack with the tool's drawn neck (#852, [`neck_bands`]);
    /// an empty neck is the stated shank.
    #[must_use]
    pub fn with_neck(self, neck: Vec<HolderBand>) -> Self {
        Self { neck, ..self }
    }

    /// The bands the laws sweep as the shank of `g`, bottom-up: the
    /// stated cylinder `(flute_len, shank_len, shank_radius)` - exactly
    /// the band every law always read - unless this stack carries a neck
    /// that fits `g` (contiguous from the flute top to the stickout,
    /// finite, never wider than the shank, at most [`MAX_BANDS`]); then
    /// the neck, each band widened to any body band beside it.
    #[must_use]
    pub fn shank_bands(&self, g: &ToolGeom) -> Vec<HolderBand> {
        let cylinder = vec![HolderBand { above: g.flute_len, len: g.shank_len, r: g.shank_radius }];
        if self.neck.is_empty() {
            return cylinder;
        }
        let (fl, top, sr) = (g.flute_len, g.flute_len + g.shank_len, g.shank_radius);
        let ok = |v: f64| v.is_finite() && v >= 0.0;
        if !(ok(fl) && ok(top) && ok(sr)) || self.neck.len() > MAX_BANDS {
            return cylinder;
        }
        let mut z = fl;
        for b in &self.neck {
            let fits = ok(b.above) && ok(b.len) && ok(b.r) && b.len > 0.0 && b.r <= sr + EPS && (b.above - z).abs() <= EPS;
            if !fits {
                return cylinder;
            }
            z = b.above + b.len;
        }
        if (z - top).abs() > EPS {
            return cylinder;
        }
        // A body band beside the shank (its foot below the stickout):
        // the neck there is never narrower than it.
        let beside = |lo: f64, hi: f64| {
            self.bands.iter().filter(|h| h.above < hi - 1e-9 && h.above + h.len > lo + 1e-9).fold(0.0_f64, |m, h| m.max(h.r))
        };
        let bands: Vec<HolderBand> = self
            .neck
            .iter()
            .map(|b| HolderBand { r: b.r.max(beside(b.above, b.above + b.len)).min(sr), ..*b })
            .collect();
        if bands.iter().all(|b| b.r >= sr) { cylinder } else { bands }
    }
}

#[cfg(test)]
#[path = "neck_tests.rs"]
mod tests;
