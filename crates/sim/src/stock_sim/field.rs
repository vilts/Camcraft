//! The signed-distance stock field behind the `stock_sim` seam (M7.1 A).
//!
//! Provenance: narrow-band sparse-field CSG and the brick (clipmap)
//! storage layout are published implicit-surface practice - the
//! multi-source BFS distance transform of Barilla et al. (2024) and the
//! brick-map discipline the Phase 7 reference note names. The sign is
//! the ray-parity law this codebase already ships in `solid_rest`,
//! reused whole. Nothing derived from vendor binaries or forensics.
//!
//! ## The laws here
//!
//! - **The mesh stays the source of truth.** This field is the
//!   preview/verdict-display stock of Phase 7: what it says about a
//!   point answers "what is left here" where a column top cannot speak
//!   (undercuts, radially-machine bodies, 5-axis sweeps), and the M2.2
//!   parity module remains the sign's only author.
//! - **Sign exact, magnitude bounded.** The sign is ray-parity along
//!   the query's OWN ray, evaluated against the shell live: no cell
//!   size, band width, or BFS slack can ever call material "air" -
//!   material is never hidden. (A per-column cache of the CENTER
//!   ray's crossings is what this module planned first; it is not
//!   exact - a wall thinner than a cell can pass between the point
//!   and its column's center and flip the answer - so stage A keeps
//!   the honest live call and leaves the ray-keyed render-side
//!   acceleration to stage C.) The band magnitude is the BFS graph
//!   distance, within about a cell of the true distance near the
//!   surface, clamped so nothing past the band is ever bigger than
//!   `cap`; `cap` sits within two cells of the band's truth edge -
//!   clearance rounds TOWARD the surface up to that slack, the
//!   direction the ROADMAP's subtraction law and the dexel header's
//!   "over-reporting rest is honest noise" choose on purpose. Stage
//!   C retired this slack's verdict half without an eikonal: the
//!   seam's ray march (`march`) reads only the sign and the step
//!   bound, so column heights come out exact where stored values
//!   under-read. The eikonal sweep now belongs to the render pass
//!   (M7.2), where smooth shading wants the gradient honest.
//! - **Sparse by brick, not octree.** 8x8x8-cell bricks of f64 under
//!   a flat hash map; only near-surface bricks exist. An allocated
//!   brick's out-of-band cell is NaN, and NaN answers the same law as
//!   an absent brick: `cap`. A second layer (M7.1 B's cuts) rides the
//!   same lattice: `subtract` folds `max` of the swept tool's negated
//!   distance into it, and a query takes the max of the two layers,
//!   so subtraction commutes and can never hide material.
//! - **Total input.** A non-finite vertex, a zero-area facet, a
//!   non-watertight shell, or a nonsense cell size refuses with the
//!   defect named, before any cell is written. The query contract
//!   carries the law through: a non-finite point answers NaN, never
//!   a panic; an `Open` parity column (unreachable past the gate)
//!   answers material.
//! - **Orientation-free.** Distance reads points, parity reads
//!   crossings; flipping every facet's winding changes nothing (the
//!   `solid_rest` law (c), inherited whole).

use std::collections::HashMap;
use std::sync::Arc;

use crate::geometry::{Aabb, Point3, Triangle};
use crate::solid_rest::{column_crossings, Crossings};

pub(crate) use super::field_shell::validate_shell;

/// Cells on a brick edge: 512 f64 per allocated brick.
pub(crate) const BRICK: i32 = 8;
pub(crate) const BRICK3: usize = (BRICK * BRICK * BRICK) as usize;

/// The seed shell radius, in cells: every surface point has a cell
/// center within half a diagonal (sqrt(3)/2 cells), so a radius of
/// sqrt(3) cells keeps the seed set dense on the shell and the BFS
/// never starts from nothing.
pub(crate) const SEED_R: f64 = 1.7320508075688772; // sqrt(3), cells

/// BFS hop cost, in cells: a six-neighbor hop steps exactly one cell,
/// and an L1 staircase costs L1/sqrt(3) <= its own straight length,
/// which is the bound that keeps the graph distance under the true
/// distance near the surface.
pub(crate) const HOP: f64 = 0.5773502691896258; // 1 / sqrt(3), cells

/// One cut layer: brick key -> 512 cell values. A brick is shared, so a
/// clone of the field copies the keys and the pointers only; a write
/// copies its brick first when a clone still holds it (#2194).
pub(crate) type CutLayer = HashMap<[i32; 3], Arc<[f64; BRICK3]>>;

/// One narrow-band signed-distance grid: a watertight mesh sampled
/// once into sparse bricks. Construct with `SdfField::from_mesh`.
#[derive(Clone, Debug)]
pub struct SdfField {
    pub(crate) cell: f64,
    /// Magnitudes beyond the band answer `sign * cap`.
    pub(crate) cap: f64,
    pub(crate) mesh: Arc<Vec<Triangle>>,
    /// Shared on clone (the mesh layer is written only while seeding),
    /// so a playback snapshot never copies this one.
    pub(crate) bricks: Arc<HashMap<[i32; 3], Box<[f64; BRICK3]>>>,
    /// M7.1 B's subtraction layer: per cell, the running max of the
    /// negated swept-tool distances; NaN cells and absent bricks say
    /// "no tool has claimed this cell". Its bricks are shared on clone
    /// and copied on write (`CutLayer`), as are the two layers below.
    pub(crate) cuts: CutLayer,
    /// M7.1 C's ray-entry companion: per claimed cell, the lowest
    /// point ABOVE the center whose proven-removed status is certain
    /// (the vertical ray's first solid crossing, a lower bound),
    /// kept as a min. NaN says "this cell's ray never meets the
    /// solid" - lateral claims like a pocket's rim, whose surface
    /// stays the mesh's own. `field_ray` owns the law.
    pub(crate) cut_ups: CutLayer,
    /// #637 E2a: `cut_ups`'s mirror - the distance DOWN from a claimed
    /// cell's centre to the ray's entry into the swept solid. Written
    /// only by a flipped bake (`field_turn`), where every "up" record
    /// becomes a "down" one; read by the up-march (`march_up`, E2b)
    /// for the floor of a column a cut from below reached.
    pub(crate) cut_downs: CutLayer,
    /// M7.3's cost law: the cut region, the full-brick ledger and the
    /// counters (`field_batch::CostLedger`), boxed so the `Stock`
    /// enum's arms stay the same size class.
    pub(crate) cost: Box<super::field_batch::CostLedger>,
    /// #633: a block stock's exact box distance - the mesh layer's
    /// magnitude wherever no brick was stored (`mag_at`), so a block
    /// builds with no band fill. `None` for every mesh-seeded field.
    pub(crate) analytic: Option<super::field_box::AnalyticBox>,
    /// #637 E1: a cut by a solid that does not span the verdict world
    /// above its tip (a truncated taper, a bare ball, a lathe sweep, a
    /// posed axis) has been subtracted. The floor read's monotone law
    /// (`field_floor`) holds only for spanning tools; once this is set
    /// a floor a cut came near is refused by name instead.
    pub(crate) bounded_cuts: bool,
}

/// World cell of a point on the origin-locked lattice.
pub(crate) fn cell_of(v: f64, cell: f64) -> i32 {
    (v / cell).floor().clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// Cell-center coordinate of lattice index `i`.
pub(crate) fn center_of(i: i32, cell: f64) -> f64 {
    (i as f64 + 0.5) * cell
}

/// The band magnitude wearing the parity sign: the shell's own point
/// (magnitude zero) reads zero whatever parity says - only a removal
/// claim moves it, the `distance` law stated once.
pub(crate) fn signed(mag: f64, inside: bool) -> f64 {
    if mag == 0.0 {
        0.0
    } else if inside {
        -mag
    } else {
        mag
    }
}

impl SdfField {
    /// The field's cell edge length.
    #[must_use]
    pub fn cell(&self) -> f64 {
        self.cell
    }

    /// The far-field magnitude every out-of-band query returns.
    #[must_use]
    pub fn cap(&self) -> f64 {
        self.cap
    }

    /// Allocated bricks: the sparsity promise is measurable.
    #[must_use]
    pub fn brick_count(&self) -> usize {
        self.bricks.len()
    }

    /// The signed distance at `p` in the STOCK AFTER CUTS: negative
    /// in material, positive in air. The mesh term keeps the exact
    /// sign (parity on p's own ray) and the band magnitude; the cut
    /// term (`field_cut`'s max law) can only lift it, so a cell the
    /// tool reached reads removed and material is never hidden.
    /// Before any `subtract` the answer is the mesh term alone: zero
    /// at the shell, band value near it, `cap` beyond.
    #[must_use]
    pub fn distance(&self, p: Point3) -> f64 {
        if !p.is_finite() {
            return f64::NAN;
        }
        let key = [cell_of(p.x, self.cell), cell_of(p.y, self.cell), cell_of(p.z, self.cell)];
        self.mesh_distance(p).max(self.cut_at(key))
    }

    /// The mesh term alone - the stock BEFORE any cut. Negative in
    /// material, positive in air, same sign law as `distance`; the
    /// cut layer never lifts it here. The verdict march (`march`)
    /// walks this to find the uncut column top, so a report can say
    /// what the program removed, not only what it left.
    #[must_use]
    pub fn mesh_distance(&self, p: Point3) -> f64 {
        if !p.is_finite() {
            return f64::NAN;
        }
        let key = [cell_of(p.x, self.cell), cell_of(p.y, self.cell), cell_of(p.z, self.cell)];
        signed(self.mag_at(key), self.inside(p))
    }

    /// The stored magnitude at a cell: out of band, in an unallocated
    /// brick, or in a brick's NaN cell, it is `cap` by one law - or,
    /// for a block stock (#633), the box's exact distance, clamped to
    /// `cap` by the same band law.
    pub(crate) fn mag_at(&self, key: [i32; 3]) -> f64 {
        let unstored = || match self.analytic {
            Some(b) => b.at_cell(key, self.cell).min(self.cap),
            None => self.cap,
        };
        match self.bricks.get(&brick_of(key)) {
            Some(b) => {
                let v = b[in_brick(key)];
                if v.is_nan() {
                    unstored()
                } else {
                    v
                }
            }
            None => unstored(),
        }
    }

    /// The cached parity column for ray `(x, y)`: the crossings a
    /// vertical query stands in, `None` for an `Open` column (whose
    /// law is material). `inside` does this scan per call; the verdict
    /// march walks a column at FIXED XY, so one scan serves every
    /// probe - the ray-keyed acceleration stage A promised to stage C.
    #[must_use]
    pub fn shell_crossings(&self, x: f64, y: f64) -> Option<Vec<f64>> {
        match column_crossings(&self.mesh, x, y) {
            Crossings::Paired(z) => Some(z),
            Crossings::Open { .. } => None,
        }
    }

    /// Parity at height `z` from a cached `shell_crossings` column.
    #[must_use]
    pub fn parity_of(crossings: &Option<Vec<f64>>, z: f64) -> bool {
        match crossings {
            None => true, // Open answers material, the `inside` law
            Some(v) => v.iter().filter(|&&c| c > z).count() % 2 == 1,
        }
    }

    /// The shell the field was seeded from - the mesh stays the
    /// source of truth, so a verdict can re-read it (part surfaces,
    /// the march's world).
    #[must_use]
    pub fn facets(&self) -> &[Triangle] {
        &self.mesh
    }

    /// The shell's bounding box: the world the verdict march runs in.
    #[must_use]
    pub fn bounds(&self) -> Aabb {
        let (mut lo, mut hi) = (
            Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for t in self.mesh.iter() {
            for v in [t.a, t.b, t.c] {
                lo = Point3::new(lo.x.min(v.x), lo.y.min(v.y), lo.z.min(v.z));
                hi = Point3::new(hi.x.max(v.x), hi.y.max(v.y), hi.z.max(v.z));
            }
        }
        Aabb { min: lo, max: hi }
    }

    /// The cut layer's value at a cell: NEGATIVE INFINITY where no
    /// tool has claimed the cell, so `max` leaves the mesh term alone.
    pub(crate) fn cut_at(&self, key: [i32; 3]) -> f64 {
        match self.cuts.get(&brick_of(key)) {
            Some(b) => {
                let v = b[in_brick(key)];
                if v.is_nan() {
                    f64::NEG_INFINITY
                } else {
                    v
                }
            }
            None => f64::NEG_INFINITY,
        }
    }

    /// Allocated cut bricks: the swept-volume footprint is measurable.
    #[must_use]
    pub fn cut_brick_count(&self) -> usize {
        self.cuts.len()
    }

    /// Ray-parity membership of `p` on p's own ray, the `solid_rest`
    /// law: an odd number of shell crossings strictly above `p.z` is
    /// inside. `Open` is unreachable past `from_mesh`'s watertightness
    /// gate; should a mesh ever be built around it, it answers
    /// material - never hidden material is the law.
    #[must_use]
    pub fn inside(&self, p: Point3) -> bool {
        Self::parity_of(&self.shell_crossings(p.x, p.y), p.z)
    }

    /// The exact unsigned distance from `p` to the shell, mesh-read:
    /// the number the band's magnitude is bounded against. Linear in
    /// the facets - a calibration and a test oracle, never a hot path.
    #[must_use]
    pub fn true_distance(&self, p: Point3) -> f64 {
        let mut best = f64::INFINITY;
        for t in self.mesh.iter() {
            best = best.min(super::gouge::dist_pt_tri(p, t));
        }
        best
    }

    /// The band cell's storage slot, allocating its brick.
    pub(crate) fn brick_mut(&mut self, key: [i32; 3]) -> &mut [f64; BRICK3] {
        Arc::make_mut(&mut self.bricks)
            .entry(brick_of(key))
            .or_insert_with(|| Box::new([f64::NAN; BRICK3]))
    }

    /// The cut layer's slot for a cell, allocating its brick. A caller
    /// that changes the value must mark the brick dirty (the only one,
    /// `claim_record`, does) - the playback upload trusts that ledger.
    pub(crate) fn cut_mut(&mut self, key: [i32; 3]) -> &mut f64 {
        &mut layer_brick_mut(&mut self.cuts, brick_of(key))[in_brick(key)]
    }

    /// Bytes this field holds that `prev` does not (#2194): each cut
    /// brick that is not `prev`'s own allocation at the same key, and
    /// the map entries, which every clone copies. Pointer identity is
    /// the test, so equal bricks in two allocations count twice: the
    /// answer is never under what the field holds beyond `prev`.
    #[must_use]
    pub fn cut_bytes_after(&self, prev: &SdfField) -> usize {
        let layers = [(&self.cuts, &prev.cuts), (&self.cut_ups, &prev.cut_ups), (&self.cut_downs, &prev.cut_downs)];
        let (mut own, mut entries) = (0usize, 0usize);
        for (now, was) in layers {
            entries = entries.saturating_add(now.len());
            own = own.saturating_add(now.iter().filter(|(k, b)| !was.get(*k).is_some_and(|p| Arc::ptr_eq(p, b))).count());
        }
        own.saturating_mul(BRICK3 * std::mem::size_of::<f64>())
            .saturating_add(entries.saturating_mul(std::mem::size_of::<([i32; 3], Arc<[f64; BRICK3]>)>()))
    }
}

/// A cut layer's brick for writing, allocated when absent and copied
/// first when a clone of the field still shares it (copy-on-write).
pub(crate) fn layer_brick_mut(layer: &mut CutLayer, brick: [i32; 3]) -> &mut [f64; BRICK3] {
    Arc::make_mut(layer.entry(brick).or_insert_with(|| Arc::new([f64::NAN; BRICK3])))
}

/// The brick a cell lives in.
pub(crate) fn brick_of(key: [i32; 3]) -> [i32; 3] {
    [
        key[0].div_euclid(BRICK),
        key[1].div_euclid(BRICK),
        key[2].div_euclid(BRICK),
    ]
}

/// The cell's slot inside its brick (x fastest, then y, then z).
pub(crate) fn in_brick(key: [i32; 3]) -> usize {
    let (x, y, z) = (
        key[0].rem_euclid(BRICK) as usize,
        key[1].rem_euclid(BRICK) as usize,
        key[2].rem_euclid(BRICK) as usize,
    );
    x + BRICK as usize * y + BRICK as usize * BRICK as usize * z
}
