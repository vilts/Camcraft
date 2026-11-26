//! The file mirror of a tool's modelled body (#368, project v58): the
//! placed facets (tool frame) and the placement they landed with. The
//! envelope is never written - it is re-derived from the facets on
//! load, so file and judgement cannot disagree, and a body the build's
//! envelope law refuses refuses the file by name. Our own format.
//!
//! A lathe body (#369) writes its turning-frame placement under
//! `lathe` and leaves `axis` / `offset` out: a reader that predates it
//! then refuses the file (a missing field) rather than reading
//! lathe-placed facets as a spindle body. Its clearance bands are
//! re-derived on load, like the envelope.
//!
//! A lathe body's insert zone (#396) is an optional `insert_zone` key
//! beside `lathe`, absent when there is none, so a zoneless body's bytes
//! are unchanged and no version rung is spent: a reader that predates
//! it skips the unknown key (serde's default) and judges the whole body,
//! plate and all - an over-report that blocks the post, never one that
//! passes a rub.
//!
//! A drawn spun holder (#93) also writes its outline under `spun`, which
//! is re-drawn on load (a spindle-frame body; never beside `lathe`).
//!
//! A tool's drawn cutter (#93 slice 2) rides the tool record as
//! `cutter`, the same `[z, r, arc]` points, re-judged against the
//! tool's numbers on load (`spun_to_dto`, `cutter_from_dto`).

//!
//! A library holder (#2016, project v70) is written as a reference on
//! the tool (`holder`: the entry's name, the gauge length, the
//! projection) and no `body`: the outline is drawn from the library on
//! load (`tool_body::build_library`), the same call that drew it when
//! it was set. An older file's drawn copy of a library holder stays
//! what it is, a drawn profile with the same shape. [`stated_body`] is
//! the one reader of the two keys.

use serde::{Deserialize, Serialize};

use super::{dto_to_facets, facets_to_dto};
use crate::schema::ToolBody;
use camcraft_tools::spun::{SpunPoint, SpunProfile};
use crate::tool_body::BodyFrame;
use camcraft_tools::lathe_holder::LathePlacement;
use camcraft_tools::lathe_insert_zone::InsertZone;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ToolBodyDto {
    pub source: String,
    /// A milling body's placement; absent on a lathe body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<[f64; 3]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<f64>,
    /// #369: a lathe body's placement; absent on a milling body.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lathe: Option<LathePlacementDto>,
    /// #396: a lathe body's insert zone (tool frame); absent when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub insert_zone: Option<InsertZoneDto>,
    /// Placed facets, tool frame (milling: tip at the origin, axis +Z;
    /// lathe: tip at the origin, +x radial, +z along the spindle).
    pub facets: Vec<[[f64; 3]; 3]>,
    /// #93, additive (no version step): the drawn outline of a spun
    /// holder profile, `[z, r, arc]` per point. When present it is the
    /// truth - the body is re-drawn from it and the facets (its revolve,
    /// written too) are not read. A reader without it judges those
    /// facets, whose ring vertices sit on the conservative outline, so
    /// its envelope is never smaller than the drawn solid - dropping the
    /// key can only over-report, never under-read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spun: Option<Vec<[f64; 3]>>,
    /// #828, additive (no version step): the holder is a machine's
    /// default, assumed for a new tool - judged no smaller than the
    /// tool's holder cylinder. Absent (every older file) = picked by the
    /// user, judged by its real shape. A reader without the key judges
    /// the drawn holder as it stands.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub machine_default: bool,
    /// #361 / #1040, additive: modular pieces (and their tool-frame
    /// coordinate systems). Omitted for single-piece bodies without
    /// coordinate systems so existing files stay byte-identical.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<BodyPieceDto>,
}

/// #2016: a tool's library holder on file - the reference, not the
/// outline.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HolderRefDto {
    /// The library entry's name, "BT30 ER20".
    pub entry: String,
    /// Gauge line to nose face, mm.
    pub gauge_len: f64,
    /// Nose face above the tool tip, mm.
    pub projection: f64,
    /// #828: the holder is a machine's default, assumed for a new tool
    /// (judged no smaller than the tool's holder cylinder). Written
    /// only when true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub machine_default: bool,
}

impl HolderRefDto {
    /// The reference a body carries, if it is a library holder.
    #[must_use]
    pub fn from_body(b: &ToolBody) -> Option<Self> {
        b.library.as_ref().map(|r| Self {
            entry: r.entry.clone(),
            gauge_len: r.gauge_len,
            projection: r.projection,
            machine_default: b.machine_default,
        })
    }

    /// The body the reference names, drawn from the library; a refusal
    /// (an unknown entry, lengths the entry cannot take) names the tool.
    pub fn to_body(&self, tool: &str) -> Result<ToolBody, String> {
        let reference = camcraft_tools::holder_library::HolderRef {
            entry: self.entry.clone(),
            gauge_len: self.gauge_len,
            projection: self.projection,
        };
        let mut body = crate::tool_body::build_library(&reference).map_err(|e| format!("tool '{tool}': {e}"))?;
        body.machine_default = self.machine_default;
        Ok(body)
    }
}

/// What a tool's `holder` and `body` keys say, split from `body` for
/// writing: a library holder goes out as its reference alone.
#[must_use]
pub fn body_to_dto(b: &ToolBody) -> (Option<HolderRefDto>, Option<ToolBodyDto>) {
    match HolderRefDto::from_body(b) {
        Some(h) => (Some(h), None),
        None => (None, Some(ToolBodyDto::from_body(b))),
    }
}

/// The body a tool record states through `holder` (a library
/// reference) or `body` (a custom one): the one reader of both, for the
/// project loader and the tool library file. Both keys at once is no
/// tool - the file refuses by name rather than choose one.
pub fn stated_body(holder: Option<&HolderRefDto>, body: Option<&ToolBodyDto>, tool: &str) -> Result<Option<ToolBody>, String> {
    match (holder, body) {
        (Some(_), Some(_)) => Err(format!(
            "tool '{tool}': it states both a library holder (\"holder\") and a body (\"body\") - a tool has one"
        )),
        (Some(h), None) => h.to_body(tool).map(Some),
        (None, Some(b)) => b.to_body(tool).map(Some),
        (None, None) => Ok(None),
    }
}

/// One named coordinate system on file (#361 / #1040), tool frame.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BodyCsysDto {
    pub name: String,
    pub origin: [f64; 3],
    pub z: [f64; 3],
    pub x: [f64; 3],
}

/// One modular piece on file (#361 / #1040), facets already in tool frame.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BodyPieceDto {
    pub source: String,
    pub facets: Vec<[[f64; 3]; 3]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub csys: Vec<BodyCsysDto>,
}

/// The turning-frame placement on file (#369).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LathePlacementDto {
    pub radial: [f64; 3],
    pub along: [f64; 3],
    pub tip: [f64; 3],
}

/// The insert zone on file (#396): the box's corners, tool frame.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct InsertZoneDto {
    pub lo: [f64; 3],
    pub hi: [f64; 3],
}

impl ToolBodyDto {
    #[must_use]
    pub fn from_body(b: &ToolBody) -> Self {
        let facets = facets_to_dto(&b.facets);
        let spun = b.spun.as_ref().map(|s| s.points().iter().map(|p| [p.z, p.r, p.arc]).collect());
        let write_pieces = b.pieces.len() > 1 || b.pieces.iter().any(|p| !p.csys.is_empty());
        let pieces = if write_pieces {
            b.pieces
                .iter()
                .map(|p| BodyPieceDto {
                    source: p.source.clone(),
                    facets: facets_to_dto(&p.facets),
                    csys: p
                        .csys
                        .iter()
                        .map(|c| BodyCsysDto { name: c.name.clone(), origin: c.origin, z: c.z, x: c.x })
                        .collect(),
                })
                .collect()
        } else {
            Vec::new()
        };
        match &b.frame {
            BodyFrame::Spindle { axis, offset, .. } => Self {
                source: b.source.clone(),
                axis: Some(*axis),
                offset: Some(*offset),
                lathe: None,
                insert_zone: None,
                facets,
                spun,
                machine_default: b.machine_default,
                pieces,
            },
            BodyFrame::Lathe { place, zone, .. } => Self {
                source: b.source.clone(),
                axis: None,
                offset: None,
                lathe: Some(LathePlacementDto { radial: place.radial, along: place.along, tip: place.tip }),
                insert_zone: zone.map(|z| InsertZoneDto { lo: z.lo, hi: z.hi }),
                facets,
                spun: None,
                machine_default: false,
                pieces,
            },
        }
    }

    /// The body, its envelope or clearance re-derived; a refusal names
    /// the tool.
    pub fn to_body(&self, tool: &str) -> Result<ToolBody, String> {
        let mut body = self.to_drawn_body(tool)?;
        body.machine_default = self.machine_default;
        if !self.pieces.is_empty() {
            let mut pieces = Vec::with_capacity(self.pieces.len());
            let mut total_facets = 0usize;
            for p in &self.pieces {
                let pf = dto_to_facets(p.facets.clone());
                total_facets += pf.len();
                let mut csys = Vec::with_capacity(p.csys.len());
                for c in &p.csys {
                    csys.push(
                        crate::tool_body_assembly::BodyCsys::new(c.name.clone(), c.origin, c.z, c.x)
                            .map_err(|e| format!("tool '{tool}': {e}"))?,
                    );
                }
                pieces.push(crate::tool_body_assembly::BodyPiece {
                    source: p.source.clone(),
                    facets: std::sync::Arc::from(pf),
                    csys,
                });
            }
            if total_facets != body.facets.len() {
                return Err(format!(
                    "tool '{tool}': modular piece facets ({total_facets}) do not match combined body facets ({})",
                    body.facets.len()
                ));
            }
            // Pieces rebuild the judged body after attachment/removal. Require
            // the same ordered geometry that was validated above, not merely
            // the same number of facets, so a saved piece cannot shrink it.
            if !pieces.iter().flat_map(|p| p.facets.iter()).eq(body.facets.iter()) {
                return Err(format!(
                    "tool '{tool}': modular piece coordinates do not match combined body facets"
                ));
            }
            body.pieces = pieces;
        }
        Ok(body)
    }

    fn to_drawn_body(&self, tool: &str) -> Result<ToolBody, String> {
        if let Some(pts) = &self.spun {
            if self.lathe.is_some() || self.insert_zone.is_some() {
                return Err(format!("tool '{tool}': a drawn holder profile is a spindle body, not a lathe one"));
            }
            let points = pts.iter().map(|p| SpunPoint { z: p[0], r: p[1], arc: p[2] }).collect();
            let spun = SpunProfile::new(points).map_err(|e| format!("tool '{tool}': holder profile: {e}"))?;
            return Ok(crate::tool_body::build_spun(spun));
        }
        let facets = dto_to_facets(self.facets.clone());
        let zone = self.insert_zone.map(|z| InsertZone { lo: z.lo, hi: z.hi });
        let built = match (self.axis, self.offset, self.lathe) {
            (Some(_), Some(_), None) if zone.is_some() => Err("an insert zone needs a lathe placement, not a spindle one".to_owned()),
            (Some(axis), Some(offset), None) => crate::tool_body::build(&self.source, axis, offset, facets),
            (None, None, Some(l)) => {
                crate::tool_body_lathe::build(&self.source, LathePlacement { radial: l.radial, along: l.along, tip: l.tip }, zone, facets)
            }
            _ => Err("the tool body states neither one spindle placement (axis and offset) nor one lathe placement".to_owned()),
        };
        built.map_err(|e| format!("tool '{tool}': {e}"))
    }
}

/// A drawn outline on file: `[z, r, arc]` per point, tip-up.
#[must_use]
pub fn spun_to_dto(s: &SpunProfile) -> Vec<[f64; 3]> {
    s.points().iter().map(|p| [p.z, p.r, p.arc]).collect()
}

/// A tool's drawn cutter from file, judged against `tool`'s numbers
/// (`cutter_profile::check`); a refusal names the tool.
pub fn cutter_from_dto(tool: &crate::tools::Tool, pts: &[[f64; 3]], name: &str) -> Result<SpunProfile, String> {
    let points = pts.iter().map(|p| SpunPoint { z: p[0], r: p[1], arc: p[2] }).collect();
    camcraft_tools::cutter_profile::check(tool, points).map_err(|e| format!("tool '{name}': cutter profile: {e}"))
}
