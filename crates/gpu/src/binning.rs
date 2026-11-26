//! Conservative facet binning (#2620), the host half: the grid spec, the
//! tile-relative f32 boxes with an outward margin, the buffer plan against
//! the device limits, the cell lists, and the CPU twin of the WGSL kernel.
//! The law is a SUPERSET of the f64 grid of #2619 (`camcraft-motion`'s
//! `SupSet`): each facet that the f64 cell map puts in a cell is in that
//! cell here too; extra entries are permitted. Provenance: uniform-grid
//! partitioning from C. Ericson, "Real-Time Collision Detection" (2005),
//! ch. 7; the rounding margin is our own bound on f32 error.

use crate::probe::{Compute, WORKGROUP};

/// Most cells along one axis that the kernel takes (f32 holds every index
/// below 2^24 exactly; this stays far under it).
pub const MAX_AXIS: u32 = 1 << 16;
/// Most binned entries (facet-cell pairs) for one dispatch: bounds the
/// kernel's loop work, so one call cannot stall the device.
pub const MAX_ENTRIES: u64 = 1 << 26;
/// Maximum host storage for one call, including temporary lists and read-back.
pub const MAX_HOST_BYTES: u64 = 256 * 1024 * 1024;
/// Largest tile-relative coordinate: past it f32 loses the cell.
const MAX_REL: f64 = 1.0e12;
/// Relative rounding margin: 2^-18, far above the f32 cast (2^-24) and
/// WGSL's 2.5 ULP division error together.
const MARGIN: f64 = 1.0 / 262_144.0;

/// The f64 grid: origin, cell size and cell counts, as the CPU grid has them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GridSpec {
    pub x0: f64,
    pub y0: f64,
    pub s: f64,
    pub nx: u32,
    pub ny: u32,
}

/// Facet boxes in tile-relative f32 (`[xmin, ymin, xmax, ymax]`), each
/// corner moved outward by the margin; the cell size in f32.
#[derive(Clone, Debug, PartialEq)]
pub struct PaddedBoxes {
    pub boxes: Vec<[f32; 4]>,
    pub s: f32,
    pub nx: u32,
    pub ny: u32,
}

/// The buffers and the dispatch of one binning call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub n: u32,
    /// `u32` words of one cell's facet bit set.
    pub words: u32,
    pub cells: u32,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub groups: u32,
}

/// One list of facet indices for each cell, in ascending index, once each
/// (CSR: `start` is `cells + 1` long).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellLists {
    pub nx: u32,
    pub ny: u32,
    start: Vec<usize>,
    idx: Vec<u32>,
}

impl CellLists {
    /// The facets of cell `c = cy * nx + cx`; empty past the grid.
    pub fn cell(&self, c: usize) -> &[u32] {
        match (self.start.get(c), self.start.get(c.saturating_add(1))) {
            (Some(&a), Some(&b)) => self.idx.get(a..b).unwrap_or(&[]),
            _ => &[],
        }
    }

    pub fn cells(&self) -> usize {
        self.start.len().saturating_sub(1)
    }

    pub(crate) fn from_lists(nx: u32, ny: u32, lists: Vec<Vec<u32>>) -> Result<Self, String> {
        let mut start = host_vec(lists.len().saturating_add(1))?;
        let mut idx = host_vec(lists.iter().map(Vec::len).sum())?;
        start.push(0);
        for l in lists {
            idx.extend(l);
            start.push(idx.len());
        }
        Ok(CellLists { nx, ny, start, idx })
    }

    /// The kernel's output: `words` bit-set words for each cell.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn from_bits(p: &Plan, nx: u32, ny: u32, bits: &[u32]) -> Result<Self, String> {
        let words = p.words as usize;
        let mut lists = host_vec(p.cells as usize)?;
        for c in 0..p.cells as usize {
            let mut l = Vec::new();
            let row = bits.get(c * words..(c + 1) * words).unwrap_or(&[]);
            for (w, &word) in row.iter().enumerate() {
                let mut m = word;
                while m != 0 {
                    let b = m.trailing_zeros();
                    let i = (w as u64) * 32 + u64::from(b);
                    if i < u64::from(p.n) {
                        l.try_reserve_exact(1).map_err(host_alloc_error)?;
                        l.push(i as u32);
                    }
                    m &= m - 1;
                }
            }
            lists.push(l);
        }
        Self::from_lists(nx, ny, lists)
    }
}

/// The f64 boxes (already padded by the tool radius, as the caller's grid
/// has them) as tile-relative f32 with the outward margin. Refuses by
/// name a non-finite box or spec, a cell size f32 cannot hold, or a grid
/// past `MAX_AXIS`.
pub fn pad_boxes(boxes: &[[f64; 4]], spec: &GridSpec) -> Result<PaddedBoxes, String> {
    let GridSpec { x0, y0, s, nx, ny } = *spec;
    if !(x0.is_finite() && y0.is_finite() && s.is_finite() && s > 0.0) {
        return Err(format!("binning: the grid origin or cell size is not finite and positive ({spec:?})"));
    }
    if nx == 0 || ny == 0 || nx > MAX_AXIS || ny > MAX_AXIS {
        return Err(format!("binning: the grid is {nx} x {ny} cells; each axis must be 1..={MAX_AXIS}"));
    }
    let s32 = s as f32;
    if !(s32.is_normal() && s32 > 0.0) {
        return Err(format!("binning: the cell size {s} has no normal f32 value"));
    }
    let mut ext = s * f64::from(nx.max(ny));
    for (i, b) in boxes.iter().enumerate() {
        if !b.iter().all(|v| v.is_finite()) {
            return Err(format!("binning: facet box {i} is not finite"));
        }
        for v in [b[0] - x0, b[2] - x0, b[1] - y0, b[3] - y0] {
            ext = ext.max(v.abs());
        }
    }
    if !(ext.is_finite() && ext < MAX_REL) {
        return Err(format!("binning: a tile-relative coordinate ({ext}) is past f32 range for the grid"));
    }
    let m = (ext + s) * MARGIN;
    let lo = |v: f64| ((v - m) as f32).next_down();
    let hi = |v: f64| ((v + m) as f32).next_up();
    let mut out = host_vec(boxes.len())?;
    out.extend(boxes.iter().map(|b| [lo(b[0] - x0), lo(b[1] - y0), hi(b[2] - x0), hi(b[3] - y0)]));
    Ok(PaddedBoxes { boxes: out, s: s32, nx, ny })
}

/// The kernel's cell map, in f32, as `cell` in binning.wgsl: floor, then
/// clamp to `0..=n-1`; a NaN or negative quotient is cell 0.
pub(crate) fn cell32(v: f32, s: f32, n: u32) -> u32 {
    let t = (v / s).floor();
    if t.is_nan() || t <= 0.0 {
        return 0;
    }
    t.min(n.saturating_sub(1) as f32) as u32
}

fn ranges(b: &[f32; 4], p: &PaddedBoxes) -> (u32, u32, u32, u32) {
    (cell32(b[0], p.s, p.nx), cell32(b[2], p.s, p.nx), cell32(b[1], p.s, p.ny), cell32(b[3], p.s, p.ny))
}

/// The buffers and dispatch for `p` on a device with `c`. Refuses by name
/// a buffer past the device's storage binding or buffer size, a dispatch
/// past the workgroup count, or more than `MAX_ENTRIES` binned entries.
pub fn plan(p: &PaddedBoxes, c: &Compute) -> Result<Plan, String> {
    host_layout(p)?;
    let n = u32::try_from(p.boxes.len()).map_err(|_| "binning: more facets than u32 holds".to_string())?;
    let words = n.div_ceil(32);
    let cells = p.nx.checked_mul(p.ny).ok_or("binning: the cell count overflows u32")?;
    let in_bytes = u64::from(n) * 16;
    let out_words = u64::from(cells) * u64::from(words);
    let out_bytes = out_words * 4;
    let limit = c.max_storage_buffer_binding_size.min(c.max_buffer_size);
    for (what, bytes) in [("facet box", in_bytes), ("cell bit set", out_bytes)] {
        if bytes > limit {
            return Err(format!("binning: the {what} buffer ({bytes} bytes) is past the device limit ({limit} bytes)"));
        }
    }
    if out_words > u64::from(u32::MAX) {
        return Err("binning: the cell bit set has more words than a u32 index holds".into());
    }
    let groups = n.div_ceil(WORKGROUP);
    if groups > c.max_compute_workgroups_per_dimension {
        return Err(format!(
            "binning: {groups} workgroups are past the device limit {}",
            c.max_compute_workgroups_per_dimension
        ));
    }
    Ok(Plan { n, words, cells, in_bytes, out_bytes, groups })
}

fn host_alloc_error(e: std::collections::TryReserveError) -> String {
    format!("binning: host allocation failed: {e}")
}

pub(crate) fn host_vec<T>(len: usize) -> Result<Vec<T>, String> {
    let bytes = len.checked_mul(std::mem::size_of::<T>()).ok_or("binning: host allocation size overflow")?;
    if bytes as u64 > MAX_HOST_BYTES {
        return Err("binning: host memory budget exceeded".into());
    }
    let mut v = Vec::new();
    v.try_reserve_exact(len).map_err(host_alloc_error)?;
    Ok(v)
}

/// Bound peak storage before either path allocates cell lists, even with no facets.
fn host_layout(p: &PaddedBoxes) -> Result<usize, String> {
    let n = u32::try_from(p.boxes.len()).map_err(|_| "binning: more facets than u32 holds")?;
    let cells = p.nx.checked_mul(p.ny).ok_or("binning: the cell count overflows u32")?;
    let mut entries = 0u64;
    for b in &p.boxes {
        let (x0, x1, y0, y1) = ranges(b, p);
        if x0 <= x1 && y0 <= y1 {
            entries = entries.saturating_add(u64::from(x1 - x0 + 1) * u64::from(y1 - y0 + 1));
        }
    }
    if entries > MAX_ENTRIES {
        return Err(format!("binning: {entries} facet-cell entries are past the limit {MAX_ENTRIES}"));
    }
    // Cell vectors, CSR offsets, entries in both representations, padded/upload
    // boxes and two host copies of the GPU bit sets. Use u64 on 32-bit hosts too.
    let bytes = (u64::from(cells) + 1) * (std::mem::size_of::<Vec<u32>>() + std::mem::size_of::<usize>()) as u64
        + entries * 8
        + u64::from(n) * 32
        + u64::from(cells) * u64::from(n).div_ceil(32) * 8;
    if bytes > MAX_HOST_BYTES {
        return Err(format!("binning: host memory budget exceeded ({bytes} bytes > {MAX_HOST_BYTES})"));
    }
    usize::try_from(cells).map_err(|_| "binning: host cell count overflow".into())
}

/// The CPU twin of the kernel: the same f32 cell map, the same lists.
/// The test oracle of the shader math on a machine without a GPU; never
/// the fallback (the caller's f64 grid is the fallback).
pub fn bin_twin(p: &PaddedBoxes) -> Result<CellLists, String> {
    let cells = host_layout(p)?;
    let mut lists: Vec<Vec<u32>> = host_vec(cells)?;
    lists.resize_with(cells, Vec::new);
    for (i, b) in p.boxes.iter().enumerate() {
        let (x0, x1, y0, y1) = ranges(b, p);
        for cy in y0..=y1 {
            for cx in x0..=x1 {
                let c = (cy as usize).saturating_mul(p.nx as usize).saturating_add(cx as usize);
                if let Some(l) = lists.get_mut(c) {
                    l.try_reserve_exact(1).map_err(host_alloc_error)?;
                    l.push(i as u32);
                }
            }
        }
    }
    CellLists::from_lists(p.nx, p.ny, lists)
}
