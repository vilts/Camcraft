//! A spun (revolved) profile (#93): the half-outline of a milling tool
//! or holder as (z, r) points from the tip upward, closed to the axis at
//! both ends, each segment a straight line or a circular arc - the side
//! view the holder catalogs and machine-tool handbooks draw, spun about
//! the spindle axis. Circle/chord geometry (sagitta, tangent polygon) is
//! elementary plane geometry. Our own design; no vendor source or file
//! consulted.
//!
//! Conservative by construction: an arc is flattened to points that
//! never fall inside the material - a convex arc (bulging away from the
//! axis) by its circumscribed tangent polygon, a concave one by chords
//! on the arc - so the bands read off the flattened outline are never
//! smaller than the drawn solid. A stepped (straight-segment) profile
//! reads exactly: one band per climbing segment at its larger end, a
//! zero-height spike folded into the band it touches.

use crate::holder_profile::{HolderBand, HolderProfile, MAX_BODY_HEIGHT, MAX_BODY_RADIUS};
use crate::tools::{Tool, ToolKind};

/// One vertex of the outline: `z` above the tip, `r` from the axis, and
/// the segment to the NEXT point: `arc` = 0 for a straight line, else
/// the signed radius of a circular arc through both points - positive
/// bulges away from the axis (convex), negative toward it (concave).
/// The last point's `arc` is never read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpunPoint {
    pub z: f64,
    pub r: f64,
    pub arc: f64,
}

impl SpunPoint {
    #[must_use]
    pub const fn line(z: f64, r: f64) -> Self {
        Self { z, r, arc: 0.0 }
    }
}

/// Most points one drawn profile carries.
pub const MAX_POINTS: usize = 64;
/// Shortest segment (mm) a drawn profile may have - a typo'd duplicate
/// point is refused, not silently drawn as a zero-length step.
pub const MIN_SEGMENT: f64 = 0.01;
/// Facets per flattened arc.
const ARC_STEPS: usize = 16;
/// Slack (mm) on the closing and monotonic checks: float dust only.
const EPS: f64 = 1e-9;

/// Why a drawn profile is refused. Point numbers are 1-based, as the
/// editor's table shows them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpunError {
    TooFew,
    TooMany,
    NonFinite(usize),
    Negative(usize),
    BelowTip(usize),
    OpenStart,
    OpenEnd,
    TouchesAxis(usize),
    Short(usize),
    ArcTooTight(usize),
    RunsBack(usize),
    Flat,
    TooLarge,
}

impl core::fmt::Display for SpunError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match *self {
            Self::TooFew => write!(f, "the profile needs at least 3 points (on the axis, out, back to the axis)"),
            Self::TooMany => write!(f, "the profile has more than {MAX_POINTS} points"),
            Self::NonFinite(i) => write!(f, "point {i} is not a finite number"),
            Self::Negative(i) => write!(f, "point {i} has a negative radius"),
            Self::BelowTip(i) => write!(f, "point {i} is below the tool tip (z < 0)"),
            Self::OpenStart => write!(f, "the profile must start on the axis (first point r = 0)"),
            Self::OpenEnd => write!(f, "the profile must end on the axis (last point r = 0)"),
            Self::TouchesAxis(i) => write!(f, "point {i} touches the axis between the ends - one solid only"),
            Self::Short(i) => write!(f, "the segment from point {i} is shorter than {MIN_SEGMENT} mm"),
            Self::ArcTooTight(i) => {
                write!(f, "the arc from point {i} has a radius smaller than half its chord")
            }
            Self::RunsBack(i) => write!(
                f,
                "the segment from point {i} runs back down the axis - a spun outline climbs from the tip to the top"
            ),
            Self::Flat => write!(f, "the profile has no length along the tool axis"),
            Self::TooLarge => write!(
                f,
                "the profile is larger than any tool a spindle carries ({MAX_BODY_RADIUS} mm from the axis, {MAX_BODY_HEIGHT} mm long)"
            ),
        }
    }
}

impl std::error::Error for SpunError {}

/// A validated spun profile.
#[derive(Clone, Debug, PartialEq)]
pub struct SpunProfile {
    points: Vec<SpunPoint>,
}

impl SpunProfile {
    /// Validate a drawn outline. Refuses by name: too few or too many
    /// points, a non-finite number, a negative radius, a point below the
    /// tip, an end off the axis, an interior point on it, a segment
    /// shorter than [`MIN_SEGMENT`], an arc tighter than its chord, an
    /// outline that runs back down the axis, no length, or one past the
    /// body size caps.
    pub fn new(points: Vec<SpunPoint>) -> Result<Self, SpunError> {
        let n = points.len();
        if n < 3 {
            return Err(SpunError::TooFew);
        }
        if n > MAX_POINTS {
            return Err(SpunError::TooMany);
        }
        for (i, p) in points.iter().enumerate() {
            if !(p.z.is_finite() && p.r.is_finite() && p.arc.is_finite()) {
                return Err(SpunError::NonFinite(i + 1));
            }
            if p.r < 0.0 {
                return Err(SpunError::Negative(i + 1));
            }
            if p.z < 0.0 {
                return Err(SpunError::BelowTip(i + 1));
            }
        }
        if points[0].r > EPS {
            return Err(SpunError::OpenStart);
        }
        if points[n - 1].r > EPS {
            return Err(SpunError::OpenEnd);
        }
        if let Some(i) = points[1..n - 1].iter().position(|p| p.r <= EPS) {
            return Err(SpunError::TouchesAxis(i + 2));
        }
        for (i, w) in points.windows(2).enumerate() {
            let len = (w[1].z - w[0].z).hypot(w[1].r - w[0].r);
            if len < MIN_SEGMENT {
                return Err(SpunError::Short(i + 1));
            }
            if w[0].arc != 0.0 && w[0].arc.abs() < len * 0.5 - EPS {
                return Err(SpunError::ArcTooTight(i + 1));
            }
            let seg = flatten(w[0], w[1]);
            if seg.windows(2).any(|q| q[1].0 < q[0].0 - EPS) {
                return Err(SpunError::RunsBack(i + 1));
            }
            if seg.iter().any(|q| q.1 < -EPS) {
                return Err(SpunError::Negative(i + 1));
            }
        }
        let p = Self { points };
        let out = p.outline();
        let (lo, hi) = out.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| (a.min(q.0), b.max(q.0)));
        if hi - lo <= 1e-6 {
            return Err(SpunError::Flat);
        }
        if hi > MAX_BODY_HEIGHT || out.iter().any(|q| q.1 > MAX_BODY_RADIUS) {
            return Err(SpunError::TooLarge);
        }
        Ok(p)
    }

    /// The tool's cutter and shank as drawn from its numbers: exactly
    /// [`Tool::profile`] up to the shank's top, closed there to the
    /// axis. Display data straight off the record the laws read, so it
    /// takes no drawn-profile validation. `None` for the lathe insert
    /// kinds - turning tools are not spun profiles (they keep their own
    /// editor) - and for a draft with no outline.
    #[must_use]
    pub fn cutter_of(t: &Tool) -> Option<Self> {
        if is_lathe(t.kind) {
            return None;
        }
        let points: Vec<SpunPoint> =
            Tool { holder_len: 0.0, ..t.clone() }.profile().into_iter().map(|(z, r)| SpunPoint::line(z, r)).collect();
        let ok = points.len() >= 3 && points.iter().all(|p| p.z.is_finite() && p.r.is_finite());
        ok.then_some(Self { points })
    }

    /// The tool's holder as its numbers state it: the Ø `holder_d` ×
    /// `holder_len` cylinder on the shank's top - the very sums
    /// `HolderStack::cylinder` makes, so its bands read bit-identically.
    /// `None` for the lathe kinds and for a tool with no holder.
    #[must_use]
    pub fn holder_of(t: &Tool) -> Option<Self> {
        if is_lathe(t.kind) {
            return None;
        }
        let zs = t.flute_len + t.shank_len;
        let (hr, zh) = (t.holder_d * 0.5, zs + t.holder_len);
        Self::new(vec![SpunPoint::line(zs, 0.0), SpunPoint::line(zs, hr), SpunPoint::line(zh, hr), SpunPoint::line(zh, 0.0)])
            .ok()
    }

    /// A drawn outline of the stepped bands of a body envelope (an
    /// imported body re-drawn as a profile): each band a cylinder,
    /// joined by flat steps.
    #[must_use]
    pub fn from_bands(bands: &[HolderBand]) -> Option<Self> {
        let first = bands.first()?;
        let mut pts = vec![SpunPoint::line(first.above, 0.0)];
        for b in bands {
            pts.push(SpunPoint::line(b.above, b.r));
            pts.push(SpunPoint::line(b.above + b.len, b.r));
        }
        pts.push(SpunPoint::line(pts.last()?.z, 0.0));
        pts.dedup_by(|b, a| (a.z - b.z).hypot(a.r - b.r) < MIN_SEGMENT);
        Self::new(pts).ok()
    }

    #[must_use]
    pub fn points(&self) -> &[SpunPoint] {
        &self.points
    }

    /// The outline as a (z, r) polyline with every arc flattened
    /// conservatively (the module's law). The display mesh and the
    /// bands both read this one polyline, so they cannot disagree.
    #[must_use]
    pub fn outline(&self) -> Vec<(f64, f64)> {
        let mut out = Vec::with_capacity(self.points.len() + ARC_STEPS);
        for w in self.points.windows(2) {
            let seg = flatten(w[0], w[1]);
            let skip = usize::from(!out.is_empty());
            out.extend(seg.into_iter().skip(skip));
        }
        out
    }

    /// The contiguous bands the stack laws sweep: one per climbing
    /// segment of the outline at its larger radius. A flat (zero-height)
    /// step carries no length of its own, so its larger radius is folded
    /// into a band touching its height when none already reaches it - a
    /// plain step is held by the bands either side, but a flange drawn
    /// out and back at one z (at the top, the foot or mid-body) would
    /// otherwise fall out of the envelope. Never smaller than the
    /// outline: every outline radius is inside a band at its height.
    #[must_use]
    pub fn bands(&self) -> Vec<HolderBand> {
        let out = self.outline();
        let mut bands: Vec<HolderBand> = out
            .windows(2)
            .filter(|w| w[1].0 > w[0].0)
            .map(|w| HolderBand { above: w[0].0, len: w[1].0 - w[0].0, r: w[0].1.max(w[1].1) })
            .collect();
        for w in out.windows(2).filter(|w| w[1].0 <= w[0].0) {
            let (z, r) = (w[0].0.max(w[1].0), w[0].1.max(w[1].1));
            let touches = |b: &HolderBand| b.above - EPS <= z && z <= b.above + b.len + EPS;
            if bands.iter().any(|b| touches(b) && b.r >= r) {
                continue;
            }
            // The band ending at z (below the step) first, else the one
            // starting there; `Flat` validation means one always exists.
            let below = bands.iter().position(|b| (b.above + b.len - z).abs() <= EPS);
            let pick = below.or_else(|| bands.iter().position(touches));
            if let Some(b) = pick.and_then(|i| bands.get_mut(i)) {
                b.r = b.r.max(r);
            }
        }
        bands
    }

    /// The largest radius on the outline - what the editor's summary
    /// reports as the body's diameter, and what the bands never fall
    /// under.
    #[must_use]
    pub fn max_radius(&self) -> f64 {
        self.outline().iter().fold(0.0, |m, q| m.max(q.1))
    }

    /// The body envelope the judging laws read (`HolderProfile`): these
    /// bands, merged only past `MAX_BANDS` (and merging only grows).
    #[must_use]
    pub fn holder_profile(&self) -> HolderProfile {
        HolderProfile::from_bands(self.bands())
    }
}

fn is_lathe(k: ToolKind) -> bool {
    matches!(k, ToolKind::Turning | ToolKind::Groove | ToolKind::Thread | ToolKind::Form)
}

/// The segment `a` → `b` as (z, r) points, both ends included: a line
/// is its two ends; an arc of signed radius `a.arc` is its circumscribed
/// tangent polygon when convex (the centre on the material side) and
/// its inscribed chords when concave - never inside the drawn solid.
pub(crate) fn flatten(a: SpunPoint, b: SpunPoint) -> Vec<(f64, f64)> {
    let (dz, dr) = (b.z - a.z, b.r - a.r);
    let len = dz.hypot(dr);
    if a.arc == 0.0 || len.is_nan() || len <= 0.0 || a.arc.abs() < len * 0.5 - EPS {
        return vec![(a.z, a.r), (b.z, b.r)];
    }
    let rad = a.arc.abs();
    // n: the chord's normal away from the material (outward for an
    // outline climbing the axis); v: the chord direction.
    let n = (-dr / len, dz / len);
    let v = (dz / len, dr / len);
    let half = (len * 0.5 / rad).clamp(-1.0, 1.0).asin();
    let s = a.arc.signum();
    let mid = ((a.z + b.z) * 0.5, (a.r + b.r) * 0.5);
    let back = rad * half.cos();
    // Convex (s > 0): the bulge is along +n, the centre behind the chord.
    let c = (mid.0 - s * back * n.0, mid.1 - s * back * n.1);
    let u = (s * n.0, s * n.1);
    let at = |th: f64, k: f64| (c.0 + k * (th.cos() * u.0 + th.sin() * v.0), c.1 + k * (th.cos() * u.1 + th.sin() * v.1));
    let step = 2.0 * half / ARC_STEPS as f64;
    let mut out = vec![(a.z, a.r)];
    if s > 0.0 {
        // Tangent lines at -half + k*step meet at the mid-angles, on the
        // circle of radius rad / cos(step / 2).
        let k = rad / (step * 0.5).cos();
        for i in 0..ARC_STEPS {
            out.push(at(-half + (i as f64 + 0.5) * step, k));
        }
    } else {
        for i in 1..ARC_STEPS {
            out.push(at(-half + i as f64 * step, rad));
        }
    }
    out.push((b.z, b.r));
    out
}

#[cfg(test)]
#[path = "spun_tests.rs"]
mod tests;

#[path = "spun_envelope.rs"]
mod envelope;
