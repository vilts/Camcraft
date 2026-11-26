//! M2.5 F: the offset shell - what makes the 3-D pocket's rings ride
//! the model's normals instead of the level grid.
//!
//! A level's ring loop walks the same XY region the plain ladder plans
//! (the islands' erosion - the dive superset law and every entry arm
//! read that region, unchanged); what the shell changes is the Z each
//! cut VERTEX lands on. For vertex `q` of a level at plane `z`:
//!
//! ```text
//! tip(q) = min(z, roof(q, r + ℓ) + leave)   floored at  max(z - stepdown, bottom_z)
//! ```
//!
//! The floor is the level's own stepdown budget (the same axial bite a
//! plain ladder's dive already takes) capped by the pocket's drawn
//! floor: the shell's early descent stops where the program stops.
//! Raising a tip can never uncover part material, so the floor rides
//! on the safe side of the clamp.
//!
//! `roof(q, rad)` is the model's highest facet top whose XY box reaches
//! within `rad` of `q` - a conservative stand-in for the surface `T`
//! over the whole decision disc. A part point p under the cutter at q
//! has T(p) ≤ roof, and part material ABOVE the plane never sits
//! within `r + leave` of a center-region point at all (that is the
//! islands' erosion, plain as ever), so `tip(q) ≥ T(p)` on every
//! below-plane point of the disc: a vertex bites part material nowhere.
//!
//! ## Why the chords are capped (the rising-leg gouge)
//!
//! A leg's Z interpolates linearly between its endpoints' tips, so a
//! point m(t) mid-leg is safe iff BOTH endpoints' decision discs reach
//! every part point under the leg near them. If a vertex's tip were
//! computed from the leg it merely ENDS (the endpoint formulation this
//! module first carried), the other side betrays it: the PREVIOUS leg
//! can be long while the vertex's own disc is short. Concrete gouge -
//! a vertical dive lands beside a step whose top T sits strictly
//! between `plane - stepdown` and the plane (the dive's disc, radius
//! r, never sees the step; its tip clamps to the floor). The next leg
//! is a long ring chord running past the step: at the crossing, the
//! leg's interpolated Z rises from the dive's floor tip toward the far
//! endpoint's, and while the disc still covers the step's column the
//! leg rides BELOW T - the sim (the column arm's `stock_sim::Stock::cut_chord`, which
//! removes everything above the chord's lowest reached Z at each
//! touched cell) drops the step's column below the model surface. A
//! gouge.
//!
//! Capping every XY leg at ℓ and taking each vertex's corridor to
//! `r + ℓ` closes it: a part point within r of a chord point is within
//! `r + ℓ` of BOTH that chord's endpoints, both tips bound its T, and
//! no interpolation between two bounds can fall under it. The cap
//! costs vertices, not depth: within one planar facet the tips along a
//! leg are equal anyway (the roof is constant there), so subdivision
//! adds only collinear, co-planar points - the same fidelity class as
//! every chord law in this codebase, bounded by the mesh's own
//! tessellation scale. This ladder takes ℓ = the tool radius, so a
//! vertex's decision disc is one tool diameter wide (see
//! `machining3d::ladder`).
//!
//! Where the surface is steep the roof outgrows the plane and the tip
//! stays AT it - the waterline the plain ladder always cut there;
//! where the surface is shallow the tip follows it at the leave
//! standoff, and the staircase's pyramid between plane and surface is
//! gone from the single pass.
//!
//! The roof is conservative on both sides of every leg: a facet counts
//! when its XY bounding box touches the corridor, and contributes its
//! global maximum z. A coarse mesh may then name a high vertex no
//! point of the tube reaches, and the tip rides up toward the plane -
//! the plain ladder's own cut, never deeper, never a gouge. The
//! verdict of what any of it leaves standing is the M2.3 simulation's,
//! here as everywhere.
//!
//! Provenance: our own design, specced from published CAM offset-shell
//! roughing behavior, per the Clean-Room Protocol.

use camcraft_sim::verify::{AVOID_PAD, AvoidSpec};

use crate::geo2d::Point2;
use crate::geometry::Triangle;

/// One facet's XY bounding box and maximum height: the entire memory
/// of the model this field needs.
struct TriBox {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    zmax: f64,
}

struct AvoidRoofBox {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
    /// Protected tip height floor (`item_zmax + req_clear`).
    tip_z: f64,
}

pub(super) struct Field {
    tris: Vec<TriBox>,
    avoid: Vec<AvoidRoofBox>,
}

impl Field {
    /// Box the model without protected-item roofs.
    pub(super) fn new(facets: &[Triangle]) -> Result<Self, String> {
        Self::new_with_avoid(facets, None)
    }

    /// Box the model. A non-finite vertex is a corrupt model, said by
    /// name, exactly as the waterline scan says it.
    pub(super) fn new_with_avoid(facets: &[Triangle], avoid_spec: Option<&AvoidSpec>) -> Result<Self, String> {
        let mut tris = Vec::with_capacity(facets.len());
        for f in facets {
            if !(f.a.is_finite() && f.b.is_finite() && f.c.is_finite()) {
                return Err("model has a non-finite facet".into());
            }
            tris.push(TriBox {
                x0: f.a.x.min(f.b.x).min(f.c.x),
                x1: f.a.x.max(f.b.x).max(f.c.x),
                y0: f.a.y.min(f.b.y).min(f.c.y),
                y1: f.a.y.max(f.b.y).max(f.c.y),
                zmax: f.a.z.max(f.b.z).max(f.c.z),
            });
        }
        let mut avoid = Vec::new();
        if let Some(av) = avoid_spec {
            let pad = av.clearance.max(0.0) + AVOID_PAD;
            for item in &av.items {
                for f in &item.tris {
                    avoid.push(AvoidRoofBox {
                        x0: f.a.x.min(f.b.x).min(f.c.x),
                        x1: f.a.x.max(f.b.x).max(f.c.x),
                        y0: f.a.y.min(f.b.y).min(f.c.y),
                        y1: f.a.y.max(f.b.y).max(f.c.y),
                        tip_z: f.a.z.max(f.b.z).max(f.c.z) + pad,
                    });
                }
                for &(a, b) in &item.edges {
                    avoid.push(AvoidRoofBox {
                        x0: a[0].min(b[0]),
                        x1: a[0].max(b[0]),
                        y0: a[1].min(b[1]),
                        y1: a[1].max(b[1]),
                        tip_z: a[2].max(b[2]) + pad,
                    });
                }
            }
        }
        Ok(Self { tris, avoid })
    }

    /// The conservative roof: the highest facet zmax whose XY box
    /// reaches within `rad` of `q`, or None when the model is nowhere
    /// near the center (pure air: the plane's own cut is the law).
    fn roof(&self, q: Point2, rad: f64) -> Option<f64> {
        let r2 = rad * rad;
        let mut top: Option<f64> = None;
        for t in &self.tris {
            let dx = (q.x - t.x1).max(t.x0 - q.x).max(0.0);
            let dy = (q.y - t.y1).max(t.y0 - q.y).max(0.0);
            if dx * dx + dy * dy <= r2 {
                top = Some(top.map_or(t.zmax, |z| z.max(t.zmax)));
            }
        }
        top
    }

    fn avoid_roof(&self, q: Point2, rad: f64) -> Option<f64> {
        let r2 = rad * rad;
        let mut top: Option<f64> = None;
        for t in &self.avoid {
            let dx = (q.x - t.x1).max(t.x0 - q.x).max(0.0);
            let dy = (q.y - t.y1).max(t.y0 - q.y).max(0.0);
            if dx * dx + dy * dy <= r2 {
                top = Some(top.map_or(t.tip_z, |z| z.max(t.tip_z)));
            }
        }
        top
    }

    /// The shell tip of the cut VERTEX `q` of a level at plane `z`:
    /// never above the plane, never below the level's own stepdown
    /// budget (`floor_z` - the same axial bite a plain ladder's dive
    /// already takes), and never below what the decision disc's model
    /// roof plus the leave demands. `rad` is the corridor this ladder
    /// promised this vertex: `r + ℓ`, widened only for a sweep wider
    /// than any chord reaching it (a helix orbit, by its radius).
    pub(super) fn tip(&self, q: Point2, z: f64, rad: f64, leave: f64, floor_z: f64) -> f64 {
        let part = self.roof(q, rad).map(|roof| roof + leave);
        let avoid = self.avoid_roof(q, rad);
        match (part, avoid) {
            (None, None) => z,
            (Some(p), None) => z.min(p).max(floor_z),
            (None, Some(a)) => z.min(a).max(floor_z),
            (Some(p), Some(a)) => z.min(p.max(a)).max(floor_z),
        }
    }
}
