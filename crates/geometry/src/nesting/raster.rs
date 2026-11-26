//! Conservative bit-packed rasteriser for `geo2d::Region` (#2612, slice 1 of
//! the raster nest #2525).
//!
//! Provenance: scanline polygon fill (even-odd parity at cell centres) and
//! conservative coverage (each cell that a boundary edge touches), from
//! Bennell and Oliveira (EJOR 2008) and the general computer-graphics
//! literature on scan conversion. Integer arithmetic only.
//!
//! Two outputs, both safe for a first filter:
//! - [`rasterize_over`] (parts): a bit is set for each cell whose closed
//!   square touches the closed region. Each point of the region is in a set
//!   cell.
//! - [`rasterize_free`] (usable sheet): a bit is set only for a cell whose
//!   closed square lies fully in the open interior of the region. Holes are
//!   not free.
//!
//! Units are `geo2d` integer units ([`crate::geo2d::SCALE`] per mm, 1 um).
//! No float enters the decision of a cell: region vertices are quantized
//! with the same rule as the Clipper bridge, then all tests are exact
//! `i128` arithmetic. A raster value never reaches posted output or a
//! verdict; the exact vector check decides (#2525).

use thiserror::Error;

use crate::geo2d::{GeoError, Region};

mod scan;

/// Default cell size in units: 1 mm (research comment 2 on #2525, the
/// coarse level of the coarse-to-fine search).
pub const DEFAULT_CELL_UNITS: i64 = 1_000_000;

/// Upper limit of cells in one grid: 4096 x 4096 (one fine tile of #2525).
/// A 3000 x 1500 mm sheet at 1 mm (4.5e6 cells) fits.
pub const MAX_CELLS: u64 = 1 << 24;

/// Upper limit of cells on one side of a grid.
pub const MAX_SIDE: u32 = 1 << 16;

/// Why a raster was refused. Each variant names its reason.
#[derive(Clone, Debug, Error, PartialEq, Eq)]
pub enum RasterError {
    #[error("raster cell size must be positive, got {0} units")]
    NonPositiveCellSize(i64),
    #[error("raster cell size is not a finite number")]
    NonFiniteCellSize,
    #[error("raster grid of {width} x {height} cells exceeds the limit ({MAX_CELLS} cells, {MAX_SIDE} on a side)")]
    GridTooLarge { width: u64, height: u64 },
    #[error("raster grid must have at least one cell on each side")]
    EmptyGrid,
    #[error("raster input has a non-finite coordinate")]
    NonFinite,
    #[error("raster input coordinates are outside the supported range")]
    OutOfRange,
    #[error("raster needs a region, but the region is empty")]
    EmptyRegion,
    #[error("region does not fit in the raster grid; an over-approximation must not be clipped")]
    RegionExceedsGrid,
}

impl From<GeoError> for RasterError {
    fn from(e: GeoError) -> Self {
        match e {
            GeoError::OutOfRange => RasterError::OutOfRange,
            GeoError::Invalid(_) => RasterError::NonFinite,
        }
    }
}

/// Converts a cell size in mm to units (rounded to the nearest unit).
/// Refuses a non-finite, non-positive or out-of-range value.
pub fn cell_units_from_mm(mm: f64) -> Result<i64, RasterError> {
    if !mm.is_finite() {
        return Err(RasterError::NonFiniteCellSize);
    }
    let units = crate::geo2d::quantize(mm)?;
    if units <= 0 {
        return Err(RasterError::NonPositiveCellSize(units));
    }
    Ok(units)
}

/// A square-cell lattice. Cell `(i, j)` is the closed square
/// `[ox + i*c, ox + (i+1)*c] x [oy + j*c, oy + (j+1)*c]` in units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RasterGrid {
    origin_x: i64,
    origin_y: i64,
    cell: i64,
    width: u32,
    height: u32,
}

impl RasterGrid {
    /// A grid with an explicit origin (units), cell size (units) and size
    /// (cells). Refuses a non-positive cell, an empty or too large grid,
    /// and a far corner that does not fit in `i64`.
    pub fn new(
        origin_x: i64,
        origin_y: i64,
        cell: i64,
        width: u32,
        height: u32,
    ) -> Result<Self, RasterError> {
        if cell <= 0 {
            return Err(RasterError::NonPositiveCellSize(cell));
        }
        check_size(u64::from(width), u64::from(height))?;
        for (o, n) in [(origin_x, width), (origin_y, height)] {
            let far = i128::from(o) + i128::from(cell) * i128::from(n);
            if i64::try_from(far).is_err() {
                return Err(RasterError::OutOfRange);
            }
        }
        Ok(RasterGrid { origin_x, origin_y, cell, width, height })
    }

    /// The smallest grid on the global lattice (origin a multiple of the
    /// cell size) whose closed extent contains the region. Refuses an
    /// empty region and a grid over the limit.
    pub fn covering(region: &Region, cell: i64) -> Result<Self, RasterError> {
        if cell <= 0 {
            return Err(RasterError::NonPositiveCellSize(cell));
        }
        let (lo, hi) = scan::unit_bounds(region)?.ok_or(RasterError::EmptyRegion)?;
        let c = i128::from(cell);
        let ox = scan::floor_div(i128::from(lo.0), c) * c;
        let oy = scan::floor_div(i128::from(lo.1), c) * c;
        let w = scan::ceil_div(i128::from(hi.0) - ox, c).max(1);
        let h = scan::ceil_div(i128::from(hi.1) - oy, c).max(1);
        let w = u64::try_from(w).map_err(|_| RasterError::OutOfRange)?;
        let h = u64::try_from(h).map_err(|_| RasterError::OutOfRange)?;
        check_size(w, h)?;
        let ox = i64::try_from(ox).map_err(|_| RasterError::OutOfRange)?;
        let oy = i64::try_from(oy).map_err(|_| RasterError::OutOfRange)?;
        // check_size proved both sides fit in u32.
        Self::new(ox, oy, cell, w as u32, h as u32)
    }

    pub fn origin(&self) -> (i64, i64) {
        (self.origin_x, self.origin_y)
    }

    pub fn cell(&self) -> i64 {
        self.cell
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// `u32` words for one row: `ceil(width / 32)`.
    pub fn words_per_row(&self) -> usize {
        (self.width as usize).div_ceil(32)
    }

    /// Closed square of cell `(i, j)` in units, `(x0, y0, x1, y1)`; `None`
    /// outside the grid.
    pub fn cell_rect(&self, i: u32, j: u32) -> Option<(i64, i64, i64, i64)> {
        if i >= self.width || j >= self.height {
            return None;
        }
        // `new` proved the far corner fits in i64.
        let x0 = self.origin_x + i64::from(i) * self.cell;
        let y0 = self.origin_y + i64::from(j) * self.cell;
        Some((x0, y0, x0 + self.cell, y0 + self.cell))
    }
}

fn check_size(w: u64, h: u64) -> Result<(), RasterError> {
    if w == 0 || h == 0 {
        return Err(RasterError::EmptyGrid);
    }
    let too_large = w > u64::from(MAX_SIDE)
        || h > u64::from(MAX_SIDE)
        || w.checked_mul(h).is_none_or(|n| n > MAX_CELLS);
    if too_large {
        return Err(RasterError::GridTooLarge { width: w, height: h });
    }
    Ok(())
}

/// Bits of a grid. Row `j` is the words `[j*wpr, (j+1)*wpr)`; cell `i` is
/// bit `i % 32` (least significant bit first) of word `i / 32`. Padding
/// bits past the width are always zero.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BitRaster {
    grid: RasterGrid,
    words: Vec<u32>,
}

impl BitRaster {
    pub(crate) fn zeroed(grid: RasterGrid) -> Self {
        let n = grid.words_per_row() * grid.height as usize;
        BitRaster { grid, words: vec![0; n] }
    }

    pub fn grid(&self) -> &RasterGrid {
        &self.grid
    }

    pub fn words(&self) -> &[u32] {
        &self.words
    }

    pub(crate) fn words_mut(&mut self) -> &mut [u32] {
        &mut self.words
    }

    /// Bit of cell `(i, j)`; `false` outside the grid.
    pub fn get(&self, i: u32, j: u32) -> bool {
        if i >= self.grid.width || j >= self.grid.height {
            return false;
        }
        let idx = j as usize * self.grid.words_per_row() + (i / 32) as usize;
        self.words.get(idx).is_some_and(|w| (w >> (i % 32)) & 1 == 1)
    }

    /// Number of set cells.
    pub fn count_ones(&self) -> u64 {
        self.words.iter().map(|w| u64::from(w.count_ones())).sum()
    }
}

/// Over-approximate raster (parts): set each cell whose closed square
/// touches the closed region. Refuses a region that does not fit in the
/// grid, because a clipped part raster is no longer an over-approximation.
/// An empty region gives an all-zero raster.
pub fn rasterize_over(region: &Region, grid: &RasterGrid) -> Result<BitRaster, RasterError> {
    let edges = scan::unit_edges(region)?;
    if let Some((lo, hi)) = scan::unit_bounds(region)?
        && !scan::fits(grid, lo, hi)
    {
        return Err(RasterError::RegionExceedsGrid);
    }
    let inside = scan::centre_fill(&edges, grid)?;
    let mut out = scan::edge_touch(&edges, grid)?;
    for (o, i) in out.words_mut().iter_mut().zip(inside.words()) {
        *o |= *i;
    }
    Ok(out)
}

/// Under-approximate raster (usable sheet): set a cell (free) only when its
/// closed square is inside the open region: the centre is inside and no
/// boundary edge touches the square. The grid may extend past the region;
/// a cell outside the region is never free. An empty region gives an
/// all-zero raster.
pub fn rasterize_free(region: &Region, grid: &RasterGrid) -> Result<BitRaster, RasterError> {
    let edges = scan::unit_edges(region)?;
    let mut out = scan::centre_fill(&edges, grid)?;
    let touched = scan::edge_touch(&edges, grid)?;
    for (o, t) in out.words_mut().iter_mut().zip(touched.words()) {
        *o &= !*t;
    }
    Ok(out)
}
