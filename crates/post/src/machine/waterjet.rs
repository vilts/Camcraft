//! Waterjet machine doors (#1881).
//! Provenance: our own rules from the issue's XY gantry and optional Z specification.

use serde_json::Value;
use super::{kind, Kind, Machine};
use super::no_spindle::{head_refusals, NoSpindle};

/// File-form refusals, shared by the parser and editor validator.
#[must_use]
pub fn waterjet_refusals(v: &Value) -> Vec<(String, String)> {
    let rules = NoSpindle {
        spindle: "a waterjet has no spindle; remove the spindle key",
        live_tool: "a waterjet has no live tool",
        coolant: "a waterjet states no milling coolant keys",
    };
    let (mut out, rotaries) = head_refusals(v, &rules);
    for name in rotaries {
        out.push((name, "a waterjet states no rotary axis".into()));
    }
    fn has(parts: Option<&Value>, name: &str) -> bool {
        parts.and_then(Value::as_array).is_some_and(|parts| parts.iter().any(|p| {
            (p.get("type").and_then(Value::as_str) == Some("linear")
                && p.get("name").or_else(|| p.get("id")).and_then(Value::as_str)
                    .is_some_and(|n| n.eq_ignore_ascii_case(name)))
                || has(p.get("parts"), name)
        }))
    }
    for name in ["X", "Y"] {
        if !has(v.pointer("/kinematics/default/parts"), name) {
            out.push((format!("{name} axis"), "required for a waterjet machine".into()));
        }
    }
    out
}

impl Machine {
    /// The same rules for machines loaded from a saved project.
    #[must_use]
    pub fn waterjet_sane(&self) -> bool {
        if kind(Some(self)) != Kind::Waterjet { return true; }
        self.spindle_min.is_none() && self.spindle_max.is_none()
            && self.spindle_power.is_none() && self.spindle_ranges.is_empty()
            && self.rigid_tapping.is_none() && self.spindles.is_none()
            && self.spindle_d.is_none() && self.spindle_len.is_none()
            && self.live_tool.is_none() && self.coolants.is_empty()
            && self.axes.iter().all(|a| !a.rotary)
            && [0, 1].iter().all(|&k| self.axis(k).is_some_and(|a| !a.rotary))
    }
}
