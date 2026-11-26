//! Integer scan conversion for the raster filter: centre parity fill and
//! conservative edge coverage.
//!
//! Provenance: scanline polygon fill with the half-open edge rule and
//! conservative (supercover) segment traversal, from the general
//! computer-graphics literature (scan conversion of polygons and lines) and
//! Bennell and Oliveira (EJOR 2008). Our own integer formulation.
//!
//! All values are `i128`. Input units are below 1e18 in magnitude (the
//! `geo2d` coordinate limit), so a product of two differences is below
//! about 2e37, far inside the `i128` range (1.7e38). The checked helpers
//! still refuse an overflow by name instead of wrapping.

use super::{BitRaster, RasterError, RasterGrid};
use crate::geo2d::{Region, quantize};

/// One boundary edge in units, `(ax, ay, bx, by)`.
pub(super) type Edge = (i128, i128, i128, i128);

pub(super) fn floor_div(a: i128, b: i128) -> i128 {
    let q = a / b;
    if a % b != 0 && ((a < 0) != (b < 0)) { q - 1 } else { q }
}

pub(super) fn ceil_div(a: i128, b: i128) -> i128 {
    let q = a / b;
    if a % b != 0 && ((a < 0) == (b < 0)) { q + 1 } else { q }
}

fn mul(a: i128, b: i128) -> Result<i128, RasterError> {
    a.checked_mul(b).ok_or(RasterError::OutOfRange)
}

fn add(a: i128, b: i128) -> Result<i128, RasterError> {
    a.checked_add(b).ok_or(RasterError::OutOfRange)
}

/// All boundary edges of the region, quantized to units with the
/// Clipper-bridge rule.
pub(super) fn unit_edges(region: &Region) -> Result<Vec<Edge>, RasterError> {
    let mut edges = Vec::new();
    for l in region.loops() {
        let pts = l
            .points()
            .iter()
            .map(|p| Ok((i128::from(quantize(p.x)?), i128::from(quantize(p.y)?))))
            .collect::<Result<Vec<_>, RasterError>>()?;
        for (k, a) in pts.iter().enumerate() {
            let b = pts[(k + 1) % pts.len()];
            if *a != b {
                edges.push((a.0, a.1, b.0, b.1));
            }
        }
    }
    Ok(edges)
}

/// Integer box `((xmin, ymin), (xmax, ymax))` in units.
pub(super) type UnitBox = ((i64, i64), (i64, i64));

/// Integer bounds of the region; `None` for an empty region.
pub(super) fn unit_bounds(region: &Region) -> Result<Option<UnitBox>, RasterError> {
    let mut out: Option<UnitBox> = None;
    for l in region.loops() {
        for p in l.points() {
            let (x, y) = (quantize(p.x)?, quantize(p.y)?);
            out = Some(match out {
                None => ((x, y), (x, y)),
                Some((lo, hi)) => ((lo.0.min(x), lo.1.min(y)), (hi.0.max(x), hi.1.max(y))),
            });
        }
    }
    Ok(out)
}

/// True when the closed box `lo..hi` lies in the closed grid extent.
pub(super) fn fits(grid: &RasterGrid, lo: (i64, i64), hi: (i64, i64)) -> bool {
    let c = i128::from(grid.cell);
    let (ox, oy) = (i128::from(grid.origin_x), i128::from(grid.origin_y));
    let (ex, ey) = (ox + c * i128::from(grid.width), oy + c * i128::from(grid.height));
    i128::from(lo.0) >= ox && i128::from(lo.1) >= oy && i128::from(hi.0) <= ex && i128::from(hi.1) <= ey
}

/// Set bits `lo..=hi` of one row.
fn set_range(row: &mut [u32], lo: usize, hi: usize) {
    let (wl, wh) = (lo / 32, hi / 32);
    for w in wl..=wh {
        let from = if w == wl { lo % 32 } else { 0 };
        let to = if w == wh { hi % 32 } else { 31 };
        let mask = (u32::MAX >> (31 - (to - from))) << from;
        if let Some(word) = row.get_mut(w) {
            *word |= mask;
        }
    }
}

/// Closed index range `[lo, hi]` of cells along one axis that a closed
/// coordinate interval `[a, b]` touches, clamped to `[0, n)`. `None` when
/// it misses the grid. A value on a cell line touches both cells.
fn touch_range(a: i128, b: i128, origin: i128, cell: i128, n: u32) -> Option<(usize, usize)> {
    let mut lo = floor_div(a - origin, cell);
    if (a - origin) % cell == 0 {
        lo -= 1;
    }
    let hi = floor_div(b - origin, cell);
    let last = i128::from(n) - 1;
    if hi < 0 || lo > last {
        return None;
    }
    // Both values are now inside [0, n) with n <= MAX_SIDE.
    Some((lo.max(0) as usize, hi.min(last) as usize))
}

/// Floor and ceiling of `x(y)` on the edge, for a non-horizontal edge.
fn x_at(e: &Edge, y: i128) -> Result<(i128, i128), RasterError> {
    let (ax, ay, bx, by) = *e;
    let num = mul(y - ay, bx - ax)?;
    let den = by - ay;
    Ok((add(ax, floor_div(num, den))?, add(ax, ceil_div(num, den))?))
}

/// Cells whose closed square touches a closed boundary edge.
pub(super) fn edge_touch(edges: &[Edge], grid: &RasterGrid) -> Result<BitRaster, RasterError> {
    let mut out = BitRaster::zeroed(*grid);
    let wpr = grid.words_per_row();
    let c = i128::from(grid.cell);
    let (ox, oy) = (i128::from(grid.origin_x), i128::from(grid.origin_y));
    for e in edges {
        let (ax, ay, bx, by) = *e;
        let (ylo, yhi) = (ay.min(by), ay.max(by));
        let Some((jlo, jhi)) = touch_range(ylo, yhi, oy, c, grid.height) else {
            continue;
        };
        for j in jlo..=jhi {
            // Slab of row j, clipped to the edge's y extent.
            let y0 = oy + c * j as i128;
            let (ya, yb) = (ylo.max(y0), yhi.min(y0 + c));
            if ya > yb {
                continue;
            }
            let (xmin, xmax) = if ay == by {
                (ax.min(bx), ax.max(bx))
            } else {
                let (fa, ca) = x_at(e, ya)?;
                let (fb, cb) = x_at(e, yb)?;
                (fa.min(fb), ca.max(cb))
            };
            if let Some((ilo, ihi)) = touch_range(xmin, xmax, ox, c, grid.width) {
                let row = &mut out.words_mut()[j * wpr..(j + 1) * wpr];
                set_range(row, ilo, ihi);
            }
        }
    }
    Ok(out)
}

/// Cells whose centre is inside the region by even-odd parity of a ray to
/// +x, with the half-open edge rule of `geo2d` (an edge counts when the
/// centre y is in `[ylo, yhi)`). Doubled coordinates keep the centre exact.
pub(super) fn centre_fill(edges: &[Edge], grid: &RasterGrid) -> Result<BitRaster, RasterError> {
    let mut diff = BitRaster::zeroed(*grid);
    let wpr = grid.words_per_row();
    let c = i128::from(grid.cell);
    let (ox2, oy2) = (2 * i128::from(grid.origin_x), 2 * i128::from(grid.origin_y));
    let last_row = i128::from(grid.height) - 1;
    let w = i128::from(grid.width);
    for e in edges {
        let (ax, ay, bx, by) = *e;
        if ay == by {
            continue;
        }
        let (ylo, yhi) = (ay.min(by), ay.max(by));
        // Rows j with ylo <= Yc_j < yhi, where 2*Yc_j = 2*oy + (2j+1)*c.
        let jfrom = ceil_div(2 * ylo - oy2 - c, 2 * c).max(0);
        let jto = (ceil_div(2 * yhi - oy2 - c, 2 * c) - 1).min(last_row);
        if jfrom > jto {
            continue;
        }
        // Signed denominator made positive for the rational crossing.
        let (sign, den) = if by > ay { (1, by - ay) } else { (-1, ay - by) };
        for j in jfrom..=jto {
            let y2 = oy2 + (2 * j + 1) * c;
            // Cell i flips when its doubled centre X_i = 2*ox + (2i+1)*c is
            // left of the doubled crossing 2*ax + (y2 - 2*ay)*(bx-ax)/(by-ay),
            // that is 2*i*c*den < num with the num below.
            let lean = mul(sign * (y2 - 2 * ay), bx - ax)?;
            let num = add(add(mul(2 * ax - ox2, den)?, lean)?, -mul(c, den)?)?;
            let k = if num <= 0 { 0 } else { ceil_div(num, mul(2 * c, den)?) };
            if k == 0 {
                continue;
            }
            // Flip cells [0, k): toggle the difference bits at 0 and k.
            let row = &mut diff.words_mut()[j as usize * wpr..(j as usize + 1) * wpr];
            row[0] ^= 1;
            if k < w {
                let k = k as usize;
                row[k / 32] ^= 1 << (k % 32);
            }
        }
    }
    prefix_xor_rows(&mut diff, wpr, grid.width);
    Ok(diff)
}

/// Turn each row of difference bits into parity: bit i becomes the XOR of
/// bits 0..=i. Padding bits are cleared.
fn prefix_xor_rows(r: &mut BitRaster, wpr: usize, width: u32) {
    let tail = width % 32;
    let pad_mask = if tail == 0 { u32::MAX } else { (1u32 << tail) - 1 };
    for row in r.words_mut().chunks_mut(wpr.max(1)) {
        let mut carry = false;
        for word in row.iter_mut() {
            let mut v = *word;
            v ^= v << 1;
            v ^= v << 2;
            v ^= v << 4;
            v ^= v << 8;
            v ^= v << 16;
            if carry {
                v = !v;
            }
            carry = v >> 31 == 1;
            *word = v;
        }
        if let Some(last) = row.last_mut() {
            *last &= pad_mask;
        }
    }
}
