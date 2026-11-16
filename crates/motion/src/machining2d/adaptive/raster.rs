//! The planner's in-process stock: a material bitmap of one cut plane,
//! cut by exact capsule rows, read by the front-half contact angle (the
//! engagement arc of peripheral milling, `quality`'s provenance). The
//! planner asks it "what would this walk load the tool with" BEFORE it
//! commits a pass - the measured-not-asserted law, at planning time.

use crate::geo2d::{Point2, Region};

use crate::machining2d::GenError;

/// Front-half rim samples (over 180 degrees): the measuring stock's
/// own count, so a reading here is a reading there.
const FRONT_SAMPLES: usize = crate::quality::RIM_SAMPLES;

/// Most cells the planner grid may hold; past it the cell grows.
const MAX_CELLS: usize = 6_000_000;

#[derive(Clone, Debug)]
pub(super) struct Raster {
    x0: f64,
    y0: f64,
    pub(super) h: f64,
    nx: usize,
    ny: usize,
    cells: Vec<bool>,
    /// The part's walls (outside the reach): read as the material a
    /// strip on the wall continues into (`quality`'s wall law).
    wall: std::rc::Rc<Vec<bool>>,
}

impl Raster {
    /// Material = the cells whose centers lie inside `reg` (even-odd
    /// scanlines), on a cell of `h` (grown until the grid fits).
    pub(super) fn from_region(reg: &Region, h: f64, pad: f64) -> Result<Self, GenError> {
        let Some((lo, hi)) = reg.bounds() else {
            return Err(GenError::Empty("nothing to clear".into()));
        };
        if !(h.is_finite() && h > 0.0 && pad.is_finite()) {
            return Err(GenError::Invalid("planner grid must be finite and positive".into()));
        }
        let (w, ht) = (hi.x - lo.x + 2.0 * pad, hi.y - lo.y + 2.0 * pad);
        let mut h = h;
        while (w / h).ceil() * (ht / h).ceil() > MAX_CELLS as f64 {
            h *= 1.25;
        }
        let nx = (w / h).ceil().max(1.0) as usize;
        let ny = (ht / h).ceil().max(1.0) as usize;
        let mut r = Raster { x0: lo.x - pad, y0: lo.y - pad, h, nx, ny, cells: vec![false; nx * ny], wall: std::rc::Rc::new(Vec::new()) };
        let mut xs: Vec<f64> = Vec::new();
        for iy in 0..ny {
            let y = r.y0 + (iy as f64 + 0.5) * h;
            xs.clear();
            for l in reg.loops() {
                let pts = l.points();
                let n = pts.len();
                for i in 0..n {
                    let (a, b) = (pts[i], pts[(i + 1) % n]);
                    let (ylo, yhi) = if a.y <= b.y { (a.y, b.y) } else { (b.y, a.y) };
                    if y >= ylo && y < yhi {
                        xs.push(a.x + (y - a.y) * (b.x - a.x) / (b.y - a.y));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.as_chunks::<2>().0 {
                let i0 = ((pair[0] - r.x0) / h - 0.5).ceil().max(0.0) as usize;
                let i1 = ((pair[1] - r.x0) / h - 0.5).floor();
                if i1 < 0.0 {
                    continue;
                }
                let i1 = (i1 as usize).min(nx - 1);
                for ix in i0..=i1.min(nx - 1) {
                    r.cells[ix + iy * nx] = true;
                }
            }
        }
        r.wall = std::rc::Rc::new(r.cells.iter().map(|&c| !c).collect());
        Ok(r)
    }

    /// What the rim sample at `p` stands in (off the grid: wall).
    #[inline]
    fn rim(&self, p: Point2) -> crate::quality::Rim {
        use crate::quality::Rim;
        let fx = (p.x - self.x0) / self.h;
        let fy = (p.y - self.y0) / self.h;
        if !(fx >= 0.0 && fy >= 0.0) || fx >= self.nx as f64 || fy >= self.ny as f64 {
            return Rim::Wall;
        }
        let i = fx as usize + fy as usize * self.nx;
        if self.cells[i] {
            Rim::Stock
        } else if self.wall[i] {
            Rim::Wall
        } else {
            Rim::Open
        }
    }

    #[inline]
    pub(super) fn mat(&self, p: Point2) -> bool {
        let fx = (p.x - self.x0) / self.h;
        let fy = (p.y - self.y0) / self.h;
        if !(fx >= 0.0 && fy >= 0.0) || fx >= self.nx as f64 || fy >= self.ny as f64 {
            return false;
        }
        self.cells[fx as usize + fy as usize * self.nx]
    }

    /// Clear every cell whose center is within `r` of the segment
    /// `a -> b` (the flat cutter's capsule): row by row, the capsule's
    /// chord through the row is one interval.
    pub(super) fn cut(&mut self, a: Point2, b: Point2, r: f64) {
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite()) {
            return;
        }
        let h = self.h;
        let y_lo = a.y.min(b.y) - r;
        let y_hi = a.y.max(b.y) + r;
        let iy0 = ((y_lo - self.y0) / h - 0.5).ceil().max(0.0) as usize;
        let iy1 = ((y_hi - self.y0) / h - 0.5).floor();
        if iy1 < 0.0 {
            return;
        }
        let iy1 = (iy1 as usize).min(self.ny.saturating_sub(1));
        for iy in iy0..=iy1 {
            let y = self.y0 + (iy as f64 + 0.5) * h;
            let Some((xl, xr)) = capsule_row(a, b, r, y) else { continue };
            let i0 = ((xl - self.x0) / h - 0.5).ceil().max(0.0) as usize;
            let i1 = ((xr - self.x0) / h - 0.5).floor();
            if i1 < 0.0 {
                continue;
            }
            let i1 = (i1 as usize).min(self.nx - 1);
            let row = iy * self.nx;
            for ix in i0..=i1 {
                self.cells[row + ix] = false;
            }
        }
    }

    /// Contact angle (degrees) of a cutter at `p` travelling along the
    /// unit `dir`: the longest arc of the FRONT half of the rim (radius
    /// `rim`, just outside the cutter - `quality::rim_for`) standing in
    /// material; a straight wall at radial width ae reads
    /// `acos((r - ae) / rim)` (`load::deg_at_rim`).
    pub(super) fn engage(&self, p: Point2, dir: Point2, rim: f64) -> f64 {
        use crate::quality::Rim;
        // The samples' angles step by a fixed rotation (no trigonometry
        // per sample); the arc is `quality::longest_arc`'s law, open
        // (the front half never wraps), counted without allocating.
        let step = std::f64::consts::PI / FRONT_SAMPLES as f64;
        let a0 = dir.y.atan2(dir.x) - std::f64::consts::FRAC_PI_2 + 0.5 * step;
        let (mut c, mut sn) = (a0.cos(), a0.sin());
        let (dc, ds) = (step.cos(), step.sin());
        let (mut best, mut cur, mut stock) = (0usize, 0usize, false);
        for _ in 0..FRONT_SAMPLES {
            match self.rim(Point2::new(p.x + rim * c, p.y + rim * sn)) {
                Rim::Open => {
                    cur = 0;
                    stock = false;
                }
                h => {
                    cur += 1;
                    stock |= h == Rim::Stock;
                    if stock && cur > best {
                        best = cur;
                    }
                }
            }
            (c, sn) = (c * dc - sn * ds, sn * dc + c * ds);
        }
        // The contact ARC, the yardstick's own reading.
        best as f64 * 180.0 / FRONT_SAMPLES as f64
    }

    /// Grid size (cells along x, along y).
    pub(super) fn dims(&self) -> (usize, usize) {
        (self.nx, self.ny)
    }

    /// Is cell `i` stock or wall?
    pub(super) fn blocked(&self, i: usize) -> bool {
        self.cells.get(i).copied().unwrap_or(true) || self.wall.get(i).copied().unwrap_or(true)
    }

    /// The cell `p` lies in, when on the grid.
    pub(super) fn cell_of(&self, p: Point2) -> Option<(usize, usize)> {
        let fx = (p.x - self.x0) / self.h;
        let fy = (p.y - self.y0) / self.h;
        (fx >= 0.0 && fy >= 0.0 && fx < self.nx as f64 && fy < self.ny as f64).then_some((fx as usize, fy as usize))
    }

    /// The center of cell `i`.
    pub(super) fn center(&self, i: usize) -> Point2 {
        Point2::new(self.x0 + ((i % self.nx) as f64 + 0.5) * self.h, self.y0 + ((i / self.nx) as f64 + 0.5) * self.h)
    }

    /// Does a cell stand here that `other` has cleared (two cuts compared)?
    pub(super) fn stands_over(&self, other: &Raster) -> bool {
        self.cells.len() != other.cells.len() || self.cells.iter().zip(&other.cells).any(|(&a, &b)| a && !b)
    }

    /// Material cells standing (the progress yardstick of a round).
    pub(super) fn standing(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }

    /// Is the whole disc of radius `rad` at `p` free of material?
    /// (Sampled on two rims and the center: a stay-down test.)
    pub(super) fn disc_clear(&self, p: Point2, rad: f64) -> bool {
        if self.mat(p) {
            return false;
        }
        for (rr, n) in [(rad, 48usize), (rad * 0.6, 24)] {
            for k in 0..n {
                let a = k as f64 * std::f64::consts::TAU / n as f64;
                if self.mat(Point2::new(p.x + rr * a.cos(), p.y + rr * a.sin())) {
                    return false;
                }
            }
        }
        true
    }

    /// Walk `pts` from its first point, reading the front-half contact
    /// angle at each step (radius `rim`) BEFORE that step cuts - the
    /// measuring stock's own law (`quality::measure`), so what the planner
    /// keeps is what is read; returns the worst angle and where.
    pub(super) fn walk(&mut self, pts: &[Point2], r: f64, rim: f64, step: f64) -> (f64, Point2) {
        self.walk_until(pts, (r, rim, step), f64::INFINITY)
    }

    /// [`Raster::walk`], abandoned at the first reading past `limit`
    /// (a trial that already failed need not finish; the stock it
    /// leaves is then partial and only fit to be dropped).
    pub(super) fn walk_until(&mut self, pts: &[Point2], (r, rim, step): (f64, f64, f64), limit: f64) -> (f64, Point2) {
        let mut worst = (0.0f64, pts.first().copied().unwrap_or_default());
        // Where the tool stands it has already cleared its own disc (the
        // walk before it, or the entry, brought it there).
        if let Some(&s) = pts.first() {
            self.cut(s, s, r);
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len <= 1e-12 {
                continue;
            }
            let n = ((len / step).ceil() as usize).clamp(1, 1 << 16);
            let mut prev = a;
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let p = Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                let deg = self.engage(p, Point2::new((b.x - a.x) / len, (b.y - a.y) / len), rim);
                if deg > worst.0 {
                    worst = (deg, p);
                    if deg > limit {
                        return worst;
                    }
                }
                self.cut(prev, p, r);
                prev = p;
            }
        }
        worst
    }

    /// [`Raster::walk`], read per segment: the worst angle each segment
    /// `pts[i] -> pts[i + 1]` read (the microlift's air/cut marks).
    pub(super) fn walk_marks(&mut self, pts: &[Point2], (r, rim, step): (f64, f64, f64)) -> Vec<f64> {
        let mut marks = Vec::with_capacity(pts.len().saturating_sub(1));
        if let Some(&s) = pts.first() {
            self.cut(s, s, r);
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            let mut worst = 0.0f64;
            if len > 1e-12 {
                let dir = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
                let n = ((len / step).ceil() as usize).clamp(1, 1 << 16);
                let mut prev = a;
                for k in 1..=n {
                    let t = k as f64 / n as f64;
                    let p = Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                    worst = worst.max(self.engage(p, dir, rim));
                    self.cut(prev, p, r);
                    prev = p;
                }
            }
            marks.push(worst);
        }
        marks
    }

    /// Does the capsule of radius `rad` about `a -> b` hold no stock
    /// and no wall cell? (A lifted link's test: it cuts nothing.)
    pub(super) fn capsule_clear(&self, a: Point2, b: Point2, rad: f64) -> bool {
        self.capsule_free(a, b, rad, true)
    }

    /// Does the disc of radius `rad` at `p` hold no stock (walls aside)?
    /// (A landing's test: the rim there reads nothing.)
    pub(super) fn stock_clear(&self, p: Point2, rad: f64) -> bool {
        self.capsule_free(p, p, rad, false)
    }

    fn capsule_free(&self, a: Point2, b: Point2, rad: f64, walls: bool) -> bool {
        if !(a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite() && rad >= 0.0) {
            return false;
        }
        let h = self.h;
        let iy0 = ((a.y.min(b.y) - rad - self.y0) / h - 0.5).ceil();
        let iy1 = ((a.y.max(b.y) + rad - self.y0) / h - 0.5).floor();
        if iy0 < 0.0 || iy1 >= self.ny as f64 {
            return false;
        }
        for iy in iy0 as usize..=iy1 as usize {
            let y = self.y0 + (iy as f64 + 0.5) * h;
            let Some((xl, xr)) = capsule_row(a, b, rad, y) else { continue };
            let i0 = ((xl - self.x0) / h - 0.5).ceil();
            let i1 = ((xr - self.x0) / h - 0.5).floor();
            if i1 < i0 {
                continue;
            }
            if i0 < 0.0 || i1 >= self.nx as f64 {
                return false;
            }
            let row = iy * self.nx;
            if (i0 as usize..=i1 as usize).any(|ix| self.cells[row + ix] || (walls && self.wall[row + ix])) {
                return false;
            }
        }
        true
    }

    /// Worst contact angle along `pts` without cutting (links: they
    /// run where the stock is already open).
    pub(super) fn peek(&self, pts: &[Point2], rim: f64, step: f64) -> f64 {
        let mut worst = 0.0f64;
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.x - a.x).hypot(b.y - a.y);
            if len <= 1e-12 {
                continue;
            }
            let dir = Point2::new((b.x - a.x) / len, (b.y - a.y) / len);
            let n = ((len / step).ceil() as usize).clamp(1, 1 << 16);
            for k in 1..=n {
                let t = k as f64 / n as f64;
                worst = worst.max(self.engage(Point2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t), dir, rim));
            }
        }
        worst
    }
}

/// The x-interval of the capsule `{q : dist(q, ab) <= r}` on the line
/// `y`: the union of the two end discs' chords and the band's, which is
/// one interval because the capsule is convex.
fn capsule_row(a: Point2, b: Point2, r: f64, y: f64) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for c in [a, b] {
        let dy = y - c.y;
        let s = r * r - dy * dy;
        if s >= 0.0 {
            let w = s.sqrt();
            lo = lo.min(c.x - w);
            hi = hi.max(c.x + w);
        }
    }
    let (dx, dyy) = (b.x - a.x, b.y - a.y);
    let len = dx.hypot(dyy);
    if len > 1e-12 {
        // Band: 0 <= (q - a).u <= len and |(q - a).n| <= r, q = (x, y).
        let (ux, uy) = (dx / len, dyy / len);
        let (nx, ny) = (-uy, ux);
        let mut t0 = f64::NEG_INFINITY;
        let mut t1 = f64::INFINITY;
        // f(x) = k*x + c must lie in [lo_b, hi_b].
        for (k, c, lo_b, hi_b) in [
            (ux, (y - a.y) * uy - a.x * ux, 0.0, len),
            (nx, (y - a.y) * ny - a.x * nx, -r, r),
        ] {
            if k.abs() < 1e-15 {
                if c < lo_b || c > hi_b {
                    t0 = f64::INFINITY;
                }
                continue;
            }
            let (x0, x1) = ((lo_b - c) / k, (hi_b - c) / k);
            t0 = t0.max(x0.min(x1));
            t1 = t1.min(x0.max(x1));
        }
        if t0 <= t1 {
            lo = lo.min(t0);
            hi = hi.max(t1);
        }
    }
    (lo <= hi).then_some((lo, hi))
}
