//! Waterjet nozzle and plate recipe data (#1881).
//! Provenance: our own data model from #1881 and #1521; no cutting values are inferred.

use serde::{Deserialize, Serialize};
use crate::tools::{Tool, ToolKind};

/// A nozzle with a stated stream kerf. Invalid kerfs remain invalid at the tool door.
#[must_use]
pub fn waterjet_tool(kerf: f64) -> Tool {
    Tool { kind: ToolKind::Waterjet, material_group: String::new(), ..crate::tools::torch_tool(kerf) }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PierceMethod { Stationary, Dynamic, Wiggle }

/// Millimetres and seconds. Material and cut feed belong to the enclosing row.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WaterjetCutData {
    pub thickness: f64,
    pub kerf: f64,
    pub pierce_time: f64,
    pub abrasive_delay: f64,
    pub re_pierce_time: f64,
    pub pierce_method: PierceMethod,
}

impl WaterjetCutData {
    #[must_use]
    pub fn refusal(&self) -> Option<String> {
        for (name, value) in [("thickness", self.thickness), ("kerf", self.kerf)] {
            if !value.is_finite() || value <= 0.0 {
                return Some(format!("waterjet {name} must be finite and above zero"));
            }
        }
        for (name, value) in [("pierce_time", self.pierce_time), ("abrasive_delay", self.abrasive_delay), ("re_pierce_time", self.re_pierce_time)] {
            if !value.is_finite() || value < 0.0 {
                return Some(format!("waterjet {name} must be finite and non-negative"));
            }
        }
        None
    }
}

/// Named tool refusal used at command and file doors.
#[must_use]
pub fn refusal(tool: &Tool) -> Option<String> {
    if tool.kind != ToolKind::Waterjet { return None; }
    if tool.rpm != 0.0 {
        return Some("waterjet rpm must be 0; the nozzle has no spindle".into());
    }
    if !tool.diameter.is_finite() || tool.diameter <= 0.0 {
        return Some("waterjet kerf (diameter) must be finite and above zero".into());
    }
    if tool.corner_r != 0.0 {
        return Some("waterjet corner_r must be 0; the diameter is the kerf".into());
    }
    crate::cutting_rows::refusal_for_kind(&tool.cutting, tool.kind)
}
