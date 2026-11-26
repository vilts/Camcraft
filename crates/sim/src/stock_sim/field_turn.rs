//! A field carried by baking the turn into its bricks (#637 E2a, route
//! (ii) of the step-E design): the `LatticeTurn`'s signed permutation is
//! applied to the DATA - bricks, cut layers, shell, closed-form box - and
//! only its translation is left as the view. `CarriedField` reads the
//! same solid through `LatticeTurn::unapply` per query; the bake lets a
//! later setup cut the carried stock with the shipped `+Z` code (E2c).
//!
//! ## The laws here
//!
//! - **Re-index, never re-sample.** The lattice is origin-locked
//!   (`cell_of(v) = floor(v / cell)`), so a negated axis maps cell `i` to
//!   cell `-i - 1 == !i` (cell centres onto cell centres) and a quarter
//!   turn swaps two axes' indices: `Body::carry`'s law, on bricks. The
//!   brick lattice is origin-locked too, so brick `b` maps to `!b` with
//!   its slot mirrored (`s -> 7 - s`): a whole brick moves as a whole
//!   brick. `!` is total on `i32` - no index arithmetic can overflow, so
//!   the bake has no failure arm.
//! - **Bit-identical to the view.** Every stored value is copied, every
//!   coordinate is negated or swapped (both exact in IEEE arithmetic), and
//!   the box's distance is symmetric under both, so
//!   `turned(t).0.distance(p - t.shift()) == CarriedField::new(f, t).distance(p)`
//!   bit for bit - off the lattice planes (a point ON a cell boundary
//!   belongs to a different one of its two cells after a negation) and off
//!   the shell's crossings (where the parity count's rounding decides).
//! - **On a flip, up becomes down.** `cut_ups` holds the distance UP from
//!   a cell's centre to the ray's entry into the swept solid; upside down
//!   that is the distance DOWN, so the flipped bake moves every record into
//!   `cut_downs` and vice versa. The swap IS the record: nothing is
//!   recomputed, the cut hot path is untouched.
//! - **The floor law follows the records.** A flipped bake's cuts came
//!   from below in the new frame; `field_floor` reads their tops from
//!   `cut_downs` by the up-march (`march_up`, #637 E2b), so the bake keeps
//!   `bounded_cuts` as it stood: only a bounded tool (one that never
//!   spanned the world above its tip) still refuses by name.
//! - **The cost ledger follows the data.** The region is rebuilt
//!   (`region_of`), the full-brick counts are re-keyed, and every cut
//!   brick is marked dirty: everything moved, and the dirty ledger is a
//!   superset of what moved, never a subset.
//!
//! Provenance: rigid-motion invariance of the Euclidean distance and the
//! exactness of IEEE negation; the re-index law of `carry::Body::carry`;
//! our own design (#637 step-E research comment, route (ii)).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::carry::LatticeTurn;
use super::field::{CutLayer, SdfField, BRICK, BRICK3};
use super::field_box::AnalyticBox;
use crate::geometry::{Point3, Triangle};

/// The band layer: brick key -> 512 cell values.
type Layer = HashMap<[i32; 3], Box<[f64; BRICK3]>>;

/// The turn's signed permutation per NEW axis: which old axis it reads,
/// and whether that axis is negated.
#[derive(Clone, Copy, Debug)]
struct Axes {
    src: [usize; 3],
    neg: [bool; 3],
}

impl Axes {
    fn of(turn: &LatticeTurn) -> Axes {
        let (m, s) = turn.linear();
        // A lattice turn's XY rows each hold exactly one +-1.
        let row = |r: [i8; 2]| if r[0] != 0 { (0, r[0] < 0) } else { (1, r[1] < 0) };
        let ((x, nx), (y, ny)) = (row(m[0]), row(m[1]));
        Axes { src: [x, y, 2], neg: [nx, ny, s < 0] }
    }

    /// An old lattice index triple (cells or bricks) in the new frame.
    fn key(&self, k: [i32; 3]) -> [i32; 3] {
        let f = |a: usize| {
            let v = k[self.src[a]];
            if self.neg[a] { !v } else { v }
        };
        [f(0), f(1), f(2)]
    }

    /// An old point in the new frame (no translation: that is the view).
    fn point(&self, p: Point3) -> Point3 {
        let c = [p.x, p.y, p.z];
        let f = |a: usize| {
            let v = c[self.src[a]];
            if self.neg[a] { -v } else { v }
        };
        Point3::new(f(0), f(1), f(2))
    }

    /// One brick's 512 values, slots permuted and mirrored.
    fn brick(&self, old: &[f64; BRICK3]) -> Box<[f64; BRICK3]> {
        let n = BRICK as usize;
        let mut out = Box::new([f64::NAN; BRICK3]);
        for (i, v) in old.iter().enumerate() {
            let s = [i % n, (i / n) % n, i / (n * n)];
            let t = |a: usize| {
                let c = s[self.src[a]];
                if self.neg[a] { (n - 1).saturating_sub(c) } else { c }
            };
            if let Some(slot) = out.get_mut(t(0) + n * t(1) + n * n * t(2)) {
                *slot = *v;
            }
        }
        out
    }

    fn layer(&self, old: &Layer) -> Layer {
        old.iter().map(|(k, b)| (self.key(*k), self.brick(b))).collect()
    }

    /// A cut layer, each brick a new allocation (the bake shares none).
    fn cut_layer(&self, old: &CutLayer) -> CutLayer {
        old.iter().map(|(k, b)| (self.key(*k), Arc::from(self.brick(b)))).collect()
    }
}

impl SdfField {
    /// This field carried through `turn` with the turn's signed
    /// permutation baked into its data (#637 E2a; the module law): the
    /// returned field `g` and residual shift `t` answer
    /// `g.distance(p - t) == CarriedField::new(self, turn).distance(p)`.
    /// On a flip `cut_ups` and `cut_downs` trade places.
    #[must_use]
    pub fn turned(&self, turn: &LatticeTurn) -> (SdfField, Point3) {
        let ax = Axes::of(turn);
        let flips = turn.flips();
        let mesh: Vec<Triangle> =
            self.mesh.iter().map(|t| Triangle::new(ax.point(t.a), ax.point(t.b), ax.point(t.c))).collect();
        let analytic = self.analytic.map(|b| {
            let (a, c) = (ax.point(b.lo), ax.point(b.hi));
            AnalyticBox {
                lo: Point3::new(a.x.min(c.x), a.y.min(c.y), a.z.min(c.z)),
                hi: Point3::new(a.x.max(c.x), a.y.max(c.y), a.z.max(c.z)),
            }
        });
        let (ups, downs) = if flips { (&self.cut_downs, &self.cut_ups) } else { (&self.cut_ups, &self.cut_downs) };
        let mut g = SdfField {
            cell: self.cell,
            cap: self.cap,
            mesh: Arc::new(mesh),
            bricks: Arc::new(ax.layer(&self.bricks)),
            cuts: ax.cut_layer(&self.cuts),
            cut_ups: ax.cut_layer(ups),
            cut_downs: ax.cut_layer(downs),
            cost: self.cost.clone(),
            analytic,
            bounded_cuts: self.bounded_cuts,
        };
        g.cost.full = self.cost.full.iter().map(|(k, n)| (ax.key(*k), *n)).collect();
        g.cost.region = super::field_batch::region_of(&g);
        g.cost.dirty = if g.cost.track_dirty {
            g.cuts.keys().chain(g.cut_ups.keys()).chain(g.cut_downs.keys()).copied().collect()
        } else {
            HashSet::new()
        };
        (g, turn.shift())
    }
}

#[cfg(test)]
mod tests {
    use super::super::field_shell::tests::box_facets;
    use super::*;
    use crate::cl::{Move, MoveKind};

    fn turn(r: [[f64; 3]; 3], t: [f64; 3]) -> LatticeTurn {
        LatticeTurn::from_matrix(r, t).expect("a lattice turn")
    }

    /// The 4-box, a flat slot through its top half: every layer populated.
    fn cut_box() -> SdfField {
        let mut f = SdfField::from_mesh(&box_facets(), 0.25, 4).expect("field");
        let p = |x: f64, y: f64, z: f64| Point3::new(x, y, z);
        let mv = Move::new(MoveKind::Feed, p(-1.0, 2.1, 2.3), p(5.0, 2.1, 2.3), Some(500.0));
        f.subtract(&mv, &crate::stock_sim::SdfTool::Flat { r: 0.7, flank: 10.0 }, 0.01).expect("cut");
        f
    }

    fn same<B: std::ops::Deref<Target = [f64; BRICK3]>>(a: &HashMap<[i32; 3], B>, b: &HashMap<[i32; 3], B>) -> bool {
        a.len() == b.len()
            && a.iter().all(|(k, v)| b.get(k).is_some_and(|w| v.iter().zip(w.iter()).all(|(x, y)| x.to_bits() == y.to_bits())))
    }

    #[test]
    fn four_quarter_turns_restore_every_layer_bit_for_bit() {
        let f = cut_box();
        assert!(!f.cut_ups.is_empty(), "the slot records its ray entries");
        let q = turn([[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], [0.0; 3]);
        let mut g = f.clone();
        for _ in 0..4 {
            g = g.turned(&q).0;
        }
        assert!(same(&f.bricks, &g.bricks) && same(&f.cuts, &g.cuts) && same(&f.cut_ups, &g.cut_ups));
        assert!(g.cut_downs.is_empty());
        assert_eq!(f.mesh.len(), g.mesh.len());
        for (a, b) in f.mesh.iter().zip(g.mesh.iter()) {
            assert_eq!((a.a, a.b, a.c), (b.a, b.b, b.c));
        }
        assert_eq!(f.cost.region, g.cost.region);
        assert_eq!(f.cost.full, g.cost.full);
    }

    #[test]
    fn a_flip_moves_every_up_record_down_and_back() {
        let f = cut_box();
        let flip = turn([[1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, -1.0]], [0.0, 4.0, 4.0]);
        let (g, shift) = f.turned(&flip);
        assert_eq!(shift, Point3::new(0.0, 4.0, 4.0));
        assert!(g.cut_ups.is_empty(), "nothing was recorded going down before the flip");
        assert_eq!(g.cut_downs.len(), f.cut_ups.len());
        let mut seen = 0;
        for (bk, b) in &f.cut_ups {
            for (i, u) in b.iter().enumerate() {
                let n = BRICK as usize;
                let s = [i % n, (i / n) % n, i / (n * n)];
                let k = [0, 1, 2].map(|a| bk[a] * BRICK + s[a] as i32);
                let k2 = [k[0], !k[1], !k[2]];
                assert_eq!(u.to_bits(), g.down_at(k2).to_bits(), "cell {k:?} -> {k2:?}");
                seen += usize::from(u.is_finite());
            }
        }
        assert!(seen > 0, "the fixture recorded entries");
        assert!(!g.bounded_cuts, "a flipped bake keeps the floor law: the up-march reads cut_downs (#780)");
        let back = g.turned(&flip).0;
        assert!(same(&f.cut_ups, &back.cut_ups) && back.cut_downs.is_empty());
        assert!(same(&f.cuts, &back.cuts));
    }

    #[test]
    fn a_bake_marks_every_cut_brick_dirty() {
        let mut f = cut_box();
        let _ = f.take_dirty_bricks();
        let r = turn([[-1.0, 0.0, 0.0], [0.0, -1.0, 0.0], [0.0, 0.0, 1.0]], [4.0, 4.0, 0.0]);
        let (g, _) = f.turned(&r);
        assert!(!g.bounded_cuts, "an upright bake keeps the monotone law");
        for k in g.cuts.keys().chain(g.cut_ups.keys()) {
            assert!(g.dirty_bricks().contains(k), "brick {k:?} moved and must be re-sent");
        }
    }
}
