//! The tilted cone's pose on one edge segment (#1894): the chamfer
//! plane of the break, and the tool axes that lay one cone flank on it.
//!
//! The law. A symmetric chamfer of setback `s` on a wedge with in-face
//! directions `da`, `db` (unit, perpendicular to the edge tangent `t`)
//! has its chord from `e + s da` to `e + s db`, and its plane holds `t`
//! and the across direction `c = unit(db - da)`. Its outward normal `n`
//! points away from the wedge. A cone of half-angle `h` lies on the
//! outer side of that plane and touches it along one generatrix `g`
//! exactly when its axis is `a = cos(h) g + sin(h) n` with `g` a unit
//! direction in the plane. Then the whole cone (and the shank above it,
//! which recedes along `+a`, since `a . n = sin(h) > 0`) stays on the
//! outer side, and the only material on that side near the edge is the
//! corner the break removes. Other faces and the finite edge ends need
//! the independent continuous tool-sweep check in `deburr_clear`.
//! Swept along `t`, the generatrix covers the
//! chord when its span across the edge is the chord or more. So `g` is
//! free to turn in the plane: `g = cos(l) sc + sin(l) t`, with `sc` one
//! of `+-c` (the side) and `l` the lean along the travel. That rotation
//! about `n` is the freedom the holder escape and the rotary plane use.
//!
//! Provenance: plane and cone geometry (a cone is tangent to a plane
//! along a generatrix when its axis makes the half-angle with that line
//! and the complement with the plane normal); machine-tool handbook
//! chamfer geometry. Our own design notes; no vendor material consulted.

use crate::geometry::{Point3, Triangle};

use crate::finishing3d::deburr_scan::Exposed;

/// The smallest upward axis component a pose may have: the spindle
/// stands above the tip in the setup frame.
const MIN_UP: f64 = 1e-6;
/// The lean grid step for the 5-axis escape, degrees.
const STEP_DEG: f64 = 2.5;

/// How the tool axis may move.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DeburrAxisMode {
    /// Simultaneous tilt: the axis leans freely inside `max_tilt_deg`.
    #[default]
    Axis5,
    /// One rotary axis: the tool axis stays in the plane square to the
    /// rotary axis (an A axis about X: the YZ plane).
    Axis4,
}

pub(super) fn dot(a: Point3, b: Point3) -> f64 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

pub(super) fn add(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x + b.x, a.y + b.y, a.z + b.z)
}

pub(super) fn sub(a: Point3, b: Point3) -> Point3 {
    Point3::new(a.x - b.x, a.y - b.y, a.z - b.z)
}

pub(super) fn scale(a: Point3, k: f64) -> Point3 {
    Point3::new(a.x * k, a.y * k, a.z * k)
}

pub(super) fn unit(v: Point3) -> Option<Point3> {
    let l = dot(v, v).sqrt();
    (l > 1e-12 && l.is_finite()).then(|| scale(v, 1.0 / l))
}

/// The angle between two unit axes, degrees.
pub(super) fn ang(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]).clamp(-1.0, 1.0).acos().to_degrees()
}

/// The chamfer plane of one segment, per unit setback.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cross {
    /// The chord midpoint's offset from the edge per unit setback.
    pub o1: Point3,
    /// Unit across direction, face A side to face B side.
    pub c: Point3,
    /// Unit outward normal of the chamfer plane.
    pub n: Point3,
    /// The wedge's included angle, degrees.
    pub wedge_deg: f64,
}

impl Cross {
    /// The chord length at setback `s`: `2 s sin(theta / 2)`.
    pub fn chord(&self, s: f64) -> f64 {
        2.0 * s * (0.5 * self.wedge_deg.to_radians()).sin()
    }

    /// The edge-to-chord depth at setback `s`: `s cos(theta / 2)`.
    pub fn depth(&self, s: f64) -> f64 {
        s * (0.5 * self.wedge_deg.to_radians()).cos()
    }
}

/// The chamfer plane of the segment `e0`-`e1` of an exposed edge. A
/// rim assumes a perpendicular second face behind its one face, whose
/// normal is read with the sky convention (`+Z` side up), as the 3D
/// Deburr sizes a rim on a 90-degree wedge. `None` when the segment or
/// its faces are degenerate.
pub(super) fn cross(e0: Point3, e1: Point3, kind: &Exposed, facets: &[Triangle]) -> Option<Cross> {
    let t = unit(sub(e1, e0))?;
    let perp = |v: Point3| unit(sub(v, scale(t, dot(v, t))));
    let (da, db) = match *kind {
        Exposed::Rim { inside, facet } => {
            let tri = facets.get(facet)?;
            let mut nf = unit(tri.normal())?;
            if nf.z < 0.0 {
                nf = scale(nf, -1.0);
            }
            (perp(sub(inside, e0))?, perp(scale(nf, -1.0))?)
        }
        Exposed::Crease { qa, qb, .. } => (perp(sub(qb, e0))?, perp(sub(qa, e0))?),
    };
    let cosw = dot(da, db).clamp(-1.0, 1.0);
    let wedge_deg = cosw.acos().to_degrees();
    if !(wedge_deg > 1e-6 && wedge_deg < 180.0 - 1e-6) {
        return None;
    }
    let c = unit(sub(db, da))?;
    let mut n = unit(t.cross(c))?;
    let o1 = scale(add(da, db), 0.5);
    if dot(n, o1) > 0.0 {
        n = scale(n, -1.0);
    }
    Some(Cross { o1, c, n, wedge_deg })
}

/// One candidate pose: the unit axis and the unit generatrix that lies
/// on the chamfer plane (from the tip toward the cone's top).
#[derive(Clone, Copy, Debug)]
pub(super) struct Pose {
    pub a: [f64; 3],
    pub g: Point3,
}

/// The lean law's numbers, fixed for one operation.
#[derive(Clone, Copy, Debug)]
pub(super) struct Lean {
    pub mode: DeburrAxisMode,
    /// The cone's half-angle, radians.
    pub half: f64,
    /// The flank length from the tip to the cone's top, mm.
    pub flank: f64,
    /// The nominal lean along the travel, radians (`lead - lag`).
    pub lean: f64,
    /// The most the axis may turn off the natural pose, degrees.
    pub max_tilt_deg: f64,
    /// The rotary axis of `Axis4`, unit.
    pub rotary: Point3,
}

impl Lean {
    fn pose(&self, cr: &Cross, t: Point3, side: f64, l: f64) -> Pose {
        let g = add(scale(cr.c, side * l.cos()), scale(t, l.sin()));
        let a = add(scale(g, self.half.cos()), scale(cr.n, self.half.sin()));
        Pose { a: [a.x, a.y, a.z], g }
    }

    /// The tip of `pose` at edge point `e` and setback `s`: the chord
    /// midpoint stands at the flank's middle.
    pub fn tip(&self, cr: &Cross, pose: &Pose, e: Point3, s: f64) -> Point3 {
        sub(add(e, scale(cr.o1, s)), scale(pose.g, 0.5 * self.flank))
    }

    /// The natural pose: no lean, on the side whose axis stands up most.
    pub fn natural(&self, cr: &Cross, t: Point3) -> Pose {
        self.pose(cr, t, if cr.c.z >= 0.0 { 1.0 } else { -1.0 }, 0.0)
    }

    /// The poses this edge segment admits at chord `chord`, the nominal
    /// first (when it is admitted), then the nearest to it. A
    /// pose is admitted when it turns no more than `max_tilt_deg` off
    /// the natural pose, stands up, and lays a flank span across the
    /// whole chord. Empty when none is.
    pub fn candidates(&self, cr: &Cross, t: Point3, chord: f64) -> Vec<Pose> {
        let nat = self.natural(cr, t);
        let side0 = if cr.c.z >= 0.0 { 1.0 } else { -1.0 };
        let nominal = self.pose(cr, t, side0, self.lean);
        let mut leans: Vec<(f64, f64)> = Vec::new();
        match self.mode {
            DeburrAxisMode::Axis5 => self.grid(side0, &mut leans),
            DeburrAxisMode::Axis4 => {
                for side in [side0, -side0] {
                    let ca = self.half.cos();
                    let (a, b) = (ca * side * dot(cr.c, self.rotary), ca * dot(t, self.rotary));
                    let k = self.half.sin() * dot(cr.n, self.rotary);
                    let r = a.hypot(b);
                    if r <= 1e-12 {
                        if k.abs() <= 1e-12 {
                            self.grid(side, &mut leans); // every lean is square to the rotary axis
                        }
                        continue;
                    }
                    let q = -k / r;
                    if q.abs() > 1.0 {
                        continue;
                    }
                    let phi = b.atan2(a);
                    leans.push((side, phi + q.acos()));
                    leans.push((side, phi - q.acos()));
                }
            }
        }
        let mut out: Vec<Pose> = leans
            .into_iter()
            .filter(|&(_, l)| l.cos() > 1e-9 && chord / l.cos() <= self.flank * (1.0 - 1e-9))
            .map(|(side, l)| self.pose(cr, t, side, l))
            .filter(|p| p.a[2] > MIN_UP && ang(p.a, nat.a) <= self.max_tilt_deg + 1e-9)
            .collect();
        // The nominal and its neighbours first; where the nominal is not
        // admitted (a vertical edge's natural axis lies flat), the most
        // upright pose first - the holder stays as high as it can.
        if out.iter().any(|p| ang(p.a, nominal.a) <= 1e-9) {
            out.sort_by(|x, y| ang(x.a, nominal.a).total_cmp(&ang(y.a, nominal.a)));
        } else {
            out.sort_by(|x, y| y.a[2].total_cmp(&x.a[2]));
        }
        out
    }

    /// The 5-axis lean grid on one side: the nominal lean, then steps
    /// of `STEP_DEG` out to twice the tilt cap each way (the angle
    /// filter keeps the admitted ones).
    fn grid(&self, side: f64, leans: &mut Vec<(f64, f64)>) {
        let steps = (2.0 * self.max_tilt_deg / STEP_DEG).ceil().clamp(0.0, 72.0) as i32;
        for k in -steps..=steps {
            leans.push((side, self.lean + f64::from(k) * STEP_DEG.to_radians()));
        }
    }
}
