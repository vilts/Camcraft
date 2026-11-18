//! The machined body carried from one setup into the next (#317): a
//! two-sided column body (top and floor per column) moved through the
//! rigid transform between two setups by re-indexing its lattice.
//!
//! Provenance: our own design after #308's research note - one
//! workpiece kept on one lattice and viewed through each setup's
//! transform instead of re-sampled onto a new grid (Stifter 1995 on
//! dexel resampling error; Museth 2013 on keeping a sparse grid apart
//! from its transform). The column model is the classic Z-map with a
//! second (floor) height per column. No vendor material consulted.
//!
//! ## The laws here
//!
//! - **Re-index, never re-sample.** A turn that keeps the tool axis on
//!   the column axis (flip over X or Y, quarter turns about Z, and
//!   their products: the eight signed permutations that keep Z on +-Z)
//!   maps every column centre onto a column centre of an image lattice
//!   exactly. Such a carry moves numbers between cells and loses
//!   nothing. A side-on quarter turn is `carry_side`'s (#874); a turn
//!   off the quarter (30 degrees about Z) would need a re-sample at an
//!   angle, and is refused by name ([`CARRY_TURN_REFUSAL`]), never guessed.
//! - **Two sides.** A column holds material from its floor up to its
//!   top. Upright cuts only lower tops; a flip swaps the sides
//!   (`top' = tz - floor`, `floor' = tz - top`). A column cut through
//!   (`top < floor`) stays cut through on either side, so its damage
//!   cannot be flipped out of sight.
//! - **The floor is judged once it has faced a tool.** After a flip the
//!   floor is the face an earlier setup machined: the verdict then
//!   measures the floor against the part's bottom as well as the top
//!   against its top. A body never flipped is judged exactly as the
//!   one-sided column verdict always was (single-setup verdicts do not
//!   move).

use super::dexel::Dexel;
use super::kernel::part_surface;

use crate::geometry::{Point3, Triangle};

/// The refusal for a turn no carry holds exactly (off the quarter turns).
/// Plain words (#866, shortened by #874): what the simulation carries -
/// the caller says which turn and what to do.
pub const CARRY_TURN_REFUSAL: &str = "the stock simulation carries machined stock only through quarter turns (steps of 90° about X, Y or Z)";

/// How close a rotation entry must sit to -1, 0 or 1 to be read as one.
const UNIT_EPS: f64 = 1e-9;

/// A rigid transform whose rotation keeps Z on +-Z and maps the XY
/// axes onto +-X / +-Y: `xy' = m * xy + t.xy`, `z' = s * z + t.z`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LatticeTurn {
    m: [[i8; 2]; 2],
    s: i8,
    t: [f64; 3],
}

pub(super) fn unit(v: f64) -> Option<i8> {
    if (v - 1.0).abs() <= UNIT_EPS {
        Some(1)
    } else if (v + 1.0).abs() <= UNIT_EPS {
        Some(-1)
    } else if v.abs() <= UNIT_EPS {
        Some(0)
    } else {
        None
    }
}

impl LatticeTurn {
    /// The identity: carries a body onto itself.
    pub const IDENTITY: LatticeTurn = LatticeTurn { m: [[1, 0], [0, 1]], s: 1, t: [0.0; 3] };

    /// Read a rotation matrix (row-major, `p' = r * p + t`) as a lattice
    /// turn, or refuse by name.
    ///
    /// # Errors
    /// A non-finite transform, a rotation that is not a signed
    /// permutation keeping Z vertical, or one that mirrors.
    pub fn from_matrix(r: [[f64; 3]; 3], t: [f64; 3]) -> Result<Self, String> {
        if r.iter().flatten().chain(t.iter()).any(|v| !v.is_finite()) {
            return Err("the transform between the setups is not finite".into());
        }
        let mut q = [[0i8; 3]; 3];
        for (i, row) in r.iter().enumerate() {
            for (j, v) in row.iter().enumerate() {
                q[i][j] = unit(*v).ok_or(CARRY_TURN_REFUSAL)?;
            }
        }
        let vertical = q[2][0] == 0 && q[2][1] == 0 && q[0][2] == 0 && q[1][2] == 0 && q[2][2] != 0;
        let m = [[q[0][0], q[0][1]], [q[1][0], q[1][1]]];
        let row_ok = |a: [i8; 2]| (a[0] == 0) != (a[1] == 0);
        let col_ok = (m[0][0] == 0) != (m[1][0] == 0) && (m[0][1] == 0) != (m[1][1] == 0);
        if !(vertical && row_ok(m[0]) && row_ok(m[1]) && col_ok) {
            return Err(CARRY_TURN_REFUSAL.into());
        }
        let det = i32::from(m[0][0]) * i32::from(m[1][1]) - i32::from(m[0][1]) * i32::from(m[1][0]);
        if det * i32::from(q[2][2]) != 1 {
            return Err("the transform between the setups mirrors the part - no rigid re-clamp does that".into());
        }
        Ok(LatticeTurn { m, s: q[2][2], t })
    }

    /// Does this turn put the part upside down?
    #[must_use]
    pub fn flips(&self) -> bool {
        self.s < 0
    }

    /// Does it swap the X and Y axes (a quarter turn)?
    #[must_use]
    pub fn swaps_xy(&self) -> bool {
        self.m[0][0] == 0
    }

    /// The turn's signed permutation (#637 E2a): `m` on XY, `s` on Z.
    pub(crate) fn linear(&self) -> ([[i8; 2]; 2], i8) {
        (self.m, self.s)
    }

    /// The turn's translation: what is left of it once the signed
    /// permutation is baked into a field (#637 E2a).
    #[must_use]
    pub fn shift(&self) -> Point3 {
        Point3::new(self.t[0], self.t[1], self.t[2])
    }

    /// Map a point.
    #[must_use]
    pub fn apply(&self, p: Point3) -> Point3 {
        let (m, t) = (self.m, self.t);
        Point3::new(
            f64::from(m[0][0]) * p.x + f64::from(m[0][1]) * p.y + t[0],
            f64::from(m[1][0]) * p.x + f64::from(m[1][1]) * p.y + t[1],
            f64::from(self.s) * p.z + t[2],
        )
    }

    /// This turn, then `next` (#637): `then(next).apply(p) ==
    /// next.apply(self.apply(p))`. Signed permutations compose into
    /// signed permutations, so the result is a lattice turn again.
    #[must_use]
    pub fn then(&self, next: LatticeTurn) -> LatticeTurn {
        let (a, b) = (self.m, next.m);
        let mul = |i: usize, j: usize| b[i][0] * a[0][j] + b[i][1] * a[1][j];
        let m = [[mul(0, 0), mul(0, 1)], [mul(1, 0), mul(1, 1)]];
        let t = [
            f64::from(b[0][0]) * self.t[0] + f64::from(b[0][1]) * self.t[1] + next.t[0],
            f64::from(b[1][0]) * self.t[0] + f64::from(b[1][1]) * self.t[1] + next.t[1],
            f64::from(next.s) * self.t[2] + next.t[2],
        ];
        LatticeTurn { m, s: self.s * next.s, t }
    }

    /// The pre-image of an image point: the inverse map (#637), exact -
    /// the XY part is the orthogonal `m`'s transpose and Z's sign is
    /// its own inverse. `unapply(apply(p)) == p` to rounding - exactly
    /// for integer shifts, within an ulp of a fractional one (review of
    /// #646).
    #[must_use]
    pub fn unapply(&self, p: Point3) -> Point3 {
        let (x, y) = self.back_xy(p.x, p.y);
        Point3::new(x, y, f64::from(self.s) * (p.z - self.t[2]))
    }

    /// The XY pre-image of an image point: `m` is orthogonal, so its
    /// inverse is its transpose.
    fn back_xy(&self, x: f64, y: f64) -> (f64, f64) {
        let (m, t) = (self.m, self.t);
        let (u, v) = (x - t[0], y - t[1]);
        (f64::from(m[0][0]) * u + f64::from(m[1][0]) * v, f64::from(m[0][1]) * u + f64::from(m[1][1]) * v)
    }
}

/// The machined body: the column grid's tops plus a floor per column
/// (`+inf` where the column holds no material at all).
#[derive(Clone, Debug)]
pub struct Body {
    pub dex: Dexel,
    pub(super) floor: Vec<f64>,
    floor_judged: bool,
}

impl Body {
    /// A body whose every material column stands on one floor height
    /// (the box and cylinder stocks).
    #[must_use]
    pub fn on_floor(mut dex: Dexel, z: f64) -> Self {
        if let Some(h) = &mut dex.history {
            std::sync::Arc::make_mut(h).lo.z = z;
        }
        let floor = (0..dex.nx * dex.ny).map(|k| if dex.top_at(k).is_finite() { z } else { f64::INFINITY }).collect();
        Body { dex, floor, floor_judged: false }
    }

    /// A body whose floor is the lowest surface of a stock mesh (the
    /// from-model stock): the mesh read from below. Retains closed mesh
    /// provenance when `FixtureMesh::build(facets)` succeeds so side-face
    /// contact resolves finer than one grid pitch.
    #[must_use]
    pub fn on_mesh(mut dex: Dexel, facets: &[Triangle]) -> Self {
        let prior = dex.history.take();
        if let Ok(mesh) = crate::fixture_mesh::FixtureMesh::build(facets) {
            let (mut mlo, mut mhi) = (
                Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
                Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
            );
            for t in facets {
                for p in [t.a, t.b, t.c] {
                    mlo = Point3::new(mlo.x.min(p.x), mlo.y.min(p.y), mlo.z.min(p.z));
                    mhi = Point3::new(mhi.x.max(p.x), mhi.y.max(p.y), mhi.z.max(p.z));
                }
            }
            if mlo.is_finite() && mhi.is_finite() {
                let (lo, hi) = match prior.as_deref().filter(|h| !h.active && h.cuts.is_empty() && h.clips.is_empty()) {
                    Some(h) => (
                        Point3::new(
                            h.lo.x.max(mlo.x),
                            h.lo.y.max(mlo.y),
                            if h.lo.z.is_finite() { h.lo.z.max(mlo.z) } else { mlo.z },
                        ),
                        Point3::new(h.hi.x.min(mhi.x), h.hi.y.min(mhi.y), h.hi.z.min(mhi.z)),
                    ),
                    None => (mlo, mhi),
                };
                dex.history = Some(std::sync::Arc::new(super::carry_history::History::with_mesh(
                    lo,
                    hi,
                    dex.cell,
                    std::sync::Arc::new(mesh),
                )));
            }
        }
        let floor = bottom_surface(facets, &dex).into_iter().map(|b| if b.is_finite() { b } else { f64::INFINITY }).collect();
        Body { dex, floor, floor_judged: false }
    }

    /// The floor under column `k` (`+inf`: no material).
    #[must_use]
    pub fn floor_at(&self, k: usize) -> f64 {
        self.floor.get(k).copied().unwrap_or(f64::INFINITY)
    }

    /// Has the floor faced a tool, so the verdict judges it?
    #[must_use]
    pub fn floor_judged(&self) -> bool {
        self.floor_judged
    }

    /// Is column `k` material-free (air, or cut through)?
    #[must_use]
    pub fn empty_at(&self, k: usize) -> bool {
        let (t, f) = (self.dex.top_at(k), self.floor_at(k));
        !(t.is_finite() && f.is_finite()) || t < f
    }

    /// The material volume standing in the body.
    #[must_use]
    pub fn volume(&self) -> f64 {
        let a = self.dex.cell * self.dex.cell;
        (0..self.dex.nx * self.dex.ny)
            .filter(|&k| !self.empty_at(k))
            .map(|k| self.material_spans_at(k).map_or(self.dex.top_at(k) - self.floor_at(k),
                |spans| spans.iter().map(|(lo, hi)| hi - lo).sum()) * a)
            .sum()
    }

    /// The bounds of the material standing, `None` if there is none.
    #[must_use]
    pub fn bounds(&self) -> Option<(Point3, Point3)> {
        let h = self.dex.cell * 0.5;
        let mut lo = Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY);
        let mut hi = Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for iy in 0..self.dex.ny {
            for ix in 0..self.dex.nx {
                let k = ix + iy * self.dex.nx;
                if self.empty_at(k) {
                    continue;
                }
                let c = self.dex.center(ix, iy);
                lo = Point3::new(lo.x.min(c.x - h), lo.y.min(c.y - h), lo.z.min(self.floor_at(k)));
                hi = Point3::new(hi.x.max(c.x + h), hi.y.max(c.y + h), hi.z.max(self.dex.top_at(k)));
            }
        }
        (lo.is_finite() && hi.is_finite()).then_some((lo, hi))
    }

    /// A body from per-column tops (in `dex`) and floors, as another
    /// engine read them (#637 step D: a carried field read back onto a
    /// lattice). `floor.len()` must match the lattice.
    pub(crate) fn from_parts(dex: Dexel, floor: Vec<f64>, floor_judged: bool) -> Result<Self, String> {
        if floor.len() != dex.nx * dex.ny {
            return Err(format!("a body of {} floors on a {}x{} lattice", floor.len(), dex.nx, dex.ny));
        }
        Ok(Body { dex, floor, floor_judged })
    }

    /// The same body seen through `turn`: its lattice re-indexed onto
    /// the image lattice, heights mapped (and swapped by a flip). No
    /// value is interpolated.
    ///
    /// # Errors
    /// A lattice the image cannot hold (never for a well-formed body).
    pub fn carry(&self, turn: &LatticeTurn) -> Result<Body, String> {
        if let Some(body) = self.exact_lattice(turn) { return body; }
        let d = &self.dex;
        let (w, h) = (d.nx as f64 * d.cell, d.ny as f64 * d.cell);
        let corners = [(d.x0, d.y0), (d.x0 + w, d.y0), (d.x0, d.y0 + h), (d.x0 + w, d.y0 + h)];
        let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
        for (x, y) in corners {
            let p = turn.apply(Point3::new(x, y, 0.0));
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
        }
        let (nx, ny) = if turn.swaps_xy() { (d.ny, d.nx) } else { (d.nx, d.ny) };
        let (s, tz) = (f64::from(turn.s), turn.t[2]);
        let mut top = vec![f64::NEG_INFINITY; nx * ny];
        let mut floor = vec![f64::INFINITY; nx * ny];
        for jy in 0..ny {
            for jx in 0..nx {
                let cx = x0 + (jx as f64 + 0.5) * d.cell;
                let cy = y0 + (jy as f64 + 0.5) * d.cell;
                let (px, py) = turn.back_xy(cx, cy);
                let (ix, iy) = (((px - d.x0) / d.cell).floor(), ((py - d.y0) / d.cell).floor());
                if !(ix >= 0.0 && iy >= 0.0 && ix < d.nx as f64 && iy < d.ny as f64) {
                    return Err(format!("the carried lattice lost column ({jx}, {jy}) - refused rather than guessed"));
                }
                let k = ix as usize + iy as usize * d.nx;
                let (t, f) = (d.top_at(k), self.floor_at(k));
                let j = jx + jy * nx;
                if s > 0.0 {
                    top[j] = t + tz;
                    floor[j] = f + tz;
                } else {
                    top[j] = tz - f;
                    floor[j] = tz - t;
                }
            }
        }
        let stock_z = top.iter().copied().filter(|v| v.is_finite()).fold(f64::NEG_INFINITY, f64::max);
        let stock_z = if stock_z.is_finite() { stock_z } else { 0.0 };
        let mut dex = Dexel::from_grid(x0, y0, d.cell, nx, ny, stock_z, top);
        dex.history = d.history.as_ref().map(|h| std::sync::Arc::new(
            h.turned(super::carry_exact::Frame::lattice(turn))
        ));
        Ok(Body { dex, floor, floor_judged: self.floor_judged || turn.flips() })
    }

    /// The tops as the eye should draw them: a column with no material
    /// (cut through on either side) is air, not a sheet at its last top.
    #[must_use]
    pub fn display(&self) -> Dexel {
        if !self.floor_judged {
            return self.dex.clone();
        }
        let mut d = self.dex.clone();
        for k in 0..d.nx * d.ny {
            if self.empty_at(k) {
                d.set(k, f64::NEG_INFINITY);
            }
        }
        d
    }
}

pub(super) fn mirror_z(t: &Triangle) -> Triangle {
    let m = |p: Point3| Point3::new(p.x, p.y, -p.z);
    Triangle::new(m(t.a), m(t.c), m(t.b))
}

/// The lowest facet surface over each column of `dex`'s lattice
/// (`-inf` from `part_surface`'s law is mapped back to `+inf` by the
/// callers: no surface, no floor).
fn bottom_surface(facets: &[Triangle], dex: &Dexel) -> Vec<f64> {
    let flipped: Vec<Triangle> = facets.iter().map(mirror_z).collect();
    part_surface(&flipped, dex.x0, dex.y0, dex.cell, dex.nx, dex.ny).into_iter().map(|z| -z).collect()
}
