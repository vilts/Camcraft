//! The tool meridian (#3367): the cutting outline rho(z) of a rotating
//! cutter, tip up, as lines and circular arcs, and its split into convex
//! solids of revolution - elementary plane geometry (chord sag
//! R (1 - cos(theta / 2))) and convexity of a revolved concave profile
//! (Boyd and Vandenberghe, Convex Optimization, 3.2.5). The point form
//! is the spun profile's ([`SpunPoint`]); the tip shapes are the
//! machine-tool handbook side views [`Tool::profile`] draws. Our own
//! design; no vendor source or file consulted.
//!
//! One shape for every consumer: the solid law, the simulation and the
//! generators read a tool through its meridian (later sub-issues of
//! #3351). A segment that is a pilot bearing band is flagged
//! non-cutting; it still occupies space.

use crate::spun::SpunPoint;
use crate::tools::{Tool, ToolKind};
use crate::undercut;

#[path = "meridian_atoms.rs"]
mod atoms;
pub use atoms::{AtomShape, Bound, MeridianAtom, MAX_SPLIT};

/// Shortest segment (mm) a meridian may have: under it, the segment is
/// a duplicated point, not a shape.
pub const MIN_LEN: f64 = 1e-6;
/// Slack (mm) on the order and fold checks: float dust only.
pub(crate) const EPS: f64 = 1e-9;
/// Largest coordinate or radius of an arc (mm). Circle calculations
/// square these values; this limit leaves room for their sums and products.
pub const MAX_ARC_COORD: f64 = 1e150;

/// Why a meridian is refused. Point numbers are 1-based; a segment is
/// numbered by the point it starts from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MeridianError {
    TooFew,
    NonFinite(usize),
    OutOfRange(usize),
    Negative(usize),
    Short(usize),
    ArcTooTight(usize),
    ArcFolds(usize),
    RunsBack(usize),
    FlagCount,
    BadSag,
    SagTooFine(usize),
    NotAMill(ToolKind),
    Unsound(String),
}

impl core::fmt::Display for MeridianError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooFew => write!(f, "a tool meridian needs at least 2 points"),
            Self::NonFinite(i) => write!(f, "meridian point {i} is not a finite number"),
            Self::OutOfRange(i) => write!(
                f,
                "meridian segment {i} exceeds the supported numeric range"
            ),
            Self::Negative(i) => write!(f, "meridian point or segment {i} has a negative radius"),
            Self::Short(i) => write!(
                f,
                "the meridian segment from point {i} is shorter than {MIN_LEN} mm"
            ),
            Self::ArcTooTight(i) => write!(
                f,
                "the meridian arc from point {i} has a radius smaller than half its chord"
            ),
            Self::ArcFolds(i) => write!(
                f,
                "the meridian arc from point {i} turns back along the tool axis"
            ),
            Self::RunsBack(i) => write!(
                f,
                "the meridian segment from point {i} runs back down the tool axis"
            ),
            Self::FlagCount => write!(f, "a tool meridian needs one cutting flag for each segment"),
            Self::BadSag => write!(
                f,
                "the sag limit of an arc split must be a finite number over 0 mm"
            ),
            Self::SagTooFine(i) => {
                write!(f, "the arc from meridian point {i} needs more than {MAX_SPLIT} parts for this sag limit")
            }
            Self::NotAMill(k) => write!(f, "a {k} has no milling meridian"),
            Self::Unsound(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for MeridianError {}

/// A validated meridian: points from the tip up, each segment a line or
/// an arc (the [`SpunPoint`] law: `arc` signed, positive bulges away
/// from the axis), with one cutting flag for each segment.
#[derive(Clone, Debug, PartialEq)]
pub struct Meridian {
    points: Vec<SpunPoint>,
    cutting: Vec<bool>,
}

impl Meridian {
    /// Validate an outline. Refuses by name: fewer than 2 points, a flag
    /// count that is not the segment count, a non-finite number, a
    /// negative radius (at a point or inside a concave arc), a segment
    /// under [`MIN_LEN`], an arc tighter than its chord, an arc that is
    /// not a function of z, a segment that runs back down the axis.
    pub fn new(points: Vec<SpunPoint>, cutting: Vec<bool>) -> Result<Self, MeridianError> {
        if points.len() < 2 {
            return Err(MeridianError::TooFew);
        }
        if cutting.len() + 1 != points.len() {
            return Err(MeridianError::FlagCount);
        }
        for (i, p) in points.iter().enumerate() {
            if !(p.z.is_finite() && p.r.is_finite() && p.arc.is_finite()) {
                return Err(MeridianError::NonFinite(i + 1));
            }
            if p.r < 0.0 {
                return Err(MeridianError::Negative(i + 1));
            }
        }
        for (i, w) in points.windows(2).enumerate() {
            check_segment(w[0], w[1], i + 1)?;
        }
        Ok(Self { points, cutting })
    }

    #[must_use]
    pub fn points(&self) -> &[SpunPoint] {
        &self.points
    }

    /// One flag for each segment: true where the edge cuts.
    #[must_use]
    pub fn cutting(&self) -> &[bool] {
        &self.cutting
    }
}

fn check_segment(a: SpunPoint, b: SpunPoint, i: usize) -> Result<(), MeridianError> {
    let (dz, dr) = (b.z - a.z, b.r - a.r);
    let len = dz.hypot(dr);
    if !len.is_finite() {
        return Err(MeridianError::OutOfRange(i));
    }
    if len < MIN_LEN {
        return Err(MeridianError::Short(i));
    }
    if dz < -EPS {
        return Err(MeridianError::RunsBack(i));
    }
    if a.arc == 0.0 {
        return Ok(());
    }
    if [a.z, a.r, a.arc, b.z, b.r]
        .iter()
        .any(|v| v.abs() > MAX_ARC_COORD)
    {
        return Err(MeridianError::OutOfRange(i));
    }
    if a.arc.abs() < len * 0.5 - EPS {
        return Err(MeridianError::ArcTooTight(i));
    }
    let g = atoms::ArcGeom::of(a, b).ok_or(MeridianError::ArcTooTight(i))?;
    // A convex arc lies on the circle's far half from the axis, a
    // concave one on its near half: only then is it a function of z.
    let side = |r: f64| {
        if g.convex() {
            r >= g.c.1 - EPS
        } else {
            r <= g.c.1 + EPS
        }
    };
    if !(side(a.r) && side(b.r)) {
        return Err(MeridianError::ArcFolds(i));
    }
    // The near half reaches rc - R where its z range holds the centre.
    if !g.convex() && a.z <= g.c.0 && g.c.0 <= b.z && g.c.1 - g.rad < -EPS {
        return Err(MeridianError::Negative(i));
    }
    Ok(())
}

impl Tool {
    /// The cutting meridian of a mill kind, from its numbers: the tip
    /// face, the ball, corner or point, and the flute side to
    /// `flute_len` (a lollipop's whole ball, to its pole). Each segment
    /// cuts. It agrees with [`Tool::profile`] on the cutting band, and
    /// its atoms are the solids the solid law builds for the cutter. A
    /// kind that is no mill, or numbers that make no tool, are refused
    /// by name.
    pub fn meridian(&self) -> Result<Meridian, MeridianError> {
        self.check_meridian_inputs()?;
        // #3369: a form mill's meridian is its drawn outline.
        if self.kind == ToolKind::FormMill {
            return crate::form_mill::meridian(self);
        }
        let (r, fl) = (self.diameter * 0.5, self.flute_len);
        let line = SpunPoint::line;
        // #3368: a flat or ball taper tip draws its own face and arc.
        if let Some(pts) = crate::taper::meridian_points(self, r, fl) {
            let n = pts.len().saturating_sub(1);
            return Meridian::new(pts, vec![true; n]);
        }
        let square = vec![line(0.0, 0.0), line(0.0, r), line(fl, r)];
        let pts = match self.kind {
            ToolKind::EndMill => square,
            ToolKind::BallNose => tip_arc(r, r, fl),
            ToolKind::BullNose => {
                let cr = self.corner_r;
                if cr.is_finite() && r.is_finite() && cr > r + EPS {
                    return Err(MeridianError::Unsound(format!(
                        "a bull-nose corner radius {cr} mm is over half its {} mm diameter",
                        self.diameter
                    )));
                }
                if cr > 0.0 {
                    tip_arc(r, cr, fl)
                } else {
                    square
                }
            }
            ToolKind::Drill | ToolKind::Taper => {
                let tan = (self.tip_angle.to_radians() * 0.5).tan();
                if self.tip_angle > 0.0 && tan.is_finite() && tan > 0.0 {
                    let top = (r / tan).min(fl);
                    let top_r = top * tan;
                    let mut p = vec![line(0.0, 0.0), line(top, top_r)];
                    // #3938: a flute that ends on the cone steps out to
                    // the full radius at its top, as `Tool::profile`
                    // draws it - the straight side above the cone, or
                    // the radial shoulder of a short flute (a face, no
                    // atom). Under MIN_LEN the cone already meets r.
                    if fl > top || r - top_r >= MIN_LEN {
                        p.push(line(fl, r));
                    }
                    p
                } else {
                    square
                }
            }
            ToolKind::TSlot | ToolKind::Keyseat | ToolKind::Dovetail | ToolKind::Lollipop => {
                if let Some(why) = undercut::refusal(self) {
                    return Err(MeridianError::Unsound(why));
                }
                match self.kind {
                    ToolKind::Lollipop => vec![
                        SpunPoint {
                            z: 0.0,
                            r: 0.0,
                            arc: r,
                        },
                        line(2.0 * r, 0.0),
                    ],
                    ToolKind::Dovetail => {
                        let cot = 1.0 / self.tip_angle.to_radians().tan();
                        vec![line(0.0, 0.0), line(0.0, r), line(fl, r - fl * cot)]
                    }
                    _ => square,
                }
            }
            k => return Err(MeridianError::NotAMill(k)),
        };
        let n = pts.len().saturating_sub(1);
        Meridian::new(pts, vec![true; n])
    }

    /// Check source dimensions before a min, comparison or fallback can
    /// hide a malformed value. Zero angle keeps the existing square tip.
    fn check_meridian_inputs(&self) -> Result<(), MeridianError> {
        if !matches!(
            self.kind,
            ToolKind::EndMill
                | ToolKind::BallNose
                | ToolKind::BullNose
                | ToolKind::Drill
                | ToolKind::Taper
                | ToolKind::TSlot
                | ToolKind::Keyseat
                | ToolKind::Dovetail
                | ToolKind::Lollipop
                | ToolKind::FormMill
        ) {
            return Err(MeridianError::NotAMill(self.kind));
        }
        for (i, value) in [self.diameter, self.flute_len].into_iter().enumerate() {
            if !value.is_finite() {
                return Err(MeridianError::NonFinite(i + 1));
            }
            if value < 0.0 {
                return Err(MeridianError::Negative(i + 1));
            }
            if value == 0.0 {
                return Err(MeridianError::Short(i + 1));
            }
        }
        if self.kind == ToolKind::BullNose {
            if !self.corner_r.is_finite() {
                return Err(MeridianError::NonFinite(1));
            }
            if self.corner_r < 0.0 {
                return Err(MeridianError::Negative(1));
            }
        }
        if matches!(
            self.kind,
            ToolKind::Drill | ToolKind::Taper | ToolKind::Dovetail
        ) {
            if !self.tip_angle.is_finite() {
                return Err(MeridianError::NonFinite(1));
            }
            if !(0.0..180.0).contains(&self.tip_angle) {
                return Err(MeridianError::Unsound(
                    "a tip angle must be at least 0 and less than 180 degrees".into(),
                ));
            }
        }
        // #3368: a flat or ball tip that breaks a tip rule draws nothing.
        if crate::taper::is_unread_tip(self)
            && let Some(why) = crate::taper::refusal(self)
        {
            return Err(MeridianError::Unsound(why));
        }
        if matches!(
            self.kind,
            ToolKind::BallNose | ToolKind::BullNose | ToolKind::Lollipop
        ) && [self.diameter, self.flute_len, self.corner_r]
            .iter()
            .any(|v| v.abs() > MAX_ARC_COORD)
        {
            return Err(MeridianError::OutOfRange(1));
        }
        Ok(())
    }
}

/// A rounded tip: the flat face to `r - cr`, the corner arc of radius
/// `cr` (centre `cr` above the tip), the flute side to `fl`. A flute
/// shorter than the corner ends the arc at `fl`, as the solid law does,
/// then steps out to `r` there.
fn tip_arc(r: f64, cr: f64, fl: f64) -> Vec<SpunPoint> {
    let flat = r - cr;
    let mut p = vec![SpunPoint::line(0.0, 0.0)];
    if flat > 0.0 {
        p.push(SpunPoint::line(0.0, flat));
    }
    if let Some(first) = p.last_mut() {
        first.arc = cr;
    }
    if fl >= cr {
        p.push(SpunPoint::line(cr, r));
        if fl > cr {
            p.push(SpunPoint::line(fl, r));
        }
    } else {
        let e = cr - fl;
        let top_r = flat + (cr * cr - e * e).max(0.0).sqrt();
        p.push(SpunPoint::line(fl, top_r));
        // #3938: the radial shoulder to the full radius at the flute
        // top, as `Tool::profile` draws it (a face, no atom).
        if r - top_r >= MIN_LEN {
            p.push(SpunPoint::line(fl, r));
        }
    }
    p
}

#[cfg(test)]
#[path = "meridian_tests.rs"]
mod tests;
