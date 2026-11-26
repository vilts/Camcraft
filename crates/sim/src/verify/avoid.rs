//! "Do not touch" protected faces and edges (#2055): full-tool 3D keep-out
//! regions (per Z interval across flutes, shank, and holder), rapid link
//! clearance elevation, and independent 3D move-sweep verification.
//!
//! Provenance: textbook 3D Sutherland-Hodgman Z-slab clipping, Andrew's
//! monotone chain 2D convex hull, and circumscribed Minkowski disk dilation
//! over `swept::Body` and `swept::pen_3d`; no vendor source consulted.

use super::arc_bulge::Bulge;
use super::avoid_poly::{add_poly_for_atoms, union_all, xy_seg_poly_dist};
use super::midpose;
use super::probe::{Atom, Cutter, Frame};
use super::solid::ARC_SAG;
use super::swept::{self, Body};
use super::types::{FixturePart, Warning};
use crate::cl::{Toolpath, normalize_axis};
use crate::geo2d::{Point2, Region};
use crate::geometry::{Point3, Triangle};

/// Generator standoff padding beyond requested clearance (mm) so chord/arc
/// discretization never crosses inside the verification tolerance.
pub const AVOID_PAD: f64 = 0.02;

/// Verification shortfall tolerance (mm): any approach closer than
/// `clearance + grow - AVOID_VERIFY_TOL` (Wait: `pen_3d >= -AVOID_VERIFY_TOL`)
/// convicts the move.
pub const AVOID_VERIFY_TOL: f64 = 1e-4;

/// One protected feature item resolved in the active frame.
#[derive(Clone, Debug, PartialEq)]
pub struct AvoidItem {
    pub label: String,
    pub tris: Vec<Triangle>,
    pub edges: Vec<([f64; 3], [f64; 3])>,
}

/// Complete "Do not touch" protection input for a generator or verifier.
#[derive(Clone, Debug, PartialEq)]
pub struct AvoidSpec {
    pub items: Vec<AvoidItem>,
    pub cutter: Cutter,
    /// Required 3D Euclidean standoff (clearance + max(stock_to_leave, 0)), mm.
    pub clearance: f64,
}

impl AvoidSpec {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub fn names_list(&self) -> String {
        let mut seen = Vec::new();
        for item in &self.items {
            if !seen.contains(&item.label) {
                seen.push(item.label.clone());
            }
        }
        seen.join(", ")
    }

    /// 2D cutter-center keep-out region covering every tip height in `[z_lo, z_hi]`
    /// with total radial+axial standoff `self.clearance + extra_pad`.
    pub fn keepout_region(&self, z_lo: f64, z_hi: f64, extra_pad: f64) -> Result<Region, String> {
        if self.items.is_empty() {
            return Ok(Region::empty());
        }
        let atoms = self.cutter.atoms();
        if atoms.is_empty() || !self.clearance.is_finite() || self.clearance < 0.0 || !extra_pad.is_finite() {
            return Err("invalid cutter or clearance for 'Do not touch' protection".into());
        }
        let req = self.clearance + extra_pad.max(0.0);
        let zlo = z_lo.min(z_hi);
        let zhi = z_lo.max(z_hi);
        let mut polys: Vec<Region> = Vec::new();
        for item in &self.items {
            for tri in &item.tris {
                if !(tri.a.is_finite() && tri.b.is_finite() && tri.c.is_finite()) {
                    return Err(format!("protected item '{}' has non-finite geometry", item.label));
                }
                let pts = [p2arr(tri.a), p2arr(tri.b), p2arr(tri.c)];
                add_poly_for_atoms(&pts, zlo, zhi, &atoms, req, &mut polys)?;
            }
            for &(a, b) in &item.edges {
                if !a.into_iter().chain(b).all(f64::is_finite) {
                    return Err(format!("protected item '{}' has non-finite edge geometry", item.label));
                }
                add_poly_for_atoms(&[a, b], zlo, zhi, &atoms, req, &mut polys)?;
            }
        }
        union_all(polys)
    }

    /// Required traverse tip height `(z_req, item_1based)` if rapid XY segment `a..b`
    /// comes within horizontal reach of any stack band above current `z_now`, else `None`.
    #[must_use]
    pub fn link_obstruction(&self, a: Point2, b: Point2, extra_pad: f64) -> Option<(f64, usize)> {
        let atoms = self.cutter.atoms();
        if atoms.is_empty() || self.items.is_empty() || !self.clearance.is_finite() {
            return None;
        }
        let req = self.clearance.max(0.0) + extra_pad.max(0.0);
        let mut worst: Option<(f64, usize)> = None;
        let consider = |z_top: f64, xy_pts: &[[f64; 3]], idx: usize, worst: &mut Option<(f64, usize)>| {
            let d = xy_seg_poly_dist([a.x, a.y], [b.x, b.y], xy_pts);
            for atom in &atoms {
                if d <= atom.rho_max() + req {
                    let h = z_top + req - atom.z0;
                    if worst.is_none_or(|(cur, _)| h > cur) {
                        *worst = Some((h, idx));
                    }
                }
            }
        };
        for (i, item) in self.items.iter().enumerate() {
            for tri in &item.tris {
                let pts = [p2arr(tri.a), p2arr(tri.b), p2arr(tri.c)];
                let z_top = pts[0][2].max(pts[1][2]).max(pts[2][2]);
                consider(z_top, &pts, i + 1, &mut worst);
            }
            for &(p, q) in &item.edges {
                let z_top = p[2].max(q[2]);
                consider(z_top, &[p, q], i + 1, &mut worst);
            }
        }
        worst
    }

    /// Independent 3D verification of every move in `path` against all protected items.
    pub fn hits_named(&self, path: &Toolpath, tol: f64) -> Result<Vec<(Warning, String)>, String> {
        if self.items.is_empty() {
            return Ok(Vec::new());
        }
        let atoms = self.cutter.atoms();
        if atoms.is_empty() || !self.clearance.is_finite() || self.clearance < 0.0 {
            return Err("invalid cutter or clearance for 'Do not touch' verification".into());
        }
        let req = self.clearance;
        let fl = self.cutter.flute_len;
        let sl = self.cutter.shank_len;
        let poses = midpose::poses(path);
        // Every assembly point lies within this radius of the tip. Rotating it
        // through at most `swing` radians moves it by at most `swing * reach`.
        let reach = atoms.iter().map(|a| a.z0.abs().max(a.z1.abs()).hypot(a.rho_max())).fold(0.0, f64::max);
        if !atoms.iter().all(|a| a.z0.is_finite() && a.z1.is_finite() && a.rho_max().is_finite()) || !reach.is_finite()
        {
            return Err("non-finite cutter reach for 'Do not touch' verification".into());
        }
        let mut pts = Vec::with_capacity(16);
        let mut out = Vec::new();
        for (mi, m) in path.moves.iter().enumerate() {
            if !(m.from.is_finite() && m.to.is_finite()) {
                continue;
            }
            let axis = poses.get(mi).and_then(|&(_, to)| to).and_then(normalize_axis).filter(|a| *a != [0.0, 0.0, 1.0]);
            let mid = poses
                .get(mi)
                .and_then(|&(f, t)| midpose::change(f, t))
                .map(|(a, b)| midpose::intervals(m, a, b, reach, ARC_SAG))
                .unwrap_or_default();
            // Sweep continuous 3D chords for every move (including XZ/YZ arcs) and
            // grow clearance by the exact arc-to-chord deviation bound so contacts
            // between samples cannot slip past (#2149).
            let segs = m.segments3(ARC_SAG);
            let grow = Bulge::of(m, segs.len()).deviation();
            if !grow.is_finite() {
                return Err(format!("move {mi} has non-finite arc geometry"));
            }
            let mut hit: Option<(usize, FixturePart, f64)> = None;
            for (ii, item) in self.items.iter().enumerate() {
                for (a, b) in &segs {
                    let fr = Frame::new(*a, axis);
                    check_frame_item(&fr, fr.local(*b), item, &atoms, req + grow, tol, fl, sl, &mut pts, ii + 1, &mut hit)?;
                }
                for iv in &mid {
                    let ax = normalize_axis(iv.am).filter(|v| *v != [0.0, 0.0, 1.0]);
                    let fr = Frame::new(iv.p0, ax);
                    // The chord sweep at the midpoint pose, expanded by this
                    // ball, encloses the rotating full assembly for the interval.
                    let margin = req + grow + iv.swing * reach;
                    if !margin.is_finite() {
                        return Err(format!("move {mi} has non-finite pose geometry"));
                    }
                    check_frame_item(
                        &fr,
                        fr.local(iv.p1),
                        item,
                        &atoms,
                        margin,
                        tol,
                        fl,
                        sl,
                        &mut pts,
                        ii + 1,
                        &mut hit,
                    )?;
                }
            }
            if let Some((item_idx, part, short_mm)) = hit {
                let label = self.items[item_idx - 1].label.clone();
                let w = Warning::AvoidHit {
                    at: mi,
                    item: item_idx as u32,
                    part,
                    short_mm: short_mm.max(0.0),
                };
                out.push((w, label));
            }
        }
        Ok(out)
    }

    pub fn hits(&self, path: &Toolpath, tol: f64) -> Result<Vec<Warning>, String> {
        Ok(self.hits_named(path, tol)?.into_iter().map(|(w, _)| w).collect())
    }
}

/// Independent check helper for `AvoidSpec::hits_named`.
pub fn avoid_hits_named(path: &Toolpath, spec: &AvoidSpec, tol: f64) -> Result<Vec<(Warning, String)>, String> {
    spec.hits_named(path, tol)
}

/// Independent check helper returning bare `Warning::AvoidHit` list.
pub fn avoid_hits(path: &Toolpath, spec: &AvoidSpec, tol: f64) -> Result<Vec<Warning>, String> {
    spec.hits(path, tol)
}

#[allow(clippy::too_many_arguments)]
fn check_frame_item(
    fr: &Frame,
    d: [f64; 3],
    item: &AvoidItem,
    atoms: &[Atom],
    m: f64,
    tol: f64,
    fl: f64,
    sl: f64,
    pts: &mut Vec<[f64; 2]>,
    item_idx: usize,
    best: &mut Option<(usize, FixturePart, f64)>,
) -> Result<(), String> {
    let part_of = |a: &Atom| {
        if a.z0 < fl {
            FixturePart::Flute
        } else if a.z0 < fl + sl {
            FixturePart::Shank
        } else {
            FixturePart::Holder
        }
    };
    let mut judge = |body: &Body| {
        for atom in atoms {
            let pen = swept::pen_3d(body, atom, m, -tol, pts);
            if pen >= -tol {
                let short = pen.max(0.0);
                if best.is_none_or(|(_, _, prev)| short > prev) {
                    *best = Some((item_idx, part_of(atom), short));
                }
            }
        }
    };
    for tri in &item.tris {
        if !(tri.a.is_finite() && tri.b.is_finite() && tri.c.is_finite()) {
            return Err(format!("protected item '{}' has non-finite geometry", item.label));
        }
        judge(&Body::new(&fr.tri(tri), d));
    }
    for &(a, b) in &item.edges {
        if !a.into_iter().chain(b).all(f64::is_finite) {
            return Err(format!("protected item '{}' has non-finite edge geometry", item.label));
        }
        let s = fr.seg(Point3::new(a[0], a[1], a[2]), Point3::new(b[0], b[1], b[2]));
        judge(&Body::from_edge(&s, d));
    }
    Ok(())
}

fn p2arr(p: Point3) -> [f64; 3] {
    [p.x, p.y, p.z]
}
