//! The draft every new operation is seeded from (#511, moved out of the
//! MCP tool surface so the GUI and the agent share one seed): a facing
//! pass in references (25 above the stock top, skimming the top), carried
//! into the kind through the registry's retype law.
//!
//! Provenance: our own defaults over this repo's operation registry.

use super::{retype_params, Height, HeightReference, OpParams};

/// The seed params of operation kind `kind` (an index into `KindInfo::ALL`).
#[must_use]
pub fn seed_params(kind: usize) -> OpParams {
    let base = OpParams::Face {
        holes: Default::default(),
        tilt: None,
        stepover: 3.0,
        clearance_z: Height::of(HeightReference::TopOfStock, 25.0),
        cut_z: Height::of(HeightReference::TopOfStock, 0.0),
        top_z: None,
        stepdown: None,
        feed: 1200.0,
        plunge_feed: 300.0,
        climb: true,
        blend_turns: false, blend_radius: None,
        picks: Default::default(),
    };
    retype_params(&base, kind)
}
