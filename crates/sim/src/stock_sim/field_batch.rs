//! The field's cost law (M7.3): a cut walks only the bricks its sweep
//! can change, and a batch of moves visits each brick once.
//!
//! Provenance: the brick-map discipline the Phase 7 reference note
//! names (dirty bricks from a sweep's reach, never a dense window),
//! with every skip proved from the same 1-Lipschitz bound the sphere
//! tracer stands on (Hart's law: a distance bounds what can change
//! within it). Nothing from vendor binaries.
//!
//! ## The laws here
//!
//! - **Skips are proofs, never opinions.** A cut changes `distance`
//!   only where the mesh term is not `+cap`; outside `region` (the
//!   allocated band and the shell's box, dilated) it is `+cap` by
//!   construction, so no claim there can move an answer. Inside, a
//!   brick whose center stands `cap` + its half-diagonal clear of the
//!   swept solid holds no cell the per-cell law would claim; a brick
//!   whose center is that deep INSIDE holds only cells the law clamps
//!   to `cap` (and whose vertical entry is the center itself). Both
//!   bounds carry the golden section's own residual as slack, so the
//!   fast path writes bit for bit what the per-cell walk would.
//! - **A full cell is final.** `cap` is the cut layer's ceiling, and
//!   a cell at `cap` answers air whatever the mesh says, so its
//!   ray-entry companion is never read (the march reads it only at a
//!   material center): later claims on it are skipped, and a brick
//!   whose 512 cells are all full is skipped whole.
//! - **Batches commute.** The layers are a running max and a running
//!   min, so walking a batch brick-major (every sweep that reaches a
//!   brick, while it is hot) answers exactly what move-major does -
//!   and a long program stops re-walking the same brick per move.

use std::collections::{HashMap, HashSet};

use super::field::{brick_of, center_of, cell_of, in_brick, SdfField, BRICK, BRICK3};
use super::field_cut::Sweep;
use super::field_ray;

use crate::geometry::Point3;

/// Moves per batch: a long program walks in batches this size, so the
/// brick work list stays bounded while each brick is visited once per
/// batch rather than once per move.
pub(crate) const BATCH_MOVES: usize = 1024;

/// The field's cost-law state: where a cut can matter, which cut
/// bricks are full (count of cells at `cap`; 512 means final), and
/// what the walks cost so far.
#[derive(Clone, Debug)]
pub(crate) struct CostLedger {
    pub(crate) region: [(i32, i32); 3],
    pub(crate) full: HashMap<[i32; 3], u16>,
    pub(crate) stats: CutStats,
    /// M13.1's upload ledger: the cut bricks whose values moved since
    /// the last `take_dirty_bricks` - what a playback picture re-sends.
    pub(crate) dirty: HashSet<[i32; 3]>,
    /// Whether the ledger is kept (off during a playback jump, whose
    /// picture is sent whole: the per-cell insert is then pure cost).
    pub(crate) track_dirty: bool,
}

impl Default for CostLedger {
    fn default() -> Self {
        CostLedger {
            region: [(0, -1); 3],
            full: HashMap::new(),
            stats: CutStats::default(),
            dirty: HashSet::new(),
            track_dirty: true,
        }
    }
}

/// What the cut walks cost: the cost law's evidence, counted cheaply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CutStats {
    /// Chords subtracted (an arc counts its chords).
    pub sweeps: u64,
    /// Distinct bricks a batch walked (its dirty-brick work list):
    /// one visit per brick per batch, however many moves reach it.
    pub bricks_visited: u64,
    /// (brick, sweep) pairs that did per-cell or fill work.
    pub bricks_touched: u64,
    /// Brick visits dismissed in O(1): full, or proved out of reach.
    pub bricks_skipped: u64,
    /// Per-cell swept-distance evaluations.
    pub cells_evaluated: u64,
}

impl SdfField {
    /// The cost counters since construction (or the last reset).
    #[must_use]
    pub fn cut_stats(&self) -> CutStats {
        self.cost.stats
    }

    /// Zero the cost counters - a bench measures one program at a time.
    pub fn reset_cut_stats(&mut self) {
        self.cost.stats = CutStats::default();
    }

    /// Bytes the cut layers' bricks hold (the mesh layer is shared): the
    /// price of a whole copy. A clone shares the bricks until a write
    /// (`cut_bytes_after` is what a playback snapshot holds of its own).
    #[must_use]
    pub fn cut_layer_bytes(&self) -> usize {
        (self.cuts.len() + self.cut_ups.len() + self.cut_downs.len()).saturating_mul(BRICK3 * std::mem::size_of::<f64>())
    }

    /// Cut bricks (brick keys) whose cut-layer values changed since the
    /// last take: a superset of what moved, never a subset.
    #[must_use]
    pub fn dirty_bricks(&self) -> &HashSet<[i32; 3]> {
        &self.cost.dirty
    }

    /// Hand over the dirty-brick set and start a fresh one.
    pub fn take_dirty_bricks(&mut self) -> HashSet<[i32; 3]> {
        std::mem::take(&mut self.cost.dirty)
    }

    /// Keep (or stop keeping) the dirty ledger. While it is off the
    /// ledger is incomplete and a consumer must send the whole layer;
    /// turning it back on starts a fresh ledger, so what it then holds
    /// is relative to the next whole upload, never to a cut it missed.
    pub fn set_dirty_tracking(&mut self, on: bool) {
        if on && !self.cost.track_dirty {
            self.cost.dirty.clear();
        }
        self.cost.track_dirty = on;
    }

    /// Whether the dirty ledger is being kept.
    #[must_use]
    pub fn tracks_dirty(&self) -> bool {
        self.cost.track_dirty
    }
}

impl super::backend::Stock {
    /// The field arm's dirty-brick set, taken (see
    /// `SdfField::take_dirty_bricks`); `None` for a column stock.
    pub fn take_dirty_bricks(&mut self) -> Option<HashSet<[i32; 3]>> {
        match self {
            super::backend::Stock::Field(f) => Some(f.take_dirty_bricks()),
            super::backend::Stock::Column(_) => None,
        }
    }

    /// `SdfField::set_dirty_tracking` on the field arm; a column stock
    /// keeps no ledger and ignores it.
    pub fn set_dirty_tracking(&mut self, on: bool) {
        if let super::backend::Stock::Field(f) = self {
            f.set_dirty_tracking(on);
        }
    }
}

/// The cell box a cut can matter in: the allocated band's bricks and
/// the shell's bounding box dilated by the band (cap) plus three
/// cells - the preview pack's margin and one more - all dilated by
/// two cells so a trilinear stencil that touches a non-`cap` mesh
/// cell lies wholly inside. Outside it, a cell is in no allocated
/// brick (magnitude `cap`) and outside the shell's box (parity: air),
/// so the mesh term is `+cap` and `max(+cap, claim <= cap)` is `+cap`.
pub(crate) fn region_of(f: &SdfField) -> [(i32, i32); 3] {
    let mut lo = [i32::MAX; 3];
    let mut hi = [i32::MIN; 3];
    for k in f.bricks.keys() {
        for a in 0..3 {
            lo[a] = lo[a].min(k[a].saturating_mul(BRICK));
            hi[a] = hi[a].max(k[a].saturating_mul(BRICK).saturating_add(BRICK - 1));
        }
    }
    let b = f.bounds();
    let pad = (f.cap / f.cell).ceil().clamp(0.0, 1e6) as i32 + 3;
    let (bl, bh) = ([b.min.x, b.min.y, b.min.z], [b.max.x, b.max.y, b.max.z]);
    for a in 0..3 {
        if bl[a].is_finite() && bh[a].is_finite() {
            lo[a] = lo[a].min(cell_of(bl[a], f.cell).saturating_sub(pad));
            hi[a] = hi[a].max(cell_of(bh[a], f.cell).saturating_add(pad));
        }
    }
    let mut r = [(0, -1); 3];
    for a in 0..3 {
        if lo[a] <= hi[a] {
            r[a] = (lo[a].saturating_sub(2), hi[a].saturating_add(2));
        }
    }
    r
}

/// Half the diagonal of a brick's cell-center lattice, in cells:
/// centers span `BRICK - 1` cells per axis.
const HALF_DIAG: f64 = 0.5 * (BRICK - 1) as f64 * 1.7320508075688772;

/// How a sweep meets a whole brick, decided at its center.
enum Meet {
    /// No cell of the brick can be claimed.
    Clear,
    /// Every cell is `cap` deep inside: the law's clamp, wholesale.
    Deep,
    /// The surface runs through: the per-cell law decides.
    Mixed,
}

impl SdfField {
    /// Fold a batch of prepared sweeps into the cut layers, brick by
    /// brick (see the module law). The caller proved every window
    /// payable, so this walk cannot refuse.
    pub(crate) fn cut_batch(&mut self, sweeps: &[([f64; 3], Sweep)]) {
        let mut work: HashMap<[i32; 3], Vec<u32>> = HashMap::new();
        for (n, (ax, sw)) in sweeps.iter().enumerate() {
            self.cost.stats.sweeps += 1;
            let Some(w) = self.clipped(sw.window(self.cell, self.cap, *ax)) else {
                continue; // the whole reach lies where no cut can matter
            };
            let lo = brick_of([w[0].0, w[1].0, w[2].0]);
            let hi = brick_of([w[0].1, w[1].1, w[2].1]);
            for bx in lo[0]..=hi[0] {
                for by in lo[1]..=hi[1] {
                    for bz in lo[2]..=hi[2] {
                        work.entry([bx, by, bz]).or_default().push(n as u32);
                    }
                }
            }
        }
        self.cost.stats.bricks_visited += work.len() as u64;
        for (bk, list) in work {
            for n in list {
                if self.cost.full.get(&bk).copied() == Some(BRICK3 as u16) {
                    self.cost.stats.bricks_skipped += 1;
                    break; // a full brick is final (the module law)
                }
                let Some((ax, sw)) = sweeps.get(n as usize) else {
                    continue;
                };
                self.cut_brick(bk, *ax, sw);
            }
        }
    }

    /// The sweep's window intersected with the cut region.
    pub(crate) fn clipped(&self, w: [(i32, i32); 3]) -> Option<[(i32, i32); 3]> {
        let mut out = w;
        for a in 0..3 {
            out[a] = (w[a].0.max(self.cost.region[a].0), w[a].1.min(self.cost.region[a].1));
            if out[a].0 > out[a].1 {
                return None;
            }
        }
        Some(out)
    }

    /// One sweep against one brick: dismissed, filled, or walked.
    fn cut_brick(&mut self, bk: [i32; 3], ax: [f64; 3], sw: &Sweep) {
        let Some(w) = self.clipped(sw.window(self.cell, self.cap, ax)) else {
            self.cost.stats.bricks_skipped += 1;
            return;
        };
        let cell = self.cell;
        let span = |a: usize| {
            let b0 = bk[a].saturating_mul(BRICK);
            (b0.max(w[a].0), b0.saturating_add(BRICK - 1).min(w[a].1))
        };
        let (si, sj, sk) = (span(0), span(1), span(2));
        if si.0 > si.1 || sj.0 > sj.1 || sk.0 > sk.1 {
            self.cost.stats.bricks_skipped += 1;
            return;
        }
        let mid = |a: usize| (bk[a] as f64 * BRICK as f64 + 0.5 * BRICK as f64) * cell;
        let c = Point3::new(mid(0), mid(1), mid(2));
        let meet = self.meet(ax, sw, c);
        if matches!(meet, Meet::Clear) {
            self.cost.stats.bricks_skipped += 1;
            return;
        }
        self.cost.stats.bricks_touched += 1;
        for k in sk.0..=sk.1 {
            for j in sj.0..=sj.1 {
                for i in si.0..=si.1 {
                    let key = [i, j, k];
                    if self.cut_at(key) >= self.cap {
                        continue; // a full cell is final
                    }
                    let (v, u) = match meet {
                        Meet::Deep => (self.cap, 0.0),
                        _ => {
                            let q = Point3::new(center_of(i, cell), center_of(j, cell), center_of(k, cell));
                            match self.claim(ax, sw, q) {
                                Some(vu) => vu,
                                None => continue,
                            }
                        }
                    };
                    if u.is_finite() {
                        self.up_record(key, u);
                    }
                    self.claim_record(key, v);
                }
            }
        }
    }

    /// The per-cell law, unchanged from M7.1 B/C: the cheap stadium
    /// reject, the honest evaluation, the clamp, the ray entry.
    fn claim(&mut self, ax: [f64; 3], sw: &Sweep, q: Point3) -> Option<(f64, f64)> {
        if sw.stadium(q) >= self.cap {
            return None; // a window corner, by one cheap test
        }
        self.cost.stats.cells_evaluated += 1;
        let phi = sw.dist(ax, q);
        if phi >= self.cap {
            return None; // its cut could never beat the mesh's -cap
        }
        // The ray-entry companion rides every claim, no matter whether
        // its magnitude wins the max (M7.1 C's law). Two answers are
        // already in hand: a covered center enters at 0 (the entry's
        // own first test, on this same value), and a solid more than a
        // cell away (true distance >= phi - residual, 1-Lipschitz along
        // the ray) never meets the cell's ray - NaN, as the search
        // would conclude.
        let u = if phi < 0.0 {
            0.0
        } else if phi - sw.residual() > self.cell {
            f64::NAN
        } else {
            field_ray::vertical_entry(ax, sw, q, self.cell)
        };
        Some(((-phi).min(self.cap), u))
    }

    /// Max one claim into the cut layer, counting the brick full.
    fn claim_record(&mut self, key: [i32; 3], v: f64) {
        let cap = self.cap;
        // Read first: a visit that changes nothing takes no writable
        // slot, so it copies no brick that a keyframe shares (#2194).
        let cur = self.cuts.get(&brick_of(key)).map_or(f64::NAN, |b| b[in_brick(key)]);
        if cur.is_nan() || v > cur {
            let was_full = cur >= cap;
            *self.cut_mut(key) = v;
            if v >= cap && !was_full {
                *self.cost.full.entry(brick_of(key)).or_insert(0) += 1;
            }
            // The only write into the cut layer: every value change
            // marks its brick for the next upload (M13.1) - while the
            // ledger is kept at all.
            if self.cost.track_dirty {
                self.cost.dirty.insert(brick_of(key));
            }
        }
    }

    /// The brick test at its center (the module law's two bounds).
    fn meet(&self, ax: [f64; 3], sw: &Sweep, c: Point3) -> Meet {
        let reach = HALF_DIAG * self.cell;
        let slack = sw.residual() + 1e-9 * self.cell;
        if sw.stadium(c) >= self.cap + reach {
            return Meet::Clear; // the stadium is a lower bound already
        }
        let d = sw.dist(ax, c);
        if d - slack >= self.cap + reach {
            Meet::Clear
        } else if d + reach + slack <= -self.cap {
            Meet::Deep
        } else {
            Meet::Mixed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_region_holds_the_band_and_the_box() {
        let f = SdfField::from_mesh(&super::super::field_shell::tests::box_facets(), 1.0, 4).unwrap();
        for (a, r) in f.cost.region.iter().enumerate() {
            // the 4-cell box, its band of 4, the pad, all inside
            assert!(r.0 <= -9 && r.1 >= 12, "axis {a}: {r:?}");
        }
        for k in f.bricks.keys() {
            for (kk, r) in k.iter().zip(f.cost.region.iter()) {
                assert!(kk * BRICK >= r.0 && kk * BRICK + BRICK - 1 <= r.1);
            }
        }
    }
}
