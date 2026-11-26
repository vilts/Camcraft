//! The tool meridian's split into convex atoms (#3367): a line is a
//! frustum, an arc that bulges away from the axis a torus zone, an arc
//! that bulges toward it chord frusta (outer bound) or tangent frusta
//! (inner bound) under a sag limit - chord sag R (1 - cos(theta / 2)),
//! tangent stand-off R (1 / cos(theta / 2) - 1), elementary plane
//! geometry; a revolved concave profile is convex (Boyd and
//! Vandenberghe, Convex Optimization, 3.2.5). Our own design.

use super::{Meridian, MeridianError, EPS};
use crate::spun::SpunPoint;

/// Most frusta one concave arc splits into.
pub const MAX_SPLIT: usize = 4096;

/// Which side of a concave arc the split keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bound {
    /// Chord frusta: their union contains the arc's solid.
    Outer,
    /// Tangent frusta: their union stays inside the arc's solid.
    Inner,
}

/// The radius profile of one atom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AtomShape {
    /// rho goes linearly from `r0` at `z0` to `r1` at `z1`.
    Frustum { r0: f64, r1: f64 },
    /// rho = rc + sqrt(rad^2 - (z - zc)^2): a torus zone (a sphere zone
    /// when `rc` is 0).
    Zone { zc: f64, rc: f64, rad: f64 },
}

/// One convex solid of revolution about the tool axis over `[z0, z1]`
/// (its rho is concave in z), with the cutting flag of the segment it
/// comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeridianAtom {
    pub z0: f64,
    pub z1: f64,
    pub shape: AtomShape,
    pub cutting: bool,
}

impl MeridianAtom {
    /// The radius at height `z`, read at the nearer end outside the span.
    #[must_use]
    pub fn rho(&self, z: f64) -> f64 {
        let z = z.clamp(self.z0, self.z1);
        match self.shape {
            AtomShape::Frustum { r0, r1 } => {
                let h = self.z1 - self.z0;
                if h > 0.0 {
                    r0 + (r1 - r0) * ((z - self.z0) / h)
                } else {
                    r0.max(r1)
                }
            }
            AtomShape::Zone { zc, rc, rad } => {
                let e = z - zc;
                rc + (rad * rad - e * e).max(0.0).sqrt()
            }
        }
    }
}

/// The circle of an arc segment: the [`SpunPoint`] law, as the spun
/// profile reads it (the minor arc; the centre behind the chord for a
/// convex arc, in front of it for a concave one).
pub(super) struct ArcGeom {
    /// Centre `(z, r)`.
    pub c: (f64, f64),
    pub rad: f64,
    /// Half the arc's angle.
    pub half: f64,
    s: f64,
    u: (f64, f64),
    v: (f64, f64),
}

impl ArcGeom {
    pub fn of(a: SpunPoint, b: SpunPoint) -> Option<Self> {
        let (dz, dr) = (b.z - a.z, b.r - a.r);
        let len = dz.hypot(dr);
        let rad = a.arc.abs();
        if a.arc == 0.0 || len.is_nan() || len <= 0.0 || !rad.is_finite() || rad < len * 0.5 - EPS {
            return None;
        }
        let n = (-dr / len, dz / len);
        let v = (dz / len, dr / len);
        let half = (len * 0.5 / rad).clamp(-1.0, 1.0).asin();
        let s = a.arc.signum();
        let back = rad * half.cos();
        let mut c = (
            a.z + dz * 0.5 - s * back * n.0,
            a.r + dr * 0.5 - s * back * n.1,
        );
        // An end at the centre's radius is the circle's z extreme, where
        // rho = rc + sqrt(rad^2 - e^2) turns float dust in the centre into
        // a visible error (1e-16 in, 1e-8 out). Snap the centre there: the
        // move is under the 1e-9 slack, and a corner, a ball's tip or a
        // lollipop's poles then read exact.
        let near = |p: SpunPoint| (p.r - c.1).abs() <= EPS * rad.max(1.0);
        if near(a) {
            c = (a.z + rad, a.r);
        } else if near(b) {
            c = (b.z - rad, b.r);
        }
        Some(Self {
            c,
            rad,
            half,
            s,
            u: (s * n.0, s * n.1),
            v,
        })
    }

    pub fn convex(&self) -> bool {
        self.s > 0.0
    }

    /// The point at angle `th` from the arc's middle, `k` from the centre.
    fn at(&self, th: f64, k: f64) -> (f64, f64) {
        let (c, s) = (th.cos(), th.sin());
        (
            self.c.0 + k * (c * self.u.0 + s * self.v.0),
            self.c.1 + k * (c * self.u.1 + s * self.v.1),
        )
    }
}

impl Meridian {
    /// The convex atoms bottom-up. Lines and convex arcs are exact; a
    /// concave arc splits to `bound` with each part within `sag` mm of
    /// the arc. A flat (zero-height) segment is a face, not an atom.
    /// Refuses a sag limit that is not a finite number over 0, and one
    /// that needs more than [`MAX_SPLIT`] parts.
    pub fn atoms(&self, bound: Bound, sag: f64) -> Result<Vec<MeridianAtom>, MeridianError> {
        if !(sag.is_finite() && sag > 0.0) {
            return Err(MeridianError::BadSag);
        }
        let mut out = Vec::with_capacity(self.points.len());
        for (i, (w, &cutting)) in self.points.windows(2).zip(&self.cutting).enumerate() {
            let (a, b) = (w[0], w[1]);
            match ArcGeom::of(a, b) {
                None => push_frustum(&mut out, (a.z, a.r), (b.z, b.r), cutting),
                Some(g) if g.convex() => {
                    if b.z - a.z > EPS {
                        let shape = AtomShape::Zone {
                            zc: g.c.0,
                            rc: g.c.1,
                            rad: g.rad,
                        };
                        out.push(MeridianAtom {
                            z0: a.z,
                            z1: b.z,
                            shape,
                            cutting,
                        });
                    }
                }
                Some(g) => {
                    let pts = split_concave(&g, a, b, bound, sag)
                        .ok_or(MeridianError::SagTooFine(i + 1))?;
                    for q in pts.windows(2) {
                        push_frustum(&mut out, q[0], q[1], cutting);
                    }
                }
            }
        }
        Ok(out)
    }
}

fn push_frustum(out: &mut Vec<MeridianAtom>, p: (f64, f64), q: (f64, f64), cutting: bool) {
    if q.0 - p.0 > EPS {
        out.push(MeridianAtom {
            z0: p.0,
            z1: q.0,
            shape: AtomShape::Frustum { r0: p.1, r1: q.1 },
            cutting,
        });
    }
}

/// The polyline of a concave arc's bound, both ends exact: chords on
/// the arc (outer), or the tangents at the split points, meeting at the
/// mid-angles (inner, clamped to the axis). `None` when the sag limit
/// needs more than [`MAX_SPLIT`] parts.
fn split_concave(
    g: &ArcGeom,
    a: SpunPoint,
    b: SpunPoint,
    bound: Bound,
    sag: f64,
) -> Option<Vec<(f64, f64)>> {
    let off = |n: usize| {
        let t = g.half / n as f64;
        match bound {
            Bound::Outer => g.rad * (1.0 - t.cos()),
            Bound::Inner => g.rad * (1.0 / t.cos() - 1.0),
        }
    };
    let n = (1..=MAX_SPLIT).find(|&n| off(n) <= sag)?;
    let step = 2.0 * g.half / n as f64;
    let mut pts = vec![(a.z, a.r)];
    match bound {
        Bound::Outer => pts.extend((1..n).map(|i| g.at(-g.half + i as f64 * step, g.rad))),
        Bound::Inner => {
            let k = g.rad / (step * 0.5).cos();
            pts.extend(
                (0..n)
                    .map(|i| g.at(-g.half + (i as f64 + 0.5) * step, k))
                    .map(|(z, r)| (z, r.max(0.0))),
            );
        }
    }
    pts.push((b.z, b.r));
    Some(pts)
}
