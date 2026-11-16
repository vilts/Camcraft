//! The entry, lead and engagement parameters of a level ladder (M11.3
//! B/C/D): how each entry motion is shaped (ramp angle and leg length,
//! ramp per level or from the top of the cut; helix radius range, pitch
//! or angle, direction; arc radius and sweep; plunge feed height), the
//! contour's lead geometry (lead-out shape, extensions, vertical lead-in
//! height, overlap, an axis-aligned lead start), and the feeds and the
//! engagement limit at entry. Every field's zero / default is the
//! pre-M11.3 behaviour, so a default `Approach` posts today's moves byte
//! for byte.
//!
//! Provenance: the standard entry/lead parameter vocabulary of published
//! CAM practice and machining handbooks (ramp angle and length, helical
//! entry radius and pitch, tangent-arc and linear leads with extensions,
//! closed-contour overlap, reduced entry feeds); the feed-vs-engagement
//! law is the material-removal-rate reading of radial engagement (MRR
//! ~ ae * vf, held at the limit's value) on top of the M2.1 / M11.4
//! engagement geometry. Our own model; no vendor material consulted.

use crate::machining2d::Lead;

/// Where the cutter radius is taken off the contour (M13.11). In the
/// computer, the generator walks the cutter-centre path and the control
/// runs it verbatim (every file before the option). In the control, the
/// post writes the PART contour under G41/G42 with the tool's offset
/// register, the control offsets by the radius it holds, and the
/// operator can shim a worn cutter at the pendant. The engage and cancel
/// must ride straight leads at least a radius long (RS-274 / ISO 6983
/// startup and cancel block behaviour); the generator refuses control
/// compensation by name where the leads cannot carry it. In wear
/// (#38), the generator walks the cutter-centre path at the nominal
/// radius as in computer, and the post writes THAT path under
/// G41/G42 Dn with the register holding only the wear delta (zero on
/// a new cutter) - the same engage / cancel leads and the same lead
/// law as in control, since the control builds and withdraws the
/// (small) offset over the same blocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Compensation {
    #[default]
    Computer,
    Control,
    Wear,
}

impl Compensation {
    /// Does the control take an offset (G41/G42) at all?
    #[must_use]
    pub fn in_control(self) -> bool {
        self != Compensation::Computer
    }

    /// The mode's name in refusals: "control" or "wear".
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Compensation::Computer => "computer",
            Compensation::Control => "control",
            Compensation::Wear => "wear",
        }
    }
}

/// The parameter set. `Approach::DEFAULT` is the identity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Approach {
    // --- B: entry shapes ---
    /// Maximum descent angle (degrees) of the entry Ramp; `0` keeps the
    /// one straight chord every pre-M11.3 ramp posted. Set, the ramp
    /// zig-zags along the first cut at no more than this angle.
    pub ramp_angle: f64,
    /// Longest leg (mm, XY) of any ramp - the chord or each zig-zag leg;
    /// `0` is no limit.
    pub ramp_max_len: f64,
    /// `true` ramps every level from the top of the cut (down through the
    /// levels already cleared); `false` - the default - from the plane
    /// above (per level).
    pub ramp_to_depth: bool,
    /// Smallest helix radius tried (mm) when the op's radius fits
    /// nowhere: the radius halves toward it before the plunge stands in.
    /// `0` tries the op's radius only.
    pub helix_min_radius: f64,
    /// Entry helix Z per turn (mm); `0` keeps the entry's law (one turn
    /// per helix diameter of drop).
    pub helix_pitch: f64,
    /// Entry helix descent angle (degrees); `0` uses the pitch. Set, the
    /// pitch is `2 pi r tan(angle)` at the radius flown.
    pub helix_angle: f64,
    /// Helix sense: `false` is counter-clockwise (G3), every pre-M11.3
    /// helix.
    pub helix_cw: bool,
    /// Arc entry radius (mm); `0` asks the kind's nominal (the tool
    /// diameter).
    pub arc_radius: f64,
    /// Arc entry sweep (degrees, at most 90); `0` sweeps whatever covers
    /// the drop. A shorter sweep covers less of the drop: the rest is an
    /// axial feed at the arc's start.
    pub arc_sweep: f64,
    /// The pocket's horizontal arc entry (M11.3, owed): the chosen entry
    /// descends at the arc's start - in the pocket's legal area - and a
    /// G17 arc at [`Self::arc_radius`] / [`Self::arc_sweep`] (a quarter
    /// turn when `0`) runs tangentially onto the first pass. `false` -
    /// the default - descends onto the pass start itself, as before.
    pub arc_xy: bool,
    /// Enter down holes an earlier drilling operation opened (#1449, the
    /// 2-D pocket only): where the application has PROVED a hole open
    /// under the tool's disc to a level's depth, the level's entry is a
    /// straight plunge down it instead of the chosen helix or ramp. The
    /// holes themselves ride the generator call, never this flag.
    /// `false` - the default - never reads a hole.
    pub predrilled: bool,
    /// Height above the material (mm) a plunge rapids down to before it
    /// feeds (held under the retract plane); also where an angle-bounded
    /// ramp starts on the first level. `0` feeds the whole way from the
    /// traverse plane, as every pre-M11.3 path did; the default is
    /// [`DEFAULT_PLUNGE_HEIGHT`].
    pub plunge_height: f64,
    // --- D: feeds and engagement at entry ---
    /// Feed of ramp, helix and arc descents (mm/min); `0` is the cutting
    /// feed.
    pub ramp_feed: f64,
    /// Feed of the entry's plunges (mm/min); `0` is the op's plunge feed.
    pub plunge_feed: f64,
    /// Lead-in feed as a percentage of the cutting feed (the contour's
    /// lead-in, a ladder's dive link onto the first cut); `100` is today.
    pub lead_feed_pct: f64,
    /// Radial engagement limit at entry, as a fraction of the tool
    /// diameter; `0` is off. A descent and the cut out of it engage the
    /// full diameter, so over the limit their feeds scale by the limit.
    pub max_engagement: f64,
    // --- C: the contour's lead geometry ---
    /// The lead-out's shape; `None` mirrors the lead-in (today).
    pub lead_out: Option<Lead>,
    /// Straight extension before the lead-in (mm), along its start.
    pub ext_in: f64,
    /// Straight extension after the lead-out (mm), along its end.
    pub ext_out: f64,
    /// The lead-in starts this far (mm) above the cut plane and descends
    /// along itself onto the cut; `0` is a flat lead.
    pub lead_height: f64,
    /// Overlap (mm) past the start on closed rings before the lead-out.
    pub overlap: f64,
    /// Prefer a ring start whose tangent is X- or Y-aligned (within
    /// [`Self::axis_tol`]): an arc entry there posts as a true G18/G19.
    pub axis_start: bool,
    /// The axis-alignment tolerance (degrees).
    pub axis_tol: f64,
    /// Where the cutter radius is compensated (M13.11, the contour's
    /// choice; every other ladder reads it as `Computer`).
    pub compensation: Compensation,
    /// The user's start point (#1448, model XY in mm): the closed 2D
    /// contour ring nearest it begins its cut at the pick's projection
    /// onto it, with the lead-in there - or the operation is refused by
    /// name. `None` - every earlier file - keeps the automatic seam.
    /// Only the closed contour reads it; every other kind refuses it.
    pub start_point: Option<crate::geo2d::Point2>,
    /// The user's exit point (#1473, model XY in mm): the closed 2D
    /// contour ring nearest it is cut all the way round, then on along
    /// itself to the pick's projection, where the lead-out leaves - or
    /// the operation is refused by name. `None` - every earlier file -
    /// leaves at the seam (or after the overlap).
    pub exit_point: Option<crate::geo2d::Point2>,
    /// Can the tool plunge axially into solid (M11.3, the user's safety
    /// call)? Not a saved parameter: the application lays the tool's own
    /// `center_cutting` over it for each generation, so a stored op
    /// always holds `true`. A tool that is not centre-cutting never dives
    /// into the level's solid - the helix, then the ramp, stand in, or
    /// the operation is refused by name.
    pub center_cutting: bool,
}

/// The note prefix of a non-centre-cutting fallback.
pub const NOT_CENTER_CUTTING: &str = "entry: the tool is not centre-cutting and may not plunge into solid";
/// The note that refuses an operation: no legal entry arm for a tool
/// that is not centre-cutting. Generators turn it into their error.
pub const CENTER_CUT_REFUSAL: &str = "entry refused: the tool is not centre-cutting";

/// The refusal a ladder raises when its path carries the
/// [`CENTER_CUT_REFUSAL`] note: `Some(the note)`.
#[must_use]
pub fn refusal(path: &crate::cl::Toolpath) -> Option<String> {
    path.notes.iter().find(|n| n.starts_with(CENTER_CUT_REFUSAL)).cloned()
}

/// The default axis-alignment tolerance, degrees.
pub const DEFAULT_AXIS_TOL: f64 = 1.0;

/// The default plunge height (M11.3, the user's hand-test: "why does the
/// path first plunge straight into the material?"): a plunge rapids to
/// 2 mm over the material under it - or the retract plane, if lower - and
/// feeds only that last stretch, never tens of millimetres of air.
pub const DEFAULT_PLUNGE_HEIGHT: f64 = 2.0;

impl Default for Approach {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The note an over-limit entry leaves, said once.
pub const ENGAGEMENT_NOTE: &str = "entry: a descent and the cut out of it engage the full tool diameter";

impl Approach {
    /// Every pre-M11.3 entry, lead and feed.
    pub const DEFAULT: Approach = Approach {
        ramp_angle: 0.0,
        ramp_max_len: 0.0,
        ramp_to_depth: false,
        helix_min_radius: 0.0,
        helix_pitch: 0.0,
        helix_angle: 0.0,
        helix_cw: false,
        arc_radius: 0.0,
        arc_sweep: 0.0,
        arc_xy: false,
        predrilled: false,
        plunge_height: DEFAULT_PLUNGE_HEIGHT,
        ramp_feed: 0.0,
        plunge_feed: 0.0,
        lead_feed_pct: 100.0,
        max_engagement: 0.0,
        lead_out: None,
        ext_in: 0.0,
        ext_out: 0.0,
        lead_height: 0.0,
        overlap: 0.0,
        axis_start: false,
        axis_tol: DEFAULT_AXIS_TOL,
        compensation: Compensation::Computer,
        start_point: None,
        exit_point: None,
        center_cutting: true,
    };

    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == Self::DEFAULT
    }

    /// Door sanity: `Err` names the field.
    pub fn validate(&self) -> Result<(), &'static str> {
        let len = |v: f64| v.is_finite() && (0.0..=10_000.0).contains(&v);
        let ang = |v: f64| v.is_finite() && (0.0..90.0).contains(&v);
        let feed = |v: f64| v.is_finite() && (0.0..=1.0e6).contains(&v);
        if !ang(self.ramp_angle) {
            return Err("the entry ramp angle must be 0 (one chord) or between 0 and 90 degrees");
        }
        if !len(self.ramp_max_len) {
            return Err("the maximum ramp length must be zero (no limit) or positive");
        }
        if !len(self.helix_min_radius) || !len(self.helix_pitch) {
            return Err("the helix minimum radius and pitch must be zero or positive");
        }
        if !ang(self.helix_angle) {
            return Err("the helix angle must be 0 (by pitch) or between 0 and 90 degrees");
        }
        if !len(self.arc_radius) {
            return Err("the arc entry radius must be zero (the tool diameter) or positive");
        }
        if !(self.arc_sweep.is_finite() && (0.0..=90.0).contains(&self.arc_sweep)) {
            return Err("the arc sweep must be 0 (cover the drop) or up to 90 degrees");
        }
        if !len(self.plunge_height) || !len(self.lead_height) || !len(self.overlap) {
            return Err("the plunge height, vertical lead-in height and overlap must be zero or positive");
        }
        if !len(self.ext_in) || !len(self.ext_out) {
            return Err("the lead extensions must be zero or positive");
        }
        if !feed(self.ramp_feed) || !feed(self.plunge_feed) {
            return Err("the entry feeds must be zero (the op's feed) or positive");
        }
        if !(self.lead_feed_pct.is_finite() && self.lead_feed_pct > 0.0 && self.lead_feed_pct <= 100.0) {
            return Err("the lead-in feed must be above 0 and at most 100 percent");
        }
        if !(self.max_engagement.is_finite() && (0.0..=1.0).contains(&self.max_engagement)) {
            return Err("the entry engagement limit must be 0 (off) up to 100 percent of the tool diameter");
        }
        if !(self.axis_tol.is_finite() && (0.0..=45.0).contains(&self.axis_tol)) {
            return Err("the axis tolerance must be between 0 and 45 degrees");
        }
        match self.lead_out {
            None | Some(Lead::None) => {}
            Some(Lead::Linear { length } | Lead::Perpendicular { length }) if len(length) => {}
            Some(Lead::Arc { radius } | Lead::Tangential { radius }) if radius > 0.0 && len(radius) => {}
            Some(_) => return Err("bad lead-out parameters"),
        }
        let coord = |v: f64| v.is_finite() && v.abs() <= 1.0e6;
        if self.start_point.is_some_and(|q| !(coord(q.x) && coord(q.y))) {
            return Err("the start point must be two finite coordinates within 1 km of the origin");
        }
        if self.exit_point.is_some_and(|q| !(coord(q.x) && coord(q.y))) {
            return Err("the exit point must be two finite coordinates within 1 km of the origin");
        }
        Ok(())
    }

    /// The feed scale the engagement limit asks of a full-diameter entry
    /// cut: the limit itself, or `1` when off.
    #[must_use]
    pub fn engagement_scale(&self) -> f64 {
        if self.max_engagement > 0.0 && self.max_engagement < 1.0 { self.max_engagement } else { 1.0 }
    }

    /// The feed of a ramp, helix or arc descent.
    #[must_use]
    pub fn descent_feed(&self, cut: f64) -> f64 {
        let f = if self.ramp_feed > 0.0 { self.ramp_feed } else { cut };
        let s = self.engagement_scale();
        if s < 1.0 { f * s } else { f }
    }

    /// The feed of an entry plunge.
    #[must_use]
    pub fn plunge(&self, op_plunge: f64) -> f64 {
        if self.plunge_feed > 0.0 { self.plunge_feed } else { op_plunge }
    }

    /// The feed of the lead-in / the cut out of a descent.
    #[must_use]
    pub fn lead_feed(&self, cut: f64) -> f64 {
        let mut f = cut;
        if self.lead_feed_pct < 100.0 {
            f *= self.lead_feed_pct / 100.0;
        }
        let s = self.engagement_scale();
        if s < 1.0 { f * s } else { f }
    }

    /// The named note when the engagement limit reduced the entry feeds.
    #[must_use]
    pub fn engagement_note(&self) -> Option<String> {
        let s = self.engagement_scale();
        (s < 1.0).then(|| {
            format!(
                "{ENGAGEMENT_NOTE} (100% of D) - over the {:.0}% limit, so their feeds are scaled to {:.0}%",
                s * 100.0,
                s * 100.0
            )
        })
    }

    /// The pitch of an entry helix at radius `r`: the angle's, the
    /// pitch, or `0` (the entry's law).
    #[must_use]
    pub fn entry_pitch(&self, r: f64) -> f64 {
        if self.helix_angle > 0.0 {
            let p = std::f64::consts::TAU * r * self.helix_angle.to_radians().tan();
            if p.is_finite() && p > 0.0 { p } else { 0.0 }
        } else {
            self.helix_pitch
        }
    }

    /// The helix radii to try, largest first: the op's, halving down to
    /// the minimum (inclusive), at most 16.
    #[must_use]
    pub fn helix_radii(&self, r: f64) -> Vec<f64> {
        let mut out = vec![r];
        let min = self.helix_min_radius;
        if !(min > 0.0 && min < r) {
            return out;
        }
        let mut q = r;
        while out.len() < 16 && q > min {
            q = (q * 0.5).max(min);
            out.push(q);
        }
        out
    }

    /// The arc entry's nominal radius: the user's, else `nominal`.
    #[must_use]
    pub fn arc_nominal(&self, nominal: f64) -> f64 {
        if self.arc_radius > 0.0 { self.arc_radius } else { nominal }
    }

    /// The arc's sweep cap in radians (`0` none).
    #[must_use]
    pub fn arc_sweep_rad(&self) -> f64 {
        self.arc_sweep.to_radians()
    }

    /// Does the entry Ramp zig-zag (an angle is set)?
    #[must_use]
    pub fn ramp_zigs(&self) -> bool {
        self.ramp_angle > 0.0
    }

    /// The point a ramp chord `leg -> e` starts at under the leg limit:
    /// `leg` itself, or the point `ramp_max_len` back from `e` along the
    /// chord (a sub-segment of the proved chord - proved with it).
    #[must_use]
    pub fn ramp_start(&self, leg: crate::geo2d::Point2, e: crate::geo2d::Point2) -> crate::geo2d::Point2 {
        let (dx, dy) = (leg.x - e.x, leg.y - e.y);
        let l = dx.hypot(dy);
        if !(self.ramp_max_len > 0.0 && l.is_finite() && l > self.ramp_max_len) {
            return leg;
        }
        let t = self.ramp_max_len / l;
        let q = crate::geo2d::Point2::new(e.x + dx * t, e.y + dy * t);
        crate::geo2d::Point2::new(crate::cl::lattice(q.x), crate::cl::lattice(q.y))
    }
}
