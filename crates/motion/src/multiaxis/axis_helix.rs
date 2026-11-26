//! Helical-thread detection (issue #3023): reads from the facet normal
//! lines if a model carries a HELICAL surface (a modelled screw thread)
//! about a line, and states its pitch and hand.
//!
//! A surface that a screw motion (turn about the axis `(o, u)` and
//! advance `h` per radian along it) carries into itself has, at each
//! point `p`, a normal `n` square to the motion's velocity
//! `u x (p - o) + h u`. With `k = n x u` that is
//! `k . (o - p) = h (n . u)`: the normal lines belong to a linear line
//! complex, and a surface of revolution is the case `h = 0` (each
//! normal line meets the axis, the law of `axis.rs`). So about a
//! candidate axis each inclined facet states its own `h`; a revolve's
//! inclined facets state zero, and a helical thread's two flanks state
//! one common nonzero `h` (the sign of `n . u` cancels between them).
//! A cylinder's facets (`n . u = 0`) and a facing flat's (`n` along
//! `u`) state nothing and are left out.
//!
//! Provenance: helical surfaces and their normals as a linear line
//! complex (Pottmann & Wallner, Computational Line Geometry, 2001,
//! ch. 3; Pottmann, Hofer, Odehnal & Wallner, "Line geometry for 3D
//! shape understanding and reconstruction", ECCV 2004). Written from
//! those sources and our own design notes; no vendor material.

use crate::geometry::{Point3, Triangle};

use super::axis::{data_of, line, moment, Axis, Datum};
use super::axis_point::{skip_budget, widest_skip};
use super::linalg::sym_eigen;

/// A facet is inclined (a flank, a cone, a chamfer) when its normal
/// keeps at least this cosine to the axis, and at most [`INCL_MAX`].
const INCL_MIN: f64 = 0.2;
/// Above this cosine the facet is a facing flat: it states no `h`.
const INCL_MAX: f64 = 0.98;
/// An `h` under this fraction of the mean radius is a revolve's zero
/// (rounding and the line fit's own noise), not a screw.
const H_FLOOR: f64 = 2e-3;
/// A facet agrees with the screw when its `h` is within this fraction
/// of the screw's `h`.
const H_BAND: f64 = 0.3;
/// The agreeing area must be at least this share of the inclined area
/// (a revolved chamfer beside the thread is fine; scattered `h` of a
/// cross hole is not a screw)...
const INCL_SHARE: f64 = 0.5;
/// ...and at least this share of the whole part.
const PART_SHARE: f64 = 0.02;

/// A helical surface found about an axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Helix {
    pub axis: Axis,
    /// The advance for each full turn, in model units; positive for a
    /// right-hand helix about the upward axis, negative for a left-hand.
    pub pitch: f64,
}

/// The helical surface the facets carry, or `None` when they carry
/// none that the gates accept (a revolve, a prism, a freeform).
pub fn helix(facets: &[Triangle]) -> Option<Helix> {
    let (data, a_total) = data_of(facets);
    if data.is_empty() || !(a_total > 0.0 && a_total.is_finite()) {
        return None;
    }
    let mut cands: Vec<Point3> = Vec::with_capacity(6);
    if let Some((_, vecs)) = sym_eigen(moment(&data)) {
        cands.extend((0..3).map(|c| Point3::new(vecs[0][c], vecs[1][c], vecs[2][c])));
    }
    cands.extend(super::axis_poles::candidates(&data, a_total));
    let mut best: Option<(f64, Helix)> = None;
    for u in cands {
        if let Some((share, hx)) = about(&data, u, a_total)
            && best.as_ref().is_none_or(|(s, _)| share > *s)
        {
            best = Some((share, hx));
        }
    }
    best.map(|(_, hx)| hx)
}

/// The screw about the line fitted in direction `u`, with the share of
/// the inclined area that agrees with it; `None` when the gates fail.
fn about(data: &[Datum], u: Point3, a_total: f64) -> Option<(f64, Helix)> {
    let (ax, rho) = line(data, u, a_total)?;
    let (u, o) = (ax.dir, ax.origin);
    let floor = H_FLOOR * rho;
    let mut hs: Vec<(f64, f64, Point3)> = Vec::new();
    let mut a_incl = 0.0;
    for d in data {
        let c = d.n.dot(u);
        if !(INCL_MIN..=INCL_MAX).contains(&c.abs()) {
            continue;
        }
        let k = d.n.cross(u);
        let h = k.dot(o - d.p) / c;
        if !h.is_finite() {
            continue;
        }
        a_incl += d.a;
        if h.abs() > floor {
            hs.push((h, d.a, d.n));
        }
    }
    if a_incl <= 0.0 || hs.is_empty() {
        return None;
    }
    // The screw's `h`: the area-weighted median of the stated ones.
    hs.sort_by(|x, y| x.0.total_cmp(&y.0));
    let half = 0.5 * hs.iter().map(|x| x.1).sum::<f64>();
    let mut acc = 0.0;
    let h0 = hs.iter().find(|x| {
        acc += x.1;
        acc >= half
    })?.0;
    let band = H_BAND * h0.abs();
    let agree: Vec<&(f64, f64, Point3)> = hs.iter().filter(|x| (x.0 - h0).abs() <= band).collect();
    let a_agree: f64 = agree.iter().map(|x| x.1).sum();
    if a_agree <= 0.0 || a_agree < INCL_SHARE * a_incl || a_agree < PART_SHARE * a_total {
        return None;
    }
    // The pitch is the agreeing band's area-weighted MEAN: a facet
    // reads `h` scaled by the radius of its circumcenter over the radius
    // of the edge that sets its twist, so the two triangles of one
    // twisted quad read low and high by about the same ratio, and the
    // median of the band would land on one side of the truth.
    let h_mean = agree.iter().map(|x| x.0 * x.1).sum::<f64>() / a_agree;
    // A screw surface goes all the way round its axis: one tilted flat
    // can state a common `h` over a narrow strip, but not at every bearing.
    if widest_skip(agree.iter().map(|x| x.2), u)? > skip_budget() {
        return None;
    }
    let pitch = std::f64::consts::TAU * h_mean;
    pitch.is_finite().then_some((a_agree / a_incl, Helix { axis: ax, pitch }))
}
