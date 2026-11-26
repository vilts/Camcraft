//! A plasma torch's cut-data recipe for one material and plate thickness (#1521 slice 1).
//! Provenance: our own data model from #1521, using the public LinuxCNC plasma primer's
//! terms (transfer height, pierce height, pierce delay, puddle jump, cut height); no cut
//! chart value is shipped or inferred.
//!
//! Millimetres, seconds, amperes and volts. A value left `None` is "set by the
//! controller's own table", the same meaning as a blank on the plasma operation. The
//! material and the cut feed belong to the enclosing [`CuttingRow`]. Pure module: no
//! flecs, no I/O. Nothing here changes a toolpath or posted output yet.

use serde::{Deserialize, Serialize};

use crate::cutting_rows::{CuttingRow, material_label};

/// The lift after the pierce that clears the molten puddle before the cut.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuddleJump {
    /// The lift height as a percentage of the pierce height.
    pub percent: f64,
    /// The time held at that height, s.
    pub dwell: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlasmaCutData {
    pub thickness: f64,
    pub kerf: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pierce_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pierce_delay: Option<f64>,
    /// The low height where the arc first transfers to the plate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer_height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub puddle_jump: Option<PuddleJump>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amperage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc_voltage: Option<f64>,
    /// The thickest plate this consumable set pierces; `None` = not stated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pierce_thickness: Option<f64>,
    /// The recipe is for a cut that starts off the plate edge.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edge_start: bool,
}

impl PlasmaCutData {
    /// The refusal text for a recipe that cannot be kept, or `None`.
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        let above_zero = |v: f64| v.is_finite() && v > 0.0;
        let zero_or_more = |v: f64| v.is_finite() && v >= 0.0;
        for (name, v) in [("thickness", self.thickness), ("kerf", self.kerf)] {
            if !above_zero(v) {
                return Some(format!("plasma {name} must be finite and above zero (it states {v})"));
            }
        }
        let jump = self.puddle_jump;
        for (name, v) in [
            ("amperage", self.amperage),
            ("arc voltage", self.arc_voltage),
            ("pierce capacity", self.max_pierce_thickness),
            ("puddle jump percent", jump.map(|j| j.percent)),
        ] {
            if let Some(v) = v.filter(|v| !above_zero(*v)) {
                return Some(format!("a stated plasma {name} must be finite and above zero (it states {v})"));
            }
        }
        for (name, v) in [
            ("cut height", self.cut_height),
            ("pierce height", self.pierce_height),
            ("transfer height", self.transfer_height),
            ("pierce delay", self.pierce_delay),
            ("puddle jump dwell", jump.map(|j| j.dwell)),
        ] {
            if let Some(v) = v.filter(|v| !zero_or_more(*v)) {
                return Some(format!("a stated plasma {name} must be zero or more (it states {v}) - leave it blank for the controller's"));
            }
        }
        if jump.is_some() && self.pierce_height.is_none() {
            return Some("a plasma puddle jump is a percentage of the pierce height: state the pierce height too".into());
        }
        None
    }

    /// Is the plate thicker than the stated pierce capacity?
    #[must_use]
    pub fn above_pierce_capacity(&self) -> bool {
        self.max_pierce_thickness.is_some_and(|max| self.thickness > max)
    }
}

/// The refusal text for the plasma recipes among a torch's `rows`, or
/// `None`. Rows without a recipe are judged by the plain-row law.
#[must_use]
pub fn rows_refusal(rows: &[CuttingRow]) -> Option<String> {
    for (i, r) in rows.iter().enumerate() {
        let Some(p) = r.plasma else { continue };
        let Some(material) = r.material else {
            return Some("plasma cutting data names its workpiece material".into());
        };
        let what = format!("{} at thickness {}", material.label(), p.thickness);
        if let Some(why) = p.refusal() {
            return Some(format!("plasma cutting data for {}: {why}", material_label(r.material)));
        }
        if r.rpm != 0.0 {
            return Some(format!("plasma cutting data for {what}: rpm must be 0 on a torch (it states {})", r.rpm));
        }
        if !(r.feed.is_finite() && r.feed > 0.0) {
            return Some(format!("plasma cutting data for {what}: feed must be a finite number above zero (it states {})", r.feed));
        }
        if p.above_pierce_capacity() && !p.edge_start {
            return Some(format!(
                "plasma cutting data for {what}: the plate is thicker than the pierce capacity {} - state an edge start, or a capacity that covers it",
                p.max_pierce_thickness.unwrap_or_default()
            ));
        }
        if rows[..i].iter().any(|q| q.material == r.material && q.plasma.is_some_and(|q| q.thickness == p.thickness)) {
            return Some(format!("plasma cutting data names {what} twice"));
        }
    }
    None
}
