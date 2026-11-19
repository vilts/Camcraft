//! Swept-tool solids and their conservative lattice windows.
//!
//! Provenance: the original closed-form and convex-distance laws in
//! `field_cut`; solid evaluation and window bounds moved unchanged.

use super::field::cell_of;
use super::field_arc::ArcPiece;
use super::field_bull::{bull_closed, bull_dist, bull_tie};
use super::field_lathe::LatheSweep;
use super::field_solid::{add, dist_pt_para, dot, flat_tie, pose_dist, scale, sub, upright_flat_foot, RevKind};
use super::scan::dist_pt_seg3;
use crate::geometry::Point3;

/// Golden-section iterations: the interval shrinks by 4.6e-9 per run,
/// far under any cell the field can ask about.
const GS_ITERS: u32 = 32;

/// The golden-section contraction ratio, (sqrt(5) - 1) / 2.
const PHI: f64 = 0.6180339887498949;

/// The golden section's final bracket, as a fraction of t's unit
/// interval, with room to spare: PHI^GS_ITERS is ~2e-7.
const GS_RESIDUAL: f64 = 1e-6;

/// One chord's swept solid, prepped for the walk (`field_ray` reads
/// it too - the ray-entry layer is this layer's companion).
pub(crate) enum Sweep {
    /// The annular sector of a turning move (M13.1 owed): the nose
    /// center's chord in the (radius, station) half-plane, revolved
    /// about the lathe axis, dilated by the nose radius - the
    /// revolved stadium `turning::revolve`'s meridian law cuts.
    Lathe(LatheSweep),
    /// The capsule of a ball center path.
    Ball { c0: Point3, c1: Point3, r: f64 },
    /// The ball center path swept up the axis by `up` (the shank),
    /// dilated by `r`: a parallelogram's offset, closed form.
    Shanked { c0: Point3, c1: Point3, up: [f64; 3], r: f64 },
    /// The Minkowski sum of a tip path and a revolved profile.
    Revolve { a: Point3, b: Point3, kind: RevKind },
    /// One piece of an upright tool's XY arc, swept on the true circle
    /// (#413, `field_arc`).
    Arc(ArcPiece),
}

impl Sweep {
    /// Radius of a ball this solid fits in around any anchor point:
    /// the quick reject's slack. NOT the window's dilation - a
    /// spanning flank (M7.1 C) is a cylinder's bounding radius
    /// inflated into every axis, which would refuse payables.
    fn extent(&self) -> f64 {
        match self {
            Sweep::Lathe(l) => l.bound(),
            Sweep::Ball { r, .. } => *r,
            Sweep::Shanked { r, up, .. } => r + dot(*up, *up).sqrt(),
            Sweep::Revolve { kind, .. } => match kind {
                RevKind::Flat { r, flank } => r.hypot(*flank),
                RevKind::Taper { r, cot } => r.hypot(r * cot),
                RevKind::Drill { r, cot, flank } => r.hypot(flank.max(r * cot)),
                RevKind::Bull { r, flank, .. } => r.hypot(*flank),
            },
            Sweep::Arc(_) => 0.0, // its stadium is its own (`ArcPiece::stadium`)
        }
    }

    /// The solid's own reach: (lateral, along-axis). A vertical tool
    /// is r wide and flank tall - the honest box the window dilates
    /// by, axis component for axis component. The along-axis reach
    /// runs one way, from the tip UP the axis (M7.3: a flank never
    /// reaches below its own tip); the ball's path is its center
    /// path, so its solid is the path dilated by `r` and nothing more.
    pub(super) fn reach(&self) -> (f64, f64) {
        match self {
            // The revolved solid's reach is its own window (`window_of`
            // reads the annulus directly); the nose is its dilation.
            Sweep::Lathe(l) => (l.bound(), 0.0),
            Sweep::Ball { r, .. } => (*r, 0.0),
            Sweep::Shanked { r, up, .. } => (*r, dot(*up, *up).sqrt()),
            Sweep::Revolve { kind, .. } => match kind {
                RevKind::Flat { r, flank } => (*r, *flank),
                RevKind::Taper { r, cot } => (*r, r * cot),
                RevKind::Drill { r, cot, flank } => (*r, flank.max(r * cot)),
                RevKind::Bull { r, flank, .. } => (*r, *flank),
            },
            Sweep::Arc(p) => p.reach(),
        }
    }

    /// The path endpoints the solid rides (tip path, or center path
    /// for the ball).
    pub(super) fn path(&self) -> (Point3, Point3) {
        match self {
            Sweep::Ball { c0, c1, .. } | Sweep::Shanked { c0, c1, .. } => (*c0, *c1),
            Sweep::Revolve { a, b, .. } => (*a, *b),
            // The meridian chord at azimuth 0: one generator of the
            // solid, never its extent (the window and the stadium
            // test read the revolved solid, not this).
            Sweep::Lathe(l) => l.path(),
            Sweep::Arc(p) => p.path(),
        }
    }

    /// The lattice window this sweep can reach (`window_of`).
    pub(crate) fn window(&self, cell: f64, cap: f64, ax: [f64; 3]) -> [(i32, i32); 3] {
        window_of(self, cell, cap, ax)
    }

    /// Does the solid span the world above its tip - hold every point
    /// above any point it holds, up to height `z`? The monotone law
    /// `field_floor` reads a cut floor by (#637 E1): true only for an
    /// upright axis and a flank or shank that reaches `z` from the
    /// lowest anchor. The truncated taper, the bare ball and the
    /// lathe's revolved sweep are bounded.
    pub(crate) fn spans_above(&self, ax: [f64; 3], z: f64) -> bool {
        if ax != [0.0, 0.0, 1.0] {
            return false;
        }
        match self {
            Sweep::Lathe(_) | Sweep::Ball { .. } => false,
            Sweep::Shanked { c0, c1, up, .. } => c0.z.min(c1.z) + up[2] >= z,
            Sweep::Revolve { a, b, kind } => match kind {
                RevKind::Flat { flank, .. } | RevKind::Drill { flank, .. } | RevKind::Bull { flank, .. } => {
                    a.z.min(b.z) + flank >= z
                }
                RevKind::Taper { .. } => false,
            },
            Sweep::Arc(p) => p.spans_above(z),
        }
    }

    /// How far `dist` may sit ABOVE the true distance: the golden
    /// section's final bracket times the path length (the anchor
    /// moves `|b - a|` per unit t, and one pose's distance is
    /// 1-Lipschitz in its anchor); the capsule is closed-form.
    pub(crate) fn residual(&self) -> f64 {
        match self {
            Sweep::Ball { .. } | Sweep::Shanked { .. } | Sweep::Lathe(_) => 0.0,
            Sweep::Revolve { a, b, .. } => {
                let d = sub(*b, *a);
                dot(d, d).sqrt() * GS_RESIDUAL
            }
            Sweep::Arc(p) => p.residual(),
        }
    }

    /// A cheap LOWER bound of `dist` (a stadium, not the solid), used
    /// to skip window corners before the honest evaluation.
    pub(crate) fn stadium(&self, q: Point3) -> f64 {
        if let Sweep::Lathe(l) = self {
            return l.dist(q); // closed form already: its own bound
        }
        if let Sweep::Arc(p) = self {
            return p.stadium(q);
        }
        let (p0, p1) = self.path();
        dist_pt_seg3(q.x, q.y, q.z, p0, p1) - self.extent()
    }

    /// The distance from `q` to the swept solid: negative inside,
    /// and every evaluation of the convex 1-D minimum is an UPPER
    /// bound of the truth (the conservative half of the law above).
    pub(crate) fn dist(&self, ax: [f64; 3], q: Point3) -> f64 {
        match self {
            // The half-plane law (`field_solid`'s header): a solid of
            // revolution's nearest point lies in the query's own
            // meridian, so the 2-D stadium distance IS the 3-D
            // distance. A chord that crosses the axis (parting) is
            // revolved from its mirror too, so both signs of rho are
            // read and the nearer wins - exact, no sampling.
            Sweep::Lathe(l) => l.dist(q),
            // Upright only (`field_cut` builds no other): the axis is +Z.
            Sweep::Arc(p) => p.dist(q),
            Sweep::Ball { c0, c1, r } => dist_pt_seg3(q.x, q.y, q.z, *c0, *c1) - r,
            Sweep::Shanked { c0, c1, up, r } => dist_pt_para(q, *c0, sub(*c1, *c0), *up) - r,
            Sweep::Revolve { a, b, kind } => {
                let d = sub(*b, *a);
                if dot(d, d) == 0.0 {
                    return pose_dist(ax, *a, q, kind); // a station: one pose
                }
                if let Some(t) = upright_flat_foot(ax, *a, d, q, kind) {
                    // Exact, then ties settled by the column's law
                    // (M7.3, `flat_tie`): the flank is inclusive, the
                    // floor is not.
                    let foot = add(*a, scale(d, t));
                    return flat_tie(foot, q, kind, pose_dist(ax, foot, q, kind));
                }
                if let RevKind::Bull { r, corner, flank } = kind {
                    // Level and plunge moves in closed form, ties the
                    // column's way (`field_bull`'s law).
                    let at = |p: Point3| [p.x, p.y, p.z];
                    if let Some((t, h)) = bull_closed(ax, at(*a), d, at(q), *flank) {
                        let foot = add(*a, scale(d, t));
                        let (zl, rho) = (q.z - foot.z, (q.x - foot.x).hypot(q.y - foot.y));
                        return bull_tie(rho, zl, *r, *corner, h, bull_dist(rho, zl, *r, *corner, h));
                    }
                }
                // Golden section on the convex map t -> phi(q - (a + t*d)).
                let (mut lo, mut hi) = (0.0f64, 1.0f64);
                let mut c = hi - PHI * (hi - lo);
                let mut dd = lo + PHI * (hi - lo);
                let mut fc = pose_dist(ax, add(*a, scale(d, c)), q, kind);
                let mut fd = pose_dist(ax, add(*a, scale(d, dd)), q, kind);
                for _ in 0..GS_ITERS {
                    if fc < fd {
                        hi = dd;
                        dd = c;
                        fd = fc;
                        c = hi - PHI * (hi - lo);
                        fc = pose_dist(ax, add(*a, scale(d, c)), q, kind);
                    } else {
                        lo = c;
                        c = dd;
                        fc = fd;
                        dd = lo + PHI * (hi - lo);
                        fd = pose_dist(ax, add(*a, scale(d, dd)), q, kind);
                    }
                }
                fc.min(fd)
            }
        }
    }
}

/// The lattice window one sweep can reach: the path's AABB swept
/// along the tool axis by the solid's axial reach (one way: tip to
/// top, M7.3), then dilated by the lateral reach, the cap, and one
/// cell. A spanning flank reaches far up its axis and not one micron
/// sideways or below its tip; dilating every axis by the bounding
/// radius (the M7.1 B law), or the axial reach both ways (M7.1 C),
/// made a vertical tool's window as deep as it is tall.
fn window_of(sw: &Sweep, cell: f64, cap: f64, ax: [f64; 3]) -> [(i32, i32); 3] {
    if let Sweep::Lathe(l) = sw {
        return l.window(cell, cap); // the revolved stadium's own box
    }
    let (p0, p1) = sw.path();
    let (lat, axial) = sw.reach();
    let span = |lo: f64, hi: f64, u: usize| {
        let e = axial * ax[u];
        let pad = lat + cap + cell;
        (
            cell_of(lo.min(hi) + e.min(0.0) - pad, cell),
            cell_of(lo.max(hi) + e.max(0.0) + pad, cell),
        )
    };
    [
        span(p0.x, p1.x, 0),
        span(p0.y, p1.y, 1),
        span(p0.z, p1.z, 2),
    ]
}
