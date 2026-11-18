//! A jaw chuck's jaws (#1468): which way they grip, the step the work
//! sits on, the law both obey and their shells.
//!
//! Provenance: our own parametric construction from general machining
//! knowledge (the machine-tool handbooks' chuck work: jaws grip a bar
//! from outside or open into a bore from inside, and stepped jaws seat
//! the work on a step). No vendor model or HSMWorks material consulted.
//!
//! ## The figures
//!
//! `r0` is half the grip diameter. An **outside** jaw is a box from `r0`
//! outward by the jaw length, its gripping face flat on the work. An
//! **inside** jaw runs from `r0` inward by the jaw length, its gripping
//! face the arc of the bore it opens into, between the jaw's two sides.
//! A **stepped** jaw is two shells: below the step height the jaw
//! reaches the step depth past the gripping face, under the work
//! (inward for an outside grip, outward for an inside grip); above it
//! the plain jaw. The work sits on the step.
//!
//! ## Never less steel
//!
//! An inside jaw's arc is drawn as a polygon whose flats touch the grip
//! circle (circumscribed, like every outer round face here), in steps
//! of at most [`ARC_STEP`]: the drawn steel stands into the work by
//! `r0 * (1 / cos(ARC_STEP / 2) - 1)` at most (0.0075 % of `r0`), under
//! the clash check's depth for any real chuck.

use serde::{Deserialize, Serialize};

use super::JawChuck;
use crate::geometry::{Point3, Triangle};

/// The largest angle one flat of an inside jaw's gripping arc spans.
pub const ARC_STEP: f64 = std::f64::consts::TAU / 256.0;

/// Which way a jaw chuck's jaws grip the work.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Grip {
    /// The jaws close on the work's outside.
    #[default]
    Outside,
    /// The jaws open into the work's bore.
    Inside,
}

impl Grip {
    /// The file and MCP keys, in the order shown.
    pub const KEYS: [&'static str; 2] = ["outside", "inside"];

    /// This grip's key.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Inside => "inside",
        }
    }

    /// The grip of key `key`.
    #[must_use]
    pub fn of(key: &str) -> Option<Self> {
        match key {
            "outside" => Some(Self::Outside),
            "inside" => Some(Self::Inside),
            _ => None,
        }
    }

    /// The grip as the UI names it.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Outside => "Outside",
            Self::Inside => "Inside (in a bore)",
        }
    }
}

/// The jaws' law for `jaws` jaws (the plain figures already sane):
/// the step, the reach past the body, the axis and each other.
pub(super) fn check(c: &JawChuck, what: &str, jaws: u32) -> Result<(), String> {
    let (sh, sd) = (c.step_height, c.step_depth);
    for (label, v) in [("step height", sh), ("step depth", sd)] {
        if !v.is_finite() {
            return Err(format!("{what} {label} must be a finite length (got {v})"));
        }
        if v < 0.0 {
            return Err(format!("{what} {label} must be zero (plain jaws) or more (got {v} mm)"));
        }
        if v > super::MAX_DIM {
            return Err(format!("{what} {label} {v:.3} mm is over {} mm", super::MAX_DIM));
        }
    }
    if (sh > 0.0) != (sd > 0.0) {
        return Err(format!(
            "{what} step height {sh:.3} mm and step depth {sd:.3} mm must both be set (stepped jaws) or both be zero (plain jaws)"
        ));
    }
    if sh >= c.jaw_height {
        return Err(format!(
            "{what} step height {sh:.3} mm must be less than the jaw height {:.3} mm (the jaw grips above its step)",
            c.jaw_height
        ));
    }
    let (r0, hw, body) = (0.5 * c.grip_diameter, 0.5 * c.jaw_width, 0.5 * c.body_diameter);
    let between = std::f64::consts::PI / f64::from(jaws);
    match c.grip {
        Grip::Outside => {
            let reach = r0 + c.jaw_length;
            if reach > body {
                return Err(format!(
                    "{what} jaws reach {reach:.3} mm from the axis (half the grip diameter plus the jaw length), past the body's {body:.3} mm radius"
                ));
            }
            let inner = r0 - sd;
            if inner <= 0.0 {
                return Err(format!(
                    "{what} step depth {sd:.3} mm reaches the axis: it must be less than half the grip diameter {:.3} mm",
                    c.grip_diameter
                ));
            }
            // Adjacent jaws meet where each spans half the angle between
            // them or more: at the grip, or at the step's inner end.
            if hw.atan2(inner) >= between {
                return Err(if sd > 0.0 {
                    format!(
                        "{what} jaw width {:.3} mm is too wide: the steps of {jaws} jaws, {inner:.3} mm from the axis, would meet each other",
                        c.jaw_width
                    )
                } else {
                    format!(
                        "{what} jaw width {:.3} mm is too wide: {jaws} jaws closed on a {:.3} mm grip would meet each other",
                        c.jaw_width, c.grip_diameter
                    )
                });
            }
        }
        Grip::Inside => {
            if hw >= r0 {
                return Err(format!(
                    "{what} jaw width {:.3} mm does not go into a {:.3} mm bore (inside grip)",
                    c.jaw_width, c.grip_diameter
                ));
            }
            let reach = r0 + sd;
            if reach > body {
                return Err(format!(
                    "{what} jaws reach {reach:.3} mm from the axis (half the inside grip diameter plus the step depth), past the body's {body:.3} mm radius"
                ));
            }
            let inner = r0 - c.jaw_length;
            if inner <= 0.0 {
                return Err(format!(
                    "{what} jaw length {:.3} mm reaches the axis from a {:.3} mm inside grip: it must be less than half the grip diameter",
                    c.jaw_length, c.grip_diameter
                ));
            }
            // The jaw's sides meet its gripping arc this far out; a jaw
            // shorter than the arc's own depth has no body behind it.
            let shoulder = (r0 * r0 - hw * hw).sqrt();
            if inner >= shoulder {
                return Err(format!(
                    "{what} jaw length {:.3} mm is too short: a {:.3} mm wide jaw's gripping arc in a {:.3} mm bore is {:.3} mm deep",
                    c.jaw_length,
                    c.jaw_width,
                    c.grip_diameter,
                    r0 - shoulder
                ));
            }
            if hw.atan2(inner) >= between {
                return Err(format!(
                    "{what} jaw width {:.3} mm is too wide: {jaws} jaws opened into a {:.3} mm bore would meet each other {inner:.3} mm from the axis",
                    c.jaw_width, c.grip_diameter
                ));
            }
        }
    }
    Ok(())
}

/// One jaw's shells, turned `a` radians about the z axis (`c` checked).
pub(super) fn shells(c: &JawChuck, a: f64) -> Vec<Triangle> {
    let (r0, hw) = (0.5 * c.grip_diameter, 0.5 * c.jaw_width);
    let (sh, sd, top) = (c.step_height, c.step_depth, c.jaw_height);
    let stepped = sh > 0.0 && sd > 0.0;
    match c.grip {
        Grip::Outside => {
            let far = r0 + c.jaw_length;
            if stepped {
                let mut t = boxed(a, (r0 - sd, far), (-hw, hw), (0.0, sh));
                t.extend(boxed(a, (r0, far), (-hw, hw), (sh, top)));
                t
            } else {
                boxed(a, (r0, far), (-hw, hw), (0.0, top))
            }
        }
        Grip::Inside => {
            let near = r0 - c.jaw_length;
            if stepped {
                let mut t = arced(a, near, r0 + sd, hw, (0.0, sh));
                t.extend(arced(a, near, r0, hw, (sh, top)));
                t
            } else {
                arced(a, near, r0, hw, (0.0, top))
            }
        }
    }
}

/// A closed, outward jaw: the box radial `u`, tangential `v`, axial `z`,
/// turned `a` radians about the z axis.
fn boxed(a: f64, u: (f64, f64), v: (f64, f64), z: (f64, f64)) -> Vec<Triangle> {
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

/// The outline (radial, tangential; counter-clockwise, convex) of an
/// inside jaw: sides at `±hw` from `near` out to the arc of radius `r`,
/// the arc drawn with its flats on the circle and its two ends on it.
/// Assumes `hw < r` and `near < sqrt(r² - hw²)` (the jaws' law).
#[must_use]
pub fn arc_outline(near: f64, r: f64, hw: f64) -> Vec<(f64, f64)> {
    let theta = (hw / r).clamp(-1.0, 1.0).asin();
    // At most a quarter turn each side, so the count is small and finite.
    let n = ((2.0 * theta / ARC_STEP).ceil() as usize).clamp(1, 256);
    let step = 2.0 * theta / n as f64;
    let out = r / (0.5 * step).cos();
    let mut p = Vec::with_capacity(n + 4);
    p.push((near, -hw));
    p.push((r * theta.cos(), -hw));
    for k in 0..n {
        let mid = -theta + (k as f64 + 0.5) * step;
        p.push((out * mid.cos(), out * mid.sin()));
    }
    p.push((r * theta.cos(), hw));
    p.push((near, hw));
    p
}

/// A closed, outward inside jaw: [`arc_outline`] over axial `z`, turned
/// `a` radians about the z axis.
fn arced(a: f64, near: f64, r: f64, hw: f64, z: (f64, f64)) -> Vec<Triangle> {
    let outline = arc_outline(near, r, hw);
    let (s, c) = a.sin_cos();
    let at = |k: usize, z: f64| {
        let (u, v) = outline[k % outline.len()];
        Point3::new(u * c - v * s, u * s + v * c, z)
    };
    let n = outline.len();
    let mut t = Vec::with_capacity(4 * n);
    for k in 1..n.saturating_sub(1) {
        t.push(Triangle::new(at(0, z.1), at(k, z.1), at(k + 1, z.1)));
        t.push(Triangle::new(at(0, z.0), at(k + 1, z.0), at(k, z.0)));
    }
    for k in 0..n {
        t.push(Triangle::new(at(k, z.0), at(k + 1, z.0), at(k + 1, z.1)));
        t.push(Triangle::new(at(k, z.0), at(k + 1, z.1), at(k, z.1)));
    }
    t
}
