//! Level transitions and links (M11.2 C/D): how a multi-level ladder
//! descends from one cut level to the next, and how it travels between
//! levels and between the passes of one level - every choice the user's.
//!
//! Provenance: the standard multi-depth transitions and link strategies
//! of published CAM practice and machining handbooks - plunge, linear
//! ramp at a maximum angle (zig-zag along the cut), helical
//! interpolation at a pitch, circular ramp in the vertical plane; links
//! that retract to the clearance plane, retract to the retract plane, or
//! stay down at feed within a maximum distance where the link is in
//! cleared air. RS-274 / ISO 6983 linear and circular interpolation. The
//! laws (what proves a keep-down link, what is named when it lifts) are
//! ours; no vendor material consulted (Clean-Room Protocol).

pub use crate::approach::{Approach, Compensation};
use crate::cl::{MoveKind, PathBuilder};
use crate::entry::Entry3D;
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// How a ladder travels from where one cut ends to where the next one
/// starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Link {
    /// Lift to the clearance plane, traverse, come back down.
    #[default]
    Clearance,
    /// Lift only to the retract (hop) plane - validated at or above the
    /// first cut level, so the traverse is in air over everything.
    Retract,
    /// Stay at depth and feed straight over to the next start - where
    /// the link is proved clear (inside the certified cutter-center
    /// area, crossing no planned path, no longer than
    /// [`Links::max_stay_down`]); anywhere else it lifts to the retract
    /// plane, and the path's notes say how often and why.
    KeepDown,
}

/// The descent and link choices of a level-ladder operation. One
/// struct for every ladder (pocket family, slot, circular, bore,
/// contour, 3-D roughing), so the operation page, the stamp and the
/// project file read one shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Links {
    /// The descent from one level to the next. `None` repeats the op's
    /// first-entry motion (its `entry` field) verbatim - the default,
    /// every file before the option.
    pub transition: Option<Entry3D>,
    /// Maximum descent angle (degrees) of a ramp transition: the ramp
    /// zig-zags along the level's first cut (on a contour: walks the
    /// ring) so it never descends steeper than this.
    pub ramp_angle: f64,
    /// Z per revolution (mm) of a helix transition; `0` keeps the
    /// entry's own law (one turn per tool diameter of drop, at least one
    /// turn). The radius is the op's helix radius.
    pub helix_pitch: f64,
    /// Between levels.
    pub level: Link,
    /// Between the passes (rings, rows) of one level.
    pub pass: Link,
    /// Longest keep-down link (mm, in XY); `0` puts no length limit on
    /// it - the proof of clearance still applies.
    pub max_stay_down: f64,
    /// The entry, lead and engagement parameters (M11.3 B/C/D) - the
    /// rest of the Lead in / Lead out section; [`Approach::DEFAULT`] is
    /// every file before them.
    pub approach: Approach,
}

/// The ramp transition's default maximum angle, degrees: a gentle
/// descent a flat end mill takes without center cutting.
pub const DEFAULT_RAMP_ANGLE: f64 = 3.0;

impl Default for Links {
    /// The planar ladders' legacy shape: the entry motion at every
    /// level, clearance between levels, the retract plane between
    /// passes.
    fn default() -> Self {
        Self::PLANAR
    }
}

impl Links {
    /// The planar (2-D) ladders' default - what every pre-M11.2 pocket,
    /// slot, circular, bore and contour posted.
    pub const PLANAR: Links = Links {
        transition: None,
        ramp_angle: DEFAULT_RAMP_ANGLE,
        helix_pitch: 0.0,
        level: Link::Clearance,
        pass: Link::Retract,
        max_stay_down: 0.0,
        approach: Approach::DEFAULT,
    };

    /// The 3-D roughing ladder's default: its pass links have always
    /// fed straight on where the chord is proved and hopped where not
    /// - keep-down with no length limit.
    pub const ROUGH3D: Links = Links { pass: Link::KeepDown, ..Links::PLANAR };

    /// The motion a level change takes: the transition, or the entry.
    #[must_use]
    pub fn arm(&self, entry: Entry3D) -> Entry3D {
        self.transition.unwrap_or(entry)
    }

    /// Does the ramp angle get read? A ramp transition reads it; so does
    /// a keep-down level link (the contour's stay-down ramp along its
    /// ring is the keep-down descent).
    #[must_use]
    pub fn reads_ramp_angle(&self) -> bool {
        self.transition == Some(Entry3D::Ramp) || self.level == Link::KeepDown
    }

    /// Door sanity, shared by every generator's `validate` and the
    /// operation layer: `Err` names the field.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.reads_ramp_angle()
            && !(self.ramp_angle.is_finite() && self.ramp_angle > 0.0 && self.ramp_angle < 90.0)
        {
            return Err("the ramp angle must be between 0 and 90 degrees");
        }
        if !(self.helix_pitch.is_finite() && self.helix_pitch >= 0.0) {
            return Err("the helix pitch must be zero (the entry's law) or positive");
        }
        if !(self.max_stay_down.is_finite() && self.max_stay_down >= 0.0) {
            return Err("the maximum stay-down distance must be zero (no limit) or positive");
        }
        self.approach.validate()
    }

    /// Is a keep-down link of XY length `len` within the limit?
    #[must_use]
    pub fn within_stay_down(&self, len: f64) -> bool {
        len.is_finite() && (self.max_stay_down <= 0.0 || len <= self.max_stay_down + 1e-9)
    }
}

/// Why a keep-down link lifted, tallied per ladder so a pocket with a
/// hundred passes says it once, with the first place it happened.
#[derive(Clone, Default)]
pub(crate) struct LinkTally {
    lifted: Vec<(&'static str, &'static str, usize, Point2, f64)>,
}

/// The plane a lift goes to by default: the retract plane.
pub(crate) const RETRACT_PLANE: &str = "retract plane";
/// #3822: the plane a failed link lifts to when the user chose Clearance.
pub(crate) const CLEARANCE_PLANE: &str = "clearance height";

/// The keep-down link was not proved clear.
pub(crate) const NOT_PROVED: &str = "not proved clear of the part and the planned cuts";
/// The keep-down link was longer than the stay-down limit.
pub(crate) const TOO_LONG: &str = "longer than the maximum stay-down distance";

impl LinkTally {
    /// A keep-down link from `at` (on plane `z`) lifted for `why`.
    pub(crate) fn lift(&mut self, why: &'static str, at: Point2, z: f64) {
        self.lift_to(why, RETRACT_PLANE, at, z);
    }

    /// The same, for a lift to the named `plane` (#3822).
    pub(crate) fn lift_to(&mut self, why: &'static str, plane: &'static str, at: Point2, z: f64) {
        match self.lifted.iter_mut().find(|(w, p, ..)| *w == why && *p == plane) {
            Some(slot) => slot.2 += 1,
            None => self.lifted.push((why, plane, 1, at, z)),
        }
    }

    /// One note per reason, onto the builder - in words a user reads
    /// on the status line (M11.3: what happened, why, and where).
    pub(crate) fn say(&self, b: &mut PathBuilder, what: &str) {
        for (why, plane, n, at, z) in &self.lifted {
            let plain = match *why {
                NOT_PROVED => "staying down would have crossed passes on that plane or left the area proved safe",
                TOO_LONG => "the link was longer than the maximum stay-down distance",
                other => other,
            };
            b.note(format!(
                "link: the tool lifted to the {plane} {n} time(s) instead of the keep-down {what} link - {plain}; it went back down at the next entry (first lift at ({:.3}, {:.3}), Z {z:.3})",
                at.x, at.y
            ));
        }
    }
}

/// The most out-and-back legs one zig-zag ramp may take: past this the
/// cut is too short for the angle and the descent substitutes, named.
pub const MAX_ZIG_LEGS: usize = 64;

/// The prefix of the polyline `path` (from `path[0]`) of arc length
/// `len`, ending on the exact point at that length.
fn prefix(path: &[Point2], len: f64) -> Vec<Point2> {
    let mut out = Vec::with_capacity(path.len());
    let Some(&p0) = path.first() else {
        return out;
    };
    out.push(p0);
    let mut s = 0.0;
    for w in path.windows(2) {
        let d = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
        if s + d >= len {
            let t = if d > 0.0 { (len - s) / d } else { 0.0 };
            out.push(Point2::new(w[0].x + (w[1].x - w[0].x) * t, w[0].y + (w[1].y - w[0].y) * t));
            return out;
        }
        s += d;
        out.push(w[1]);
    }
    out
}

/// Plan a zig-zag ramp that descends `dz` at no more than `angle_deg`
/// along the cut `path` (starting at `path[0]`) and returns to
/// `path[0]`: `n` out-and-back legs over the path's first stretch.
/// `None` when the path is too short for the angle within
/// [`MAX_ZIG_LEGS`], or a number is not usable. Each leg is at most
/// `max_leg` long (M11.3 B's maximum ramp length; `0` no cap): more
/// out-and-back legs over a shorter stretch of the same cut.
pub(crate) fn plan_zig_capped(path: &[Point2], dz: f64, angle_deg: f64, max_leg: f64) -> Option<(Vec<Point2>, usize)> {
    let tan = angle_deg.to_radians().tan();
    let avail: f64 = path.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
    if !(dz.is_finite() && dz > 0.0 && tan.is_finite() && tan > 0.0 && avail.is_finite() && avail > 1e-6) {
        return None;
    }
    let run = dz / tan;
    let reach = if max_leg > 0.0 && max_leg.is_finite() { avail.min(max_leg) } else { avail };
    let n = (run / (2.0 * reach)).ceil().max(1.0);
    if n > MAX_ZIG_LEGS as f64 {
        return None;
    }
    let leg = (run / (2.0 * n)).min(reach);
    let pre = prefix(path, leg);
    (pre.len() >= 2).then_some((pre, n as usize))
}

/// Post a planned zig-zag: from `pre[0]` at `z_from`, `n` times out
/// along `pre` and back, Z falling linearly in travelled length, landing
/// on `pre[0]` at `z_to` exactly. Every point lies on the cut itself,
/// so the cutter center never leaves the path the level law proved.
pub(crate) fn push_zig(b: &mut PathBuilder, pre: &[Point2], n: usize, z_from: f64, z_to: f64, feed: f64) {
    let lap: f64 = pre.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
    let total = 2.0 * lap * n as f64;
    if !(total.is_finite() && total > 0.0) || pre.len() < 2 {
        return;
    }
    let back: Vec<Point2> = pre.iter().rev().copied().collect();
    let mut s = 0.0;
    for k in 0..n {
        for (half, pts) in [pre, back.as_slice()].into_iter().enumerate() {
            for (j, w) in pts.windows(2).enumerate() {
                s += (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
                let last = k + 1 == n && half == 1 && j + 2 == pts.len();
                let z = if last { z_to } else { z_from - (z_from - z_to) * (s / total) };
                b.push(MoveKind::Feed, Point3::new(w[1].x, w[1].y, z), Some(feed));
            }
        }
    }
}

/// The most helix turns one transition may take before it substitutes.
pub const MAX_HELIX_TURNS: f64 = 1024.0;

/// Turns a helix descent of `span` takes: at `pitch` per turn when the
/// pitch is set, the entry's law (one turn per `2 rh` of drop) when it is
/// zero; at least one. `None` past [`MAX_HELIX_TURNS`] or on a nonsense
/// number.
#[must_use]
pub fn helix_turns(span: f64, rh: f64, pitch: f64) -> Option<f64> {
    let per = if pitch > 0.0 { pitch } else { rh * 2.0 };
    let t = (span / per).ceil().max(1.0);
    (t.is_finite() && per.is_finite() && per > 0.0 && t <= MAX_HELIX_TURNS).then_some(t)
}
