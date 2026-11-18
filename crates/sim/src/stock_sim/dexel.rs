//! Split from `stock_sim.rs` by M5.0.1; structure only.

use super::dexel_arc::Atom;
use super::kernel::{MAX_CELLS_PER_AXIS, StockError};
use super::tiles::{Tile, TILE};

use crate::cl::{Move, MoveKind};
use crate::geo2d::{Loop, Point2, Region};
use crate::geometry::Point3;
/// The height field: material top per cell, row-major from (`x0`, `y0`)
/// at the lower-left corner, cell centers at `x0 + (i + 0.5) * cell`.
/// The storage is tiled (M5.3 E2, see `tiles`): `top_at(k)` answers
/// what the dense `Vec` used to hold at `top[k]`, value for value.
#[derive(Clone, Debug)]
pub struct Dexel {
    pub x0: f64,
    pub y0: f64,
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    /// The surface the whole grid started at: removal is counted from
    /// here, so a column's removed depth is one subtraction, not a
    /// history.
    pub stock_z: f64,
    /// Top of the material standing at each column, in `TILE`-squared
    /// boxes: `None` is "every cell at `stock_z`", a cut tile carries
    /// its own cells (the `tiles` module owns the representation and
    /// every law that reads or writes it).
    pub(super) tiles: Vec<Option<Tile>>,
    /// Continuous box/cut provenance for a later side turn. Direct grid
    /// edits clear it; the validated program and clip paths preserve it.
    pub(super) history: Option<std::sync::Arc<super::carry_history::History>>,
}

impl Dexel {
    /// A stock block `x0..x1 × y0..y1 × ..top_z` on the given cell size:
    /// every column's top starts at the stock surface.
    pub fn block(x0: f64, y0: f64, x1: f64, y1: f64, top_z: f64, cell: f64) -> Result<Self, StockError> {
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() && top_z.is_finite())
            || x1 <= x0
            || y1 <= y0
            || !(cell.is_finite() && cell > 0.0)
        {
            return Err(StockError::Invalid("stock box and cell must be finite, box non-empty, cell positive".into()));
        }
        let nx = ((x1 - x0) / cell).ceil() as usize;
        let ny = ((y1 - y0) / cell).ceil() as usize;
        if nx == 0 || ny == 0 || nx > MAX_CELLS_PER_AXIS || ny > MAX_CELLS_PER_AXIS {
            return Err(StockError::Invalid(format!(
                "grid would be {nx}x{ny} cells; at most {MAX_CELLS_PER_AXIS} per axis"
            )));
        }
        Ok(Dexel {
            x0,
            y0,
            cell,
            nx,
            ny,
            stock_z: top_z,
            tiles: vec![None; nx.div_ceil(TILE) * ny.div_ceil(TILE)],
            history: Some(std::sync::Arc::new(super::carry_history::History::block(
                x0, y0, x1, y1, top_z, cell,
            ))),
        })
    }

    #[must_use]
    pub fn center(&self, ix: usize, iy: usize) -> Point2 {
        Point2::new(
            self.x0 + (ix as f64 + 0.5) * self.cell,
            self.y0 + (iy as f64 + 0.5) * self.cell,
        )
    }

    #[must_use]
    pub fn col(&self, q: Point2) -> Option<usize> {
        let ix = (q.x - self.x0) / self.cell;
        let iy = (q.y - self.y0) / self.cell;
        if ix < 0.0 || iy < 0.0 || ix >= self.nx as f64 || iy >= self.ny as f64 {
            return None;
        }
        Some(ix as usize + (iy as usize) * self.nx)
    }

    /// Remove material down to the LOWEST Z of one straight, Z-linear
    /// chord whose disc of radius `r` touches the cell center - the
    /// atom every move reduces to. A zero-XY-length chord (a plunge)
    /// is its own interval. The reach interval is exact for a segment,
    /// so this is never an endpoint approximation: an oblique move
    /// cannot over-report its cut, and cannot under-report one either.
    pub fn cut_chord(&mut self, a: Point3, b: Point3, r: f64) {
        if !(a.is_finite() && b.is_finite()) || !(r.is_finite() && r >= 0.0) {
            return; // non-finite geometry cuts nothing; refusing lives
                    // in the callers' validation laws, not mid-sweep
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        // Cells whose centers can be within r of the segment: the
        // segment bbox grown by r, clamped to the grid. Clamping is
        // harmless - every candidate still faces the exact reach test.
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) - r).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + r).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) - r).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + r).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                // Parameter interval where the disc reaches q: the
                // shared reach law (see `reach`), exact on the segment.
                let Some((t0, t1)) = reach(q.x - a.x, q.y - a.y, dx, dy, r) else {
                    continue;
                };
                // Z is linear along the chord, so its minimum over the
                // interval is at one of the interval ends.
                let za = a.z + (b.z - a.z) * t0;
                let zb = a.z + (b.z - a.z) * t1;
                let cut = za.min(zb);
                self.lower(ix + iy * self.nx, cut);
            }
        }
    }

    /// The cone atom (M2.4 B): a cone of half-angle `alpha` from the
    /// axis, widening UPWARD from its virtual sharp, whose TIP
    /// (apex) travels the straight, Z-linear chord `a -> b`; `cot` is
    /// cot(alpha). Above a cell center `q` at horizontal distance
    /// `d(t)` from the axis at instant `t`, the cone's lowest steel
    /// sits at `z(t) + d(t) * cot` - the surface itself - so the cut
    /// is the minimum of that over the reach window (`reach` at the
    /// cone's full radius: beyond it there is no steel, exactly like
    /// the ball's horizon). Unlike `cut_ball` the bottom function is
    /// convex in `t` the same way (`|w - d t|` is convex, `+` linear),
    /// so the minimum is again at a window end or the single interior
    /// stationary point: `g'(t) = k + cot (t*D2 - w.d)/rho`, whose
    /// squaring gives the quadratic below and the unsquared law keeps
    /// only roots where `(t*D2 - w.d)` is opposite `k` in sign (for
    /// `k = 0`, the constant-Z ride, the double root IS the
    /// XY-nearest instant `t_c` - always evaluated too, so a root
    /// lost to rounding degrades toward an endpoint, never past it).
    /// The `min` with the column's own top makes the atom
    /// self-limiting: a column beyond the surface's reach simply
    /// keeps its stock, which is why no footprint cap is needed here.
    pub fn cut_cone(&mut self, a: Point3, b: Point3, r: f64, cot: f64) {
        if !(a.is_finite() && b.is_finite()) || !(r.is_finite() && r >= 0.0) {
            return; // the cut_chord gate: validation lives upstream
        }
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let dd = dx * dx + dy * dy;
        let k = b.z - a.z;
        let fx = |v: f64| ((v - self.x0) / self.cell).floor();
        let fy = |v: f64| ((v - self.y0) / self.cell).floor();
        let lo_x = fx(a.x.min(b.x) - r).max(0.0) as usize;
        let hi_x = fx(a.x.max(b.x) + r).clamp(0.0, self.nx as f64 - 1.0) as usize;
        let lo_y = fy(a.y.min(b.y) - r).max(0.0) as usize;
        let hi_y = fy(a.y.max(b.y) + r).clamp(0.0, self.ny as f64 - 1.0) as usize;
        if lo_x >= self.nx || lo_y >= self.ny {
            return;
        }
        for iy in lo_y..=hi_y {
            for ix in lo_x..=hi_x {
                let q = self.center(ix, iy);
                let (wx, wy) = (q.x - a.x, q.y - a.y);
                let Some((t0, t1)) = reach(wx, wy, dx, dy, r) else {
                    continue;
                };
                let f = |t: f64| {
                    let ex = wx - dx * t;
                    let ey = wy - dy * t;
                    a.z + k * t + cot * (ex * ex + ey * ey).sqrt()
                };
                let t_c = if dd > f64::EPSILON {
                    (wx * dx + wy * dy) / dd
                } else {
                    0.0
                };
                let mut cut = f(t0).min(f(t1)).min(f(t_c.clamp(t0, t1)));
                if dd > f64::EPSILON {
                    // (t*dd - wd)^2 = (k*tan a)^2 * rho(t)^2, tan a = 1/cot.
                    let wd = wx * dx + wy * dy;
                    let ww = wx * wx + wy * wy;
                    let s2 = (k / cot).powi(2);
                    let qa = dd * (dd - s2);
                    let qb = 2.0 * s2 * wd - 2.0 * dd * wd;
                    let qc = wd * wd - s2 * ww;
                    if qa.abs() <= 1e-300 || !qa.is_finite() {
                        // The move runs along the cone's own slope
                        // (qa = 0): the stationary equation is linear.
                        if qb.abs() > 1e-300 && qb.is_finite() {
                            let t = -qc / qb;
                            if t > t0 && t < t1 && (t * dd - wd) * k <= 0.0 {
                                cut = cut.min(f(t));
                            }
                        }
                    } else {
                        let mut disc = qb * qb - 4.0 * qa * qc;
                        if disc < 0.0 && disc > -qb * qb * 1e-12 {
                            disc = 0.0; // a tangential double root erased by rounding
                        }
                        if disc >= 0.0 && disc.is_finite() {
                            let sd = disc.sqrt();
                            for t in [(-qb - sd) / (2.0 * qa), (-qb + sd) / (2.0 * qa)] {
                                if t > t0 && t < t1 && (t * dd - wd) * k <= 0.0 && t.is_finite() {
                                    cut = cut.min(f(t));
                                }
                            }
                        }
                    }
                }
                self.lower(ix + iy * self.nx, cut);
            }
        }
    }

    /// Cone version of `subtract`: moves carry TIP (= virtual sharp)
    /// positions, the codebase's tip-first convention read on the cone
    /// the chamfer mill actually is, so the axis path IS the move path
    /// and no center lift applies (a ball's center is above its tip;
    /// a cone's apex is its tip). XY arcs sweep the true arc under the
    /// same helix law as `subtract` (`dexel_arc`).
    pub fn subtract_cone(&mut self, m: &Move, r: f64, cot: f64, chord_tol: f64) {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            return;
        }
        match m.arc {
            None => self.cut_cone(m.from, m.to, r, cot),
            // XZ / YZ arcs (M11.2 A): their own 3-D chords.
            Some(arc) if !arc.plane.is_xy() => {
                for (a, b) in m.segments3(chord_tol) {
                    self.cut_cone(a, b, r, cot);
                }
            }
            // XY arcs sweep the true arc (#402), not chords.
            Some(arc) => self.cut_xy_arc(m, arc, r, Atom::Cone(cot)),
        }
    }

    /// Ball-nose version of `subtract`: moves carry TIP positions (the
    /// whole codebase stores toolpaths tip-first), so the CENTER path
    /// is the tip path lifted by `r` in Z and the swept solid is a
    /// sphere at every instant. XY arcs sweep the true arc under the
    /// same helix law as `subtract` (`dexel_arc`).
    pub fn subtract_ball(&mut self, m: &Move, r: f64, chord_tol: f64) {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            return;
        }
        match m.arc {
            None => self.cut_ball(
                Point3::new(m.from.x, m.from.y, m.from.z + r),
                Point3::new(m.to.x, m.to.y, m.to.z + r),
                r,
            ),
            // XZ / YZ arcs (M11.2 A): their own 3-D chords, lifted.
            Some(arc) if !arc.plane.is_xy() => {
                for (a, b) in m.segments3(chord_tol) {
                    self.cut_ball(
                        Point3::new(a.x, a.y, a.z + r),
                        Point3::new(b.x, b.y, b.z + r),
                        r,
                    );
                }
            }
            // XY arcs sweep the true arc (#402), not chords.
            Some(arc) => self.cut_xy_arc(m, arc, r, Atom::Ball),
        }
    }

    /// Subtract one move: rapids touch nothing, straight moves are one
    /// chord, XY arcs sweep the true arc with Z linear in the swept
    /// angle (a helix descends uniformly with its sweep - the same
    /// reading `sim::path_len` takes; `dexel_arc`, #402: inscribed
    /// chords stood a sagitta inside the arc, a false gouge on a convex
    /// corner and a hidden one on a concave corner). XZ / YZ arcs keep
    /// their in-plane chords.
    pub fn subtract(&mut self, m: &Move, r: f64, chord_tol: f64) {
        if m.kind == MoveKind::Rapid || !m.is_finite() {
            return;
        }
        match m.arc {
            None => self.cut_chord(m.from, m.to, r),
            // XZ / YZ arcs (M11.2 A): their own 3-D chords.
            Some(arc) if !arc.plane.is_xy() => {
                for (a, b) in m.segments3(chord_tol) {
                    self.cut_chord(a, b, r);
                }
            }
            // XY arcs sweep the true arc (#402), not chords.
            Some(arc) => self.cut_xy_arc(m, arc, r, Atom::Flat),
        }
    }

    /// Rectangle regions of the cells whose top stands above `z` (the
    /// mask's horizontal runs merged into strips, the strips fused by
    /// the region algebra). This is a grid query answer in region
    /// clothing: cell-quantized by construction, and its consumer's
    /// tolerance laws - not this shape's corners - decide what is cut.
    pub fn above(&self, z: f64) -> Result<Region, StockError> {
        // The strip law lives with the seam's reader (M7.3), so both
        // arms merge their masks by one body of code.
        super::columns::above_strips(self.x0, self.y0, self.cell, self.nx, self.ny, |k| self.top_at(k), z)
    }

    /// Conservative XY rotation about `origin` (#760). Each target cell takes
    /// the maximum of every source cell in its inverse-mapped bounding box.
    /// The box encloses the entire rotated cell, so it can retain extra stock
    /// but cannot miss a narrow standing region between samples.
    pub fn rotated_z_conservative(&self, origin: Point2, angle_rad: f64) -> Result<Self, StockError> {
        if !(origin.x.is_finite() && origin.y.is_finite() && angle_rad.is_finite()) {
            return Err(StockError::Invalid("dexel rotation requires finite origin and angle".into()));
        }
        if angle_rad == 0.0 || self.nx == 0 || self.ny == 0 {
            return Ok(self.clone());
        }
        let (s, c) = angle_rad.sin_cos();
        let rot = |x: f64, y: f64, s: f64| {
            Point2::new(origin.x + (x - origin.x) * c - (y - origin.y) * s, origin.y + (x - origin.x) * s + (y - origin.y) * c)
        };
        let (x1, y1) = (self.x0 + self.nx as f64 * self.cell, self.y0 + self.ny as f64 * self.cell);
        let pts = [(self.x0, self.y0), (x1, self.y0), (self.x0, y1), (x1, y1)].map(|(x, y)| rot(x, y, s));
        let lo_x = pts.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let hi_x = pts.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max);
        let lo_y = pts.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
        let hi_y = pts.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max);
        let (mut out, h) = (Dexel::block(lo_x, lo_y, hi_x, hi_y, self.stock_z, self.cell)?, 0.5 * self.cell);
        for iy in 0..out.ny {
            for ix in 0..out.nx {
                let q = out.center(ix, iy);
                let corners = [(-h, -h), (h, -h), (h, h), (-h, h)]
                    .map(|(dx, dy)| rot(q.x + dx, q.y + dy, -s));
                // Include a one-cell halo around the inverse AABB. Besides
                // boundary-touching cells, this covers rounding at grid lines.
                // Overfetch only raises the answer: it never invents clearance.
                let bounds = |base: f64, n: usize, coord: fn(&Point2) -> f64| {
                    let lo = corners.iter().map(coord).fold(f64::INFINITY, f64::min);
                    let hi = corners.iter().map(coord).fold(f64::NEG_INFINITY, f64::max);
                    let start = (((lo - base) / self.cell).floor() - 1.0).clamp(0.0, n as f64) as usize;
                    let end = (((hi - base) / self.cell).floor() + 2.0).clamp(0.0, n as f64) as usize;
                    start..end
                };
                let xs = bounds(self.x0, self.nx, |p| p.x);
                let ys = bounds(self.y0, self.ny, |p| p.y);
                let top = ys.flat_map(|y| xs.clone().map(move |x| x + y * self.nx))
                    .map(|k| self.top_at(k))
                    .filter(|z| !z.is_nan())
                    .reduce(f64::max)
                    .unwrap_or(f64::NEG_INFINITY);
                if top.to_bits() != self.stock_z.to_bits() {
                    out.set(ix + iy * out.nx, top);
                }
            }
        }
        Ok(out)
    }
}

/// The parameter interval of a chord `0 -> d` whose tool disc of
/// radius `r` reaches a point offset `w` from the start - the reach
/// law shared by the flat and ball kernels so both inherit one
/// phantom rule: the window is on the SEGMENT, not the infinite line,
/// and an interval that falls entirely outside `[0, 1]` stays empty
/// (clamping it open would invent a cut at the nearest endpoint - the
/// exact bug the M2.1b phantom test pins). A degenerate `d` (plunge)
/// is its own interval, gated by distance.
pub fn reach(wx: f64, wy: f64, dx: f64, dy: f64, r: f64) -> Option<(f64, f64)> {
    let l2 = dx * dx + dy * dy;
    if l2 <= f64::EPSILON {
        if wx.hypot(wy) > r {
            None
        } else {
            Some((0.0, 1.0))
        }
    } else {
        let t_c = (wx * dx + wy * dy) / l2;
        let cross = wx * dy - wy * dx;
        let d_perp2 = (cross * cross) / l2;
        if d_perp2 > r * r {
            return None;
        }
        let half = (r * r - d_perp2).sqrt() / l2.sqrt();
        let lo = (t_c - half).max(0.0);
        let hi = (t_c + half).min(1.0);
        if lo > hi {
            None
        } else {
            Some((lo, hi))
        }
    }
}

/// One merged strip: cells `ia..ib` of row `iy`, as a closed rectangle.
pub(super) fn strip(x0: f64, y0: f64, cell: f64, ia: usize, ib: usize, iy: usize) -> Result<Region, StockError> {
    let (a, b) = (
        Point2::new(x0 + ia as f64 * cell, y0 + iy as f64 * cell),
        Point2::new(x0 + ib as f64 * cell, y0 + (iy as f64 + 1.0) * cell),
    );
    let loop_ = Loop::new(vec![
        a,
        Point2::new(b.x, a.y),
        b,
        Point2::new(a.x, b.y),
    ])
    .map_err(StockError::Geo)?;
    Region::new(vec![loop_]).map_err(StockError::Geo)
}

pub(super) fn fy0(v: f64, y0: f64, cell: f64) -> f64 {
    ((v - y0) / cell).floor()
}
