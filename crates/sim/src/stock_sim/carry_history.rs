//! Retained swept solids for side-on stock carry (#1805).
//! Provenance: interval subtraction and the existing field sweep distance laws.

use std::sync::Arc;

use super::carry_exact::{Frame, box_span};
use super::carry_profile::Profile;
use super::cut::SimOp;
use super::dexel::Dexel;
use super::field_arc::arc_sweeps;
use super::field_cut::{SdfTool, chord_sweep, chords};
use super::field_solid::tool_kind;
use super::field_sweep::Sweep;
use crate::cl::MoveKind;
use crate::fixture_mesh::FixtureMesh;
use crate::geometry::Point3;

pub(super) type Span = (f64, f64);
const RAY_EPS: f64 = 0.0001;
const RAY_BUDGET: usize = 100_000;
const MESH_RAY_BUDGET: u64 = 1_000_000;

#[cfg(test)]
#[path = "carry_history_tests.rs"]
mod tests;

#[derive(Clone)]
pub(super) enum Shape {
    Sweep(Arc<Sweep>),
    Cone(f64),
}

#[derive(Clone)]
pub(super) struct Cut {
    pub(super) frame: Frame,
    pub(super) shape: Shape,
    pub(super) profile: Arc<Profile>,
}

impl std::fmt::Debug for Cut {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cut").field("frame", &self.frame).finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub(super) struct Clip {
    pub(super) frame: Frame,
    pub(super) grid: Dexel,
}

#[derive(Clone, Debug)]
pub(super) struct History {
    pub(super) frame: Frame,
    pub(super) lo: Point3,
    pub(super) hi: Point3,
    pub(super) cylinder: Option<(f64, f64, f64)>,
    pub(super) mesh: Option<Arc<FixtureMesh>>,
    pub(super) cell: f64,
    pub(super) cuts: Vec<Cut>,
    pub(super) clips: Vec<Clip>,
    pub(super) active: bool,
    pub(super) entry: Option<Arc<History>>,
}

impl History {
    pub(super) fn block(x0: f64, y0: f64, x1: f64, y1: f64, top: f64, cell: f64) -> Self {
        Self {
            frame: Frame::IDENTITY,
            lo: Point3::new(x0, y0, f64::NEG_INFINITY),
            hi: Point3::new(x1, y1, top),
            cylinder: None,
            mesh: None,
            cell,
            cuts: Vec::new(),
            clips: Vec::new(),
            active: false,
            entry: None,
        }
    }

    pub(super) fn with_mesh(lo: Point3, hi: Point3, cell: f64, mesh: Arc<FixtureMesh>) -> Self {
        Self {
            frame: Frame::IDENTITY,
            lo,
            hi,
            cylinder: None,
            mesh: Some(mesh),
            cell,
            cuts: Vec::new(),
            clips: Vec::new(),
            active: false,
            entry: None,
        }
    }

    pub(super) fn add_ops(&mut self, ops: &[SimOp]) -> Result<(), String> {
        let top = if self.lo.z.is_finite() { self.bounds().1.z } else { self.hi.z };
        let chord_tol = (self.cell * 0.25).max(1e-9);
        for op in ops {
            let bottom = op.moves.iter().map(|m| m.from.z.min(m.to.z)).fold(top, f64::min);
            let flank = (top - bottom).max(0.0) + 2.0 * op.radius + 1.0;
            if !flank.is_finite() {
                return Err("exact side-on carry tool span is not finite".into());
            }
            let tool = match (op.cone, op.corner) {
                (Some(cot), _) => SdfTool::Taper { r: op.radius, cot },
                (None, Some(corner)) => SdfTool::BullNose { r: op.radius, corner, flank },
                (None, None) if op.ball => SdfTool::Shanked { r: op.radius, shank: flank },
                _ => SdfTool::Flat { r: op.radius, flank },
            };
            let kind = tool_kind(&tool).map_err(|e| e.to_string())?;
            for m in op.moves.iter().filter(|m| m.kind != MoveKind::Rapid) {
                let length = (m.to.x - m.from.x).hypot(m.to.y - m.from.y).hypot(m.to.z - m.from.z);
                if !(op.radius * op.radius).is_finite() || !(length * length).is_finite() {
                    return Err("exact side-on carry tool coordinates exceed finite geometric arithmetic".into());
                }
                let profile = Arc::new(Profile::new(op, *m, chord_tol));
                if let Some(cot) = op.cone {
                    self.cuts.push(Cut { frame: Frame::IDENTITY, shape: Shape::Cone(cot), profile });
                    continue;
                }
                let pieces = match m.arc {
                    Some(arc) if arc.plane.is_xy() => arc_sweeps(m, arc, &kind, chord_tol),
                    _ => None,
                };
                let pieces = match pieces {
                    Some(p) => p,
                    None => chords(m, chord_tol)
                        .into_iter()
                        .map(|(a, b)| chord_sweep(&kind, [0.0, 0.0, 1.0], a, b))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|e| e.to_string())?,
                };
                self.cuts.extend(pieces.into_iter().map(|s| Cut {
                    frame: Frame::IDENTITY,
                    shape: Shape::Sweep(Arc::new(s)),
                    profile: profile.clone(),
                }));
            }
        }
        Ok(())
    }

    pub(super) fn clip(&mut self, grid: &Dexel) {
        let mut grid = grid.clone();
        grid.history = None;
        self.clips.push(Clip { frame: Frame::IDENTITY, grid });
    }

    pub(super) fn spans(&self, x: f64, y: f64) -> Result<Vec<Span>, String> {
        let Some((mut lo, mut hi)) = box_span(self.frame, self.lo, self.hi, x, y) else {
            return Ok(Vec::new());
        };
        if !(lo.is_finite() && hi.is_finite()) {
            return Err("exact side-on carry cannot read an unbounded stock ray".into());
        }
        if let Some((cx, cy, r)) = self.cylinder {
            let Some((cl, ch)) = cylinder_span(self.frame, cx, cy, r, x, y) else {
                return Ok(Vec::new());
            };
            lo = lo.max(cl);
            hi = hi.min(ch);
            if lo > hi {
                return Ok(Vec::new());
            }
        }
        let mut spans = if let Some(mesh) = &self.mesh {
            let (a, d) = self.frame.ray(x, y);
            let mut mesh_budget = MESH_RAY_BUDGET;
            let raw = mesh
                .steel_on_axis_line(a, d, &mut mesh_budget)
                .ok_or_else(|| "exact side-on carry exhausted its stock mesh ray budget".to_owned())?;
            intersect_with_span(&raw, (lo, hi))
        } else {
            vec![(lo, hi)]
        };
        let mut budget = RAY_BUDGET;
        for cut in &self.cuts {
            if spans.is_empty() {
                return Ok(spans);
            }
            if budget == 0 {
                return Err("exact side-on carry exhausted its ray budget".into());
            }
            budget -= 1;
            let mut kept = Vec::new();
            for span in spans {
                cut.keep(x, y, span, &mut budget, &mut kept)?;
            }
            spans = joined(kept);
        }
        for clip in &self.clips {
            let (p, direction) = clip.frame.ray(x, y);
            let cells = |v: f64, o: f64, n: usize, d: f64| {
                if d != 0.0 {
                    return 0..n;
                }
                let i = ((v - o) / clip.grid.cell).floor();
                if i >= 0.0 && i < n as f64 { i as usize..i as usize + 1 } else { 0..0 }
            };
            let xs = cells(p.x, clip.grid.x0, clip.grid.nx, direction[0]);
            let ys = cells(p.y, clip.grid.y0, clip.grid.ny, direction[1]);
            for iy in ys {
                for ix in xs.clone() {
                    if spans.is_empty() {
                        return Ok(spans);
                    }
                    if budget == 0 {
                        return Err("exact side-on carry exhausted its ray budget".into());
                    }
                    budget -= 1;
                    let top = clip.grid.top_at(ix + iy * clip.grid.nx);
                    if !top.is_finite() {
                        continue;
                    }
                    let (a, b) = (clip.grid.x0 + ix as f64 * clip.grid.cell, clip.grid.y0 + iy as f64 * clip.grid.cell);
                    let low = Point3::new(a, b, top);
                    let high = Point3::new(a + clip.grid.cell, b + clip.grid.cell, f64::INFINITY);
                    if let Some(remove) = box_span(clip.frame, low, high, x, y) {
                        spans = minus(spans, remove);
                    }
                }
            }
        }
        Ok(spans)
    }
}

impl Cut {
    fn distance(&self, p: Point3) -> (f64, f64) {
        match &self.shape {
            Shape::Sweep(s) => (s.dist([0.0, 0.0, 1.0], p), s.residual()),
            Shape::Cone(cot) => self.profile.cone_distance(p, *cot),
        }
    }

    /// Only an interval proven wholly inside the swept tool is removed.
    /// Ambiguous surface intervals remain material, including sub-cell skins.
    fn keep(&self, x: f64, y: f64, span: Span, budget: &mut usize, out: &mut Vec<Span>) -> Result<(), String> {
        let (p, direction) = self.frame.ray(x, y);
        if direction[2] != 0.0 {
            let floor = self.profile.floor(p.x, p.y);
            if floor == f64::INFINITY {
                out.push(span);
                return Ok(());
            }
            if !floor.is_finite() {
                return Err("exact side-on carry read a non-finite tool floor".into());
            }
            let z = self.frame.apply(Point3::new(p.x, p.y, floor)).z;
            let kept = if direction[2] > 0.0 { (span.0, span.1.min(z)) } else { (span.0.max(z), span.1) };
            if kept.0 <= kept.1 {
                out.push(kept);
            }
            return Ok(());
        }
        if self.profile.straight_flat() {
            match self.profile.flat_span(p, direction) {
                Some(removed) => out.extend(minus(vec![span], removed)),
                None => out.push(span),
            }
            return Ok(());
        }
        let mut pending = vec![span];
        while let Some((lo, hi)) = pending.pop() {
            if *budget == 0 {
                return Err("exact side-on carry exhausted its ray budget".into());
            }
            *budget -= 1;
            let mid = lo + (hi - lo) * 0.5;
            let half = (hi - lo) * 0.5;
            let p = self.frame.back(Point3::new(x, y, mid));
            let (d, residual) = self.distance(p);
            if !d.is_finite() {
                return Err("exact side-on carry read a non-finite tool distance".into());
            }
            let convex = match &self.shape {
                Shape::Sweep(s) => !matches!(s.as_ref(), Sweep::Arc(_) | Sweep::Lathe(_)),
                Shape::Cone(_) => self.profile.motion.arc.is_none(),
            };
            if convex {
                let a = self.distance(self.frame.back(Point3::new(x, y, lo))).0;
                let b = self.distance(self.frame.back(Point3::new(x, y, hi))).0;
                // A convex swept solid contains the whole segment between
                // two proven-inside endpoints, including a stock-edge tie.
                if a <= 0.0 && b <= 0.0 {
                    continue;
                }
            }
            if d + half < -1e-9 {
                continue;
            }
            if d - residual > half || hi - lo <= RAY_EPS || mid == lo || mid == hi {
                out.push((lo, hi));
            } else {
                pending.push((mid, hi));
                pending.push((lo, mid));
            }
        }
        Ok(())
    }
}

fn cylinder_span(frame: Frame, cx: f64, cy: f64, r: f64, x: f64, y: f64) -> Option<Span> {
    let (a, d) = frame.ray(x, y);
    if d[2] != 0.0 {
        return ((a.x - cx).hypot(a.y - cy) <= r).then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let (perp, axis_a, axis_d) = if d[0] != 0.0 {
        (a.y - cy, a.x - cx, d[0])
    } else {
        (a.x - cx, a.y - cy, d[1])
    };
    let rest = r * r - perp * perp;
    if rest < 0.0 {
        return None;
    }
    let half = rest.sqrt();
    let (u, v) = ((-half - axis_a) / axis_d, (half - axis_a) / axis_d);
    Some((u.min(v), u.max(v)))
}

fn intersect_with_span(spans: &[Span], bound: Span) -> Vec<Span> {
    spans
        .iter()
        .filter_map(|&(a, b)| {
            let (lo, hi) = (a.max(bound.0), b.min(bound.1));
            (lo < hi).then_some((lo, hi))
        })
        .collect()
}

pub(super) fn joined(mut spans: Vec<Span>) -> Vec<Span> {
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<Span> = Vec::new();
    for span in spans {
        if let Some(last) = out.last_mut()
            && span.0 <= last.1
        {
            last.1 = last.1.max(span.1);
        } else {
            out.push(span);
        }
    }
    out
}

fn minus(spans: Vec<Span>, remove: Span) -> Vec<Span> {
    let mut out = Vec::new();
    for (lo, hi) in spans {
        if remove.1 <= lo || remove.0 >= hi {
            out.push((lo, hi));
            continue;
        }
        if remove.0 > lo {
            out.push((lo, remove.0));
        }
        if remove.1 < hi {
            out.push((remove.1, hi));
        }
    }
    out
}
