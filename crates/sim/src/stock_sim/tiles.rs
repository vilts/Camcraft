//! The tiled storage behind the height field (M5.3 E2). A stock grid
//! is mostly untouched lattice: box and model stock begin uniform and
//! every cut is local, so the field lives in `TILE`x`TILE` tiles -
//! an untouched tile is not stored at all (`None`, its value IS
//! `stock_z`), a never-varying tile is one number (`Uniform`), and
//! only tiles a cut actually reached carry their cells (`Cut`). Every
//! value the dense `Vec<f64>` used to answer, `top_at` answers the
//! same; the representation is private, the verdicts are not.

use super::dexel::Dexel;

/// Tile edge in cells: 64x64 heights is a 32 KiB box, small enough
/// that a tool's footprint rarely straddles more than four, large
/// enough that a million-cell grid is a few hundred slots.
pub(super) const TILE: usize = 64;

#[derive(Clone, Debug)]
pub(super) enum Tile {
    /// Every cell of this tile stands at exactly this height.
    Uniform(f64),
    /// The tile holds a boundary: one height per cell, row-major
    /// within the tile (cells past the grid edge keep the value the
    /// tile materialized from and are never read).
    Cut(Box<[f64; TILE * TILE]>),
}

// Representation-independent equality, as the old per-cell Vec
// compare was: two grids are equal when their lattices are and every
// cell answers the same number (a `None` and a `Uniform(stock_z)`
// differ in bytes, never in verdict).
impl PartialEq for Dexel {
    fn eq(&self, other: &Self) -> bool {
        self.x0 == other.x0
            && self.y0 == other.y0
            && self.cell == other.cell
            && self.nx == other.nx
            && self.ny == other.ny
            && self.stock_z == other.stock_z
            && self.cells() == other.cells()
            && (0..self.cells()).all(|k| self.top_at(k) == other.top_at(k))
    }
}

impl Dexel {
    #[must_use]
    pub fn cells(&self) -> usize {
        self.nx * self.ny
    }

    #[must_use]
    fn tx(&self) -> usize {
        self.nx.div_ceil(TILE)
    }

    /// (tile slot, offset within it) for a cell key, `None` past the
    /// grid - where the dense Vec used to panic; reading past the end
    /// answers NaN instead (the Prime Directive: the callers gate
    /// with `col`, nobody's verdict may ever ride an index bug).
    #[inline]
    fn slot(&self, k: usize) -> Option<(usize, usize)> {
        if self.nx == 0 || k >= self.cells() {
            return None;
        }
        let (ix, iy) = (k % self.nx, k / self.nx);
        let off = (iy % TILE) * TILE + ix % TILE;
        Some((ix / TILE + (iy / TILE) * self.tx(), off))
    }

    /// The height standing at cell `k` - the dense `top[k]`.
    #[must_use]
    #[inline]
    pub fn top_at(&self, k: usize) -> f64 {
        let Some((t, off)) = self.slot(k) else {
            return f64::NAN;
        };
        match &self.tiles[t] {
            None => self.stock_z,
            Some(Tile::Uniform(z)) => *z,
            Some(Tile::Cut(b)) => b[off],
        }
    }

    /// `top[k] = min(top[k], z)` - the cut law. A cell that does not
    /// drop never touches (never even allocates) its tile.
    pub fn lower(&mut self, k: usize, z: f64) {
        self.history = None;
        let Some((t, off)) = self.slot(k) else { return };
        let cur = match &self.tiles[t] {
            None => self.stock_z,
            Some(Tile::Uniform(u)) => *u,
            Some(Tile::Cut(b)) => b[off],
        };
        match z.partial_cmp(&cur) {
            Some(std::cmp::Ordering::Less) => {}
            // Not lower - including either side being NaN, where the
            // old `if cut < self.top[k]` simply did not write.
            _ => return,
        }
        if let Some(Tile::Cut(b)) = &mut self.tiles[t] {
            b[off] = z;
            return;
        }
        let base = match &self.tiles[t] {
            Some(Tile::Uniform(u)) => *u,
            _ => self.stock_z,
        };
        let mut b = Box::new([base; TILE * TILE]);
        b[off] = z;
        self.tiles[t] = Some(Tile::Cut(b));
    }

    /// `top[k] = z` - the unconditional write the masks and the feed
    /// law need. A uniform tile keeps its economy when the new value
    /// IS its value; only a real change pays a box.
    pub fn set(&mut self, k: usize, z: f64) {
        self.history = None;
        let Some((t, off)) = self.slot(k) else { return };
        if let Some(Tile::Cut(b)) = &mut self.tiles[t] {
            b[off] = z;
            return;
        }
        let cur = match &self.tiles[t] {
            Some(Tile::Uniform(u)) => *u,
            _ => self.stock_z,
        };
        if z == cur {
            return; // same answer, cheaper representation
        }
        let mut b = Box::new([cur; TILE * TILE]);
        b[off] = z;
        self.tiles[t] = Some(Tile::Cut(b));
    }

    /// Every cell to one height: the whole field collapses to the
    /// representation of "nothing happened but this".
    pub fn fill(&mut self, z: f64) {
        self.history = None;
        if z == self.stock_z {
            self.tiles.fill(None);
        } else {
            self.tiles.fill(Some(Tile::Uniform(z)));
        }
    }

    /// Every cell's height in index order - what the dense Vec's
    /// iterator was, answered on the fly.
    pub fn tops(&self) -> impl Iterator<Item = f64> + '_ {
        (0..self.cells()).map(move |k| self.top_at(k))
    }

    /// The sparse law's own witness: tiles actually holding memory.
    /// A fresh block must answer zero - that is M5.3 E2's contract.
    #[must_use]
    pub fn tiles_allocated(&self) -> usize {
        self.tiles.iter().filter(|t| t.is_some()).count()
    }

    /// Move every stored height by `dz`, air (the non-finite) never.
    /// Untouched tiles ride on `stock_z` and must NOT shift here:
    /// the caller translates `stock_z` itself, and they travel
    /// together - what the old whole-field loop did cell by cell.
    pub fn translate_finite(&mut self, dz: f64) {
        self.history = self.history.as_ref().filter(|h| !h.active && h.cuts.is_empty() && h.clips.is_empty())
            .map(|_| std::sync::Arc::new(super::carry_history::History::block(
                self.x0, self.y0, self.x0 + self.nx as f64 * self.cell,
                self.y0 + self.ny as f64 * self.cell, self.stock_z, self.cell,
            )));
        for t in &mut self.tiles {
            match t {
                None => {}
                Some(Tile::Uniform(u)) => {
                    if u.is_finite() {
                        *u += dz;
                    }
                }
                Some(Tile::Cut(b)) => {
                    for c in b.iter_mut() {
                        if c.is_finite() {
                            *c += dz;
                        }
                    }
                }
            }
        }
    }

    /// Wrap a dense field: tiles whose cells all answer one number
    /// collapse to that number, and one equal to `stock_z` collapses
    /// to nothing. Model stock is mostly this - air and full height,
    /// in runs the old Vec paid for cell by cell.
    pub(super) fn from_grid(
        x0: f64,
        y0: f64,
        cell: f64,
        nx: usize,
        ny: usize,
        stock_z: f64,
        top: Vec<f64>,
    ) -> Self {
        let (tx, ty) = (nx.div_ceil(TILE), ny.div_ceil(TILE));
        let mut tiles = vec![None; tx * ty];
        for tiy in 0..ty {
            for tix in 0..tx {
                let (ix0, iy0) = (tix * TILE, tiy * TILE);
                let (ix1, iy1) = ((ix0 + TILE).min(nx), (iy0 + TILE).min(ny));
                let first = top[ix0 + iy0 * nx];
                let mut uniform = true;
                'scan: for iy in iy0..iy1 {
                    for ix in ix0..ix1 {
                        if top[ix + iy * nx] != first {
                            uniform = false;
                            break 'scan;
                        }
                    }
                }
                tiles[tix + tiy * tx] = if uniform {
                    if first == stock_z {
                        None
                    } else {
                        Some(Tile::Uniform(first))
                    }
                } else {
                    let mut b = Box::new([f64::NAN; TILE * TILE]);
                    for iy in iy0..iy1 {
                        for ix in ix0..ix1 {
                            b[(iy - iy0) * TILE + (ix - ix0)] = top[ix + iy * nx];
                        }
                    }
                    Some(Tile::Cut(b))
                };
            }
        }
        Dexel {
            x0,
            y0,
            cell,
            nx,
            ny,
            stock_z,
            tiles,
            history: None,
        }
    }

    /// The cylinder's world beside the round: every cell whose center
    /// stands farther than `r` from the origin is air. Tile-aware
    /// (M5.3 E2): a tile wholly outside the circle is one number, a
    /// tile wholly inside costs nothing, and only the ring the
    /// boundary crosses pays a box. The cell test on the ring is the
    /// old `hypot` law exactly, so the mask is bit-identical.
    pub(super) fn air_outside_circle(&mut self, r: f64) {
        self.history = None;
        let air = f64::NEG_INFINITY;
        let (tx, ty) = (self.tx(), self.ny.div_ceil(TILE));
        for tiy in 0..ty {
            for tix in 0..tx {
                let ti = tix + tiy * tx;
                let (ax, ay) = (
                    self.x0 + (tix * TILE) as f64 * self.cell,
                    self.y0 + (tiy * TILE) as f64 * self.cell,
                );
                let (bx, by) = (ax + TILE as f64 * self.cell, ay + TILE as f64 * self.cell);
                let near_x = if ax > 0.0 { ax } else if bx < 0.0 { bx } else { 0.0 };
                let near_y = if ay > 0.0 { ay } else if by < 0.0 { by } else { 0.0 };
                if near_x * near_x + near_y * near_y > r * r {
                    self.tiles[ti] = Some(Tile::Uniform(air));
                    continue; // wholly air: one number, not 4096
                }
                let far_x = ax.abs().max(bx.abs());
                let far_y = ay.abs().max(by.abs());
                if far_x * far_x + far_y * far_y <= r * r {
                    continue; // wholly material: stays exactly as it was
                }
                for iy in (tiy * TILE)..((tiy + 1) * TILE).min(self.ny) {
                    for ix in (tix * TILE)..((tix + 1) * TILE).min(self.nx) {
                        let c = self.center(ix, iy);
                        if c.x.hypot(c.y) > r {
                            self.set(ix + iy * self.nx, air);
                        }
                    }
                }
            }
        }
    }
}
