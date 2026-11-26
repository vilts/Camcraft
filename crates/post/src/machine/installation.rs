//! Machine installation facts from Camcraft's .mch design and HEIDENHAIN's release-specific NC manuals.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Names are exact identifiers. An absent option list means the installed options are unknown.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControllerIdentity {
    pub model: String,
    pub nc_software: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_options: Option<Vec<String>>,
}

impl ControllerIdentity {
    pub(super) fn validate(&self) -> Result<(), String> {
        let identifier =
            |s: &str| !s.is_empty() && s.trim() == s && !s.chars().any(char::is_control);
        if !identifier(&self.model) {
            return Err(
                "machine file: controller.identity.model must be a non-empty control identifier"
                    .into(),
            );
        }
        if !identifier(&self.nc_software) {
            return Err("machine file: controller.identity.nc_software must be an exact NC software identifier".into());
        }
        if let Some(options) = &self.installed_options {
            let mut seen = std::collections::BTreeSet::new();
            for option in options {
                if !identifier(option) || !seen.insert(option) {
                    return Err("machine file: controller.identity.installed_options must contain distinct, non-empty option identifiers".into());
                }
            }
        }
        Ok(())
    }
}

/// The physical procedure is separate from the controller language or any post instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToolChangeMode {
    Manual,
    ManualRetractTop,
    Changer,
}

/// The controller file form and the post engine's static form can both carry
/// installation facts. A mixed input has two sources for a limit or capability.
/// Refuse it before the CLI or API can discard a stated limit during mirroring.
pub(crate) fn check_input_form(v: &Value) -> Result<(), String> {
    if v.pointer("/kinematics/default/parts").is_none()
        || ["/controller/identity", "/tooling/tool_change"]
            .iter()
            .all(|k| v.pointer(k).is_none_or(Value::is_null))
    {
        return Ok(());
    }
    const STATIC_KEYS: &[&str] = &[
        "capabilities",
        "coolants",
        "coordinateRotation",
        "feedRateMax",
        "invertedAxes",
        "kind",
        "liveTool",
        "name",
        "rapidRate",
        "rigidTapping",
        "rtcp",
        "safeRetractZ",
        "signConvention",
        "spindleMax",
        "spindleMin",
        "spindleRanges",
        "spindles",
        "units",
        "additionalLinearAxes",
    ];
    let key = if v.pointer("/kinematics/static").is_some() {
        Some("kinematics.static")
    } else {
        STATIC_KEYS.iter().copied().find(|k| v.get(*k).is_some())
    };
    if let Some(key) = key {
        return Err(format!(
            "machine file: {key} is a static post property; a machine declaring installation facts with kinematics.default.parts must use the file form"
        ));
    }
    Ok(())
}

pub(crate) fn identity(v: &Value) -> Result<Option<ControllerIdentity>, String> {
    let Some(value) = v.pointer("/controller/identity").filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let control: ControllerIdentity = serde_json::from_value(value.clone())
        .map_err(|e| format!("machine file: controller.identity: {e}"))?;
    control.validate()?;
    Ok(Some(control))
}

pub(crate) fn tool_change(v: &Value) -> Result<Option<ToolChangeMode>, String> {
    v.pointer("/tooling/tool_change")
        .filter(|v| !v.is_null())
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()
        .map_err(|e| format!("machine file: tooling.tool_change: {e}"))
}
