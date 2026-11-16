//! The file mirror of `links::Approach` (M11.3 B/C/D): one optional
//! `approach` object inside an op's `links`, absent when every entry,
//! lead and feed parameter is its default and, inside it, only the fields
//! that differ - so every earlier file reads unchanged and every op that
//! never touched them saves byte-identically (no version step: the
//! serde-default law). The runtime `center_cutting` flag is the tool's,
//! never written here. Provenance: our own file format.

use super::*;

use crate::links::{Approach, Compensation};
use crate::machining2d::Lead;

/// The changed fields of an op's [`Approach`]; every field absent is
/// the default.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ApproachDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_max_len: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_to_depth: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helix_min_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helix_pitch: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helix_angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub helix_cw: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc_sweep: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plunge_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramp_feed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plunge_feed: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_feed_pct: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_engagement: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_out: Option<LeadDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext_in: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ext_out: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lead_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overlap: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_start: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis_tol: Option<f64>,
    /// The pocket's horizontal (G17) arc entry (M11.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc_xy: Option<bool>,
    /// Pre-drilled entry (#1449): written only when on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predrilled: Option<bool>,
    /// Cutter compensation in the control (M13.11): written only when
    /// on; absent is in computer, every earlier file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_comp: Option<bool>,
    /// The wear variant (#38): written only when on; absent is not
    /// wear, every earlier file. `control_comp` wins if both are set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wear_comp: Option<bool>,
    /// The user's start point (#1448), `[x, y]` in mm: written only when
    /// set; absent is the automatic seam, every earlier file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_point: Option<[f64; 2]>,
    /// The user's exit point (#1473), `[x, y]` in mm: written only when
    /// set; absent leaves at the seam, every earlier file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_point: Option<[f64; 2]>,
}

fn lead_dto(l: Lead) -> LeadDto {
    match l {
        Lead::None => LeadDto::None,
        Lead::Linear { length } => LeadDto::Linear { length },
        Lead::Arc { radius } => LeadDto::Arc { radius },
        Lead::Tangential { radius } => LeadDto::Tangential { radius },
        Lead::Perpendicular { length } => LeadDto::Perpendicular { length },
    }
}

fn lead_of(d: LeadDto) -> Lead {
    match d {
        LeadDto::None => Lead::None,
        LeadDto::Linear { length } => Lead::Linear { length },
        LeadDto::Arc { radius } => Lead::Arc { radius },
        LeadDto::Tangential { radius } => Lead::Tangential { radius },
        LeadDto::Perpendicular { length } => Lead::Perpendicular { length },
    }
}

/// `a` as written: `None` when every saved field is the default.
#[must_use]
pub fn approach_to_dto(a: &Approach) -> Option<ApproachDto> {
    let d = Approach::DEFAULT;
    let num = |v: f64, w: f64| (v.to_bits() != w.to_bits()).then_some(v);
    let flag = |v: bool, w: bool| (v != w).then_some(v);
    let dto = ApproachDto {
        ramp_angle: num(a.ramp_angle, d.ramp_angle),
        ramp_max_len: num(a.ramp_max_len, d.ramp_max_len),
        ramp_to_depth: flag(a.ramp_to_depth, d.ramp_to_depth),
        helix_min_radius: num(a.helix_min_radius, d.helix_min_radius),
        helix_pitch: num(a.helix_pitch, d.helix_pitch),
        helix_angle: num(a.helix_angle, d.helix_angle),
        helix_cw: flag(a.helix_cw, d.helix_cw),
        arc_radius: num(a.arc_radius, d.arc_radius),
        arc_sweep: num(a.arc_sweep, d.arc_sweep),
        plunge_height: num(a.plunge_height, d.plunge_height),
        ramp_feed: num(a.ramp_feed, d.ramp_feed),
        plunge_feed: num(a.plunge_feed, d.plunge_feed),
        lead_feed_pct: num(a.lead_feed_pct, d.lead_feed_pct),
        max_engagement: num(a.max_engagement, d.max_engagement),
        lead_out: a.lead_out.map(lead_dto),
        ext_in: num(a.ext_in, d.ext_in),
        ext_out: num(a.ext_out, d.ext_out),
        lead_height: num(a.lead_height, d.lead_height),
        overlap: num(a.overlap, d.overlap),
        axis_start: flag(a.axis_start, d.axis_start),
        axis_tol: num(a.axis_tol, d.axis_tol),
        arc_xy: flag(a.arc_xy, d.arc_xy),
        predrilled: flag(a.predrilled, d.predrilled),
        control_comp: (a.compensation == Compensation::Control).then_some(true),
        wear_comp: (a.compensation == Compensation::Wear).then_some(true),
        start_point: a.start_point.map(|q| [q.x, q.y]),
        exit_point: a.exit_point.map(|q| [q.x, q.y]),
    };
    (dto != ApproachDto::default()).then_some(dto)
}

/// The op's approach from the file: the default with every stated field
/// laid over it (the runtime `center_cutting` stays the default `true`).
#[must_use]
pub fn approach_from_dto(d: Option<&ApproachDto>) -> Approach {
    let a = Approach::DEFAULT;
    let Some(d) = d else {
        return a;
    };
    Approach {
        ramp_angle: d.ramp_angle.unwrap_or(a.ramp_angle),
        ramp_max_len: d.ramp_max_len.unwrap_or(a.ramp_max_len),
        ramp_to_depth: d.ramp_to_depth.unwrap_or(a.ramp_to_depth),
        helix_min_radius: d.helix_min_radius.unwrap_or(a.helix_min_radius),
        helix_pitch: d.helix_pitch.unwrap_or(a.helix_pitch),
        helix_angle: d.helix_angle.unwrap_or(a.helix_angle),
        helix_cw: d.helix_cw.unwrap_or(a.helix_cw),
        arc_radius: d.arc_radius.unwrap_or(a.arc_radius),
        arc_sweep: d.arc_sweep.unwrap_or(a.arc_sweep),
        plunge_height: d.plunge_height.unwrap_or(a.plunge_height),
        ramp_feed: d.ramp_feed.unwrap_or(a.ramp_feed),
        plunge_feed: d.plunge_feed.unwrap_or(a.plunge_feed),
        lead_feed_pct: d.lead_feed_pct.unwrap_or(a.lead_feed_pct),
        max_engagement: d.max_engagement.unwrap_or(a.max_engagement),
        lead_out: d.lead_out.map(lead_of),
        ext_in: d.ext_in.unwrap_or(a.ext_in),
        ext_out: d.ext_out.unwrap_or(a.ext_out),
        lead_height: d.lead_height.unwrap_or(a.lead_height),
        overlap: d.overlap.unwrap_or(a.overlap),
        axis_start: d.axis_start.unwrap_or(a.axis_start),
        axis_tol: d.axis_tol.unwrap_or(a.axis_tol),
        arc_xy: d.arc_xy.unwrap_or(a.arc_xy),
        predrilled: d.predrilled.unwrap_or(a.predrilled),
        compensation: if d.control_comp == Some(true) {
            Compensation::Control
        } else if d.wear_comp == Some(true) {
            Compensation::Wear
        } else {
            Compensation::Computer
        },
        start_point: d.start_point.map(|[x, y]| crate::geo2d::Point2::new(x, y)),
        exit_point: d.exit_point.map(|[x, y]| crate::geo2d::Point2::new(x, y)),
        center_cutting: true,
    }
}
