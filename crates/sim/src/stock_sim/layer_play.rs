//! The layer playback (#2629, GPU stock sim S3b): the display cut of the
//! ops that the f32 twin can cut, kept as a dense cut layer in the
//! `pack_cut` layout instead of in the f64 field, with its own keyframes.
//! A caller-given cutter cuts each batch onto the layer: the CPU twin law
//! here, or a GPU kernel that the caller owns (camcraft-gpu's
//! `cut_onto_or_cpu` on a device of the caller's choice). Display only:
//! no verdict reads this layer.
//! Provenance: our own design (decision 2629) over the twin work list of
//! `cut_twin_work` and the keyframe store of `play_keys`.
//!
//! ## The laws here
//!
//! - **The layer at move k is a function of k.** It is the max-law fold
//!   of the layer ops' moves before k onto the `-4 cap` floor, so any
//!   keyframe at or before k plus the moves after it gives it, in any
//!   batching. No other state rides along.
//! - **Cut only for a picture.** A seek cuts from the nearest keyframe,
//!   one batch per op and stride; a jump cuts the layer once, at the
//!   picture, not at each stride on the way.
//! - **The merge is the max law.** The picture is `max(pack_cut(field),
//!   layer)`: the field holds the other ops' cuts, and a cut layer folds
//!   with the max law (`preview::pack`).

use std::sync::Arc;

use super::cut::SimOp;
use super::cut_twin_work::{run_twin_items_onto, twin_items, twin_refusal, twin_work, TwinItem};
use super::field_cut::SdfTool;
use super::kernel::StockError;
use super::partial::partial_move;
use super::preview::LatticeMeta;
use super::Stock;
use crate::cl::Move;
use crate::play_keys::{KeyCost, Keyframes};

/// The cutter of one batch: the kernel items cut onto `base` (a layer of
/// `meta`'s lattice), the new layer, or why not. It must give the layer
/// of the CPU twin law within the twin's claim margin.
pub type LayerCutter = dyn Fn(&LatticeMeta, &[TwinItem], &[f32]) -> Result<Vec<f32>, String> + Send + Sync;

/// The CPU twin law as a cutter: the fallback and the oracle.
#[must_use]
pub fn cpu_layer_cutter() -> Arc<LayerCutter> {
    Arc::new(|meta: &LatticeMeta, items: &[TwinItem], base: &[f32]| {
        run_twin_items_onto(items, meta, base).map_err(|e| e.to_string())
    })
}

/// Bytes the layer keyframes may hold.
pub const LAYER_KEY_BUDGET: usize = 256 << 20;

/// Layer keyframes past the floor, at most.
pub const LAYER_KEYS: usize = 64;

/// One layer keyframe: a whole layer, shared.
#[derive(Clone, Debug)]
pub struct LayerKey(pub Arc<Vec<f32>>);

impl KeyCost for LayerKey {
    fn key_bytes(&self) -> usize {
        self.0.len().saturating_mul(std::mem::size_of::<f32>())
    }
}

/// One layer op of the program: its first global move, its moves and the
/// tool the twin cuts with.
#[derive(Clone, Copy, Debug)]
pub struct LayerSpan<'a> {
    pub start: usize,
    pub moves: &'a [Move],
    pub tool: &'a SdfTool,
}

/// The twin tool of a plain op on `stock`, by the display cut's own law
/// (`Stock::simulate_display`: no shank, the flank from the lowest move to
/// the stock top plus slack), or why the op stays on the field: a tool
/// with no twin, a move that the twin refuses, or a stock with no field.
pub fn layer_tool(op: &SimOp, stock: &Stock) -> Result<SdfTool, StockError> {
    let Some(f) = stock.field() else {
        return Err(StockError::Invalid("layer playback: the stock has no field".into()));
    };
    let ceiling = f.bounds().max.z;
    let lowest = op.moves.iter().flat_map(|m| [m.from.z, m.to.z]).fold(f64::INFINITY, f64::min);
    let slack = 2.0 * f.cap() + 4.0 * f.cell();
    let flank = if lowest.is_finite() && ceiling.is_finite() { (ceiling - lowest).max(0.0) + slack } else { slack };
    let tool = super::backend::op_sdf_tool(op, flank, false, false);
    twin_refusal(op.moves, &tool)?;
    Ok(tool)
}

/// `max(dst, src)` texel for texel: the merge of the field's packed cut
/// with the layer.
pub fn merge_max(dst: &mut [f32], src: &[f32]) -> Result<(), StockError> {
    if dst.len() != src.len() {
        return Err(StockError::Invalid(format!(
            "layer playback: the layer has {} texels, the picture {}",
            src.len(),
            dst.len()
        )));
    }
    for (d, s) in dst.iter_mut().zip(src) {
        *d = d.max(*s);
    }
    Ok(())
}

/// The layer of the layer ops, cut to `at` moves, and its keyframes.
pub struct LayerPlay {
    meta: LatticeMeta,
    cutter: Arc<LayerCutter>,
    keys: Keyframes<LayerKey>,
    cur: LayerKey,
    at: usize,
}

impl LayerPlay {
    /// The uncut layer (the `-4 cap` floor) of `meta`'s lattice, with
    /// keyframes every `stride` moves within [`LAYER_KEY_BUDGET`].
    pub fn new(meta: LatticeMeta, cutter: Arc<LayerCutter>, stride: usize) -> Result<Self, StockError> {
        let n = (meta.dims[0] as usize)
            .checked_mul(meta.dims[1] as usize)
            .and_then(|n| n.checked_mul(meta.dims[2] as usize))
            .filter(|n| *n > 0)
            .ok_or_else(|| StockError::Invalid(format!("layer playback: the lattice dims {:?} are empty", meta.dims)))?;
        let floor = (-4.0 * meta.cap) as f32;
        if !floor.is_finite() {
            return Err(StockError::Invalid(format!("layer playback: the cap {} is not finite", meta.cap)));
        }
        let mut layer = Vec::new();
        layer.try_reserve_exact(n).map_err(|e| StockError::Invalid(format!("layer playback: no memory: {e}")))?;
        layer.resize(n, floor);
        let cur = LayerKey(Arc::new(layer));
        let mut keys = Keyframes::new(cur.clone(), stride, LAYER_KEYS);
        keys.restart(stride, LAYER_KEYS, LAYER_KEY_BUDGET);
        Ok(LayerPlay { meta, cutter, keys, cur, at: 0 })
    }

    /// The lattice of the layer.
    #[must_use]
    pub const fn meta(&self) -> &LatticeMeta {
        &self.meta
    }

    /// Moves the layer is cut to.
    #[must_use]
    pub const fn at(&self) -> usize {
        self.at
    }

    /// Layer keyframes held (the floor included).
    #[must_use]
    pub fn keyframes(&self) -> usize {
        self.keys.len()
    }

    /// The layer at `k` whole moves, plus the travelled fraction `frac`
    /// of move `k` when move `k` is a layer op's (on a copy: the layer at
    /// `k` stays exact).
    pub fn frame(&mut self, k: usize, frac: f64, spans: &[LayerSpan<'_>]) -> Result<Arc<Vec<f32>>, StockError> {
        self.seek(k, spans)?;
        let flight = spans.iter().find_map(|s| {
            let i = k.checked_sub(s.start)?;
            s.moves.get(i).map(|m| (m, s.tool))
        });
        let Some((m, tool)) = flight else {
            return Ok(Arc::clone(&self.cur.0));
        };
        let Some(sub) = partial_move(m, frac) else {
            return Ok(Arc::clone(&self.cur.0));
        };
        Ok(Arc::new(self.cut_onto(std::slice::from_ref(&sub), tool, &self.cur.0)?.unwrap_or_else(|| (*self.cur.0).clone())))
    }

    /// Cut (or restore) the layer to exactly `k` moves.
    pub fn seek(&mut self, k: usize, spans: &[LayerSpan<'_>]) -> Result<(), StockError> {
        if k == self.at {
            return Ok(());
        }
        if self.keys.restores(self.at, k) {
            let (at, key) = self
                .keys
                .floor(k)
                .ok_or_else(|| StockError::Invalid("layer playback lost its floor".into()))?;
            self.cur = key.clone();
            self.at = *at;
        }
        let stride = self.keys.stride();
        while self.at < k {
            let boundary = (self.at / stride).saturating_add(1).saturating_mul(stride);
            let to = k.min(boundary);
            self.cut_range(self.at, to, spans)?;
            self.at = to;
            if to == boundary {
                self.keys.offer(to, &self.cur);
            }
        }
        Ok(())
    }

    /// Cut global moves `[from, to)` of the layer ops onto the layer.
    fn cut_range(&mut self, from: usize, to: usize, spans: &[LayerSpan<'_>]) -> Result<(), StockError> {
        for s in spans {
            let end = s.start.saturating_add(s.moves.len());
            let (a, b) = (from.max(s.start), to.min(end));
            if a >= b {
                continue;
            }
            let moves = s
                .moves
                .get(a - s.start..b - s.start)
                .ok_or_else(|| StockError::Invalid("layer playback: a move range fell outside its op".into()))?;
            if let Some(next) = self.cut_onto(moves, s.tool, &self.cur.0)? {
                self.cur = LayerKey(Arc::new(next));
            }
        }
        Ok(())
    }

    /// `moves` cut onto `base` by the cutter; `None` when they claim no
    /// cell of the lattice (rapids, or a reach outside it).
    fn cut_onto(&self, moves: &[Move], tool: &SdfTool, base: &[f32]) -> Result<Option<Vec<f32>>, StockError> {
        let work = twin_work(moves, tool, &self.meta)?;
        if work.items.is_empty() {
            return Ok(None);
        }
        let items = twin_items(&work, &self.meta)?;
        let out = (self.cutter)(&self.meta, &items, base).map_err(|e| StockError::Invalid(format!("layer playback: {e}")))?;
        if out.len() != base.len() {
            return Err(StockError::Invalid(format!(
                "layer playback: the cutter gave {} texels, not {}",
                out.len(),
                base.len()
            )));
        }
        Ok(Some(out))
    }
}
