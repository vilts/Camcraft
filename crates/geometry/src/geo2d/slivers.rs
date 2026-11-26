//! Sliver removal (#2766): a boolean of two regions that share a wall
//! can leave loops of near-zero area where the two tessellations disagree
//! by a grid unit. Such a loop bounds nothing a cutter can reach, and a
//! clockwise one that stands outside every boundary is a hole with no
//! parent. Provenance: our own clean-up rule over the `clipper2-rust`
//! boolean output; no vendor material consulted.

use super::Region;

impl Region {
    /// This region without the loops whose absolute area is below
    /// `min_area` (mm^2). The other loops are kept as they are: same
    /// points, same order, same orientation. A `min_area` that is not a
    /// positive finite number removes nothing.
    #[must_use]
    pub fn without_slivers(&self, min_area: f64) -> Region {
        if !(min_area.is_finite() && min_area > 0.0) {
            return self.clone();
        }
        Region { loops: self.loops.iter().filter(|l| l.area().abs() >= min_area).cloned().collect() }
    }
}
