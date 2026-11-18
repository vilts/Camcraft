//! Continuous stock carried through quarter turns (#1805).
//! Provenance: rigid transforms, interval CSG, and the Lipschitz distance bound.

use std::sync::Arc;

use super::carry::{Body, LatticeTurn};
use super::carry_history::{History, Span};
use super::carry_side::SideTurn;
use super::dexel::Dexel;
use super::kernel::MAX_CELLS_PER_AXIS;
use crate::geometry::Point3;

/// A signed permutation and translation, from an object's frame to the reader.
#[derive(Clone, Copy, Debug)]
pub(super) struct Frame {
    pub(super) q: [[f64; 3]; 3],
    pub(super) t: Point3,
}

impl Frame {
    pub(super) const IDENTITY: Self =
        Self { q: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]], t: Point3 { x: 0.0, y: 0.0, z: 0.0 } };

    pub(super) fn lattice(turn: &LatticeTurn) -> Self {
        let (m, s) = turn.linear();
        Self {
            q: [
                [f64::from(m[0][0]), f64::from(m[0][1]), 0.0],
                [f64::from(m[1][0]), f64::from(m[1][1]), 0.0],
                [0.0, 0.0, f64::from(s)],
            ],
            t: turn.shift(),
        }
    }

    fn side(turn: &SideTurn) -> Self {
        Self { q: turn.q.map(|r| r.map(f64::from)), t: Point3::new(turn.t[0], turn.t[1], turn.t[2]) }
    }

    pub(super) fn apply(self, p: Point3) -> Point3 {
        let c = [p.x, p.y, p.z];
        let at = |i: usize| self.q[i].iter().zip(c).map(|(a, b)| a * b).sum::<f64>();
        Point3::new(at(0) + self.t.x, at(1) + self.t.y, at(2) + self.t.z)
    }

    pub(super) fn back(self, p: Point3) -> Point3 {
        let c = [p.x - self.t.x, p.y - self.t.y, p.z - self.t.z];
        let at = |j: usize| (0..3).map(|i| self.q[i][j] * c[i]).sum::<f64>();
        Point3::new(at(0), at(1), at(2))
    }

    pub(super) fn ray(self, x: f64, y: f64) -> (Point3, [f64; 3]) {
        (self.back(Point3::new(x, y, 0.0)), self.q[2])
    }

    pub(super) fn then(self, next: Self) -> Self {
        let mut q = [[0.0; 3]; 3];
        for (i, row) in q.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = (0..3).map(|k| next.q[i][k] * self.q[k][j]).sum();
            }
        }
        Self { q, t: next.apply(self.t) }
    }
}

/// The intersection of a vertical reader ray with a box in another frame.
pub(super) fn box_span(frame: Frame, lo: Point3, hi: Point3, x: f64, y: f64) -> Option<Span> {
    let (a, direction) = frame.ray(x, y);
    let (a, lo, hi) = ([a.x, a.y, a.z], [lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z]);
    let (mut enter, mut exit) = (f64::NEG_INFINITY, f64::INFINITY);
    for i in 0..3 {
        let d = direction[i];
        if d == 0.0 {
            if a[i] < lo[i] || a[i] > hi[i] {
                return None;
            }
        } else {
            let (u, v) = ((lo[i] - a[i]) / d, (hi[i] - a[i]) / d);
            enter = enter.max(u.min(v));
            exit = exit.min(u.max(v));
        }
    }
    (enter <= exit).then_some((enter, exit))
}

impl History {
    pub(super) fn turned(&self, turn: Frame) -> Self {
        let mut out = self.clone();
        out.frame = out.frame.then(turn);
        for cut in &mut out.cuts {
            cut.frame = cut.frame.then(turn);
        }
        for clip in &mut out.clips {
            clip.frame = clip.frame.then(turn);
        }
        out.entry = self.entry.as_ref().map(|h| Arc::new(h.turned(turn)));
        out
    }

    pub(super) fn bounds(&self) -> (Point3, Point3) {
        let a = self.frame.apply(self.lo);
        let b = self.frame.apply(self.hi);
        (Point3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)), Point3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)))
    }

    pub(super) fn read(&self, floor_judged: bool) -> Result<Body, String> {
        let (lo, hi) = self.bounds();
        if !(lo.is_finite() && hi.is_finite()) || hi.x <= lo.x || hi.y <= lo.y || hi.z < lo.z {
            return Err("exact side-on carry needs a finite stock floor and bounds".into());
        }
        let count = |span: f64| -> Result<usize, String> {
            let n = (span / self.cell).ceil().max(1.0);
            if !n.is_finite() || n > MAX_CELLS_PER_AXIS as f64 {
                return Err(format!(
                    "exact side-on carry lattice limit: the machined stock is {n} cells tall - at most {MAX_CELLS_PER_AXIS} cells on one axis"
                ));
            }
            Ok(n as usize)
        };
        let (nx, ny) = (count(hi.x - lo.x)?, count(hi.y - lo.y)?);
        let n = nx.checked_mul(ny).ok_or("exact side-on carry lattice is too large")?;
        if n.saturating_mul(self.cuts.len().saturating_add(self.clips.len()).saturating_add(1)) > 10_000_000 {
            return Err("exact side-on carry exceeds its read budget - use a coarser stock grid".into());
        }
        let (x0, y0) = (
            lo.x - (nx as f64 * self.cell - (hi.x - lo.x)) * 0.5,
            lo.y - (ny as f64 * self.cell - (hi.y - lo.y)) * 0.5,
        );
        let mut tops = Vec::with_capacity(n);
        let mut floors = Vec::with_capacity(n);
        for iy in 0..ny {
            for ix in 0..nx {
                let (x, y) = (x0 + (ix as f64 + 0.5) * self.cell, y0 + (iy as f64 + 0.5) * self.cell);
                let spans = self.spans(x, y)?;
                let entry = if spans.is_empty() {
                    self.entry
                        .as_ref()
                        .filter(|h| h.cuts.len() != self.cuts.len() || h.clips.len() != self.clips.len())
                        .map(|h| h.spans(x, y))
                        .transpose()?
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                // A cut-through column is convicted only if this setup
                // received material there. Earlier gouges were judged before
                // the side turn; charging an empty entry ray would measure
                // the whole new height as that old gouge's depth.
                let empty_top = entry.first().map_or(f64::NEG_INFINITY, |s| s.0);
                let empty_floor = entry.last().map_or(f64::INFINITY, |s| s.1);
                floors.push(spans.first().map_or(empty_floor, |s| s.0));
                tops.push(spans.last().map_or(empty_top, |s| s.1));
            }
        }
        let mut dex = Dexel::from_grid(x0, y0, self.cell, nx, ny, hi.z, tops);
        dex.history = Some(Arc::new(self.clone()));
        Body::from_parts(dex, floors, floor_judged)
    }
}

impl Body {
    pub(super) fn exact_side(&self, turn: &SideTurn) -> Option<Result<Body, String>> {
        self.dex.history.as_ref().map(|h| {
            let mut h = h.turned(Frame::side(turn));
            h.active = true;
            h.entry = None;
            h.entry = Some(Arc::new(h.clone()));
            h.read(false)
        })
    }

    pub(super) fn exact_lattice(&self, turn: &LatticeTurn) -> Option<Result<Body, String>> {
        self.dex
            .history
            .as_ref()
            .filter(|h| h.active)
            .map(|h| h.turned(Frame::lattice(turn)).read(self.floor_judged() || turn.flips()))
    }

    /// The occupied intervals on a column, preserving a carried cavity.
    /// A failed exact read refuses instead of treating unread stock as air.
    ///
    /// # Errors
    /// An exact ray that exceeds its budget, an invalid column index,
    /// or a non-finite geometric reading.
    pub fn material_spans_at(&self, k: usize) -> Result<Vec<Span>, String> {
        if let Some(h) = self.dex.history.as_ref().filter(|h| h.active) {
            if k >= self.dex.cells() {
                return Err("stock column index is outside the lattice".into());
            }
            let c = self.dex.center(k % self.dex.nx, k / self.dex.nx);
            return h.spans(c.x, c.y);
        }
        Ok(if self.empty_at(k) { Vec::new() } else { vec![(self.floor_at(k), self.dex.top_at(k))] })
    }
}
