//! Trunnion kinematics fields and explicit rotary sign convention
//! (#1962): parent-child rotary order, `pivot`, child `offset`,
//! `zero_offset`, `rollover`, `sign_convention`, and `controller.rtcp`.
//!
//! Provenance: clean-room `.mch` schema extension specced from
//! `assets/machines/AGENTS.md` and RS-274 / ISO 841 axis conventions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::def::{AxisLimits, Machine};

/// Explicit rotary sign convention (#1962): whether a positive rotary
/// angle turns the tool right-handed relative to the workpiece (`ToolRh`,
/// ISO 841 / RS-274) or turns the table/workpiece right-handed about the
/// stated `axis` vector (`TableRh`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotarySignConvention {
    #[default]
    ToolRh,
    TableRh,
}

impl RotarySignConvention {
    /// Canonical `.mch` and mirror word (`"tool_rh"` or `"table_rh"`).
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::ToolRh => "tool_rh",
            Self::TableRh => "table_rh",
        }
    }

    /// Parse `"tool_rh"` or `"table_rh"`.
    #[must_use]
    pub fn from_word(w: &str) -> Option<Self> {
        match w {
            "tool_rh" => Some(Self::ToolRh),
            "table_rh" => Some(Self::TableRh),
            _ => None,
        }
    }
}

/// Parsed trunnion fields for one axis part.
#[derive(Default)]
pub(super) struct AxisTrunnion {
    pub pivot: Option<[f64; 3]>,
    pub offset: Option<[f64; 3]>,
    pub zero_offset: Option<f64>,
    pub rollover: Option<bool>,
    pub sign_convention: Option<RotarySignConvention>,
}

fn vec3_length(
    name: &str,
    key: &str,
    part: &Value,
    rotary: bool,
    f: f64,
) -> Result<Option<[f64; 3]>, (String, String)> {
    let v = match part.get(key) {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let field = format!("{name} {key}");
    if !rotary {
        return Err((field, format!("only a rotary axis states {key}")));
    }
    let arr = v
        .as_array()
        .filter(|a| a.len() == 3)
        .ok_or_else(|| (field.clone(), format!("{v} must be three finite numbers")))?;
    let x = arr[0].as_f64().filter(|n| n.is_finite());
    let y = arr[1].as_f64().filter(|n| n.is_finite());
    let z = arr[2].as_f64().filter(|n| n.is_finite());
    let (Some(x), Some(y), Some(z)) = (x, y, z) else {
        return Err((field, format!("{v} must be three finite numbers")));
    };
    let scaled = [x * f, y * f, z * f];
    if !scaled.iter().all(|n| n.is_finite()) {
        return Err((field, "scaled coordinates are not finite".to_owned()));
    }
    Ok(Some(scaled))
}

fn zero_offset(name: &str, part: &Value, rotary: bool) -> Result<Option<f64>, (String, String)> {
    let v = match part.get("zero_offset") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let field = format!("{name} zero_offset");
    if !rotary {
        return Err((field, "only a rotary axis states zero_offset".to_owned()));
    }
    match v.as_f64() {
        Some(deg) if deg.is_finite() => Ok(Some(deg)),
        _ => Err((field, format!("{v} must be a finite angle in degrees"))),
    }
}

fn rollover(name: &str, part: &Value, rotary: bool) -> Result<Option<bool>, (String, String)> {
    let v = match part.get("rollover") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let field = format!("{name} rollover");
    if !rotary {
        return Err((field, "only a rotary axis states rollover".to_owned()));
    }
    match v.as_bool() {
        Some(b) => Ok(Some(b)),
        None => Err((field, format!("{v} must be true or false"))),
    }
}

fn conv_word(key: &str, v: Option<&Value>) -> Result<Option<RotarySignConvention>, (String, String)> {
    let v = match v {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    match v.as_str().and_then(RotarySignConvention::from_word) {
        Some(c) => Ok(Some(c)),
        None => Err((
            key.to_owned(),
            format!("{v} must be \"tool_rh\" or \"table_rh\""),
        )),
    }
}

fn part_sign_convention(
    name: &str,
    part: &Value,
    rotary: bool,
) -> Result<Option<RotarySignConvention>, (String, String)> {
    let v = match part.get("sign_convention") {
        None | Some(Value::Null) => return Ok(None),
        Some(v) => v,
    };
    let field = format!("{name} sign_convention");
    if !rotary {
        return Err((field, "only a rotary axis states sign_convention".to_owned()));
    }
    conv_word(&field, Some(v))
}

/// Parse `controller.rtcp` (boolean or absent/null).
pub(super) fn controller_rtcp(v: &Value) -> Result<Option<bool>, (String, String)> {
    match v.pointer("/controller/rtcp") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Bool(b)) => Ok(Some(*b)),
        Some(other) => Err((
            "controller.rtcp".to_owned(),
            format!("{other} must be true or false"),
        )),
    }
}

/// Parse machine-level `sign_convention` from `kinematics.default` and/or
/// `controller`. Refuses if both are stated and disagree.
pub(super) fn machine_sign_convention(
    v: &Value,
) -> Result<Option<RotarySignConvention>, (String, String)> {
    let k = conv_word(
        "kinematics.default.sign_convention",
        v.pointer("/kinematics/default/sign_convention"),
    )?;
    let c = conv_word(
        "controller.sign_convention",
        v.pointer("/controller/sign_convention"),
    )?;
    if let (Some(kv), Some(cv)) = (k, c)
        && kv != cv
    {
        return Err((
            "controller.sign_convention".to_owned(),
            format!(
                "{:?} disagrees with kinematics.default.sign_convention ({:?})",
                cv.word(),
                kv.word()
            ),
        ));
    }
    Ok(k.or(c))
}

/// Parse all trunnion fields for one part, scaling lengths by `f`.
pub(super) fn parse_axis_fields(
    name: &str,
    part: &Value,
    rotary: bool,
    f: f64,
) -> Result<AxisTrunnion, (String, String)> {
    Ok(AxisTrunnion {
        pivot: vec3_length(name, "pivot", part, rotary, f)?,
        offset: vec3_length(name, "offset", part, rotary, f)?,
        zero_offset: zero_offset(name, part, rotary)?,
        rollover: rollover(name, part, rotary)?,
        sign_convention: part_sign_convention(name, part, rotary)?,
    })
}

/// All named refusals for trunnion schema fields across `v` and `axes`
/// (shared single source of truth for `parse_mch` and `validate_text`).
#[must_use]
pub fn trunnion_refusals(v: &Value, axes: &[(String, &Value, bool)]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let f = match v
        .pointer("/kinematics/default/units/length")
        .and_then(Value::as_str)
    {
        Some("inch" | "inches") => 25.4,
        _ => 1.0,
    };
    if let Err(e) = controller_rtcp(v) {
        out.push(e);
    }
    let k = conv_word(
        "kinematics.default.sign_convention",
        v.pointer("/kinematics/default/sign_convention"),
    );
    let c = conv_word(
        "controller.sign_convention",
        v.pointer("/controller/sign_convention"),
    );
    if let Err(e) = &k {
        out.push(e.clone());
    }
    if let Err(e) = &c {
        out.push(e.clone());
    }
    if let (Ok(Some(kv)), Ok(Some(cv))) = (k, c)
        && kv != cv
    {
        out.push((
            "controller.sign_convention".to_owned(),
            format!(
                "{:?} disagrees with kinematics.default.sign_convention ({:?})",
                cv.word(),
                kv.word()
            ),
        ));
    }
    for (name, part, rotary) in axes {
        out.extend(
            [
                vec3_length(name, "pivot", part, *rotary, f).err(),
                vec3_length(name, "offset", part, *rotary, f).err(),
                zero_offset(name, part, *rotary).err(),
                rollover(name, part, *rotary).err(),
                part_sign_convention(name, part, *rotary).err(),
            ]
            .into_iter()
            .flatten(),
        );
    }
    out
}

impl Machine {
    /// Rotary axes in parent-child `kinematics.default.parts` chain order
    /// (#1962), falling back to `axes` rotary order when `chain_order`
    /// is unstated (legacy or hand-built structs).
    #[must_use]
    pub fn rotary_axes(&self) -> Vec<&AxisLimits> {
        let mut out: Vec<&AxisLimits> = self.axes.iter().filter(|a| a.rotary).collect();
        out.sort_by_key(|a| a.chain_order.unwrap_or(u32::MAX));
        out
    }

    /// The resolved rotary sign convention for `axis` (or the machine
    /// default when `axis` states none), defaulting to ISO 841 `ToolRh`.
    #[must_use]
    pub fn effective_sign_convention(&self, axis: Option<&AxisLimits>) -> RotarySignConvention {
        axis.and_then(|a| a.sign_convention)
            .or(self.sign_convention)
            .unwrap_or(RotarySignConvention::ToolRh)
    }

    /// Sanity checks for trunnion fields (used by `Machine::is_sane`).
    pub(super) fn trunnion_sane(&self) -> bool {
        let vec_ok = |v: Option<[f64; 3]>| v.is_none_or(|p| p.iter().all(|x| x.is_finite()));
        for a in &self.axes {
            if !a.rotary {
                if a.parent.is_some()
                    || a.chain_order.is_some()
                    || a.pivot.is_some()
                    || a.offset.is_some()
                    || a.zero_offset.is_some()
                    || a.rollover.is_some()
                    || a.sign_convention.is_some()
                {
                    return false;
                }
                continue;
            }
            if !vec_ok(a.pivot)
                || !vec_ok(a.offset)
                || !a.zero_offset.is_none_or(f64::is_finite)
            {
                return false;
            }
            if let Some(p) = a.parent.as_deref()
                && (p.is_empty()
                    || p == a.name
                    || !self.axes.iter().any(|x| x.rotary && x.name == p))
            {
                return false;
            }
            // Reject parent cycles.
            let mut cur = a.parent.as_deref();
            for _ in 0..self.axes.len() {
                let Some(p) = cur else { break };
                if p == a.name {
                    return false;
                }
                cur = self
                    .axes
                    .iter()
                    .find(|x| x.rotary && x.name == p)
                    .and_then(|x| x.parent.as_deref());
                if cur.is_some() && cur == Some(p) {
                    return false;
                }
            }
            if cur.is_some() {
                return false;
            }
        }
        true
    }
}
