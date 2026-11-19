//! The move vocabulary: kinds, the lattice, arcs and their chords.
//!
//! Provenance: Move kinds map to the G0/G1 vocabulary of ISO 6983 /
//! RS-274 (public standards); the rapid/feed/plunge split is general
//! machining practice. Arc moves carry the exact circular segment
//! (center + direction); the arc is geometry, the kind stays the
//! motion class. Chord flattening - for verification, display and the
//! golden dump - is [`Move::chords`], the one sagitta law for the
//! whole codebase. Arcs turn in XY; M2.1 (helical entries) widened
//! the law so an arc's endpoints may differ in Z - a Z change along
//! an XY circle is a helix, which the timeline (`sim::path_len`,
//! exact arc length over Z) and the post (`G2/G3` blocks carry the Z
//! word) already read. Endpoints whose XY coincides are never an arc:
//! that is a plunge, and straight it stays. M11.2 A gave the arc its
//! plane ([`ArcPlane`]: XY / XZ / YZ, G17 / G18 / G19) - the
//! convention, direction rule and worked examples live in
//! [`super::plane`]; an XY arc is exactly the arc it always was.

use std::f64::consts::PI;

use crate::geo2d::Point2;
use crate::geometry::Point3;

use super::plane::{vertical_segments, ArcPlane};
use super::pose::normalize_axis;

/// G0 rapid, G1 feed, G1 with a plunge (reduced) feed - the three verbs a
/// 2.5-D path needs. The distinction drives display color now and post
/// vocabulary later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoveKind {
    Rapid,
    Feed,
    Plunge,
}

/// The circle an arc move traverses: its center in XY and the direction
/// of travel. The endpoints are the move's own `from`/`to`; together
/// they pin the sweep exactly (`from == to` never reaches a move - the
/// builder drops zero-sweep arcs, so fitted full circles are split by
/// `machining2d::fit_arcs` before they get here).
///
/// Centers stand on the 1 um lattice (see [`lattice`]): a fitted
/// circle's center is an ill-conditioned float, and the shortest
/// decimal of an off-lattice float need not parse back to the exact
/// bits it was written from - an arc cache could not then round-trip
/// byte-identically. Endpoints stay exact, so chaining and the sagitta
/// law are measured against the plan, never against the snap.
///
/// `plane` (M11.2 A) names the plane the circle lies in; `center` is
/// then that plane's word-order pair and `ccw` is G3 by the RS-274
/// view-from-+normal rule - [`super::plane`] states it all once.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArcMove {
    pub center: Point2,
    pub ccw: bool,
    pub plane: ArcPlane,
}

/// Snap to the project's fixed-point grid (microns): the same lattice the
/// golden writer, the geo2d bridge and the stamp digests quantize to.
/// Public since M4.1 R2 - the generator crates quantize through this one
/// function, so the lattice is part of the CL contract, not a private
/// helper of the motion model.
/// Non-finite values pass through untouched - refusing is the callers'
/// job, and the chord flattening already falls back around them.
pub fn lattice(v: f64) -> f64 {
    if v.is_finite() {
        (v * 1e6).round() / 1e6
    } else {
        v
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Move {
    pub kind: MoveKind,
    pub from: Point3,
    pub to: Point3,
    /// mm/min while in contact with material; None for rapids.
    pub feed: Option<f64>,
    /// None: a straight segment. Some: the circular arc from `from` to
    /// `to` about this center - in XY; the endpoints' Zs may differ
    /// (a helix, M2.1) or agree (the planar case every 2.5-D path so far
    /// has had). `chords` speaks XY; the Z ride between the endpoints is
    /// the timeline's and the post's to read.
    pub arc: Option<ArcMove>,
    /// The tool's pose through this move (M5.1 stage A): a unit vector
    /// from tip toward shank, or None when the move states no angle -
    /// which is the ordinary, prismatic case. The pose is the whole
    /// tool's, not the path's: a move with a tilted axis travels its
    /// tip along `from..to` while the assembly leans along the axis's
    /// direction. Conversion to and from the RS-274 letter angles the
    /// stream commands is stated once, in [`super::pose`].
    pub axis: Option<[f64; 3]>,
    /// The spindle's stated C angle through this move (issue #33,
    /// [`super::rotary`]): degrees, UNWRAPPED (a part turned three
    /// times reads 1080, never 0), or None - every move but a
    /// continuous C-axis live-tool cut. Some means the part turns on
    /// the machine's C line so that `to` and `axis`, both stated in
    /// the PART frame as always, land on the fixed live-tool station;
    /// the angle is the generator's statement, never re-derived from
    /// the point (an atan2 cannot carry the winding count).
    pub rotary: Option<f64>,
}

impl Move {
    pub fn new(kind: MoveKind, from: Point3, to: Point3, feed: Option<f64>) -> Self {
        Move {
            kind,
            from,
            to,
            feed,
            arc: None,
            axis: None,
            rotary: None,
        }
    }

    /// This move, traversing the circular segment about `center`
    /// (snapped to the arc center lattice - see [`ArcMove`]).
    pub fn with_arc(self, center: Point2, ccw: bool) -> Self {
        self.with_arc_in(center, ccw, ArcPlane::Xy)
    }

    /// This move, traversing the circular segment about `center` in
    /// `plane` (center in the plane's word order, snapped to the
    /// lattice like every center). No split happens here - the
    /// builder's [`super::PathBuilder::push_arc_in`] is the door that
    /// keeps vertical arcs Z-monotone.
    pub fn with_arc_in(mut self, center: Point2, ccw: bool, plane: ArcPlane) -> Self {
        self.arc = Some(ArcMove {
            center: Point2::new(lattice(center.x), lattice(center.y)),
            ccw,
            plane,
        });
        self
    }

    /// This move holding the tool axis `axis` (unit-normalized here;
    /// the pose convention is [`super::pose`]'s). A vector that states
    /// no direction - non-finite, or shorter than any real direction -
    /// is refused LOUDLY and the move stays prismatic: silently keeping
    /// a garbage pose would be the wrong answer the Prime Directive
    /// forbids, and silently dropping one a generator asked for would
    /// be the same crime wearing a safe face.
    pub fn with_axis(mut self, axis: [f64; 3]) -> Self {
        match normalize_axis(axis) {
            Some(a) => self.axis = Some(a),
            None => {
                tracing::warn!("cl: refusing a move axis of {axis:?}: not a direction");
                self.axis = None;
            }
        }
        self
    }

    pub fn is_finite(self) -> bool {
        self.from.is_finite()
            && self.to.is_finite()
            && self.feed.is_none_or(|f| f.is_finite() && f > 0.0)
            && self
                .arc
                .is_none_or(|a| a.center.x.is_finite() && a.center.y.is_finite())
            && self.axis.is_none_or(|a| a.iter().all(|x| x.is_finite()))
            && self.rotary.is_none_or(f64::is_finite)
    }

    /// This move stating the spindle's C angle `deg` (unwrapped
    /// degrees; see [`Move::rotary`]). A non-finite angle states no
    /// rotation: refused LOUDLY and the move keeps no C - the same
    /// law [`Move::with_axis`] holds for a garbage pose.
    #[must_use]
    pub fn with_rotary(mut self, deg: f64) -> Self {
        if deg.is_finite() {
            self.rotary = Some(deg);
        } else {
            tracing::warn!("cl: refusing a C angle of {deg}: not an angle");
            self.rotary = None;
        }
        self
    }

    /// Attach a pose VERBATIM - no normalization, no refusal. The
    /// door for callers that carry an already-stated value: the
    /// builder (whose `set_axis` normalized on the way in) and the
    /// project loader (whose stored components are the file's own
    /// lattice numbers, and a load that re-normalized them would
    /// break the save-load-save byte law). Anyone holding a raw
    /// vector uses [`Move::with_axis`], which normalizes and refuses
    /// loudly.
    #[must_use]
    pub fn attach_axis(mut self, axis: Option<[f64; 3]>) -> Self {
        self.axis = axis;
        self
    }

    /// The XY segments this move traverses: itself for a straight move,
    /// chord samples about `tol` for an arc (endpoints exact). Anything
    /// that measures along a path - the verifier's laws, the overlay,
    /// the golden dump - goes through here so all three agree on what
    /// an arc means geometrically.
    ///
    /// A vertical-plane (XZ / YZ) arc answers its 3-D chords projected
    /// to XY: a back-and-forth trace along one line, exactly what the
    /// tool's XY footprint is. Laws that need its Z read
    /// [`Move::segments3`].
    pub fn chords(&self, tol: f64) -> Vec<(Point2, Point2)> {
        let a = Point2::new(self.from.x, self.from.y);
        let b = Point2::new(self.to.x, self.to.y);
        match self.arc {
            None => vec![(a, b)],
            Some(arc) if arc.plane.is_xy() => arc_chords(a, b, arc, tol),
            Some(arc) => vertical_segments(self, arc, tol)
                .into_iter()
                .map(|(p, q)| (Point2::new(p.x, p.y), Point2::new(q.x, q.y)))
                .collect(),
        }
    }

    /// The 3-D straight segments this move traverses at sagitta `tol`:
    /// itself for a straight move; for an XY arc its chords with Z
    /// interpolated uniformly at each joint (the helix law the stock
    /// engines read, float for float); for an XZ / YZ arc its chords
    /// in its own plane, normal coordinate linear in the sweep, both
    /// endpoints exact.
    pub fn segments3(&self, tol: f64) -> Vec<(Point3, Point3)> {
        match self.arc {
            None => vec![(self.from, self.to)],
            Some(arc) if arc.plane.is_xy() => {
                let a2 = Point2::new(self.from.x, self.from.y);
                let b2 = Point2::new(self.to.x, self.to.y);
                let cs = arc_chords(a2, b2, arc, tol);
                let n = cs.len().max(1) as f64;
                let dz = self.to.z - self.from.z;
                cs.into_iter()
                    .enumerate()
                    .map(|(j, (ca, cb))| {
                        (
                            Point3::new(ca.x, ca.y, self.from.z + dz * (j as f64 / n)),
                            Point3::new(cb.x, cb.y, self.from.z + dz * ((j + 1) as f64 / n)),
                        )
                    })
                    .collect()
            }
            Some(arc) => vertical_segments(self, arc, tol),
        }
    }
}

/// Hard cap on chord pieces per arc: a tolerance small enough to ask
/// for more gets the cap (coarser chords) rather than a hang. With
/// endpoints always kept, the chord set still proves nothing about
/// finer features than the cap resolves - honest by construction, and
/// generation never approaches it (1024 chords is ~2.5 degrees each).
pub const MAX_ARC_CHORDS: usize = 1024;

/// Tessellate the arc from `a` to `b` about `arc.center`, travelling in
/// `arc.ccw`'s direction (sweep in (0, 2pi]), into chords whose sagitta
/// stays under `tol`. Radius interpolates linearly with sweep for a
/// synchronized thread ramp. Endpoints are exact; the last chord ends at `b`.
/// Degenerate input (a zero radius, a non-finite center, zero sweep)
/// falls back to the single straight chord - never an empty result,
/// because empty would silently erase motion.
pub fn arc_chords(a: Point2, b: Point2, arc: ArcMove, tol: f64) -> Vec<(Point2, Point2)> {
    let straight = vec![(a, b)];
    let o = arc.center;
    if !o.x.is_finite() || !o.y.is_finite() {
        return straight;
    }
    let ra = ((a.x - o.x).powi(2) + (a.y - o.y).powi(2)).sqrt();
    let rb = ((b.x - o.x).powi(2) + (b.y - o.y).powi(2)).sqrt();
    let r = ra.max(rb);
    if r <= f64::EPSILON {
        return straight;
    }
    let a0 = (a.y - o.y).atan2(a.x - o.x);
    let a1 = (b.y - o.y).atan2(b.x - o.x);
    let mut d = if arc.ccw { a1 - a0 } else { a0 - a1 };
    if !d.is_finite() {
        return straight;
    }
    d = d.rem_euclid(2.0 * PI);
    if d <= 0.0 {
        // From and to share a ray: the zero-sweep "arc" the builder
        // refuses. Hand back the chord (zero motion) and let the
        // caller's own chaining decide; nothing sane produces this.
        return straight;
    }
    let tol = if tol.is_finite() && tol > 0.0 { tol } else { 0.01 };
    // Sagitta r(1 - cos(step/2)) <= tol bounds the sweep per chord.
    let mut max_step = 2.0 * (1.0 - (tol / r).clamp(-1.0, 1.0)).clamp(-1.0, 1.0).acos();
    if ra != rb {
        // For p(theta) = r(theta) * (cos(theta), sin(theta)),
        // |p''| <= hypot(max_radius, 2 * |dr/dtheta|). Linear
        // interpolation error is at most |p''| * step^2 / 8.
        let curvature_bound = r.hypot(2.0 * (rb - ra).abs() / d);
        max_step = max_step.min((8.0 * tol / curvature_bound).sqrt());
    }
    let n = if max_step > 0.0 {
        // M4.2b: `d` arrives from atan2 and `max_step` from acos, so a
        // sweep that is a clean multiple of the chord budget lands on
        // the ceil boundary ± a couple of last bits - and which side
        // of it the platform's libm leaves you is not a machining
        // fact. Quantize before the decision: one chord fewer only
        // when the budget is met to a relative 1e-6 (a sagitta short
        // of tol by 1e-11 mm), never a silent miss of a real one.
        (lattice(d / max_step) + 1e-12).ceil() as usize
    } else {
        usize::MAX
    }
    .clamp(1, MAX_ARC_CHORDS);
    let mut pts = Vec::with_capacity(n + 1);
    pts.push(a);
    for j in 1..n {
        // Measured off `a0` in the travel direction, so rounding cannot
        // accumulate along the run.
        let th = a0 + if arc.ccw { d * j as f64 / n as f64 } else { -d * j as f64 / n as f64 };
        let radius = ra + (rb - ra) * j as f64 / n as f64;
        pts.push(Point2::new(o.x + radius * th.cos(), o.y + radius * th.sin()));
    }
    pts.push(b);
    pts.windows(2).map(|w| (w[0], w[1])).collect()
}
