//! Per-post declared option schemas (`declaredOptions`), resolved option
//! merging, and program tool list construction (#258).
//!
//! Provenance: original schema model over public CNC controller option
//! handling and RS-274 setup tooling metadata; no vendor template consulted.

use super::cnc::Record;
use super::PostError;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Number, Value};
use std::collections::BTreeMap;

pub use super::options_parse::{
    declared_options, parse_cli_override, parse_json_overrides, validate_overrides, validate_value,
};

/// Built-in `postOptions` keys controlled by the host (`Units`, `Threading`,
/// `Smoothing`, `EndUnits`, `Explain`, `BetweenSetups`). A `.cps` script must
/// not re-declare these in `declaredOptions`.
pub const RESERVED_KEYS: &[&str] = &[
    "Units",
    "Threading",
    "Smoothing",
    "EndUnits",
    "Explain",
    "BetweenSetups",
];

/// One discrete choice for an [`OptionKind::Enum`] option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnumChoice {
    pub id: String,
    pub title: String,
}

/// Supported declared option value types and bounds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OptionKind {
    Boolean,
    Integer { min: Option<i64>, max: Option<i64> },
    Number { min: Option<f64>, max: Option<f64> },
    String,
    Enum { choices: Vec<EnumChoice> },
}

/// A strongly-typed post script option value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl OptionValue {
    /// True if this value holds no NaN or infinite float.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        !matches!(self, Self::Float(v) if !v.is_finite())
    }

    /// Convert to JSON.
    #[must_use]
    pub fn to_json(&self) -> Value {
        match self {
            Self::Bool(b) => Value::Bool(*b),
            Self::Int(i) => Value::Number((*i).into()),
            Self::Float(f) => Number::from_f64(*f).map_or(Value::Null, Value::Number),
            Self::Str(s) => Value::String(s.clone()),
        }
    }
}

/// One declared option entry extracted from a `.cps` script's `declaredOptions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeclaredOption {
    pub key: String,
    pub title: String,
    pub description: String,
    pub group: String,
    pub kind: OptionKind,
    pub default: OptionValue,
}

/// Resolve all declared options to their defaults plus validated overrides.
pub fn resolve_options(
    schema: &[DeclaredOption],
    overrides: &BTreeMap<String, OptionValue>,
) -> Result<Map<String, Value>, String> {
    validate_overrides(schema, overrides)?;
    let mut out = Map::new();
    for opt in schema {
        let val = overrides
            .get(&opt.key)
            .map(|v| validate_value(opt, v))
            .transpose()?
            .unwrap_or_else(|| opt.default.clone());
        out.insert(opt.key.clone(), val.to_json());
    }
    Ok(out)
}

/// Merge resolved declared script options into a host `postOptions` JSON value,
/// preserving built-in host fields (`Units`, `Threading`, etc.).
pub fn merge_post_options(
    schema: &[DeclaredOption],
    base: &Value,
    overrides: &BTreeMap<String, OptionValue>,
) -> Result<Value, String> {
    let mut map = match base {
        Value::Object(m) => m.clone(),
        Value::Null => Map::new(),
        _ => return Err("postOptions base value must be an object or null".into()),
    };
    for (k, v) in resolve_options(schema, overrides)? {
        map.insert(k, v);
    }
    Ok(Value::Object(map))
}

/// Format unique tools referenced by a CNC record stream as a JS global definition.
pub(super) fn program_tools_js(records: &[Record]) -> Result<String, PostError> {
    let mut dia: Option<f64> = None;
    let mut kind: Option<String> = None;
    let mut tools: BTreeMap<u32, Value> = BTreeMap::new();
    let mut current_tool: Option<u32> = None;

    for r in records {
        match r {
            Record::Param { name, value } if name == "ToolDiameter" => {
                dia = value.as_f64().filter(|v| v.is_finite());
                if let Some(n) = current_tool
                    && let Some(Value::Object(obj)) = tools.get_mut(&n)
                {
                    obj.insert("diameter".into(), dia.map_or(Value::Null, |d| json!(d)));
                }
            }
            Record::Param { name, value } if name == "ToolKind" => {
                kind = value.as_str().map(str::to_owned);
                if let Some(n) = current_tool
                    && let Some(Value::Object(obj)) = tools.get_mut(&n)
                {
                    obj.insert(
                        "kind".into(),
                        kind.as_deref().map_or(Value::Null, |k| Value::String(k.to_owned())),
                    );
                }
            }
            Record::Tool { number, name } => {
                current_tool = Some(*number);
                tools.entry(*number).or_insert_with(|| {
                    json!({
                        "number": number,
                        "name": name,
                        "diameter": dia,
                        "kind": kind.as_deref(),
                    })
                });
            }
            Record::Section { begin: false } => {
                dia = None;
                kind = None;
                current_tool = None;
            }
            _ => {}
        }
    }
    let arr = Value::Array(tools.into_values().collect());
    let json = serde_json::to_string(&arr)
        .map_err(|e| PostError::Script(format!("programTools serialize failed: {e}")))?;
    Ok(format!("var programTools = {json};"))
}
