//! The file mirror of an operation's area controls (M11.5): one optional
//! `area` key on the operation, absent at the identity - so every file
//! saved before M11.5, and every save of an untouched operation, is the
//! same bytes; no version bump (the v22 `axis` precedent). Every key
//! inside defaults too, so a file names only what it changed.
//!
//! Provenance: our own file grammar; no vendor format consulted.

use super::*;
use crate::operation::{
    AreaControls, BoundaryMode, BoundaryShape, ChainSide, Containment, CornerStyle, DoNotTouch,
    ExtraBoundary, OpenBoundary,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ContainmentDto {
    Inside,
    On,
    Outside,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CornerDto {
    #[default]
    Round,
    Sharp,
}

impl CornerDto {
    fn is_round(&self) -> bool {
        *self == CornerDto::Round
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryModeDto {
    Include,
    Exclude,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case", tag = "from")]
pub enum BoundaryShapeDto {
    Picks { items: Vec<GeoSelDto> },
    Stock,
    Silhouette,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ExtraBoundaryDto {
    pub mode: BoundaryModeDto,
    pub shape: BoundaryShapeDto,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChainSideDto {
    Inside,
    Outside,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct OpenBoundaryDto {
    pub items: Vec<GeoSelDto>,
    pub side: ChainSideDto,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DoNotTouchDto {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub clearance: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<GeoSelDto>,
}

impl DoNotTouchDto {
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == DoNotTouchDto::default()
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct AreaDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub containment: Option<ContainmentDto>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub offset: f64,
    /// #3338: absent means "the same as `offset`" - a file saved before
    /// the outer offset existed moved both lines by its one value, and a
    /// save of equal values keeps those bytes (no version bump).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_offset: Option<f64>,
    #[serde(default, skip_serializing_if = "CornerDto::is_round")]
    pub corner: CornerDto,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub extension: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<ExtraBoundaryDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open: Option<OpenBoundaryDto>,
    #[serde(default, skip_serializing_if = "DoNotTouchDto::is_default")]
    pub do_not_touch: DoNotTouchDto,
}

pub(super) fn geo_sel_dto(g: &GeoSel) -> GeoSelDto {
    GeoSelDto {
        entity: g.entity.clone(),
        feature: match &g.feature {
            FeatureSel::Face(id) => FeatureSelDto::Face { id: *id },
            FeatureSel::Chain(id) => FeatureSelDto::Chain { id: *id },
            FeatureSel::Patch(id) => FeatureSelDto::Patch { id: *id },
            FeatureSel::Vertex(id) => FeatureSelDto::Vertex { id: *id },
            FeatureSel::SketchChain(id) => FeatureSelDto::SketchChain { id: ChainIdDto(*id) },
            FeatureSel::SketchLayer(layer) => FeatureSelDto::SketchLayer { layer: layer.clone() },
        },
    }
}

pub(super) fn geo_sel_of(g: &GeoSelDto) -> GeoSel {
    GeoSel {
        entity: g.entity.clone(),
        feature: match &g.feature {
            FeatureSelDto::Face { id } => FeatureSel::Face(*id),
            FeatureSelDto::Chain { id } => FeatureSel::Chain(*id),
            FeatureSelDto::Patch { id } => FeatureSel::Patch(*id),
            FeatureSelDto::Vertex { id } => FeatureSel::Vertex(*id),
            FeatureSelDto::SketchChain { id } => FeatureSel::SketchChain(id.0),
            FeatureSelDto::SketchLayer { layer } => FeatureSel::SketchLayer(layer.clone()),
        },
    }
}

pub(super) fn chain_side_dto(s: ChainSide) -> ChainSideDto {
    match s {
        ChainSide::Inside => ChainSideDto::Inside,
        ChainSide::Outside => ChainSideDto::Outside,
    }
}

pub(super) fn chain_side_of(s: ChainSideDto) -> ChainSide {
    match s {
        ChainSideDto::Inside => ChainSide::Inside,
        ChainSideDto::Outside => ChainSide::Outside,
    }
}

pub(super) fn sel_dto(items: &[GeoSel]) -> Vec<GeoSelDto> {
    items.iter().map(geo_sel_dto).collect()
}

pub(super) fn sel_of(items: &[GeoSelDto]) -> Vec<GeoSel> {
    items.iter().map(geo_sel_of).collect()
}

impl AreaDto {
    /// Absent from the file exactly at the identity.
    #[must_use]
    pub fn is_default(&self) -> bool {
        *self == AreaDto::default()
    }

    #[must_use]
    pub fn from_controls(a: &AreaControls) -> Self {
        AreaDto {
            containment: a.containment.map(|c| match c {
                Containment::Inside => ContainmentDto::Inside,
                Containment::On => ContainmentDto::On,
                Containment::Outside => ContainmentDto::Outside,
            }),
            offset: a.offset,
            outer_offset: (a.outer_offset != a.offset).then_some(a.outer_offset),
            corner: match a.corner {
                CornerStyle::Round => CornerDto::Round,
                CornerStyle::Sharp => CornerDto::Sharp,
            },
            extension: a.extension,
            extra: a
                .extra
                .iter()
                .map(|b| ExtraBoundaryDto {
                    mode: match b.mode {
                        BoundaryMode::Include => BoundaryModeDto::Include,
                        BoundaryMode::Exclude => BoundaryModeDto::Exclude,
                    },
                    shape: match &b.shape {
                        BoundaryShape::Picks(p) => BoundaryShapeDto::Picks { items: sel_dto(p) },
                        BoundaryShape::Stock => BoundaryShapeDto::Stock,
                        BoundaryShape::Silhouette => BoundaryShapeDto::Silhouette,
                    },
                })
                .collect(),
            open: a.open.as_ref().map(|o| OpenBoundaryDto {
                items: sel_dto(&o.picks),
                side: match o.side {
                    ChainSide::Inside => ChainSideDto::Inside,
                    ChainSide::Outside => ChainSideDto::Outside,
                },
            }),
            do_not_touch: DoNotTouchDto {
                clearance: a.do_not_touch.clearance,
                items: sel_dto(&a.do_not_touch.picks),
            },
        }
    }

    #[must_use]
    pub fn to_controls(&self) -> AreaControls {
        AreaControls {
            containment: self.containment.map(|c| match c {
                ContainmentDto::Inside => Containment::Inside,
                ContainmentDto::On => Containment::On,
                ContainmentDto::Outside => Containment::Outside,
            }),
            offset: self.offset,
            outer_offset: self.outer_offset.unwrap_or(self.offset),
            corner: match self.corner {
                CornerDto::Round => CornerStyle::Round,
                CornerDto::Sharp => CornerStyle::Sharp,
            },
            extension: self.extension,
            extra: self
                .extra
                .iter()
                .map(|b| ExtraBoundary {
                    mode: match b.mode {
                        BoundaryModeDto::Include => BoundaryMode::Include,
                        BoundaryModeDto::Exclude => BoundaryMode::Exclude,
                    },
                    shape: match &b.shape {
                        BoundaryShapeDto::Picks { items } => BoundaryShape::Picks(sel_of(items)),
                        BoundaryShapeDto::Stock => BoundaryShape::Stock,
                        BoundaryShapeDto::Silhouette => BoundaryShape::Silhouette,
                    },
                })
                .collect(),
            open: self.open.as_ref().map(|o| OpenBoundary {
                picks: sel_of(&o.items),
                side: match o.side {
                    ChainSideDto::Inside => ChainSide::Inside,
                    ChainSideDto::Outside => ChainSide::Outside,
                },
            }),
            do_not_touch: DoNotTouch {
                clearance: self.do_not_touch.clearance,
                picks: sel_of(&self.do_not_touch.items),
            },
        }
    }
}
