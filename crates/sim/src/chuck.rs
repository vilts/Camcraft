//! Lathe chucks generated from their figures (#1442): a self-centring jaw
//! chuck (three or four jaws) and a collet chuck, as closed outward
//! shells in the chuck's own frame, plus the fit of either to a round
//! stock standing on the spindle axis.
//!
//! Provenance: our own parametric construction from general machining
//! knowledge (a chuck is a round body on the spindle nose with jaws
//! sliding radially in its face; a collet chuck is a body with a nose
//! closing a collet round the work, as the machine-tool handbooks'
//! workholding chapters draw them). No vendor CAD model, catalogue file
//! or HSMWorks material consulted.
//!
//! ## The frame
//!
//! The spindle axis is +z through the origin; z = 0 is the face the work
//! stands on (a jaw chuck's body face, a collet chuck's nose face), the
//! work above it. A jaw chuck's body is z -body_length..0; its jaws stand
//! z 0..jaw_height, the first along +x, the rest evenly round the axis,
//! each gripping face flat at the grip radius. A collet chuck's nose is z
//! -nose_length..0 and its body below it; the collet bore runs through
//! both (the work stands in it).
//!
//! ## The laws here
//!
//! - **Never less steel anywhere.** Every drawn face is on or outside the
//!   real steel, so a tool clear of the drawing is clear of the chuck.
//!   Round outer faces are polygons ([`SIDES`] sides) whose flats touch
//!   the true circle: circumscribed, the vertices stand out by the
//!   tessellation allowance. Bores are the other way round (#1470):
//!   inscribed, the vertices on the bore circle and the flats inside it,
//!   so the drawn bore wall is on or inside the true one. A flat touching
//!   the bore circle would leave the vertices outside it - up to 0.12 %
//!   of the bore radius of steel missing - which is the unsafe side. The
//!   price is that the drawn steel stands up to that sag into the bore,
//!   the work's own room: a part or stock exactly at the bore can read as
//!   sunk into it by that sag (the clash check's warning, never a
//!   refusal, counts only past `PENETRATION_MM`).
//! - **Shells touch, never overlap.** The jaws stand on the body face and
//!   the nose on the body, each a closed shell of its own, so the mesh
//!   law's ray-crossing test reads one inside per point.
//! - **Figures are refused by name** ([`JawChuck::check`],
//!   [`ColletChuck::check`]): non-finite, zero, negative or absurd sizes,
//!   a bore not inside the body, jaws reaching past the body or meeting
//!   each other at the grip, a collet bore not inside the nose.

use serde::{Deserialize, Serialize};

use crate::geometry::{Point3, Triangle};

/// Sides of every drawn round face.
pub const SIDES: usize = 64;
/// No chuck figure past this (mm): a typo, never a real chuck.
pub const MAX_DIM: f64 = 5000.0;
/// How far a round stock's X and Y spans may differ (relative) and still
/// read as round on a vertical spindle axis.
pub const ROUND_TOL: f64 = 0.005;

/// A self-centring jaw chuck's figures (mm).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct JawChuck {
    pub body_diameter: f64,
    pub body_length: f64,
    /// The through bore.
    pub bore_diameter: f64,
    /// The work diameter the jaws close on (outside grip).
    pub grip_diameter: f64,
    /// Radial: from the gripping face outward.
    pub jaw_length: f64,
    /// Across the jaw, tangential.
    pub jaw_width: f64,
    /// Axial: how far the jaw stands proud of the body face.
    pub jaw_height: f64,
}

/// A collet chuck's figures (mm).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColletChuck {
    pub body_diameter: f64,
    pub body_length: f64,
    pub nose_diameter: f64,
    pub nose_length: f64,
    /// The work diameter the collet closes on.
    pub collet_bore: f64,
}

/// Every figure finite, over zero and at most [`MAX_DIM`], by label.
fn sane<'a>(what: &str, dims: impl IntoIterator<Item = (&'a str, f64)>) -> Result<(), String> {
    for (label, v) in dims {
        let label = label.to_lowercase();
        if !v.is_finite() {
            return Err(format!("{what} {label} must be a finite length (got {v})"));
        }
        if v <= 0.0 {
            return Err(format!("{what} {label} must be more than zero (got {v} mm)"));
        }
        if v > MAX_DIM {
            return Err(format!("{what} {label} {v:.3} mm is over {MAX_DIM} mm"));
        }
    }
    Ok(())
}

impl JawChuck {
    /// The chuck's figures as (key, label, value), in the order shown.
    pub fn dims_mut(&mut self) -> [(&'static str, &'static str, &mut f64); 7] {
        [
            ("body_diameter", "Body diameter", &mut self.body_diameter),
            ("body_length", "Body length", &mut self.body_length),
            ("bore_diameter", "Through bore", &mut self.bore_diameter),
            ("grip_diameter", "Grip diameter", &mut self.grip_diameter),
            ("jaw_length", "Jaw length (radial)", &mut self.jaw_length),
            ("jaw_width", "Jaw width", &mut self.jaw_width),
            ("jaw_height", "Jaw height (above the face)", &mut self.jaw_height),
        ]
    }

    /// The jaw-chuck law for `jaws` jaws, refusals naming `what` (the
    /// kind's label) and the figure.
    pub fn check(&self, what: &str, jaws: u32) -> Result<(), String> {
        let mut c = *self;
        sane(what, c.dims_mut().into_iter().map(|(_, l, v)| (l, *v)))?;
        if !(2..=12).contains(&jaws) {
            return Err(format!("{what} must have 2 to 12 jaws (got {jaws})"));
        }
        if self.bore_diameter >= self.body_diameter {
            return Err(format!(
                "{what} through bore {:.3} mm must be less than the body diameter {:.3} mm",
                self.bore_diameter, self.body_diameter
            ));
        }
        let reach = 0.5 * self.grip_diameter + self.jaw_length;
        if reach > 0.5 * self.body_diameter {
            return Err(format!(
                "{what} jaws reach {:.3} mm from the axis (half the grip diameter plus the jaw length), past the body's {:.3} mm radius",
                reach,
                0.5 * self.body_diameter
            ));
        }
        // Adjacent jaws meet at the grip when each spans half the angle
        // between them or more.
        let half = (0.5 * self.jaw_width).atan2(0.5 * self.grip_diameter);
        if half >= std::f64::consts::PI / f64::from(jaws) {
            return Err(format!(
                "{what} jaw width {:.3} mm is too wide: {jaws} jaws closed on a {:.3} mm grip would meet each other",
                self.jaw_width, self.grip_diameter
            ));
        }
        Ok(())
    }

    /// The chuck's shells for `jaws` jaws (`self` assumed checked).
    #[must_use]
    pub fn solid(&self, jaws: u32) -> Vec<Triangle> {
        let mut t = ring(0.5 * self.bore_diameter, 0.5 * self.body_diameter, -self.body_length, 0.0);
        let (r0, hw) = (0.5 * self.grip_diameter, 0.5 * self.jaw_width);
        for k in 0..jaws {
            let a = std::f64::consts::TAU * f64::from(k) / f64::from(jaws);
            t.extend(jaw(a, (r0, r0 + self.jaw_length), (-hw, hw), (0.0, self.jaw_height)));
        }
        t
    }

    /// The same chuck closed on round stock `diameter` across.
    #[must_use]
    pub fn gripping(mut self, diameter: f64) -> Self {
        self.grip_diameter = diameter;
        self
    }
}

impl ColletChuck {
    /// The chuck's figures as (key, label, value), in the order shown.
    pub fn dims_mut(&mut self) -> [(&'static str, &'static str, &mut f64); 5] {
        [
            ("body_diameter", "Body diameter", &mut self.body_diameter),
            ("body_length", "Body length", &mut self.body_length),
            ("nose_diameter", "Nose diameter", &mut self.nose_diameter),
            ("nose_length", "Nose length", &mut self.nose_length),
            ("collet_bore", "Collet bore", &mut self.collet_bore),
        ]
    }

    /// The collet-chuck law, refusals naming `what` and the figure.
    pub fn check(&self, what: &str) -> Result<(), String> {
        let mut c = *self;
        sane(what, c.dims_mut().into_iter().map(|(_, l, v)| (l, *v)))?;
        if self.collet_bore >= self.nose_diameter {
            return Err(format!(
                "{what} collet bore {:.3} mm must be less than the nose diameter {:.3} mm",
                self.collet_bore, self.nose_diameter
            ));
        }
        if self.nose_diameter > self.body_diameter {
            return Err(format!(
                "{what} nose diameter {:.3} mm must not be more than the body diameter {:.3} mm",
                self.nose_diameter, self.body_diameter
            ));
        }
        Ok(())
    }

    /// The chuck's shells (`self` assumed checked).
    #[must_use]
    pub fn solid(&self) -> Vec<Triangle> {
        let r = 0.5 * self.collet_bore;
        let mut t = ring(r, 0.5 * self.nose_diameter, -self.nose_length, 0.0);
        t.extend(ring(r, 0.5 * self.body_diameter, -self.nose_length - self.body_length, -self.nose_length));
        t
    }

    /// The same chuck's collet closed on round stock `diameter` across.
    #[must_use]
    pub fn gripping(mut self, diameter: f64) -> Self {
        self.collet_bore = diameter;
        self
    }
}

/// A round stock read from its box `lo..hi` standing on a vertical
/// spindle axis: its diameter and the axis point at its underside.
/// Refused by name when the box is not finite or not round in X and Y.
pub fn round_stock(lo: Point3, hi: Point3) -> Result<(f64, Point3), String> {
    let (dx, dy, dz) = (hi.x - lo.x, hi.y - lo.y, hi.z - lo.z);
    if !(dx.is_finite() && dy.is_finite() && dz.is_finite() && dx > 0.0 && dy > 0.0 && dz > 0.0) {
        return Err("the stock box is empty or not finite".into());
    }
    let d = dx.max(dy);
    if (dx - dy).abs() > ROUND_TOL * d {
        return Err(format!(
            "the stock is {dx:.3} x {dy:.3} mm across: a chuck fits only round stock standing on a vertical spindle axis"
        ));
    }
    Ok((d, Point3::new(0.5 * (lo.x + hi.x), 0.5 * (lo.y + hi.y), lo.z)))
}

/// The polygon radius whose flats touch a circle of radius `r`.
fn flats_on(r: f64) -> f64 {
    r / (std::f64::consts::PI / SIDES as f64).cos()
}

/// A closed, outward annulus about the z axis: radii `r_in..r_out`, z
/// `z0..z1`. Conservative both ways: the outer face circumscribes its
/// circle (flats on it), the bore is inscribed in its circle (vertices
/// on it), so the drawn steel holds every point of the true annulus.
#[must_use]
pub fn ring(r_in: f64, r_out: f64, z0: f64, z1: f64) -> Vec<Triangle> {
    let (ri, ro) = (r_in, flats_on(r_out));
    let at = |r: f64, k: usize, z: f64| {
        let a = std::f64::consts::TAU * (k % SIDES) as f64 / SIDES as f64;
        Point3::new(r * a.cos(), r * a.sin(), z)
    };
    let mut t = Vec::with_capacity(8 * SIDES);
    for k in 0..SIDES {
        let (o0, o1, oo0, oo1) = (at(ro, k, z0), at(ro, k + 1, z0), at(ro, k, z1), at(ro, k + 1, z1));
        let (i0, i1, ii0, ii1) = (at(ri, k, z0), at(ri, k + 1, z0), at(ri, k, z1), at(ri, k + 1, z1));
        t.push(Triangle::new(o0, o1, oo1));
        t.push(Triangle::new(o0, oo1, oo0));
        t.push(Triangle::new(i0, ii1, i1));
        t.push(Triangle::new(i0, ii0, ii1));
        t.push(Triangle::new(oo0, oo1, ii1));
        t.push(Triangle::new(oo0, ii1, ii0));
        t.push(Triangle::new(o0, i1, o1));
        t.push(Triangle::new(o0, i0, i1));
    }
    t
}

/// A closed, outward jaw: the box radial `u`, tangential `v`, axial `z`,
/// turned `a` radians about the z axis.
fn jaw(a: f64, u: (f64, f64), v: (f64, f64), z: (f64, f64)) -> Vec<Triangle> {
    let (s, c) = a.sin_cos();
    let corner = |i: usize| {
        let (uu, vv) = (if i & 1 == 0 { u.0 } else { u.1 }, if i & 2 == 0 { v.0 } else { v.1 });
        Point3::new(uu * c - vv * s, uu * s + vv * c, if i & 4 == 0 { z.0 } else { z.1 })
    };
    let quads = [[0, 2, 3, 1], [4, 5, 7, 6], [0, 1, 5, 4], [2, 6, 7, 3], [0, 4, 6, 2], [1, 3, 7, 5]];
    quads
        .iter()
        .flat_map(|q| {
            [Triangle::new(corner(q[0]), corner(q[1]), corner(q[2])), Triangle::new(corner(q[0]), corner(q[2]), corner(q[3]))]
        })
        .collect()
}
