//! A field carried between setups (#637 steps A, C): the SAME field - its
//! shell, its closed-form box (#633), its cut layers - viewed through a
//! setup change's rigid turn, never re-sampled. A signed distance field
//! is invariant under rigid motion, so the distance from `p` to the
//! carried solid is the distance from `T^-1 p` to the original: exact,
//! the field analogue of `carry`'s "re-index, never re-sample".
//!
//! Phase 1 keeps `carry`'s scope: the eight lattice turns (`LatticeTurn`,
//! Z kept on +-Z), so a vertical column in the new frame is a vertical
//! column (perhaps reversed) in the field's own - the verdict march
//! stays exact. A side-on quarter turn is refused on this arm by
//! `FIELD_SIDE_CARRY_REFUSAL` (#874), every turn off the quarter
//! upstream by `CARRY_TURN_REFUSAL`.
//!
//! Provenance: rigid-motion invariance of the Euclidean distance (any
//! geometry text); our own design after #637's research comment.

use super::carry::LatticeTurn;
use super::field::SdfField;
use crate::geometry::Point3;

/// A field seen through a setup change's turn, optionally clipped to the
/// next setup's own stock block (#637 step C).
#[derive(Clone, Debug)]
pub struct CarriedField {
    pub field: SdfField,
    pub turn: LatticeTurn,
    /// The later setup's block in the NEW frame: the carried solid is
    /// intersected with it - `max(d_body, d_block)`, the field's form of
    /// the column body's `lower` clip. `None`: unclipped.
    pub clip: Option<Clip>,
    /// Has any turn of the carry chain put the part upside down? Sticky,
    /// as `Body::carry`'s `floor_judged || flips` is: two flips compose
    /// into an upright turn, and a flip baked into the bricks
    /// (`simulate`, #637 E2c) leaves only a translation as the view, yet
    /// the floor has faced the spindle either way.
    flipped: bool,
}

/// A clip region: a block, or nothing at all once two clips miss each
/// other (review of #651: an empty intersection stays empty through every
/// later turn - it is never re-sorted back into a box).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Clip {
    Block(Point3, Point3),
    Empty,
}

/// Why a clip block was refused at `clipped`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipRefusal {
    /// A corner coordinate is NaN or infinite.
    NonFinite,
    /// `lo` exceeds `hi` on some axis.
    Inverted,
}

/// The exact signed distance to the box `[lo, hi]`: negative inside.
#[must_use]
pub fn box_signed(lo: Point3, hi: Point3, p: Point3) -> f64 {
    let (l, h, q) = ([lo.x, lo.y, lo.z], [hi.x, hi.y, hi.z], [p.x, p.y, p.z]);
    // `f64::max` drops a NaN; a NaN point must stay NaN (review of #651),
    // never read as "inside".
    if q.iter().chain(&l).chain(&h).any(|v| v.is_nan()) {
        return f64::NAN;
    }
    let mut out2 = 0.0f64;
    let mut inner = f64::NEG_INFINITY;
    for a in 0..3 {
        let d = (l[a] - q[a]).max(q[a] - h[a]); // > 0 outside along this axis
        if d > 0.0 {
            out2 += d * d;
        }
        inner = inner.max(d);
    }
    if out2 > 0.0 { out2.sqrt() } else { inner.min(0.0) }
}

impl CarriedField {
    /// Carry `field` through `turn` (no data moves: the turn is the view).
    #[must_use]
    pub fn new(field: SdfField, turn: LatticeTurn) -> Self {
        CarriedField { field, turn, clip: None, flipped: turn.flips() }
    }

    /// Clip to the next setup's block (new frame). A second clip
    /// intersects with the first's block, never widens it; two blocks
    /// that miss each other leave `Clip::Empty`. A non-finite or inverted
    /// block is refused by name.
    pub fn clipped(mut self, lo: Point3, hi: Point3) -> Result<CarriedField, ClipRefusal> {
        let c = [lo.x, lo.y, lo.z, hi.x, hi.y, hi.z];
        if c.iter().any(|v| !v.is_finite()) {
            return Err(ClipRefusal::NonFinite);
        }
        if lo.x > hi.x || lo.y > hi.y || lo.z > hi.z {
            return Err(ClipRefusal::Inverted);
        }
        self.clip = Some(match self.clip {
            None => Clip::Block(lo, hi),
            Some(Clip::Empty) => Clip::Empty,
            Some(Clip::Block(a, b)) => {
                let (l, h) = (
                    Point3::new(a.x.max(lo.x), a.y.max(lo.y), a.z.max(lo.z)),
                    Point3::new(b.x.min(hi.x), b.y.min(hi.y), b.z.min(hi.z)),
                );
                if l.x > h.x || l.y > h.y || l.z > h.z { Clip::Empty } else { Clip::Block(l, h) }
            }
        });
        Ok(self)
    }

    /// Carry again: the turns compose (`next` after this one).
    #[must_use]
    pub fn then(self, next: LatticeTurn) -> CarriedField {
        // A clip stated in the old frame is the block it was; carrying the
        // body again carries the clip through the same turn.
        // Only a non-empty block has corners to re-sort; `Empty` stays
        // empty.
        let clip = self.clip.map(|c| match c {
            Clip::Empty => Clip::Empty,
            Clip::Block(lo, hi) => {
                let (a, b) = (next.apply(lo), next.apply(hi));
                Clip::Block(
                    Point3::new(a.x.min(b.x), a.y.min(b.y), a.z.min(b.z)),
                    Point3::new(a.x.max(b.x), a.y.max(b.y), a.z.max(b.z)),
                )
            }
        });
        CarriedField { field: self.field, turn: self.turn.then(next), clip, flipped: self.flipped || next.flips() }
    }

    /// Has any turn of the chain flipped the part (see the field)?
    #[must_use]
    pub fn flipped(&self) -> bool {
        self.flipped
    }

    /// The carried solid's signed distance at a point of the new frame -
    /// intersected with the clip block when one is set.
    #[must_use]
    pub fn distance(&self, p: Point3) -> f64 {
        let d = self.field.distance(self.turn.unapply(p));
        let b = match self.clip {
            None => return d,
            Some(Clip::Empty) => f64::INFINITY,
            Some(Clip::Block(lo, hi)) => box_signed(lo, hi, p),
        };
        // NaN in either term stays NaN (`f64::max` would drop it).
        if d.is_nan() || b.is_nan() { f64::NAN } else { d.max(b) }
    }

    /// The uncut stock's signed distance (the mesh term) in the new frame.
    #[must_use]
    pub fn mesh_distance(&self, p: Point3) -> f64 {
        self.field.mesh_distance(self.turn.unapply(p))
    }

    /// Membership in the new frame by the UNCUT shell's parity, AND
    /// inside the clip block when one is set. Inside a cut this disagrees
    /// with `distance` (which includes the cuts): the carried verdict
    /// (#637 step D) reads `distance`, never this.
    #[must_use]
    pub fn inside(&self, p: Point3) -> bool {
        let in_clip = match self.clip {
            None => true,
            Some(Clip::Empty) => false,
            Some(Clip::Block(lo, hi)) => box_signed(lo, hi, p) < 0.0,
        };
        in_clip && self.field.inside(self.turn.unapply(p))
    }
}
