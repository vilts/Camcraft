//! A lathe-held tool body read for the revolved-stock law (#369): the
//! turning tool frame a holder mesh is placed in by numbers, and, per
//! band along the spindle, the radial and off-plane spans the body
//! occupies - from which the least and the greatest distance from the
//! spindle axis the body can reach in that band follow for any tip
//! radius. Specced from the handbook turning-tool convention (X radial,
//! Z along the spindle), the surface-of-revolution reading of a spinning
//! part, textbook Sutherland-Hodgman clipping, and the elementary fact
//! that a linear function's extremes over a convex polygon sit at its
//! vertices. Our own design; no vendor source or file consulted.
//!
//! ## The turning tool frame (tip at the origin)
//!
//! - **+x radial**: away from the spindle axis, in the meridian half
//!   plane the holder stands in (the side its moves stand on - a front
//!   or a rear post alike; #1102). A rigid holder never flips through
//!   the axis, so +x keeps that direction while a tip runs past centre.
//! - **+z along the spindle**: the lathe's +Z, the way the turning
//!   station grows (from the chuck toward the free end facing faces).
//! - **+y = z x x**, normal to that plane. The part is a solid of
//!   revolution, so only |y| matters to any law here.
//! - **The tip** is the point the moves carry: an insert's or bar's
//!   nose centre, a blade's tip at its band centre.
//!
//! A body point (x, y, z) with the tip at signed radius X and station
//! S stands sqrt((X + x)^2 + y^2) from the spindle axis at station
//! S + z, where X is signed along the holder's own +x: positive on its
//! side, negative only for a tip run past centre. A caller whose frame
//! counts the other side positive passes -X (#1102).
//!
//! ## Conservative by construction
//!
//! Every facet is clipped exactly to every fine band it spans, and the
//! band takes the clipped polygon's least and greatest x, and its least
//! and greatest |y| (x and y are linear, so their extremes over the
//! convex clip sit at its vertices; |y| is linear on a polygon that does
//! not straddle y = 0, and 0 on one that does). For a body point in the
//! band, x lies in [x_lo, x_hi] and |y| in [y_lo, y_hi], so its distance
//! from the axis lies in [`Reach::radii`]: never a point outside it.
//! The pair is the box hull, not the body: it can only over-report.

use crate::holder_profile::{area2, clip, FINE_BAND, MAX_BODY_HEIGHT, MAX_FINE_BANDS};
use crate::lathe_insert_zone::{InsertZone, MAX_INSERT_REACH};

type Tri = [[f64; 3]; 3];

/// The farthest the tip may stand from the body's bounding box (mm):
/// twice the largest ISO turning insert's inscribed circle (25.4 mm).
/// A holder carries its insert; a tip farther out is a placement typo.
pub const MAX_TIP_GAP: f64 = 50.8;
/// The farthest a tip point may sit from the model origin (mm).
pub const MAX_TIP_COORD: f64 = 10_000.0;
/// Most two placement directions may stand off square (a cosine -
/// about 0.01 degree); within it the axis is squared onto the radial.
pub const SQUARE_TOL: f64 = 1.75e-4;
/// Float dust on every span, never a clearance.
const PAD: f64 = 1e-6;

/// How a lathe-held mesh sits in the turning tool frame: the model
/// direction that is +x (radial, away from the spindle), the model
/// direction that is +z (along the spindle), and the model point that
/// is the tip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LathePlacement {
    pub radial: [f64; 3],
    pub along: [f64; 3],
    pub tip: [f64; 3],
}

/// Why a placement or a mesh is no lathe body. Each names itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LatheBodyError {
    Direction,
    NotSquare,
    Tip,
    Empty,
    NonFinite,
    Degenerate,
    Flat,
    TooLarge,
    TipAway,
    /// #396: the insert zone is no finite box about the tip within one
    /// insert (the app names the finer reason first).
    ZoneInvalid,
    /// #396: nothing of the body stands outside its insert zone.
    ZoneHoldsAll,
}

impl core::fmt::Display for LatheBodyError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::Direction => "the lathe body's radial and spindle directions must be finite, non-zero vectors",
            Self::NotSquare => "the lathe body's radial and spindle directions are not square to each other (0.01 degree)",
            Self::Tip => "the lathe body's tip point must be finite and within 10 m of the model origin",
            Self::Empty => "the tool body has no facets",
            Self::NonFinite => "the tool body has non-finite coordinates",
            Self::Degenerate => "the tool body is degenerate - every facet has zero area",
            Self::Flat => "the lathe body has no length along the spindle - check the directions",
            Self::TooLarge => "the lathe body reaches farther than 2000 mm from its tip",
            Self::TipAway => "the tip stands more than 50.8 mm from the lathe body - a holder carries its insert; check the tip point",
            Self::ZoneInvalid => "the insert zone must be a finite box holding the tip, within 25.4 mm of it",
            Self::ZoneHoldsAll => "the insert zone holds the whole lathe body - the zone is the insert plate, never the holder",
        })
    }
}

impl std::error::Error for LatheBodyError {}

impl LathePlacement {
    /// The frame's unit axes in model coordinates, `[x, y, z]`: the
    /// radial direction, the spindle direction squared onto it, and
    /// their right-handed normal. Refused by name otherwise.
    pub fn basis(&self) -> Result<[[f64; 3]; 3], LatheBodyError> {
        let x = unit(self.radial).ok_or(LatheBodyError::Direction)?;
        let a = unit(self.along).ok_or(LatheBodyError::Direction)?;
        let c = dot(x, a);
        if c.abs() > SQUARE_TOL {
            return Err(LatheBodyError::NotSquare);
        }
        let z = unit([a[0] - c * x[0], a[1] - c * x[1], a[2] - c * x[2]]).ok_or(LatheBodyError::NotSquare)?;
        if !self.tip.iter().all(|v| v.is_finite() && v.abs() <= MAX_TIP_COORD) {
            return Err(LatheBodyError::Tip);
        }
        Ok([x, cross(z, x), z])
    }

    /// The model-frame facets placed in the turning tool frame.
    pub fn place(&self, tris: &[Tri]) -> Result<Vec<Tri>, LatheBodyError> {
        let [x, y, z] = self.basis()?;
        let t = self.tip;
        let map = |p: [f64; 3]| {
            let d = [p[0] - t[0], p[1] - t[1], p[2] - t[2]];
            [dot(d, x), dot(d, y), dot(d, z)]
        };
        Ok(tris.iter().map(|tri| [map(tri[0]), map(tri[1]), map(tri[2])]).collect())
    }
}

/// One fine band's spans; an empty band (no facet crosses it) has
/// `x_lo > x_hi`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatheBand {
    pub x_lo: f64,
    pub x_hi: f64,
    pub y_lo: f64,
    pub y_hi: f64,
}

impl LatheBand {
    const EMPTY: Self = Self { x_lo: f64::INFINITY, x_hi: f64::NEG_INFINITY, y_lo: f64::INFINITY, y_hi: 0.0 };

    fn occupied(&self) -> bool {
        self.x_lo <= self.x_hi
    }
}

/// The spans the body occupies over some length along the spindle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reach {
    pub x_lo: f64,
    pub x_hi: f64,
    pub y_lo: f64,
    pub y_hi: f64,
}

impl Reach {
    /// The least and greatest distance from the spindle axis these
    /// spans reach with the tip at radius `x_tip` signed along the
    /// holder's +x (the module's frame, #1102): |X + x| over
    /// the x span (0 when it crosses the axis), combined with the |y|
    /// span - y only ever adds distance, so the least |y| raises the
    /// floor and the greatest raises the ceiling.
    #[must_use]
    pub fn radii(&self, x_tip: f64) -> (f64, f64) {
        let (u0, u1) = (x_tip + self.x_lo, x_tip + self.x_hi);
        let near = if u0 <= 0.0 && u1 >= 0.0 { 0.0 } else { u0.abs().min(u1.abs()) };
        let far = u0.abs().max(u1.abs());
        (near.hypot(self.y_lo), far.hypot(self.y_hi))
    }
}

/// A lathe body's clearance description: contiguous fine bands along
/// the spindle from `z0`, each `step` long.
#[derive(Clone, Debug, PartialEq)]
pub struct LatheClearance {
    z0: f64,
    step: f64,
    bands: Vec<LatheBand>,
}

impl LatheClearance {
    /// The bands of `tris`, placed in the turning tool frame. Refuses
    /// by name: no facets, non-finite numbers, all facets zero-area, no
    /// length along the spindle, a body past the size cap, a tip far
    /// from the body.
    pub fn of(tris: &[Tri]) -> Result<Self, LatheBodyError> {
        Self::of_zoned(tris, None)
    }

    /// As [`Self::of`], leaving out whatever of the body lies inside
    /// the insert zone `zone` (#396) - every check above still reads the
    /// whole body. Refuses a zone that is no finite box about the tip
    /// within [`MAX_INSERT_REACH`], or one holding the whole body.
    pub fn of_zoned(tris: &[Tri], zone: Option<&InsertZone>) -> Result<Self, LatheBodyError> {
        if zone.is_some_and(|z| z.check([MAX_INSERT_REACH; 3]).is_err()) {
            return Err(LatheBodyError::ZoneInvalid);
        }
        if tris.is_empty() {
            return Err(LatheBodyError::Empty);
        }
        if !tris.iter().flatten().flatten().all(|v| v.is_finite()) {
            return Err(LatheBodyError::NonFinite);
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
            return Err(LatheBodyError::Degenerate);
        }
        if lo.iter().chain(hi.iter()).any(|v| v.abs() > MAX_BODY_HEIGHT) {
            return Err(LatheBodyError::TooLarge);
        }
        let (z0, z1) = (lo[2], hi[2]);
        if z1 - z0 <= 1e-6 {
            return Err(LatheBodyError::Flat);
        }
        let gap = (0..3).map(|k| (lo[k].max(0.0) + (-hi[k]).max(0.0)).powi(2)).sum::<f64>().sqrt();
        if gap > MAX_TIP_GAP {
            return Err(LatheBodyError::TipAway);
        }
        let h = z1 - z0;
        // h <= 4000 (both ends capped), so the ceil is a small positive
        // integer and the cast exact.
        let n = ((h / FINE_BAND).ceil().clamp(1.0, MAX_FINE_BANDS as f64)) as usize;
        let step = h / n as f64;
        let edge = |k: usize| if k >= n { z1 } else { z0 + step * k as f64 };
        let mut bands = vec![LatheBand::EMPTY; n];
        for t in tris {
            let pieces = match zone {
                None => vec![t.to_vec()],
                Some(z) => z.outside(t),
            };
            for poly in &pieces {
                band_poly(poly, &mut bands, z0, step, &edge);
            }
        }
        if zone.is_some() && !bands.iter().any(LatheBand::occupied) {
            return Err(LatheBodyError::ZoneHoldsAll);
        }
        for b in bands.iter_mut().filter(|b| b.occupied()) {
            *b = LatheBand { x_lo: b.x_lo - PAD, x_hi: b.x_hi + PAD, y_lo: (b.y_lo - PAD).max(0.0), y_hi: b.y_hi + PAD };
        }
        Ok(Self { z0, step, bands })
    }

    #[must_use]
    pub fn bands(&self) -> &[LatheBand] {
        &self.bands
    }

    /// Where the body starts along the spindle, from the tip.
    #[must_use]
    pub fn z_lo(&self) -> f64 {
        self.z0
    }

    /// Where it ends.
    #[must_use]
    pub fn z_hi(&self) -> f64 {
        self.z0 + self.step * self.bands.len() as f64
    }

    /// The fine band length.
    #[must_use]
    pub fn step(&self) -> f64 {
        self.step
    }

    /// The spans of every occupied band touching `[a, b]` (tip-relative
    /// z, ends inclusive), or `None` when the body has none there.
    #[must_use]
    pub fn over(&self, a: f64, b: f64) -> Option<Reach> {
        let n = self.bands.len();
        if n == 0 || a.is_nan() || b.is_nan() || a > b || b < self.z_lo() || a > self.z_hi() {
            return None;
        }
        let rel = |z: f64| (z - self.z0) / self.step;
        // A band whose edge sits exactly on `a` touches it too.
        let k0 = (rel(a).ceil() - 1.0).clamp(0.0, (n - 1) as f64) as usize;
        let k1 = rel(b).floor().clamp(0.0, (n - 1) as f64) as usize;
        let mut out: Option<Reach> = None;
        for band in self.bands.iter().take(k1 + 1).skip(k0).filter(|b| b.occupied()) {
            out = Some(match out {
                None => Reach { x_lo: band.x_lo, x_hi: band.x_hi, y_lo: band.y_lo, y_hi: band.y_hi },
                Some(r) => Reach {
                    x_lo: r.x_lo.min(band.x_lo),
                    x_hi: r.x_hi.max(band.x_hi),
                    y_lo: r.y_lo.min(band.y_lo),
                    y_hi: r.y_hi.max(band.y_hi),
                },
            });
        }
        out
    }

    /// Every occupied run of bands as (start, end, spans), tip-relative
    /// z, bottom-up; neighbours with identical spans are one run (a
    /// prismatic shank is one), which loses nothing.
    #[must_use]
    pub fn spans(&self) -> Vec<(f64, f64, Reach)> {
        let n = self.bands.len();
        let edge = |k: usize| if k >= n { self.z_hi() } else { self.z0 + self.step * k as f64 };
        let mut out: Vec<(f64, f64, Reach)> = Vec::new();
        let mut open = false; // whether the last run reaches the band before this one
        for (k, b) in self.bands.iter().enumerate() {
            if !b.occupied() {
                open = false;
                continue;
            }
            let r = Reach { x_lo: b.x_lo, x_hi: b.x_hi, y_lo: b.y_lo, y_hi: b.y_hi };
            match out.last_mut() {
                Some(last) if open && last.2 == r => last.1 = edge(k + 1),
                _ => out.push((edge(k), edge(k + 1), r)),
            }
            open = true;
        }
        out
    }

    /// Every number a law reads off the description, for a stamp.
    pub fn words(&self) -> impl Iterator<Item = f64> + '_ {
        [self.z0, self.step].into_iter().chain(self.bands.iter().flat_map(|b| [b.x_lo, b.x_hi, b.y_lo, b.y_hi]))
    }
}

/// Clip polygon `poly` into every fine band it spans and widen each
/// band's spans by what the clip holds.
fn band_poly(poly: &[[f64; 3]], bands: &mut [LatheBand], z0: f64, step: f64, edge: &impl Fn(usize) -> f64) {
    let n = bands.len();
    let (tz0, tz1) = poly.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| (l.min(p[2]), h.max(p[2])));
    let (k0, k1) = (index(tz0, z0, step, n), index(tz1, z0, step, n));
    for (k, b) in bands.iter_mut().enumerate().take(k1 + 1).skip(k0) {
        let clipped = clip(&clip(poly, edge(k), true), edge(k + 1), false);
        if clipped.is_empty() {
            continue;
        }
        let (mut neg, mut pos) = (false, false);
        for p in &clipped {
            b.x_lo = b.x_lo.min(p[0]);
            b.x_hi = b.x_hi.max(p[0]);
            b.y_hi = b.y_hi.max(p[1].abs());
            b.y_lo = b.y_lo.min(p[1].abs());
            neg |= p[1] <= 0.0;
            pos |= p[1] >= 0.0;
        }
        if neg && pos {
            b.y_lo = 0.0; // the clip straddles the tool's plane
        }
    }
}

/// The fine band holding `z`, clamped into `0..n`.
fn index(z: f64, z0: f64, step: f64, n: usize) -> usize {
    let f = ((z - z0) / step).floor();
    if f.is_finite() && f > 0.0 {
        (f as usize).min(n - 1)
    } else {
        0
    }
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(a: [f64; 3]) -> Option<[f64; 3]> {
    let n = dot(a, a).sqrt();
    (n.is_finite() && n > 1e-12).then(|| [a[0] / n, a[1] / n, a[2] / n])
}

#[cfg(test)]
#[path = "lathe_holder_tests.rs"]
mod tests;
