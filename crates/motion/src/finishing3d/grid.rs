//! Uniform XY bucket grid over the drop-cutter support facets (#2619): the
//! conservative broad phase of uniform-grid spatial partitioning, as
//! published in C. Ericson, "Real-Time Collision Detection" (2005), ch. 7.
//! Written from that text; it only narrows which facets the exact scans read.
//!
//! The law is a SUPERSET, never a guess: a facet is binned into every cell
//! its padded box (`xmin..=xmax`, `ymin..=ymax`) touches, and a point query
//! reads the one cell that holds the point. Both use the same monotone cell
//! map (`cell` below: subtract, divide, floor, clamp - each step is
//! non-decreasing in IEEE-754 arithmetic), so `xmin <= x <= xmax` gives
//! `cell(xmin) <= cell(x) <= cell(xmax)` bit for bit: a facet whose box holds
//! the point is in the point's cell. Each cell lists its facets in
//! ascending original index, once, so the scans visit their candidates in
//! the same order as the full scan and keep the same f64 answers. Every
//! scan still runs its own box reject - the grid adds no new test.
//!
//! A non-finite box, a non-finite query point or an extent the grid cannot
//! size falls back to the full scan: the old answer, never a wrong cell.

use std::ops::Deref;

use super::support::Sup;

/// Most cells along one axis; the cell grows before the grid does.
const MAX_AXIS: usize = 1024;

/// The support facets plus their bucket grid. Derefs to the facet slice, so
/// whole-set loops read every facet exactly as before.
pub(super) struct SupSet {
    sups: Vec<Sup>,
    grid: Option<Grid>,
}

struct Grid {
    x0: f64,
    y0: f64,
    s: f64,
    nx: usize,
    ny: usize,
    /// CSR offsets into `idx`, `nx * ny + 1` long.
    start: Vec<usize>,
    idx: Vec<u32>,
}

impl Deref for SupSet {
    type Target = [Sup];
    fn deref(&self) -> &[Sup] {
        &self.sups
    }
}

/// `for s in sups` keeps reading every facet, in order.
impl<'a> IntoIterator for &'a SupSet {
    type Item = &'a Sup;
    type IntoIter = std::slice::Iter<'a, Sup>;
    fn into_iter(self) -> Self::IntoIter {
        self.sups.iter()
    }
}

/// The candidate facets of one query point, in ascending original index.
pub(super) enum Near<'a> {
    All(std::slice::Iter<'a, Sup>),
    Cell { sups: &'a [Sup], idx: std::slice::Iter<'a, u32> },
}

impl<'a> Iterator for Near<'a> {
    type Item = &'a Sup;
    fn next(&mut self) -> Option<&'a Sup> {
        match self {
            Near::All(it) => it.next(),
            Near::Cell { sups, idx } => {
                // Every stored index is < sups.len() by construction; `get`
                // keeps the read checked all the same.
                for &i in idx.by_ref() {
                    if let Some(s) = sups.get(i as usize) {
                        return Some(s);
                    }
                }
                None
            }
        }
    }
}

/// The monotone cell map: finite `v`, finite `o`, finite `s > 0`, `n >= 1`.
fn cell(v: f64, o: f64, s: f64, n: usize) -> usize {
    let hi = n.saturating_sub(1).min(MAX_AXIS) as f64;
    let t = ((v - o) / s).floor();
    if t.is_nan() {
        return 0;
    }
    t.clamp(0.0, hi) as usize
}

/// `(x0, y0, s, nx, ny, cell lists)` of a grid (tests, #2620).
#[cfg(test)]
pub(super) type GridDump = (f64, f64, f64, usize, usize, Vec<Vec<u32>>);

impl SupSet {
    /// The facets with a grid sized from their own boxes.
    pub(super) fn new(sups: Vec<Sup>) -> Self {
        let grid = Grid::auto(&sups);
        Self { sups, grid }
    }

    /// The reference: no grid, every query reads every facet (the scan as
    /// it stood before #2619). The parity tests compare against it.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn full_scan(sups: Vec<Sup>) -> Self {
        Self { sups, grid: None }
    }

    /// A grid with a forced cell size (tests: cell borders on known lines).
    #[cfg(test)]
    pub(super) fn with_cell(sups: Vec<Sup>, s: f64) -> Self {
        let grid = Grid::build(&sups, s);
        Self { sups, grid }
    }

    #[cfg(test)]
    pub(super) fn has_grid(&self) -> bool {
        self.grid.is_some()
    }

    /// The grid as plain values: `(x0, y0, s, nx, ny, cell lists)` (tests:
    /// the recorded fixture that camcraft-gpu's parity test reads, #2620).
    #[cfg(test)]
    pub(super) fn grid_dump(&self) -> Option<GridDump> {
        let g = self.grid.as_ref()?;
        let lists = g.start.windows(2).map(|w| g.idx.get(w[0]..w[1]).unwrap_or(&[]).to_vec()).collect();
        Some((g.x0, g.y0, g.s, g.nx, g.ny, lists))
    }

    /// Every facet whose padded box can hold `(x, y)`, plus some that
    /// cannot (the scans' box reject drops those), in ascending index.
    pub(super) fn near(&self, x: f64, y: f64) -> Near<'_> {
        let all = || Near::All(self.sups.iter());
        let Some(g) = &self.grid else { return all() };
        if !(x.is_finite() && y.is_finite()) {
            return all();
        }
        let c = cell(y, g.y0, g.s, g.ny)
            .saturating_mul(g.nx)
            .saturating_add(cell(x, g.x0, g.s, g.nx));
        let (Some(&a), Some(&b)) = (g.start.get(c), g.start.get(c.saturating_add(1))) else {
            return all();
        };
        match g.idx.get(a..b) {
            Some(ix) => Near::Cell { sups: &self.sups, idx: ix.iter() },
            None => all(),
        }
    }
}

impl Grid {
    /// Pick the cell from the facets: about half a mean padded box, never
    /// finer than the extent split `MAX_AXIS` ways; grown by doubling until
    /// the cell count and the binned entries stay bounded.
    fn auto(sups: &[Sup]) -> Option<Grid> {
        let n = sups.len();
        if n == 0 {
            return None;
        }
        let (x0, y0, x1, y1) = extent(sups)?;
        let (w, h) = (x1 - x0, y1 - y0);
        let mut span = 0.0;
        for s in sups {
            span += (s.xmax - s.xmin).max(s.ymax - s.ymin).max(0.0);
        }
        let mean = span / n as f64;
        let mut s = (0.5 * mean).max(w.max(h) / MAX_AXIS as f64);
        if !(s.is_finite() && s > 0.0) {
            s = 1.0;
        }
        let cap_cells = n.saturating_mul(4).saturating_add(64);
        let cap_entries = n.saturating_mul(32).saturating_add(1 << 16);
        for _ in 0..128 {
            if !(s.is_finite() && s > 0.0) {
                return None;
            }
            let (nx, ny) = (axis(w, s), axis(h, s));
            if nx <= MAX_AXIS
                && ny <= MAX_AXIS
                && nx.saturating_mul(ny) <= cap_cells
                && entries(sups, x0, y0, s, nx, ny) <= cap_entries
            {
                return Grid::build(sups, s);
            }
            s *= 2.0;
        }
        None
    }

    /// Bin every facet into each cell its padded box touches, in index
    /// order. `None` (full scan) on any box or size the map cannot hold.
    fn build(sups: &[Sup], s: f64) -> Option<Grid> {
        if !(s.is_finite() && s > 0.0) || u32::try_from(sups.len()).is_err() {
            return None;
        }
        let (x0, y0, x1, y1) = extent(sups)?;
        let (nx, ny) = (axis(x1 - x0, s), axis(y1 - y0, s));
        if nx > MAX_AXIS || ny > MAX_AXIS {
            return None;
        }
        let cells = nx.checked_mul(ny)?;
        let mut count = vec![0usize; cells];
        for_each_cell(sups, x0, y0, s, nx, ny, |_, c| {
            if let Some(k) = count.get_mut(c) {
                *k = k.saturating_add(1);
            }
        });
        let mut start = Vec::with_capacity(cells.saturating_add(1));
        let mut acc = 0usize;
        start.push(0);
        for k in &count {
            acc = acc.checked_add(*k)?;
            start.push(acc);
        }
        let mut fill = start.clone();
        let mut idx = vec![0u32; acc];
        let mut ok = true;
        for_each_cell(sups, x0, y0, s, nx, ny, |i, c| {
            let slot = fill.get_mut(c).and_then(|f| {
                let at = *f;
                *f = at.saturating_add(1);
                idx.get_mut(at)
            });
            match (slot, u32::try_from(i)) {
                (Some(v), Ok(i)) => *v = i,
                _ => ok = false,
            }
        });
        ok.then_some(Grid { x0, y0, s, nx, ny, start, idx })
    }
}

/// Cells along one axis of length `len` (finite, >= 0); saturates huge.
fn axis(len: f64, s: f64) -> usize {
    let t = (len / s).floor();
    if !(t.is_finite() && t >= 0.0) {
        return usize::MAX;
    }
    (t.min(MAX_AXIS as f64 * 4.0) as usize).saturating_add(1)
}

/// The union of the padded boxes. `None` when any box coordinate is not
/// finite: such a facet is never rejected by the scans' box test, so only
/// the full scan keeps it in every answer.
fn extent(sups: &[Sup]) -> Option<(f64, f64, f64, f64)> {
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for s in sups {
        let b = [s.xmin, s.ymin, s.xmax, s.ymax];
        if !b.iter().all(|v| v.is_finite()) {
            return None;
        }
        x0 = x0.min(s.xmin);
        y0 = y0.min(s.ymin);
        x1 = x1.max(s.xmax);
        y1 = y1.max(s.ymax);
    }
    let e = (x0, y0, x1, y1);
    let fin = [x0, y0, x1, y1, x1 - x0, y1 - y0].iter().all(|v| v.is_finite());
    (fin && x0 <= x1 && y0 <= y1).then_some(e)
}

/// The binned entry count at cell size `s`, saturating.
fn entries(sups: &[Sup], x0: f64, y0: f64, s: f64, nx: usize, ny: usize) -> usize {
    let mut total = 0usize;
    for b in sups {
        let (cx0, cx1) = (cell(b.xmin, x0, s, nx), cell(b.xmax, x0, s, nx));
        let (cy0, cy1) = (cell(b.ymin, y0, s, ny), cell(b.ymax, y0, s, ny));
        if cx0 > cx1 || cy0 > cy1 {
            continue;
        }
        let k = (cx1 - cx0 + 1).saturating_mul(cy1 - cy0 + 1);
        total = total.saturating_add(k);
    }
    total
}

/// Call `f(facet index, cell index)` for each cell of each padded box, in
/// facet order. An inverted box (`xmin > xmax`) holds no point and no cell.
fn for_each_cell(
    sups: &[Sup],
    x0: f64,
    y0: f64,
    s: f64,
    nx: usize,
    ny: usize,
    mut f: impl FnMut(usize, usize),
) {
    for (i, b) in sups.iter().enumerate() {
        let (cx0, cx1) = (cell(b.xmin, x0, s, nx), cell(b.xmax, x0, s, nx));
        let (cy0, cy1) = (cell(b.ymin, y0, s, ny), cell(b.ymax, y0, s, ny));
        for cy in cy0..=cy1 {
            for cx in cx0..=cx1 {
                f(i, cy.saturating_mul(nx).saturating_add(cx));
            }
        }
    }
}
