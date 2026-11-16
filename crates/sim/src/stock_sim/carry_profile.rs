//! The retained column tool profile, including its continuous shank.
//! Provenance: the existing dexel atoms and a convex cone/cylinder intersection.

use super::cut::SimOp;
use super::dexel::Dexel;
use super::dexel_arc::{ArcSweep, piece_min};
use crate::cl::Move;
use crate::geometry::Point3;

#[derive(Clone, Debug)]
pub(super) struct Profile {
    pub(super) motion: Move,
    pub(super) radius: f64,
    pub(super) cone: Option<f64>,
    ball: bool,
    corner: Option<f64>,
    chord_tol: f64,
}

impl Profile {
    pub(super) fn straight_flat(&self) -> bool {
        self.motion.arc.is_none() && !self.ball && self.cone.is_none() && self.corner.is_none()
    }

    pub(super) fn flat_span(&self, p: Point3, direction: [f64; 3]) -> Option<(f64, f64)> {
        let (a, b) = (self.motion.from, self.motion.to);
        if p.z <= a.z.min(b.z) {
            return None;
        }
        let mut ta: f64 = 0.0;
        let mut tb: f64 = 1.0;
        if b.z > a.z {
            tb = tb.min((p.z - a.z) / (b.z - a.z));
        }
        if b.z < a.z {
            ta = ta.max((p.z - a.z) / (b.z - a.z));
        }
        let at = |t: f64| Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, p.z);
        super::carry_capsule::capsule(at(ta), at(tb), p, direction, self.radius)
    }

    pub(super) fn new(op: &SimOp, motion: Move, chord_tol: f64) -> Self {
        Self { motion, radius: op.radius, cone: op.cone, ball: op.ball, corner: op.corner, chord_tol }
    }

    /// Read a ray parallel to the original cutter with the original atom.
    pub(super) fn floor(&self, x: f64, y: f64) -> f64 {
        let mut d = Dexel::from_grid(x - 0.5, y - 0.5, 1.0, 1, 1, f64::INFINITY, vec![f64::INFINITY]);
        match (self.cone, self.corner) {
            (Some(cot), _) => d.subtract_cone(&self.motion, self.radius, cot, self.chord_tol),
            (None, Some(c)) => d.subtract_bull(&self.motion, self.radius, c, self.chord_tol),
            _ if self.ball => d.subtract_ball(&self.motion, self.radius, self.chord_tol),
            _ => d.subtract(&self.motion, self.radius, self.chord_tol),
        }
        d.top_at(0)
    }

    /// A 1-Lipschitz implicit cone with an unlimited cylindrical shank.
    /// No internal cap exists where the cone reaches its full radius.
    pub(super) fn cone_distance(&self, p: Point3, cot: f64) -> (f64, f64) {
        let pose = |a: Point3| {
            let rho = (p.x - a.x).hypot(p.y - a.y);
            let norm = cot.hypot(1.0);
            (rho - self.radius).max(rho * (cot / norm) - (p.z - a.z) / norm)
        };
        let m = &self.motion;
        if let Some(sw) = m.arc.filter(|a| a.plane.is_xy()).and_then(|a| ArcSweep::of(m, a)) {
            let at = |t: f64| {
                Point3::new(
                    sw.ox + sw.rad * (sw.a0 + sw.dir * t).cos(),
                    sw.oy + sw.rad * (sw.a0 + sw.dir * t).sin(),
                    sw.from.z + (sw.to.z - sw.from.z) * t / sw.span,
                )
            };
            if sw.from.z == sw.to.z {
                let t = (sw.dir * ((p.y - sw.oy).atan2(p.x - sw.ox) - sw.a0)).rem_euclid(std::f64::consts::TAU);
                let mut d = pose(m.from).min(pose(m.to));
                if t <= sw.span {
                    d = d.min(pose(at(t)));
                }
                return (d, 0.0);
            }
            let n = (sw.span / std::f64::consts::FRAC_PI_2).ceil().max(1.0) as usize;
            let step = sw.span / n as f64;
            let d = (0..n)
                .map(|i| piece_min(&|t| pose(at(t)), i as f64 * step, (i + 1) as f64 * step))
                .fold(pose(m.from).min(pose(m.to)), f64::min);
            let residual = (sw.rad * step).hypot((sw.to.z - sw.from.z) / n as f64) / 16.0;
            (d, residual)
        } else {
            // Non-XY arcs follow the same chords as the column atom.
            let chords = super::field_cut::chords(m, self.chord_tol);
            let mut best = f64::INFINITY;
            let mut residual: f64 = 0.0;
            for (a, b) in chords {
                let at = |t: f64| Point3::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t);
                best = best.min(pose(a)).min(pose(b)).min(super::dexel_bull::golden(&|t| pose(at(t)), 0.0, 1.0));
                residual = residual.max((b.x - a.x).hypot(b.y - a.y).hypot(b.z - a.z) * 1e-6);
            }
            (best, residual)
        }
    }
}
