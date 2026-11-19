//! The field engine's exact XY-arc sweep (#413): the tool solid swept
//! along the true circular arc of a G2 / G3, read piece by piece, never
//! along inscribed chords. The field-engine twin of the column
//! engine's `dexel_arc` (#402). Split beside `field_sweep.rs` by the
//! 400-line law.
//!
//! Provenance: RS-274's helical interpolation (Z linear in the swept
//! angle), the circle-point distance law of plane geometry (a point's
//! horizontal distance to a circle point grows with the angle off its
//! own bearing), the convex tool solids `field_solid` reads, and
//! elementary convex analysis for the search law below; the search
//! itself is `dexel_arc`'s. Nothing from vendor material.
//!
//! ## The law here
//!
//! - **Pieces are windows, not geometry.** An arc is cut into pieces
//!   of at most a quarter turn whose chord keeps the caller's chord
//!   tolerance - exactly the old chord count, so the lattice windows
//!   and the cell ceiling refuse what they refused before - but every
//!   piece evaluates the TRUE arc. Every arc point of a piece lies
//!   within one sagitta of its chord, so the chord's window grown by
//!   that sagitta holds the piece, and the chord's stadium shrunk by
//!   it (plus the piece's Z change) stays a lower bound.
//! - **A level piece is exact, in closed form.** With the axis upright
//!   and Z constant, every tool solid is convex and symmetric about its
//!   axis, so a pose's distance grows with the horizontal distance to
//!   its anchor - and that grows with the angle off the query's own
//!   bearing. The nearest pose is the one at the bearing clamped into
//!   the piece: one evaluation, ties settled the column engine's way
//!   (`flat_tie`, `bull_tie`), and the flat floor's closed-form ray
//!   entry (`level_floor`) reads it too.
//! - **A helical piece is searched, conservatively.** Samples, then a
//!   golden section around the best one (`dexel_arc::piece_min`).
//!   Every value is a real pose evaluation, so the answer can only sit
//!   at or ABOVE the true distance (never a removal the tool did not
//!   make), and by the 1-Lipschitz anchor law it sits at most half a
//!   sample step of path above it - the `residual` the brick test
//!   allows for.
//! - **Only upright mill tools.** A tilted pose and the lathe's
//!   revolved footprint keep their chords (`field_cut`).

use super::dexel_arc::{piece_min, ArcSweep, SAMPLES};
use super::field_bull::{bull_dist, bull_tie};
use super::field_solid::{flat_tie, pose_dist, RevKind, ToolKind};
use super::field_sweep::Sweep;
use super::scan::dist_pt_seg3;

use crate::cl::{ArcMove, Move};
use crate::geometry::Point3;
use std::f64::consts::{FRAC_PI_2, TAU};

const UP: [f64; 3] = [0.0, 0.0, 1.0];

/// The solid riding one piece; anchors are tip positions for the
/// revolved profiles and ball CENTRES for the ball kinds.
pub(crate) enum ArcKind {
    Ball { r: f64 },
    Shanked { r: f64, shank: f64 },
    Rev(RevKind),
}

impl ArcKind {
    /// `RevKind` carries no `Clone`; the pieces each own a copy.
    fn copy(&self) -> ArcKind {
        match self {
            ArcKind::Ball { r } => ArcKind::Ball { r: *r },
            ArcKind::Shanked { r, shank } => ArcKind::Shanked { r: *r, shank: *shank },
            ArcKind::Rev(RevKind::Flat { r, flank }) => ArcKind::Rev(RevKind::Flat { r: *r, flank: *flank }),
            ArcKind::Rev(RevKind::Taper { r, cot }) => ArcKind::Rev(RevKind::Taper { r: *r, cot: *cot }),
            ArcKind::Rev(RevKind::Drill { r, cot, flank }) => {
                ArcKind::Rev(RevKind::Drill { r: *r, cot: *cot, flank: *flank })
            }
            ArcKind::Rev(RevKind::Bull { r, corner, flank }) => {
                ArcKind::Rev(RevKind::Bull { r: *r, corner: *corner, flank: *flank })
            }
        }
    }
}

/// One piece of an XY arc: the anchor runs the circle `(ox, oy, rad)`
/// from bearing `b0` through `span` radians in `dir`, Z linear from
/// `z0` to `z1`.
pub(crate) struct ArcPiece {
    ox: f64,
    oy: f64,
    rad: f64,
    b0: f64,
    dir: f64,
    span: f64,
    z0: f64,
    z1: f64,
    sag: f64,
    p0: Point3,
    p1: Point3,
    kind: ArcKind,
}

impl ArcPiece {
    fn anchor(&self, tau: f64) -> Point3 {
        let b = self.b0 + self.dir * tau;
        Point3::new(
            self.ox + self.rad * b.cos(),
            self.oy + self.rad * b.sin(),
            self.z0 + (self.z1 - self.z0) * (tau / self.span),
        )
    }

    fn level(&self) -> bool {
        self.z0 == self.z1
    }

    /// The piece parameter nearest `q`'s bearing (the clamped bearing).
    fn nearest_tau(&self, q: Point3) -> f64 {
        let (wx, wy) = (q.x - self.ox, q.y - self.oy);
        if wx == 0.0 && wy == 0.0 {
            return 0.0; // on the axis: every pose is as near
        }
        let u = (self.dir * (wy.atan2(wx) - self.b0)).rem_euclid(TAU);
        if u <= self.span {
            u
        } else if u - self.span < TAU - u {
            self.span
        } else {
            0.0
        }
    }

    fn pose(&self, a: Point3, q: Point3) -> f64 {
        match &self.kind {
            ArcKind::Ball { r } => (q.x - a.x).hypot(q.y - a.y).hypot(q.z - a.z) - r,
            ArcKind::Shanked { r, shank } => {
                dist_pt_seg3(q.x, q.y, q.z, a, Point3::new(a.x, a.y, a.z + shank)) - r
            }
            ArcKind::Rev(k) => pose_dist(UP, a, q, k),
        }
    }

    /// The distance from `q` to the piece's swept solid (the module
    /// law: exact when level, a conservative search when helical).
    pub(crate) fn dist(&self, q: Point3) -> f64 {
        if !self.level() {
            // The search evaluates the piece's ends exactly, so a point
            // ON an end pose's floor is a common tie: the conservative
            // hair (`flat_tie`'s outside law) keeps it material.
            let v = piece_min(&|t: f64| self.pose(self.anchor(t), q), 0.0, self.span);
            return v + 1e-12 + 1e-15 * v.abs();
        }
        let a = self.anchor(self.nearest_tau(q));
        let v = self.pose(a, q);
        match &self.kind {
            ArcKind::Rev(k @ RevKind::Flat { .. }) => flat_tie(a, q, k, v),
            ArcKind::Rev(RevKind::Bull { r, corner, flank }) => {
                let (rho, zl) = ((q.x - a.x).hypot(q.y - a.y), q.z - a.z);
                bull_tie(rho, zl, *r, *corner, *flank, bull_dist(rho, zl, *r, *corner, *flank))
            }
            _ => v,
        }
    }

    /// A level flat or bull piece's floor for the closed-form ray
    /// entry: the horizontal distance from `q` to the nearest anchor,
    /// the flat disc's radius, and the floor plane.
    pub(crate) fn level_floor(&self, q: Point3) -> Option<(f64, f64, f64)> {
        if !self.level() {
            return None;
        }
        let disc = match &self.kind {
            ArcKind::Rev(RevKind::Flat { r, .. }) => *r,
            ArcKind::Rev(RevKind::Bull { r, corner, .. }) => r - corner,
            _ => return None,
        };
        let a = self.anchor(self.nearest_tau(q));
        Some(((q.x - a.x).hypot(q.y - a.y), disc, self.z0))
    }

    /// (lateral, along-axis) reach of the tool itself.
    fn tool_reach(&self) -> (f64, f64) {
        match &self.kind {
            ArcKind::Ball { r } => (*r, 0.0),
            ArcKind::Shanked { r, shank } => (*r, *shank),
            ArcKind::Rev(RevKind::Flat { r, flank }) | ArcKind::Rev(RevKind::Bull { r, flank, .. }) => (*r, *flank),
            ArcKind::Rev(RevKind::Drill { r, cot, flank }) => (*r, flank.max(r * cot)),
            ArcKind::Rev(RevKind::Taper { r, cot }) => (*r, r * cot),
        }
    }

    /// The window's reach: the tool's, grown laterally by the sagitta
    /// that holds the piece around its chord.
    pub(crate) fn reach(&self) -> (f64, f64) {
        let (lat, axial) = self.tool_reach();
        (lat + self.sag, axial)
    }

    /// The piece's anchor chord (its window's path).
    pub(crate) fn path(&self) -> (Point3, Point3) {
        (self.p0, self.p1)
    }

    /// Does the piece's solid span the world above its anchors up to
    /// `z` (`Sweep::spans_above`, #637 E1)? The flank or shank reaches
    /// `z` from the lowest anchor; a ball or taper piece is bounded.
    pub(crate) fn spans_above(&self, z: f64) -> bool {
        match &self.kind {
            ArcKind::Ball { .. } | ArcKind::Rev(RevKind::Taper { .. }) => false,
            _ => self.z0.min(self.z1) + self.tool_reach().1 >= z,
        }
    }

    /// The bounding radius of the tool about its anchor.
    fn extent(&self) -> f64 {
        let (lat, axial) = self.tool_reach();
        lat.hypot(axial)
    }

    /// A lower bound of `dist`: the chord's stadium, shrunk by how far
    /// the arc can stand from its chord.
    pub(crate) fn stadium(&self, q: Point3) -> f64 {
        dist_pt_seg3(q.x, q.y, q.z, self.p0, self.p1) - self.sag - (self.z1 - self.z0).abs() - self.extent()
    }

    /// How far `dist` may sit above the truth: zero when level (closed
    /// form), half a sample step of anchor travel when helical.
    pub(crate) fn residual(&self) -> f64 {
        if self.level() {
            return 0.0;
        }
        let speed = self.rad.hypot((self.z1 - self.z0) / self.span);
        speed * self.span / (SAMPLES + 1) as f64 * 0.5
    }
}

/// The swept solids of an upright tool on the XY arc of `m`: its
/// pieces, plus a station at an end whose radius disagrees with the
/// arc's by rounding (`dexel_arc`'s end discs). `None` sends the move
/// back to chords: a lathe tool, or a degenerate arc `arc_chords`
/// reads as one straight chord.
pub(crate) fn arc_sweeps(m: &Move, arc: ArcMove, kind: &ToolKind, chord_tol: f64) -> Option<Vec<Sweep>> {
    let sw = ArcSweep::of(m, arc)?;
    let (lift, proto) = arc_kind(kind)?;
    let mk = || proto.copy();
    // `arc_chords`' piece budget (sagitta <= tol), capped at a quarter
    // turn so every piece sits within its chord's sagitta band.
    let tol = if chord_tol.is_finite() && chord_tol > 0.0 { chord_tol } else { 0.01 };
    let budget = (2.0 * (1.0 - (tol / sw.rad).clamp(-1.0, 1.0)).acos()).min(FRAC_PI_2);
    if budget.is_nan() || budget <= 0.0 {
        return None;
    }
    let n = ((sw.span / budget - 1e-9).ceil().clamp(1.0, 1e6) as usize).max(1);
    let step = sw.span / n as f64;
    let (za, zb) = (sw.from.z + lift, sw.to.z + lift);
    let mut out = Vec::with_capacity(n + 2);
    for j in 0..n {
        let (t0, t1) = (step * j as f64, step * (j + 1) as f64);
        let mut piece = ArcPiece {
            ox: sw.ox,
            oy: sw.oy,
            rad: sw.rad,
            b0: sw.a0 + sw.dir * t0,
            dir: sw.dir,
            span: step,
            z0: za + (zb - za) * (t0 / sw.span),
            z1: if j + 1 == n { zb } else { za + (zb - za) * (t1 / sw.span) },
            sag: sw.rad * (1.0 - (step * 0.5).cos()),
            p0: Point3::new(0.0, 0.0, 0.0),
            p1: Point3::new(0.0, 0.0, 0.0),
            kind: mk(),
        };
        piece.p0 = piece.anchor(0.0);
        piece.p1 = piece.anchor(step);
        out.push(Sweep::Arc(piece));
    }
    // An end off the arc's radius (rounding): its own station.
    for p in [sw.from, sw.to] {
        if ((p.x - sw.ox).hypot(p.y - sw.oy) - sw.rad).abs() > 1e-12 * sw.rad.max(1.0) {
            let a = Point3::new(p.x, p.y, p.z + lift);
            out.push(match mk() {
                ArcKind::Ball { r } => Sweep::Ball { c0: a, c1: a, r },
                ArcKind::Shanked { r, shank } => Sweep::Shanked { c0: a, c1: a, up: [0.0, 0.0, shank], r },
                ArcKind::Rev(kind) => Sweep::Revolve { a, b: a, kind },
            });
        }
    }
    Some(out)
}

/// The arc solid of an upright tool, with its anchor lift above the
/// tip (the ball kinds ride their centre); `None` for the lathe.
fn arc_kind(kind: &ToolKind) -> Option<(f64, ArcKind)> {
    Some(match kind {
        ToolKind::Ball(r) => (*r, ArcKind::Ball { r: *r }),
        ToolKind::Shanked { r, shank } => (*r, ArcKind::Shanked { r: *r, shank: *shank }),
        ToolKind::Flat { r, flank } => (0.0, ArcKind::Rev(RevKind::Flat { r: *r, flank: *flank })),
        ToolKind::Taper { r, cot } => (0.0, ArcKind::Rev(RevKind::Taper { r: *r, cot: *cot })),
        ToolKind::Drill { r, cot, flank } => (0.0, ArcKind::Rev(RevKind::Drill { r: *r, cot: *cot, flank: *flank })),
        ToolKind::Bull { r, corner, flank } => (0.0, ArcKind::Rev(RevKind::Bull { r: *r, corner: *corner, flank: *flank })),
        ToolKind::Lathe { .. } => return None,
    })
}

