//! How a 2D pocket is filled: the strategy choice and what Auto resolves
//! to (split from `pocket.rs` for the 400-line law, #1446; structure
//! only). Provenance: as `pocket.rs` - zigzag, ring and adaptive
//! pocketing are textbook CAM strategies (machine-tool handbooks,
//! published CAM literature); the Auto law is ours.

use crate::geo2d::Point2;

use super::ring::ring_is_circular;

/// How a pocket is filled: straight rows back and forth, or closed rings
/// marching inward.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PocketStrategy {
    Zigzag,
    Waterline,
    /// HSM adaptive clearing (M11.4 C, `adaptive`): a spiral out of
    /// the entry bore, then engagement fronts against the in-process
    /// stock with smooth stay-down links and trochoidal arcs in narrow
    /// channels, every cut measured to hold the `optimal_load` (ae/D,
    /// `crate::load`). The 3-D ladder plans every closed level area
    /// with the same planner (`machining3d::hsm`, M11.4's per-level
    /// port) and falls back to [`PocketStrategy::OffsetRings`] by name
    /// where the planner refuses a level.
    Adaptive,
    /// The pre-M11.4 C adaptive (M2.1c's law, planar since M5.1 H):
    /// the Waterline ring tiling spaced at `optimal_load` x D instead
    /// of a free stepover - concentric rings, kept as an explicit
    /// choice so every program it posted stays byte-identical.
    OffsetRings,
    /// Waterline where the pocket's outer boundary is round enough for
    /// rings to be the honest shape, Zigzag otherwise. The resolution is
    /// [`PocketStrategy::resolve`] - the panel quotes it verbatim, so an
    /// Auto pocket never hides which path it is cutting.
    Auto,
}

impl PocketStrategy {
    /// Whether the passes step by the optimal load (ae/D) rather than by
    /// the stated stepover. One answer for the tiling step, the parameter
    /// doors and the Passes page, which shows only the field that spaces
    /// the passes (#1512): Adaptive and Offset rings read the load and
    /// never the stepover; Zigzag, Waterline and Auto the other way round.
    pub fn steps_by_load(self) -> bool {
        matches!(self, PocketStrategy::Adaptive | PocketStrategy::OffsetRings)
    }

    /// What Auto picks for a cutter-center ring: fitted arcs must carry
    /// at least 90% of the boundary's length AND agree on a center
    /// within a quarter stepover - a family of true circles, not corners
    /// that happen to round - and the stepover must still be inside
    /// waterline's reach (rings a whole stepover apart leave a band no
    /// ring's edge reaches when the step exceeds one radius; zigzag
    /// clears the same floor honestly). Any other strategy answers as
    /// itself.
    pub fn resolve(self, outer_ring: &[Point2], stepover: f64, tool_diameter: f64, tol: f64) -> Self {
        match self {
            PocketStrategy::Auto
                if ring_is_circular(outer_ring, tol, stepover)
                    && stepover <= tool_diameter * 0.5 =>
            {
                PocketStrategy::Waterline
            }
            PocketStrategy::Auto => PocketStrategy::Zigzag,
            s => s,
        }
    }
}
