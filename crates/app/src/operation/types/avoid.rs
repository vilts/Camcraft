//! "Do not touch" protected faces/edges data model & structural rules (#2055).
//! Provenance: our own domain rules extending `AreaControls`; no vendor source consulted.

use std::fmt::Write as _;

use super::area::{AREA_DISTANCE_MAX, AreaControls, BoundaryMode, BoundaryShape};
use super::sel::{FeatureSel, GeoSel, OpType, SrcRef};

/// Operation protection configuration: selected faces/edges/patches and standoff.
#[derive(Clone, Debug, PartialEq)]
pub struct DoNotTouch {
    /// Additional clearance (mm, >= 0) required outside protected items (`stock_to_leave` still adds).
    pub clearance: f64,
    /// Protected face, patch, or edge-chain selections.
    pub picks: Vec<GeoSel>,
}

impl Default for DoNotTouch {
    fn default() -> Self {
        Self { clearance: 0.0, picks: Vec::new() }
    }
}

impl DoNotTouch {
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.clearance == 0.0 && self.picks.is_empty()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.picks.is_empty()
    }

    #[must_use]
    pub fn is_sane(&self) -> bool {
        self.clearance.is_finite()
            && self.clearance >= 0.0
            && self.clearance <= AREA_DISTANCE_MAX
            && self.picks.iter().all(|g| {
                !g.entity.is_empty()
                    && !matches!(
                        g.feature,
                        FeatureSel::Vertex(_) | FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_)
                    )
            })
    }
}

/// Operations that honor "Do not touch" across generation and
/// verification; 2D Pocket (#3345), 2D Adaptive with both methods (#3346),
/// 3D Finish (#3347), and Engrave (#3349).
#[must_use]
pub fn do_not_touch_applies(kind: OpType) -> bool {
    matches!(
        kind,
        OpType::Pocket
            | OpType::Offset2D
            | OpType::Adaptive2D
            | OpType::Pocket3D
            | OpType::Trace
            | OpType::Finish3D
            | OpType::Face
            | OpType::Engrave
    )
}

/// User-facing refusal when "Do not touch" is set on an unsupported operation kind.
#[must_use]
pub fn do_not_touch_unsupported(kind: OpType) -> Option<String> {
    if do_not_touch_applies(kind) {
        None
    } else {
        Some(format!(
            "'Do not touch' is supported on 2D Pocket, Trace, 2D Offset, 2D Adaptive, 3D Pocket, Face, 3D Finish and Engrave operations - clear it on {kind}"
        ))
    }
}

/// Format one `GeoSel` for refusal messages and UI diagnostics.
#[must_use]
pub fn describe_sel(g: &GeoSel) -> String {
    let mut s = String::new();
    let kind = match g.feature {
        FeatureSel::Face(_) => "face",
        FeatureSel::Patch(_) => "patch",
        FeatureSel::Chain(_) => "edge chain",
        FeatureSel::Vertex(_) => "vertex",
        FeatureSel::SketchChain(_) => "sketch chain",
        FeatureSel::SketchLayer(_) => "sketch layer",
    };
    let _ = write!(s, "{} {kind} #{:x}", g.entity, g.feature.id());
    s
}

/// Refuse direct overlap between protected picks and the machining selection or Include boundaries.
#[must_use]
pub fn do_not_touch_overlap_issue(
    picks: &[GeoSel],
    src: Option<&SrcRef>,
    area: &AreaControls,
) -> Option<String> {
    for g in picks {
        if let Some(SrcRef::Selection(sel)) = src
            && sel.iter().any(|s| s == g)
        {
            return Some(format!(
                "'Do not touch' item {} is also in the operation's machining selection - remove it from one list",
                describe_sel(g)
            ));
        }
        for eb in &area.extra {
            if eb.mode == BoundaryMode::Include
                && let BoundaryShape::Picks(bps) = &eb.shape
                && bps.iter().any(|s| s == g)
            {
                return Some(format!(
                    "'Do not touch' item {} is also in an Include machining boundary - remove it from one list",
                    describe_sel(g)
                ));
            }
        }
    }
    None
}
