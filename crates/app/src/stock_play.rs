//! The machined volume unfolding with the playhead (M7.2 C): the
//! display field cut move by move up to the transport's position,
//! seeking backward from a checkpoint. Provenance: the M7.1 field
//! engine's order-independent subtraction (a running max of swept-tool
//! distances, so any prefix of the program is cut exactly by its own
//! moves); the checkpoint journal is plain periodic snapshotting.
//! Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Playback is the field's truth unfolding.** The stock shown at
//!   move `k` is the uncut stock minus exactly the program's first `k`
//!   moves - the same cut `stock_view::field_view` makes for all of
//!   them, so the last frame of playback IS the Deviate picture.
//! - **Backward seek re-applies from a checkpoint.** A snapshot of the
//!   cut stock is kept every `stride` moves on the way forward; seeking
//!   back clones the nearest one at or before the target and cuts the
//!   rest. The snapshot count is bounded (`MAX_CHECKPOINTS`), so memory
//!   is a constant multiple of one field, never proportional to moves.
//!   (Dirty-brick deltas instead of whole snapshots are M7.3's cost law.)
//! - **Completed moves cut the kept stock; the move in flight cuts a
//!   copy (#1516).** The stock after `k` moves is exactly the prefix
//!   cut above, and it alone is snapshotted and cut on. The move under
//!   the playhead is shown as far as the tool has travelled, on a copy
//!   of that stock (`stock_play_head`), so a frame costs one copy
//!   plus one partial sweep, never a re-cut; a seek to the end cuts
//!   everything.
//! - **The timeline and the plan agree by name, or nothing plays.**
//!   The transport's script and the posting plan are walked by
//!   different callers; an operation the plan does not hold is a
//!   refusal, never a silent skip that would show material the
//!   machine removes as standing.

use std::collections::HashSet;
use std::sync::Arc;

use crate::geometry::Triangle;
use crate::stock_sim::{Planned, SimOp, Stock};
use crate::stock_view::{build_stock, build_target, display_inputs, FieldView, DEFAULT_TEXELS};

/// Snapshots kept at most - the backward seek's memory bound.
pub const MAX_CHECKPOINTS: usize = 32;

/// The smallest checkpoint stride, in moves.
const MIN_STRIDE: usize = 64;

/// Where each op's moves sit on the global move count: what the UI
/// needs to turn a playhead into a move index, without the moves.
#[derive(Clone, Debug, Default)]
pub struct PlayIndex {
    /// (op name, global index of its first move, its move count).
    ops: Vec<(String, usize, usize)>,
    total: usize,
}

impl PlayIndex {
    fn of(planned: &[Planned]) -> Self {
        let mut ix = PlayIndex::default();
        for p in planned {
            ix.ops.push((p.name.clone(), ix.total, p.moves.len()));
            ix.total = ix.total.saturating_add(p.moves.len());
        }
        ix
    }

    /// Moves in the whole program.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// The global move index the playhead `p` stands at, or `None`
    /// when the plan holds no op of that name. An op's timeline is its
    /// moves, one segment each, behind at most one connector rapid (the
    /// air between programs, cut by nothing): a span one longer than
    /// the op's moves carries that connector, and it is not a move.
    #[must_use]
    pub fn index_at(&self, p: &Progress) -> Option<usize> {
        let (_, start, len) = self.ops.iter().find(|o| o.0 == p.op)?;
        let connector = usize::from(p.span == len + 1);
        let done = p.segs_done.saturating_sub(connector).min(*len);
        Some(start.saturating_add(done))
    }
}

/// Everything playback reads from the world, gathered on the world's
/// thread: plain data, so the fields can be built (and cut) elsewhere.
pub struct PlaySeed {
    stock_facets: Vec<Triangle>,
    planned: Vec<Planned>,
    cell: f64,
    /// The model's world facets when the playback is judged.
    part: Option<Vec<Triangle>>,
    /// The verdict tolerance the colours are gated at, or why the
    /// program is not judged (#1056, `model_fidelity::program_tol`).
    tol: Result<f64, String>,
}

impl PlaySeed {
    /// Read the document's program and stock at a display budget of
    /// `texels` cells (see `FieldPlay::new` for `judged`).
    pub fn gather(world: &flecs_ecs::prelude::World, judged: bool, texels: f64) -> Result<Self, String> {
        let (stock_facets, planned, cell) = display_inputs(world, texels)?;
        let part = if judged { Some(crate::stock_sim::part_facets(world)?) } else { None };
        let tol = crate::model_fidelity::program_tol(world);
        Ok(PlaySeed { stock_facets, planned, cell, part, tol })
    }

    /// The move table the transport maps onto (cheap; no field built).
    #[must_use]
    pub fn index(&self) -> PlayIndex {
        PlayIndex::of(&self.planned)
    }
}

struct PlayOp {
    name: String,
    moves: Vec<crate::cl::Move>,
    radius: f64,
    ball: bool,
    cone: Option<f64>,
    corner: Option<f64>,
    /// Global index of this op's first move.
    start: usize,
    /// A rotary op's fitted axis: its slices are chorded about it
    /// (M13.1, `stock_sim::rotary`).
    rotary: Option<crate::verify::RotaryAxis>,
    /// A turning op's nose and spindle (#19, `stock_sim::lathe`).
    lathe: Option<crate::stock_sim::LathePlan>,
}

/// The playback field: the uncut stock, the plan, the stock cut to
/// `at` moves, and the checkpoint journal.
pub struct FieldPlay {
    ops: Vec<PlayOp>,
    index: PlayIndex,
    total: usize,
    stride: usize,
    /// (moves cut, the stock after them), ascending; [0] is uncut.
    checkpoints: Vec<(usize, Stock)>,
    cur: Stock,
    at: usize,
    /// A checkpoint replaced `cur` since the last `take_dirty`: its
    /// dirty ledger is not relative to anything uploaded (M13.1).
    restored: bool,
    /// Jump mode (M13.1 leftover): the dirty ledger is off, every
    /// take answers "send the whole layer".
    batched: bool,
    target: Option<Arc<Stock>>,
    target_note: Option<String>,
    cell: f64,
    tol: f64,
    /// The travelled fraction of move `at` the picture shows (#1516).
    frac: f64,
    /// `cur` plus the travelled part of move `at`, when there is one.
    partial: Option<Stock>,
    /// The bricks `partial` changed beyond `cur`.
    partial_bricks: HashSet<[i32; 3]>,
    /// The partial bricks of the last picture taken: the next take
    /// re-sends them, whatever stands there now.
    ghost: HashSet<[i32; 3]>,
}

impl FieldPlay {
    /// Build the playback for the document's program at the default
    /// display budget. `judged`: carry the model's field so the eye
    /// classes each hit (the Deviate cell is armed); otherwise the
    /// picture is unjudged material.
    pub fn new(world: &flecs_ecs::prelude::World, judged: bool) -> Result<Self, String> {
        Self::with_budget(world, judged, DEFAULT_TEXELS)
    }

    /// `new` at a display budget of `texels` cells (the
    /// `field_view_with` twin: coarser is cheaper, never less honest).
    pub fn with_budget(world: &flecs_ecs::prelude::World, judged: bool, texels: f64) -> Result<Self, String> {
        Self::from_seed(PlaySeed::gather(world, judged, texels)?)
    }

    /// Build the fields from a gathered seed - no world needed, so a
    /// worker thread can pay for it.
    pub fn from_seed(seed: PlaySeed) -> Result<Self, String> {
        let PlaySeed { stock_facets, planned, cell, part, tol } = seed;
        let index = PlayIndex::of(&planned);
        let stock = build_stock(&stock_facets, cell)?;
        let mut ops = Vec::with_capacity(planned.len());
        let mut total = 0usize;
        for p in planned {
            let n = p.moves.len();
            ops.push(PlayOp { name: p.name, moves: p.moves, radius: p.radius, ball: p.ball, cone: p.cone, corner: p.corner, start: total, rotary: p.rotary, lathe: p.lathe });
            total = total.saturating_add(n);
        }
        let (target, target_note) = if let (Some(_), Err(why)) = (&part, &tol) {
            // #1056: no colours off a mesh the gate refuses to judge.
            (None, Some(why.clone()))
        } else if let Some(part) = &part {
            build_target(part, cell)
        } else {
            (None, Some("playback shows material, not a verdict - arm Deviate to judge".to_string()))
        };
        let stride = total.div_ceil(MAX_CHECKPOINTS).max(MIN_STRIDE);
        Ok(FieldPlay {
            ops,
            index,
            total,
            stride,
            checkpoints: vec![(0, stock.clone())],
            cur: stock,
            at: 0,
            restored: true,
            batched: false,
            target: target.map(Arc::new),
            target_note,
            cell,
            tol: tol.unwrap_or(crate::stock_sim::DEV_TOL),
            frac: 0.0,
            partial: None,
            partial_bricks: HashSet::new(),
            ghost: HashSet::new(),
        })
    }

    /// Moves in the whole program.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Moves the shown stock has been cut by.
    #[must_use]
    pub const fn at(&self) -> usize {
        self.at
    }

    /// Whether the model's field rides along (see `new`).
    #[must_use]
    pub const fn judged(&self) -> bool {
        self.target.is_some()
    }

    /// Snapshots held (bounded by `MAX_CHECKPOINTS` + 1).
    #[must_use]
    pub fn checkpoints(&self) -> usize {
        self.checkpoints.len()
    }

    /// The global move index the playhead `p` stands at, or `None`
    /// when the plan holds no op of that name. An op's timeline is its
    /// moves, one segment each, behind at most one connector rapid (the
    /// air between programs, cut by nothing): a span one longer than
    /// the op's moves carries that connector, and it is not a move.
    #[must_use]
    pub fn index_at(&self, p: &Progress) -> Option<usize> {
        self.index.index_at(p)
    }

    /// The move table (`PlayIndex`).
    #[must_use]
    pub fn index(&self) -> &PlayIndex {
        &self.index
    }

    /// The newest checkpoint at or before `k`: where a backward seek
    /// to `k` restarts cutting from (0 when none is nearer).
    #[must_use]
    pub fn floor_checkpoint(&self, k: usize) -> usize {
        self.checkpoints.iter().rev().find(|(n, _)| *n <= k).map_or(0, |c| c.0)
    }

    /// The next checkpoint boundary past `at` (the end at the latest):
    /// a jump cuts checkpoint to checkpoint, one batch per stride.
    #[must_use]
    pub fn next_boundary(&self) -> usize {
        (self.at / self.stride.max(1)).saturating_add(1).saturating_mul(self.stride).min(self.total)
    }

    /// Jump mode on or off (M13.1 leftover): on, the field keeps no
    /// dirty ledger (a jump's picture travels whole anyway, and the
    /// per-cell bookkeeping is pure cost); off again, the next take is
    /// a whole upload and the ledger starts fresh from there.
    pub fn set_batched(&mut self, on: bool) {
        if self.batched == on {
            return;
        }
        self.batched = on;
        self.cur.set_dirty_tracking(!on);
        if !on {
            self.restored = true;
        }
    }

    /// Whether jump mode is on.
    #[must_use]
    pub const fn batched(&self) -> bool {
        self.batched
    }

    /// Cut (or restore) the shown stock to exactly `k` moves; `Ok(true)`
    /// when the picture changed. `k` past the end clamps to the end.
    pub fn seek(&mut self, k: usize) -> Result<bool, String> {
        let k = k.min(self.total);
        if k == self.at {
            return Ok(false);
        }
        // The move in flight changes: its partial picture goes.
        self.drop_partial();
        if k < self.at {
            let (at, snap) = self
                .checkpoints
                .iter()
                .rev()
                .find(|(n, _)| *n <= k)
                .ok_or("playback lost its uncut stock")?;
            self.cur = snap.clone();
            self.at = *at;
            self.restored = true;
            // The snapshot carries the ledger mode of its own time.
            self.cur.set_dirty_tracking(!self.batched);
        }
        // Forward in stride-sized runs, snapshotting each boundary
        // passed for the first time.
        while self.at < k {
            let boundary = (self.at / self.stride + 1).saturating_mul(self.stride);
            let to = k.min(boundary);
            self.cut(self.at, to)?;
            self.at = to;
            let last = self.checkpoints.last().map_or(0, |c| c.0);
            if to == boundary && to > last && self.checkpoints.len() <= MAX_CHECKPOINTS {
                self.checkpoints.push((to, self.cur.clone()));
            }
        }
        Ok(true)
    }

    /// The shown stock as the eye's view (a snapshot: the eye holds it
    /// while playback moves on). The copy is the cut layers only - the
    /// mesh layer and the model are shared (`Stock` clones share the
    /// field's mesh bricks; the target rides an `Arc`).
    #[must_use]
    pub fn view(&self) -> FieldView {
        FieldView {
            stock: self.partial.as_ref().unwrap_or(&self.cur).clone(),
            target: self.target.clone(),
            target_note: self.target_note.clone(),
            cell: self.cell,
            tol: self.tol,
            packed: None,
        }
    }

    /// The cut bricks changed since the last take, as one contiguous
    /// forward run; `None` when the delta is not provable that way (a
    /// checkpoint restore, the first take, jump mode, a column stock)
    /// and the caller must re-send the whole cut layer.
    /// A partial picture adds its own bricks and the last one's.
    pub fn take_dirty(&mut self) -> Option<HashSet<[i32; 3]>> {
        let d = self.cur.take_dirty_bricks();
        let ghost = std::mem::replace(&mut self.ghost, self.partial_bricks.clone());
        if self.batched {
            return None;
        }
        let restored = std::mem::replace(&mut self.restored, false);
        d.filter(|_| !restored).map(|mut set| {
            set.extend(ghost);
            set.extend(self.partial_bricks.iter().copied());
            set
        })
    }

    /// Subtract global moves `[from, to)` from the shown stock.
    fn cut(&mut self, from: usize, to: usize) -> Result<(), String> {
        for op in &self.ops {
            let end = op.start.saturating_add(op.moves.len());
            let (a, b) = (from.max(op.start), to.min(end));
            if a >= b {
                continue;
            }
            let moves = op.moves.get(a - op.start..b - op.start).ok_or("playback: a move range fell outside its op")?;
            let sim = SimOp { moves, radius: op.radius, ball: op.ball, cone: op.cone, corner: op.corner };
            // A rotary slice is chorded about the fitted axis, turning
            // from the pose the program held before it (M13.1).
            let cut = match (&op.rotary, &op.lathe) {
                (_, Some(l)) => self.cur.simulate_lathe_profile(moves, &l.profile, &l.axis),
                (Some(ax), None) => {
                    let tol = crate::stock_sim::rotary_pose_tol(self.cur.cell());
                    crate::stock_sim::simulate_rotary_slice(&mut self.cur, &sim, &op.moves, a - op.start, ax, tol).map(|_| ())
                }
                (None, None) => self.cur.simulate_display(std::slice::from_ref(&sim)),
            };
            cut.map_err(|e| format!("playback could not cut {}: {e}", op.name))?;
        }
        Ok(())
    }
}

#[path = "stock_play_head.rs"]
mod head;
pub use head::{progress, settle_frac, Progress};
