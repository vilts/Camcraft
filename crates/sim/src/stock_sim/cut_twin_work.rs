//! The brick work list of the f32 cut twin (#2626, GPU stock sim
//! S1): from a batch of moves, the bricks and cells that each move's
//! window touches, and the CPU run of the kernel over that list into
//! the dense lattice layout of `preview::pack_cut`.
//!
//! Provenance: the swept windows of `field_sweep::window_of` and the
//! brick-major walk of `field_batch::cut_batch`, clipped to the dense
//! lattice of `preview::pack`; nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Same windows as the f64 walk.** A chord's window is the f64
//!   sweep's own (`Sweep::window`), clipped to the lattice box (the
//!   lattice lies inside the f64 cut region, `field_batch::region_of`
//!   pads further). The work list is integer data, so it is made in
//!   f64 on the CPU and only the cell kernel runs in f32.
//! - **Refuse by name, never cut another way.** Only the ball and the
//!   flat end mill, upright, on linear moves have a twin. Any other
//!   tool, an arc, a tilted pose, a zero-length move, a non-finite
//!   value or a non-positive tool size refuses before any output.
//!   Rapids touch nothing (RS-274 G0, the f64 law).
//! - **Deterministic order.** Items are sorted brick-major, then by
//!   chord; the max law makes the result independent of that order.

use super::cut_twin::{cell_cut, from_ordered_bits, ordered_bits, TileChord, TwinChord, TwinTool};
use super::field::{brick_of, BRICK};
use super::field_cut::{chord_sweep, SdfTool};
use super::field_solid::{tool_kind, ToolKind};
use super::kernel::StockError;
use super::preview::{pack::MAX_TEXELS, LatticeMeta};

use crate::cl::{Move, MoveKind};

/// Refusal ceiling of one chord's clipped window, in cells: the f64
/// walk's `MAX_CELLS_PER_MOVE`, the same value.
pub const TWIN_MAX_CELLS_PER_CHORD: u64 = 1 << 22;

/// One unit of kernel work: chord `chord` against brick `brick`, on
/// the cells `lo..=hi` (world cell keys, inside the brick and the
/// lattice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkItem {
    pub brick: [i32; 3],
    pub chord: u32,
    pub lo: [i32; 3],
    pub hi: [i32; 3],
}

/// The chords of a batch and the brick work list over them.
#[derive(Clone, Debug, PartialEq)]
pub struct TwinWork {
    pub chords: Vec<TwinChord>,
    pub items: Vec<WorkItem>,
}

fn invalid(msg: String) -> StockError {
    StockError::Invalid(format!("f32 cut twin: {msg}"))
}

/// The tool's twin, or its refusal by name.
fn twin_tool(tool: &SdfTool) -> Result<TwinTool, StockError> {
    let name = match tool {
        SdfTool::Ball { .. } | SdfTool::Flat { .. } => None,
        SdfTool::Shanked { .. } => Some("shanked ball"),
        SdfTool::Taper { .. } => Some("taper"),
        SdfTool::Drill { .. } => Some("drill"),
        SdfTool::BullNose { .. } => Some("bull nose"),
        SdfTool::Dovetail { .. } => Some("dovetail"),
        SdfTool::Lathe { .. } => Some("lathe"),
    };
    if let Some(n) = name {
        return Err(invalid(format!(
            "the {n} tool has no f32 twin; only the ball and the flat end mill cut here"
        )));
    }
    // The f64 gate checks the sizes (positive, finite) by name.
    match tool_kind(tool)? {
        ToolKind::Ball(r) => Ok(TwinTool::Ball { r }),
        ToolKind::Flat { r, flank } => Ok(TwinTool::Flat { r, flank }),
        _ => Err(invalid("the tool has no f32 twin".into())),
    }
}

/// The lattice box `[lo, hi]` per axis, or a refusal of a bad meta.
fn lattice_box(meta: &LatticeMeta) -> Result<[(i32, i32); 3], StockError> {
    if !(meta.cell.is_finite() && meta.cell > 0.0 && meta.cap.is_finite() && meta.cap > 0.0) {
        return Err(invalid(format!(
            "the lattice cell {} and cap {} must be positive finite numbers",
            meta.cell, meta.cap
        )));
    }
    let n = (meta.dims[0] as usize)
        .checked_mul(meta.dims[1] as usize)
        .and_then(|n| n.checked_mul(meta.dims[2] as usize));
    if !n.is_some_and(|n| n > 0 && n <= MAX_TEXELS) {
        return Err(invalid(format!("the lattice dims {:?} are empty or over the texel budget", meta.dims)));
    }
    let mut out = [(0, 0); 3];
    for (a, o) in out.iter_mut().enumerate() {
        let hi = i64::from(meta.origin[a]) + i64::from(meta.dims[a]) - 1;
        let hi = i32::try_from(hi).map_err(|_| invalid(format!("the lattice axis {a} leaves the i32 range")))?;
        *o = (meta.origin[a], hi);
    }
    Ok(out)
}

/// Whether move `m` cuts on the twin (`false`: a rapid, which touches
/// nothing), or its refusal by name.
fn twin_move(m: &Move) -> Result<bool, StockError> {
    if !m.is_finite() {
        return Err(invalid("the move has a non-finite endpoint".into()));
    }
    if m.kind == MoveKind::Rapid {
        return Ok(false);
    }
    if m.arc.is_some() {
        return Err(invalid("an arc move has no twin; only linear moves cut here".into()));
    }
    if let Some(a) = m.axis
        && !(a[0] == 0.0 && a[1] == 0.0 && a[2].is_finite() && a[2] > 0.0)
    {
        return Err(invalid(format!("the tool axis {a:?} is not upright; only +Z cuts here")));
    }
    if m.from == m.to {
        return Err(invalid("a zero-length move has no swept solid".into()));
    }
    Ok(true)
}

/// The cheap precheck of a whole op (#2629, S3b): `Ok` when the tool and
/// every move have a twin, else the first refusal by name. It builds no
/// window; `twin_work` can still refuse a window over the cell ceiling.
pub fn twin_refusal(moves: &[Move], tool: &SdfTool) -> Result<(), StockError> {
    twin_tool(tool)?;
    for m in moves {
        twin_move(m)?;
    }
    Ok(())
}

/// The chords of `moves` and the brick work list that their windows
/// touch inside `meta`'s lattice. Validates the whole batch first.
pub fn twin_work(moves: &[Move], tool: &SdfTool, meta: &LatticeMeta) -> Result<TwinWork, StockError> {
    let tt = twin_tool(tool)?;
    let bx = lattice_box(meta)?;
    let kind = tool_kind(tool)?;
    let up = [0.0, 0.0, 1.0];
    let mut chords = Vec::new();
    let mut items = Vec::new();
    for m in moves {
        if !twin_move(m)? {
            continue;
        }
        let sw = chord_sweep(&kind, up, m.from, m.to)?;
        let (a, b) = match tt {
            TwinTool::Ball { r } => ([m.from.x, m.from.y, m.from.z + r], [m.to.x, m.to.y, m.to.z + r]),
            TwinTool::Flat { .. } => ([m.from.x, m.from.y, m.from.z], [m.to.x, m.to.y, m.to.z]),
        };
        let n = u32::try_from(chords.len()).map_err(|_| invalid("too many chords in one batch".into()))?;
        chords.push(TwinChord { a, b, tool: tt });
        let w = sw.window(meta.cell, meta.cap, up);
        let c: [(i32, i32); 3] = std::array::from_fn(|i| (w[i].0.max(bx[i].0), w[i].1.min(bx[i].1)));
        if c.iter().any(|s| s.0 > s.1) {
            continue; // the whole reach lies outside the lattice
        }
        let cells = c
            .iter()
            .fold(1u64, |acc, s| acc.saturating_mul((i64::from(s.1) - i64::from(s.0) + 1) as u64));
        if cells > TWIN_MAX_CELLS_PER_CHORD {
            return Err(invalid(format!(
                "a chord's window spans more than {TWIN_MAX_CELLS_PER_CHORD} cells at cell size {}; segment the move",
                meta.cell
            )));
        }
        let (blo, bhi) = (brick_of([c[0].0, c[1].0, c[2].0]), brick_of([c[0].1, c[1].1, c[2].1]));
        for kz in blo[2]..=bhi[2] {
            for ky in blo[1]..=bhi[1] {
                for kx in blo[0]..=bhi[0] {
                    let bk = [kx, ky, kz];
                    let lo: [i32; 3] = std::array::from_fn(|i| bk[i].saturating_mul(BRICK).max(c[i].0));
                    let hi: [i32; 3] =
                        std::array::from_fn(|i| bk[i].saturating_mul(BRICK).saturating_add(BRICK - 1).min(c[i].1));
                    items.push(WorkItem { brick: bk, chord: n, lo, hi });
                }
            }
        }
    }
    items.sort_by_key(|w| (w.brick, w.chord));
    Ok(TwinWork { chords, items })
}

/// One work item as the kernel reads it (#2627): the chord seen from
/// its brick, the brick origin's lattice offset in cells, and the
/// brick-local cells `lo..=hi` to cut. Plain numbers, so a caller can
/// hand them to the GPU kernel of camcraft-gpu field by field.
#[derive(Clone, Copy, Debug)]
pub struct TwinItem {
    pub tile: TileChord,
    pub base: [i32; 3],
    pub lo: [u32; 3],
    pub hi: [u32; 3],
}

/// The kernel items of a work list over `meta`. Refuses an item that
/// names no chord or whose cells leave their brick or the lattice: a
/// hand-made list refuses, never writes a wrapped slot.
pub fn twin_items(work: &TwinWork, meta: &LatticeMeta) -> Result<Vec<TwinItem>, StockError> {
    let bx = lattice_box(meta)?;
    let mut out = Vec::new();
    out.try_reserve_exact(work.items.len())
        .map_err(|e| invalid(format!("no memory for {} work items: {e}", work.items.len())))?;
    for it in &work.items {
        let Some(ch) = work.chords.get(it.chord as usize) else {
            return Err(invalid(format!("work item names chord {} of {}", it.chord, work.chords.len())));
        };
        let outside = || invalid(format!("work item cells {:?}..={:?} lie outside their brick or the lattice", it.lo, it.hi));
        let b0: [i32; 3] = std::array::from_fn(|a| it.brick[a].saturating_mul(BRICK));
        let (mut base, mut lo, mut hi) = ([0i32; 3], [0u32; 3], [0u32; 3]);
        for a in 0..3 {
            let (l, h) = (i64::from(it.lo[a]) - i64::from(b0[a]), i64::from(it.hi[a]) - i64::from(b0[a]));
            let last = i64::from(bx[a].1);
            if !(0 <= l && l <= h && h < i64::from(BRICK)) || i64::from(it.lo[a]) < i64::from(bx[a].0) || i64::from(it.hi[a]) > last {
                return Err(outside());
            }
            base[a] = i32::try_from(i64::from(b0[a]) - i64::from(bx[a].0)).map_err(|_| outside())?;
            (lo[a], hi[a]) = (u32::try_from(l).map_err(|_| outside())?, u32::try_from(h).map_err(|_| outside())?);
        }
        let origin = b0.map(|v| f64::from(v) * meta.cell);
        out.push(TwinItem { tile: TileChord::of(ch, origin, meta.cell), base, lo, hi });
    }
    Ok(out)
}

/// Run the f32 kernel over a work list: the cut layer in the dense
/// layout of `pack_cut` over `meta` (z-major, then y, then x), with
/// the same `-4 cap` floor where no chord claims a cell.
pub fn run_twin(work: &TwinWork, meta: &LatticeMeta) -> Result<Vec<f32>, StockError> {
    let items = twin_items(work, meta)?;
    let dims = [meta.dims[0] as usize, meta.dims[1] as usize, meta.dims[2] as usize];
    let floor = ordered_bits((-4.0 * meta.cap) as f32);
    fold_items(&items, meta, vec![floor; dims[0] * dims[1] * dims[2]])
}

/// The incremental CPU law (#2629, S3b): the kernel items cut onto an
/// earlier layer `base` of the same lattice (a playback keyframe). The
/// max law makes it exact: the keyframe of A, then B onto it, is the
/// one-batch cut of A and B, bit for bit. A base of the wrong size or
/// with a non-finite value is refused by name.
pub fn run_twin_items_onto(items: &[TwinItem], meta: &LatticeMeta, base: &[f32]) -> Result<Vec<f32>, StockError> {
    let n = (meta.dims[0] as usize)
        .checked_mul(meta.dims[1] as usize)
        .and_then(|n| n.checked_mul(meta.dims[2] as usize))
        .ok_or_else(|| invalid(format!("the lattice dims {:?} overflow", meta.dims)))?;
    if base.len() != n {
        return Err(invalid(format!("the base layer has {} texels, the lattice {n}", base.len())));
    }
    if let Some(at) = base.iter().position(|v| !v.is_finite()) {
        return Err(invalid(format!("the base layer texel {at} is not finite")));
    }
    let mut keys = Vec::new();
    keys.try_reserve_exact(n).map_err(|e| invalid(format!("no memory for {n} texels: {e}")))?;
    keys.extend(base.iter().map(|v| ordered_bits(*v)));
    fold_items(items, meta, keys)
}

/// Fold the claims of `items` into `keys` with the max law and decode.
fn fold_items(items: &[TwinItem], meta: &LatticeMeta, mut keys: Vec<u32>) -> Result<Vec<f32>, StockError> {
    let dims = [meta.dims[0] as usize, meta.dims[1] as usize, meta.dims[2] as usize];
    let (cell32, cap32) = (meta.cell as f32, meta.cap as f32);
    for it in items {
        for k in it.lo[2]..=it.hi[2] {
            for j in it.lo[1]..=it.hi[1] {
                for i in it.lo[0]..=it.hi[0] {
                    let local = [i, j, k];
                    let q = local.map(|v| (v as f32 + 0.5) * cell32);
                    let Some(v) = cell_cut(&it.tile, q, cap32) else {
                        continue;
                    };
                    let at: [i64; 3] = std::array::from_fn(|a| i64::from(it.base[a]) + i64::from(local[a]));
                    let inside = (0..3).all(|a| at[a] >= 0 && at[a] < dims[a] as i64);
                    let slot = inside.then(|| (at[2] as usize * dims[1] + at[1] as usize) * dims[0] + at[0] as usize);
                    let Some(dst) = slot.and_then(|s| keys.get_mut(s)) else {
                        return Err(invalid(format!("work item cell {local:?} lies outside the lattice")));
                    };
                    *dst = (*dst).max(ordered_bits(v));
                }
            }
        }
    }
    Ok(keys.into_iter().map(from_ordered_bits).collect())
}

/// [`twin_work`] then [`run_twin`]: the f32 cut layer of a batch.
pub fn twin_cut(moves: &[Move], tool: &SdfTool, meta: &LatticeMeta) -> Result<Vec<f32>, StockError> {
    run_twin(&twin_work(moves, tool, meta)?, meta)
}
