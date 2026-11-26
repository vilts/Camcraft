//! The rest-material model the verifier's laws share - per-plane island sections and column surface heights; our own conservative approximation, stated in `verify`'s module law (split from `engine.rs` by M13.7; structure only).

use super::cut_index::CutIndex;
use super::laws::dist2_seg;
use super::slab::Slabs;
use super::slice_index::IndexedSlice;
use super::types::VerifyContext;

use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use crate::cl::{MoveKind, Toolpath};
use crate::geo2d::{Point2, Region};
use crate::geometry::Triangle;

/// A cutting chord: its XY ends, the plane it cuts at, its move index.
pub(super) type Cut = (Point2, Point2, f64, usize);

/// A column's cut-order staircase (M13.7): `(move index, height)` steps,
/// index ascending and height strictly descending - the column's surface
/// once every chord of a move index below `upto` has cut is the last
/// step whose index is below `upto`, capped at the stock top.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Steps {
    steps: Vec<(usize, f64)>,
    top: f64,
}

impl Steps {
    /// The column surface before move `upto` - exactly what
    /// [`Rest::surface_before`] answers for the same column.
    pub(super) fn before(&self, upto: usize) -> f64 {
        let k = self.steps.partition_point(|s| s.0 < upto);
        let z = match k.checked_sub(1).and_then(|j| self.steps.get(j)) {
            Some(s) => s.1,
            None => f64::INFINITY,
        };
        z.min(self.top)
    }
}

/// One path's rest-material view: the plane-section cache and the
/// cutting chords the column surface is read from.
pub(super) struct Rest<'c, 'a> {
    cx: &'c VerifyContext<'a>,
    /// The per-solid index ranges of `cx.model` when the model is a
    /// union of touching bodies (M13.11): every plane section below is
    /// read through `slice::material_above_of`, the 2D union of the
    /// bodies' own sections. Fewer than two ranges: one body, the
    /// plain slice, byte for byte.
    bodies: &'c [std::ops::Range<usize>],
    /// A model-driven path's islands per plane - the generator's own
    /// `material_above` plus an exact Y-slab point index (#3615), so one law
    /// has two readers (M2.1a). Cached by the plane's bits: the level ladder
    /// repeats its planes across all three readers. `None` means the slice
    /// failed; every use of a failed slice fails closed (no proof, no pass).
    island_cache: RefCell<HashMap<u64, Option<Rc<IndexedSlice>>>>,
    /// The model's prismatic Z slabs (#3831, [`Slabs`]), found on the
    /// first section asked. `None` inside reads every plane on its own -
    /// the plane-exact reading the tests compare against.
    slabs: OnceCell<Option<Slabs>>,
    /// One section per prismatic slab (#3831): every plane of such a
    /// slab is the same point set, so a ramp that ends each move at a Z
    /// of its own sections the slab once, not once per move. Only a
    /// section that sliced is kept: a failed one is asked again at each
    /// plane, as before. A flat plane's own section is never kept here.
    slab_cache: RefCell<HashMap<usize, Rc<IndexedSlice>>>,
    /// The planes the path cuts flat at (a cutting move with one Z), by
    /// their bits (#3831). Each keeps its own section, so a level ladder
    /// reads exactly what it always read; only the planes between them -
    /// the ends of a ramp's or a helix's moves - share the slab's section.
    flat: HashSet<u64>,
    /// The model sections sliced so far (#3831), counted always - one
    /// integer add - and read only by the tests.
    sliced: Cell<usize>,
    /// Cutting (non-rapid) XY segments with the level they cut at: a
    /// segment's whole disc sweep clears its column down to `to.z`
    /// (plunges are degenerate segments at their XY, arcs contribute
    /// their chords - all within chord_tol of the true arc).
    /// Deepest plane first, so `surface`'s first reach is its deepest
    /// reach - the min the law always asked for, found without a scan.
    /// Each chord carries its move's index (M13.7 cut order): a move is
    /// judged only against the chords cut BEFORE it, never against
    /// stock a later move removes.
    cut: Vec<Cut>,
    /// The chords' grid broad phase (M13.7 cut order): with order a
    /// query can no longer stop at the first in-reach chord, so it walks
    /// only the chords that can reach its column.
    index: CutIndex,
    /// The stock hidden under overhangs per plane (M13.7 B) - read by
    /// [`super::hidden`]; `None` is a failed slice, failing closed.
    pub(super) hidden_cache: RefCell<HashMap<u64, Option<Rc<super::hidden::Hidden>>>>,
    /// ALL part material at or above a plane, projected (the
    /// generators' islands law, `slice::material_overhead`), per
    /// plane, read by the posed rapid reading ([`super::rapids_posed`]);
    /// `None` is a failed slice, failing closed.
    overhead_cache: RefCell<HashMap<u64, Option<Rc<Region>>>>,
}

impl<'c, 'a> Rest<'c, 'a> {
    pub(super) fn new(
        path: &Toolpath,
        cx: &'c VerifyContext<'a>,
        chord_tol: f64,
        bodies: &'c [std::ops::Range<usize>],
    ) -> Self {
        // #1444: an undercut head cuts under a roof - no column is cleared.
        let clears = cx.mode != super::types::Mode::Undercut;
        let mut cut: Vec<Cut> = path
            .moves
            .iter()
            .enumerate()
            .filter(|(_, m)| clears && m.kind != MoveKind::Rapid)
            .flat_map(|(i, m)| {
                m.chords(chord_tol)
                    .into_iter()
                    .map(move |(a, b)| (a, b, m.to.z, i))
            })
            .collect();
        cut.sort_by(|a, b| a.2.total_cmp(&b.2));
        let index = CutIndex::new(&cut, cx.tool.radius + 1e-9);
        let flat = path
            .moves
            .iter()
            .filter(|m| m.kind != MoveKind::Rapid && m.from.z == m.to.z)
            .map(|m| m.to.z.to_bits())
            .collect();
        Self {
            cx,
            bodies,
            island_cache: RefCell::new(HashMap::new()),
            slabs: OnceCell::new(),
            slab_cache: RefCell::new(HashMap::new()),
            flat,
            sliced: Cell::new(0),
            cut,
            index,
            hidden_cache: RefCell::new(HashMap::new()),
            overhead_cache: RefCell::new(HashMap::new()),
        }
    }

    /// Whether part material stands at or above (q, `h`): the section
    /// at `h` united with the shadow of what hangs over it. `None` is
    /// no model or a failed slice - the caller fails closed.
    pub(super) fn part_over(&self, q: Point2, h: f64) -> Option<bool> {
        let f = self.cx.model?;
        if !h.is_finite() {
            return None;
        }
        let key = h.to_bits();
        let hit = self.overhead_cache.borrow().get(&key).cloned();
        let region = match hit {
            Some(r) => r,
            None => {
                let fresh =
                    crate::geometry::slice::material_overhead_of(f, self.bodies, h).ok().map(Rc::new);
                self.overhead_cache.borrow_mut().entry(key).or_insert_with(|| fresh.clone());
                fresh
            }
        };
        region.map(|r| r.contains(q))
    }

    /// The context the laws are judged in.
    pub(super) fn cx(&self) -> &VerifyContext<'a> {
        self.cx
    }

    /// The cutting chords, deepest plane first.
    pub(super) fn cut(&self) -> &[Cut] {
        &self.cut
    }

    /// The cached indexed slice at plane `z` (one region, one exact point
    /// index). `None` is a failed slice, memoized so it fails closed on every
    /// later read too. A plane in a prismatic slab of the context's model
    /// (#3831) that the path does not cut flat at reads the slab's one
    /// section: the first such plane that sliced stands for them all.
    pub(super) fn island_slice(&self, facets: &[Triangle], z: f64) -> Option<Rc<IndexedSlice>> {
        let key = z.to_bits();
        if let Some(hit) = self.island_cache.borrow().get(&key) {
            return hit.clone();
        }
        let slab = self.slab_of(facets, z).filter(|_| !self.flat.contains(&key));
        let shared = slab.and_then(|k| self.slab_cache.borrow().get(&k).cloned());
        let fresh = shared.or_else(|| {
            self.sliced.set(self.sliced.get().saturating_add(1));
            let s = crate::geometry::slice::material_above_of(facets, self.bodies, z)
                .ok()
                .map(|r| Rc::new(IndexedSlice::new(r)))?;
            if let Some(k) = slab {
                self.slab_cache.borrow_mut().entry(k).or_insert_with(|| s.clone());
            }
            Some(s)
        });
        self.island_cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| fresh.clone());
        fresh
    }

    /// The prismatic slab of the context's model that holds plane `z`
    /// (#3831). Only `cx.model` itself is placed: any other facet slice
    /// is sectioned plane by plane.
    fn slab_of(&self, facets: &[Triangle], z: f64) -> Option<usize> {
        let model = self.cx.model?;
        if !std::ptr::eq(model, facets) {
            return None;
        }
        self.slabs.get_or_init(|| Some(Slabs::new(model))).as_ref()?.of(z)
    }

    /// The section whose vertices law (c) samples at plane `z`.
    /// A shared slab preserves geometric membership, but not the vertices
    /// or their mean that `boundary_samples` nudges toward. No replacement
    /// sample set proves coverage of those points, so section each such
    /// plane exactly. Other planes reuse their exact cached section.
    pub(super) fn reach_slice(&self, facets: &[Triangle], z: f64) -> Option<Rc<IndexedSlice>> {
        let s = self.island_slice(facets, z)?;
        if !self.slab_cache.borrow().values().any(|v| Rc::ptr_eq(v, &s)) {
            return Some(s);
        }
        self.sliced.set(self.sliced.get().saturating_add(1));
        crate::geometry::slice::material_above_of(facets, self.bodies, z)
            .ok()
            .map(|r| Rc::new(IndexedSlice::new(r)))
    }

    /// How many model sections were sliced (#3831); the tests read it.
    #[cfg(test)]
    pub(super) fn sliced(&self) -> usize {
        self.sliced.get()
    }

    /// This rest with every plane sectioned on its own, as before #3831:
    /// the reference the tests compare the slab reading against.
    #[cfg(test)]
    pub(super) fn plane_exact(self) -> Self {
        Self { slabs: OnceCell::from(None), ..self }
    }

    /// The model's section strictly above plane `z` (cached).
    pub(super) fn islands_at(&self, facets: &[Triangle], z: f64) -> Option<Region> {
        self.island_slice(facets, z).map(|s| s.region().clone())
    }

    /// Whether the cached slice at plane `z` holds `q`, without cloning the
    /// plane's region to ask it (M2.5 E): `surface`'s binary search and
    /// `part_at` probe thousands of cached slices per program. The Y-slab
    /// index (#3615) answers exactly `Region::contains` for `q`, reading only
    /// the edges that cross `q.y`.
    pub(super) fn island_holds(&self, facets: &[Triangle], z: f64, q: Point2) -> bool {
        self.island_slice(facets, z).is_some_and(|s| s.contains(q))
    }

    /// Material footprint for the reach laws: the stock prism when given,
    /// the region standing in for it otherwise.
    pub(super) fn in_stock(&self, q: Point2) -> bool {
        let cx = self.cx;
        cx.stock.map_or_else(|| cx.region.contains(q), |s| s.contains(q))
    }

    /// m(q) before move `upto`: the surface height of remaining material
    /// in q's column once every chord of a move index below `upto` has
    /// cut (M13.7 cut order) - a move is judged against the cuts before
    /// it, never against stock a later move removes. The index filter
    /// only shrinks the clearing set, so the height only rises: a
    /// verdict can gain a conviction from it, never lose one.
    /// A model-driven path adds one clause (M2.1a): a chord at plane z
    /// cleared q's column only if the section at z does not hold q -
    /// cutting around an island removes nothing inside it, and the
    /// staircase's top is the shallower plane that did reach. Sections
    /// are monotone in z, so the island test is a point-in-region on a
    /// cached slice, run only for chords that are already within reach.
    pub(super) fn surface_before(&self, q: Point2, upto: usize) -> f64 {
        let lo = self.island_threshold(q);
        // The upward test keeps the island clause: above the threshold
        // it never fires (that is the threshold's meaning), so this is
        // the old predicate restated, not an approximation of it. The
        // walk is z-ascending, so the first hit is the deepest reach.
        let cleared = self
            .near(q, lo)
            .find(|c| c.3 < upto && self.clears(c, q))
            .map_or(f64::INFINITY, |c| c.2);
        cleared.min(self.cx.stock_top_z)
    }

    /// q's whole cut-order staircase (see [`Steps`]), for a reader that
    /// memoizes one column across many moves (law (c3)'s path-wide
    /// memo). One upward walk: a clearing chord becomes a step when its
    /// move comes before every step found so far - deeper chords were
    /// seen first - so the heights descend as the indices ascend, and
    /// [`Steps::before`] reads exactly what [`Self::surface_before`]
    /// would answer for every `upto`.
    pub(super) fn surface_steps(&self, q: Point2) -> Steps {
        let lo = self.island_threshold(q);
        let mut steps: Vec<(usize, f64)> = Vec::new();
        let mut best = usize::MAX;
        for c in self.near(q, lo) {
            if c.3 < best && self.clears(c, q) {
                steps.push((c.3, c.2));
                best = c.3;
                if best == 0 {
                    break; // no move comes before the first
                }
            }
        }
        steps.reverse();
        Steps { steps, top: self.cx.stock_top_z }
    }

    /// The chords from z-sorted position `from` up that can reach q's
    /// column ([`CutIndex::near`]), deepest plane first.
    pub(super) fn near(&self, q: Point2, from: usize) -> impl Iterator<Item = &Cut> + '_ {
        self.index.near(q, from).filter_map(|k| self.cut.get(k))
    }

    /// Whether chord `c` cleared q's column: within the tool radius, and
    /// (model-driven) on a plane whose section leaves q open.
    pub(super) fn clears(&self, c: &Cut, q: Point2) -> bool {
        let cx = self.cx;
        let r = cx.tool.radius;
        let r2 = (r + 1e-9) * (r + 1e-9);
        dist2_seg(q, c.0, c.1) <= r2 && cx.model.is_none_or(|f| !self.island_holds(f, c.2, q))
    }

    /// The first chord at or above q's island threshold. The island
    /// clause is a monotone threshold in z, not a scan (M2.5 E): `q`
    /// sits inside the plane-z island exactly while z < h(q) - the
    /// slice IS the part above z - so below the threshold every chord
    /// fails the island test whatever its reach or move index, and above
    /// it every chord clears iff it reaches. Binary-searched over log
    /// probed planes, no clones (`island_holds`).
    fn island_threshold(&self, q: Point2) -> usize {
        let cut = &self.cut;
        let mut lo = 0usize;
        let mut hi = cut.len();
        if let Some(f) = self.cx.model {
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if cut.get(mid).is_some_and(|c| self.island_holds(f, c.2, q)) {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
        }
        lo
    }
}

