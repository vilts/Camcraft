//! Continuous carried-stock boundary queries for sub-pitch workholding (#1580).
//! Provenance: interval arithmetic, 3-D convex cut pruning, and stencil erosion over retained history.

use std::f64::consts::FRAC_1_SQRT_2;

use super::carry::Body;
use super::carry_boundary_prune::{
    CutRegion, box_through_frame_back, clip_effect, cut_current_bounds, cut_effect, cylinder_check,
};
use super::carry_history::{History, Span};
use crate::fixture_mesh::{BoxSteel, FixtureMesh};
use crate::geo2d::Point2;
use crate::geometry::Point3;

/// Conservative classification of a query box `x × y × z` against a
/// carried body's continuous stock solid (eroded horizontally by `xy_tol`).
#[derive(Clone, Debug, PartialEq)]
pub enum RegionStock {
    /// No eroded stock point inside the box overlaps the Z interval past `z_tol`.
    Empty,
    /// Every point inside the box shares the exact eroded material `spans`.
    Full(Vec<(f64, f64)>),
    /// The box straddles a stock face, cut wall, clip, or varying height;
    /// carries an upper bound on Z overlap and active Z envelope.
    Mixed(f64, (f64, f64)),
    /// Geometry or budget could not be read cleanly; keeps conservative `reach`.
    Unread(f64),
}

fn stencil(xy_tol: f64) -> Vec<(f64, f64)> {
    if xy_tol <= 0.0 {
        return vec![(0.0, 0.0)];
    }
    if xy_tol <= 1e-8 {
        return vec![(0.0, 0.0), (xy_tol, 0.0), (-xy_tol, 0.0), (0.0, xy_tol), (0.0, -xy_tol)];
    }
    let d = xy_tol * FRAC_1_SQRT_2;
    vec![(0.0, 0.0), (xy_tol, 0.0), (-xy_tol, 0.0), (0.0, xy_tol), (0.0, -xy_tol), (d, d), (d, -d), (-d, d), (-d, -d)]
}

fn shared_spans(a: &[Span], b: &[Span]) -> Vec<Span> {
    a.iter()
        .flat_map(|&(lo, hi)| b.iter().filter_map(move |&(l, h)| { let s = (lo.max(l), hi.min(h)); (s.0 < s.1).then_some(s) }))
        .collect()
}

fn subtract_span(spans: &[Span], cut_lo: f64, cut_hi: f64) -> Vec<Span> {
    let mut out = Vec::with_capacity(spans.len() + 1);
    for &(lo, hi) in spans {
        if hi <= cut_lo || lo >= cut_hi {
            out.push((lo, hi));
        } else {
            if lo < cut_lo {
                out.push((lo, cut_lo));
            }
            if hi > cut_hi {
                out.push((cut_hi, hi));
            }
        }
    }
    out
}

fn span_z_overlap(spans: &[Span], z: (f64, f64)) -> (f64, (f64, f64)) {
    let (mut total, mut zlo, mut zhi) = (0.0, f64::INFINITY, f64::NEG_INFINITY);
    for &(lo, hi) in spans {
        let (a, b) = (lo.max(z.0), hi.min(z.1));
        if b > a {
            total += b - a;
            zlo = zlo.min(a);
            zhi = zhi.max(b);
        }
    }
    (total, (zlo, zhi))
}

enum MeshCover {
    Clear,
    Whole,
    Straddles,
    Unread,
}

fn mesh_region_status(
    h: &History,
    mesh: &FixtureMesh,
    bounds: [(f64, f64); 3],
    xy_tol: f64,
    z_tol: f64,
    budget: &mut u64,
) -> MeshCover {
    let [x, y, z_keep] = bounds;
    let tol_ax = [h.frame.q[2][0], h.frame.q[2][1], h.frame.q[2][2]].iter().position(|v| *v != 0.0).unwrap_or(2);
    let (mlo, mhi) = box_through_frame_back(h.frame, x, y, z_keep);
    match mesh.steel_in_axis_box(None, [(mlo.x, mhi.x), (mlo.y, mhi.y), (mlo.z, mhi.z)], tol_ax, z_tol, budget) {
        BoxSteel::Clear => MeshCover::Clear,
        BoxSteel::Unread => MeshCover::Unread,
        BoxSteel::Boundary(_) => MeshCover::Straddles,
        BoxSteel::WhollyInside(_) => {
            let (glo, ghi) = box_through_frame_back(h.frame, (x.0 - xy_tol, x.1 + xy_tol), (y.0 - xy_tol, y.1 + xy_tol), z_keep);
            match mesh.steel_in_axis_box(None, [(glo.x, ghi.x), (glo.y, ghi.y), (glo.z, ghi.z)], tol_ax, z_tol, budget) {
                BoxSteel::WhollyInside(_) => MeshCover::Whole,
                BoxSteel::Unread => MeshCover::Unread,
                BoxSteel::Boundary(_) | BoxSteel::Clear => MeshCover::Straddles,
            }
        }
    }
}

impl Body {
    /// Whether this body carries continuous stock and cut provenance.
    #[must_use]
    pub fn has_continuous_history(&self) -> bool {
        self.dex.history.is_some()
    }

    /// Whether grid column `k` needs continuous history rather than the
    /// rasterized column (for open-mesh/cylinder stocks, padded edge
    /// columns, or side-turned exact carries).
    #[must_use]
    pub fn needs_continuous_column(&self, k: usize) -> bool {
        let Some(h) = &self.dex.history else { return false };
        if h.mesh.is_some() || h.active || h.cylinder.is_some() {
            return true;
        }
        let c = self.dex.center(k % self.dex.nx, k / self.dex.nx);
        let (lo, hi) = h.bounds();
        c.x < lo.x || c.x > hi.x || c.y < lo.y || c.y > hi.y
    }

    /// World-frame bounds of the continuous stock base, if retained.
    #[must_use]
    pub fn continuous_bounds(&self) -> Option<(Point3, Point3)> {
        self.dex.history.as_ref().map(|h| h.bounds())
    }

    /// Material Z intervals at `(x, y)` in the current frame, reading
    /// continuous history when retained.
    ///
    /// # Errors
    /// Continuous history ray errors or exhausted budgets.
    pub fn continuous_spans_at(&self, x: f64, y: f64) -> Result<Vec<(f64, f64)>, String> {
        if let Some(h) = &self.dex.history {
            return h.spans(x, y);
        }
        Ok(match self.dex.col(Point2::new(x, y)) {
            Some(k) if !self.empty_at(k) => vec![(self.floor_at(k), self.dex.top_at(k))],
            _ => Vec::new(),
        })
    }

    /// Material Z intervals that persist over the horizontal `xy_tol`
    /// stencil centred at `(x, y)`.
    ///
    /// # Errors
    /// Continuous history ray errors or exhausted budgets.
    pub fn eroded_spans_at(&self, x: f64, y: f64, xy_tol: f64) -> Result<Vec<(f64, f64)>, String> {
        let offsets = stencil(xy_tol);
        let mut acc = self.continuous_spans_at(x, y)?;
        for &(dx, dy) in &offsets[1..] {
            if acc.is_empty() {
                break;
            }
            let s = self.continuous_spans_at(x + dx, y + dy)?;
            acc = shared_spans(&acc, &s);
        }
        Ok(acc)
    }

    /// Conservatively classify `x × y × z` against the continuous stock
    /// solid eroded horizontally by `xy_tol` with Z tolerance `z_tol`.
    #[must_use]
    pub fn classify_region(
        &self,
        x: (f64, f64),
        y: (f64, f64),
        z: (f64, f64),
        xy_tol: f64,
        z_tol: f64,
        budget: &mut u64,
    ) -> RegionStock {
        let Some(h) = &self.dex.history else {
            return RegionStock::Unread(z.1 - z.0);
        };
        let (lo, hi) = h.bounds();
        let all = [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z, x.0, x.1, y.0, y.1, z.0, z.1, xy_tol, z_tol];
        if !all.iter().all(|v| v.is_finite()) || x.0 > x.1 || y.0 > y.1 || z.0 > z.1 {
            return RegionStock::Unread(z.1 - z.0);
        }
        let z_keep = (lo.z.max(z.0), hi.z.min(z.1));
        let max_reach = z_keep.1 - z_keep.0;
        if max_reach <= z_tol {
            return RegionStock::Empty;
        }
        if x.1 < lo.x + xy_tol || x.0 > hi.x - xy_tol || y.1 < lo.y + xy_tol || y.0 > hi.y - xy_tol {
            return RegionStock::Empty;
        }
        let mut straddles = x.0 < lo.x + xy_tol || x.1 > hi.x - xy_tol || y.0 < lo.y + xy_tol || y.1 > hi.y - xy_tol;

        if let Some((cx, cy, r)) = h.cylinder {
            let (possible, full) = cylinder_check(h, (cx, cy, r), [x, y, z_keep], xy_tol);
            if !possible {
                return RegionStock::Empty;
            }
            straddles |= !full;
        }
        if let Some(mesh) = &h.mesh {
            match mesh_region_status(h, mesh, [x, y, z_keep], xy_tol, z_tol, budget) {
                MeshCover::Clear => return RegionStock::Empty,
                MeshCover::Whole => {}
                MeshCover::Straddles => straddles = true,
                MeshCover::Unread => return RegionStock::Unread(max_reach),
            }
        }

        let mut spans = vec![(lo.z, hi.z)];
        let (mut max_reach, mut active_z) = span_z_overlap(&spans, z_keep);
        let offsets = stencil(xy_tol);
        for cut in h.cuts.iter().rev() {
            let (clo, chi) = cut_current_bounds(cut);
            if chi.x < x.0 - xy_tol || clo.x > x.1 + xy_tol || chi.y < y.0 - xy_tol || clo.y > y.1 + xy_tol || chi.z <= active_z.0 || clo.z >= active_z.1 {
                continue;
            }
            match cut_effect(cut, x, y, active_z, xy_tol, z_tol, &offsets) {
                CutRegion::RemovesAll => return RegionStock::Empty,
                CutRegion::UniformRemove(u0, u1) => {
                    spans = subtract_span(&spans, u0, u1);
                    (max_reach, active_z) = span_z_overlap(&spans, z_keep);
                    if max_reach <= z_tol {
                        return RegionStock::Empty;
                    }
                }
                CutRegion::Straddles => straddles = true,
            }
        }
        for clip in &h.clips {
            let (removes_all, touches) = clip_effect(clip, x, y, active_z, xy_tol, z_tol, &offsets);
            if removes_all {
                return RegionStock::Empty;
            }
            straddles |= touches;
        }
        if straddles { RegionStock::Mixed(max_reach, active_z) } else { RegionStock::Full(spans) }
    }
}
