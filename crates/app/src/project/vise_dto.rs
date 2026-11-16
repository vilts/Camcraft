//! A built-in vise's parameters as files hold them (#788): an optional
//! `vise` key on the fixture marker. Absent - every imported fixture and
//! every older file - reads as plain steel, and an older build ignores
//! the key and keeps the saved facets, so the file stays readable both
//! ways. Lengths are millimetres like every length in the file.
//!
//! A self-centering vise (#787) adds an optional `self_centering` key
//! (its clamping range floor and dovetail lip). A build without it reads
//! such a vise's zero housing as a broken standard vise and keeps the
//! facets as plain steel - never wrong steel. Its grip step (#1274) is
//! two more optional keys, written only when the step is on, so a vise
//! without one saves byte for byte as before and older files read as no
//! step; the project version does not move.
//!
//! Provenance: our own wire shape over `vise::ViseParams`.

use serde::{Deserialize, Serialize};

use crate::vise::{SelfCentering, ViseParams, ViseStyle};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ViseDto {
    pub size: String,
    pub jaw_width: f64,
    pub jaw_height: f64,
    pub jaw_thickness: f64,
    /// Absent in files saved before it existed: read as
    /// `table::BODY_PER_PLATE` jaw thicknesses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jaw_body_depth: Option<f64>,
    pub max_opening: f64,
    pub length: f64,
    pub width: f64,
    pub base_height: f64,
    pub housing_length: f64,
    pub housing_height: f64,
    pub opening: f64,
    /// Present on a self-centering vise (#787) only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_centering: Option<SelfCenteringDto>,
}

/// A self-centering vise's own figures, millimetres.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct SelfCenteringDto {
    pub min_opening: f64,
    #[serde(default)]
    pub lip_height: f64,
    #[serde(default)]
    pub lip_depth: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub grip_depth: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub grip_width: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl ViseDto {
    #[must_use]
    pub fn from_params(p: &ViseParams) -> Self {
        Self {
            size: p.size.clone(),
            jaw_width: p.jaw_width,
            jaw_height: p.jaw_height,
            jaw_thickness: p.jaw_thickness,
            jaw_body_depth: Some(p.jaw_body_depth),
            max_opening: p.max_opening,
            length: p.length,
            width: p.width,
            base_height: p.base_height,
            housing_length: p.housing_length,
            housing_height: p.housing_height,
            opening: p.opening,
            self_centering: match &p.style {
                ViseStyle::Standard => None,
                ViseStyle::SelfCentering(sc) => Some(SelfCenteringDto {
                    min_opening: sc.min_opening,
                    lip_height: sc.lip_height,
                    lip_depth: sc.lip_depth,
                    grip_depth: sc.grip_depth,
                    grip_width: sc.grip_width,
                }),
            },
        }
    }

    /// The parameters as read, unchecked: the load (`fixture_keys`)
    /// holds them to the vise law, so a file whose vise key was
    /// hand-edited into nonsense keeps its facets as plain steel (noted),
    /// never a vise that would regenerate wrongly.
    #[must_use]
    pub fn to_params(&self) -> ViseParams {
        ViseParams {
            size: self.size.clone(),
            jaw_width: self.jaw_width,
            jaw_height: self.jaw_height,
            jaw_thickness: self.jaw_thickness,
            jaw_body_depth: self.jaw_body_depth.unwrap_or(crate::vise::table::BODY_PER_PLATE * self.jaw_thickness),
            max_opening: self.max_opening,
            length: self.length,
            width: self.width,
            base_height: self.base_height,
            housing_length: self.housing_length,
            housing_height: self.housing_height,
            opening: self.opening,
            style: self.self_centering.map_or(ViseStyle::Standard, |d| {
                ViseStyle::SelfCentering(SelfCentering {
                    min_opening: d.min_opening,
                    lip_height: d.lip_height,
                    lip_depth: d.lip_depth,
                    grip_depth: d.grip_depth,
                    grip_width: d.grip_width,
                })
            }),
        }
    }
}
