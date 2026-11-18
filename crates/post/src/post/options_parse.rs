//! Extraction and validation of `.cps` `declaredOptions` schemas and
//! runtime overrides (#258).
//!
//! Provenance: original schema validator over public CNC controller option
//! types; no vendor post source or template consulted.

use super::js::Script;
use super::options::{
    DeclaredOption, EnumChoice, OptionKind, OptionValue, RESERVED_KEYS,
};
use super::PostError;
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const EXTRACT_DECLARED_JS: &str = r#"
var __cam_decl_opt_json = (function() {
  if (typeof declaredOptions === "undefined") return "null";
  if (declaredOptions === null || typeof declaredOptions !== "object" || Array.isArray(declaredOptions)) {
    return JSON.stringify({ __error: "declaredOptions must be an object" });
  }
  var keys = Object.keys(declaredOptions);
  var out = [];
  for (var i = 0; i < keys.length; i++) {
    var k = keys[i];
    var d = declaredOptions[k];
    if (!d || typeof d !== "object" || Array.isArray(d)) {
      return JSON.stringify({ __error: "declaredOptions." + k + ": definition must be an object" });
    }
    var minVal = d.min !== undefined ? d.min : d.minimum;
    var maxVal = d.max !== undefined ? d.max : d.maximum;
    if ((typeof d.default === "number" && !Number.isFinite(d.default)) ||
        (typeof minVal === "number" && !Number.isFinite(minVal)) ||
        (typeof maxVal === "number" && !Number.isFinite(maxVal))) {
      return JSON.stringify({ __error: "declaredOptions." + k + ": numbers must be finite" });
    }
    out.push({
      key: k,
      type: d.type,
      title: d.title,
      description: d.description,
      group: d.group,
      min: minVal,
      max: maxVal,
      choices: d.choices,
      hasDefault: Object.prototype.hasOwnProperty.call(d, "default"),
      defaultVal: d.default
    });
  }
  return JSON.stringify({ entries: out });
})();
"#;

/// Extract and validate `declaredOptions` from a `.cps` script source.
///
/// Scripts without `declaredOptions` return an empty vector.
pub fn declared_options(cps: &str) -> Result<Vec<DeclaredOption>, PostError> {
    let mut script = Script::load(cps)?;
    script.eval_src(EXTRACT_DECLARED_JS)?;
    let Some(raw) = script.string_global("__cam_decl_opt_json")? else {
        return Ok(Vec::new());
    };
    if raw == "null" {
        return Ok(Vec::new());
    }
    let parsed: Value = serde_json::from_str(&raw)
        .map_err(|e| PostError::Script(format!("declaredOptions parse failed: {e}")))?;
    if let Some(msg) = parsed.get("__error").and_then(Value::as_str) {
        return Err(PostError::Script(msg.to_owned()));
    }
    let Some(entries) = parsed.get("entries").and_then(Value::as_array) else {
        return Err(PostError::Script("declaredOptions: expected entries array".into()));
    };
    let mut seen = BTreeSet::new();
    let mut schema = Vec::with_capacity(entries.len());
    for entry in entries {
        let opt = parse_entry(entry).map_err(PostError::Script)?;
        if !seen.insert(opt.key.clone()) {
            return Err(PostError::Script(format!("declaredOptions: duplicate key '{}'", opt.key)));
        }
        schema.push(opt);
    }
    Ok(schema)
}

fn parse_entry(raw: &Value) -> Result<DeclaredOption, String> {
    let key = raw
        .get("key")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_owned();
    if key.is_empty() {
        return Err("declaredOptions: option key must not be empty".into());
    }
    if RESERVED_KEYS.contains(&key.as_str()) {
        return Err(format!("declaredOptions.{key}: key is reserved for built-in postOptions"));
    }
    let kind_str = raw
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| format!("declaredOptions.{key}: missing string 'type'"))?;
    if raw.get("hasDefault").and_then(Value::as_bool) != Some(true) {
        return Err(format!("declaredOptions.{key}: missing required 'default'"));
    }
    let def_val = raw.get("defaultVal").unwrap_or(&Value::Null);
    let title = opt_str(raw, "title")?.unwrap_or_else(|| key.clone());
    let description = opt_str(raw, "description")?.unwrap_or_default();
    let group = opt_str(raw, "group")?.unwrap_or_else(|| "General".to_owned());

    let (kind, default) = match kind_str {
        "boolean" => {
            let b = def_val
                .as_bool()
                .ok_or_else(|| format!("declaredOptions.{key}: boolean option default must be true/false"))?;
            (OptionKind::Boolean, OptionValue::Bool(b))
        }
        "integer" => {
            let min = opt_int(raw, "min", &key)?;
            let max = opt_int(raw, "max", &key)?;
            if let (Some(lo), Some(hi)) = (min, max)
                && lo > hi
            {
                return Err(format!("declaredOptions.{key}: min ({lo}) exceeds max ({hi})"));
            }
            let v = def_val
                .as_i64()
                .ok_or_else(|| format!("declaredOptions.{key}: integer option default must be a whole integer"))?;
            check_int_bounds(&key, v, min, max)?;
            (OptionKind::Integer { min, max }, OptionValue::Int(v))
        }
        "number" => {
            let min = opt_num(raw, "min", &key)?;
            let max = opt_num(raw, "max", &key)?;
            if let (Some(lo), Some(hi)) = (min, max)
                && lo > hi
            {
                return Err(format!("declaredOptions.{key}: min ({lo}) exceeds max ({hi})"));
            }
            let f = def_val
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| format!("declaredOptions.{key}: number option default must be a finite number"))?;
            check_num_bounds(&key, f, min, max)?;
            (OptionKind::Number { min, max }, OptionValue::Float(f))
        }
        "string" => {
            let s = def_val
                .as_str()
                .ok_or_else(|| format!("declaredOptions.{key}: string option default must be a string"))?;
            (OptionKind::String, OptionValue::Str(s.to_owned()))
        }
        "enum" => {
            let choices = parse_choices(raw.get("choices"), &key)?;
            let s = def_val
                .as_str()
                .ok_or_else(|| format!("declaredOptions.{key}: enum option default must be a string id"))?;
            if !choices.iter().any(|c| c.id == s) {
                return Err(format!("declaredOptions.{key}: enum default '{s}' is not among choices"));
            }
            (OptionKind::Enum { choices }, OptionValue::Str(s.to_owned()))
        }
        other => {
            return Err(format!(
                "declaredOptions.{key}: unsupported option type '{other}' (expected boolean, integer, number, string, or enum)"
            ));
        }
    };
    Ok(DeclaredOption { key, title, description, group, kind, default })
}

fn opt_str(raw: &Value, field: &str) -> Result<Option<String>, String> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("declaredOptions: '{field}' must be a string")),
    }
}

fn opt_int(raw: &Value, field: &str, key: &str) -> Result<Option<i64>, String> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .map(Some)
            .ok_or_else(|| format!("declaredOptions.{key}: '{field}' must be an integer")),
    }
}

fn opt_num(raw: &Value, field: &str, key: &str) -> Result<Option<f64>, String> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_f64()
            .filter(|f| f.is_finite())
            .map(Some)
            .ok_or_else(|| format!("declaredOptions.{key}: '{field}' must be a finite number")),
    }
}

fn parse_choices(raw: Option<&Value>, key: &str) -> Result<Vec<EnumChoice>, String> {
    let items = raw
        .and_then(Value::as_array)
        .filter(|a| !a.is_empty())
        .ok_or_else(|| format!("declaredOptions.{key}: enum option requires a non-empty 'choices' array"))?;
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(items.len());
    for item in items {
        let (id, title) = match item {
            Value::String(s) => (s.clone(), s.clone()),
            Value::Object(map) => {
                let id = map
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| format!("declaredOptions.{key}: enum choice object needs string 'id'"))?;
                let title = map.get("title").and_then(Value::as_str).unwrap_or(id);
                (id.to_owned(), title.to_owned())
            }
            _ => return Err(format!("declaredOptions.{key}: invalid enum choice item")),
        };
        if !seen.insert(id.clone()) {
            return Err(format!("declaredOptions.{key}: duplicate enum choice id '{id}'"));
        }
        out.push(EnumChoice { id, title });
    }
    Ok(out)
}

fn check_int_bounds(key: &str, v: i64, min: Option<i64>, max: Option<i64>) -> Result<(), String> {
    if let Some(lo) = min
        && v < lo
    {
        return Err(format!("post option '{key}' value {v} is below minimum {lo}"));
    }
    if let Some(hi) = max
        && v > hi
    {
        return Err(format!("post option '{key}' value {v} exceeds maximum {hi}"));
    }
    Ok(())
}

fn check_num_bounds(key: &str, v: f64, min: Option<f64>, max: Option<f64>) -> Result<(), String> {
    if let Some(lo) = min
        && v < lo
    {
        return Err(format!("post option '{key}' value {v} is below minimum {lo}"));
    }
    if let Some(hi) = max
        && v > hi
    {
        return Err(format!("post option '{key}' value {v} exceeds maximum {hi}"));
    }
    Ok(())
}

/// Validate a typed override value against a declared option.
pub fn validate_value(opt: &DeclaredOption, val: &OptionValue) -> Result<OptionValue, String> {
    let k = &opt.key;
    match (&opt.kind, val) {
        (OptionKind::Boolean, OptionValue::Bool(b)) => Ok(OptionValue::Bool(*b)),
        (OptionKind::Integer { min, max }, OptionValue::Int(i)) => {
            check_int_bounds(k, *i, *min, *max)?;
            Ok(OptionValue::Int(*i))
        }
        (OptionKind::Number { min, max }, OptionValue::Float(f)) if f.is_finite() => {
            check_num_bounds(k, *f, *min, *max)?;
            Ok(OptionValue::Float(*f))
        }
        (OptionKind::Number { min, max }, OptionValue::Int(i)) => {
            let f = *i as f64;
            check_num_bounds(k, f, *min, *max)?;
            Ok(OptionValue::Float(f))
        }
        (OptionKind::String, OptionValue::Str(s)) => Ok(OptionValue::Str(s.clone())),
        (OptionKind::Enum { choices }, OptionValue::Str(s)) => {
            if choices.iter().any(|c| c.id == *s) {
                Ok(OptionValue::Str(s.clone()))
            } else {
                let valid: Vec<&str> = choices.iter().map(|c| c.id.as_str()).collect();
                Err(format!(
                    "post option '{k}' value '{s}' is invalid; expected one of {}",
                    valid.join(", ")
                ))
            }
        }
        _ => Err(format!("post option '{k}' has incompatible value type")),
    }
}

/// Validate every override entry against `schema` by key, type, and range.
pub fn validate_overrides(
    schema: &[DeclaredOption],
    overrides: &BTreeMap<String, OptionValue>,
) -> Result<(), String> {
    for (k, v) in overrides {
        if RESERVED_KEYS.contains(&k.as_str()) {
            return Err(format!("post option '{k}' is reserved by host settings"));
        }
        let opt = schema
            .iter()
            .find(|o| o.key == *k)
            .ok_or_else(|| format!("unknown post option '{k}'"))?;
        validate_value(opt, v)?;
    }
    Ok(())
}

/// Parse and validate untyped JSON overrides against `schema`.
pub fn parse_json_overrides(
    schema: &[DeclaredOption],
    obj: &Map<String, Value>,
) -> Result<BTreeMap<String, OptionValue>, String> {
    let mut out = BTreeMap::new();
    for (k, raw) in obj {
        if RESERVED_KEYS.contains(&k.as_str()) {
            return Err(format!("post option '{k}' is reserved by host settings"));
        }
        let opt = schema
            .iter()
            .find(|o| o.key == *k)
            .ok_or_else(|| format!("unknown post option '{k}'"))?;
        let typed = match (&opt.kind, raw) {
            (OptionKind::Boolean, Value::Bool(b)) => OptionValue::Bool(*b),
            (OptionKind::Integer { .. }, Value::Number(n)) => n
                .as_i64()
                .map(OptionValue::Int)
                .ok_or_else(|| format!("post option '{k}' expects an integer"))?,
            (OptionKind::Number { .. }, Value::Number(n)) => n
                .as_f64()
                .filter(|f| f.is_finite())
                .map(OptionValue::Float)
                .ok_or_else(|| format!("post option '{k}' expects a finite number"))?,
            (OptionKind::String | OptionKind::Enum { .. }, Value::String(s)) => {
                OptionValue::Str(s.clone())
            }
            _ => return Err(format!("post option '{k}' has invalid JSON value {raw}")),
        };
        out.insert(k.clone(), validate_value(opt, &typed)?);
    }
    Ok(out)
}

/// Parse a CLI `key=value` override against `schema`.
pub fn parse_cli_override(schema: &[DeclaredOption], spec: &str) -> Result<(String, OptionValue), String> {
    let (key, raw) = spec
        .split_once('=')
        .ok_or_else(|| format!("--option expects key=value, got '{spec}'"))?;
    let key = key.trim();
    if key.is_empty() {
        return Err(format!("--option key must not be empty in '{spec}'"));
    }
    let opt = schema
        .iter()
        .find(|o| o.key == key)
        .ok_or_else(|| format!("unknown post option '{key}'"))?;
    let typed = match &opt.kind {
        OptionKind::Boolean => match raw.trim() {
            "true" | "1" => OptionValue::Bool(true),
            "false" | "0" => OptionValue::Bool(false),
            _ => return Err(format!("post option '{key}' expects true or false, got '{raw}'")),
        },
        OptionKind::Integer { .. } => raw
            .trim()
            .parse::<i64>()
            .map(OptionValue::Int)
            .map_err(|_| format!("post option '{key}' expects an integer, got '{raw}'"))?,
        OptionKind::Number { .. } => raw
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(OptionValue::Float)
            .ok_or_else(|| format!("post option '{key}' expects a finite number, got '{raw}'"))?,
        OptionKind::String | OptionKind::Enum { .. } => OptionValue::Str(raw.to_owned()),
    };
    Ok((key.to_owned(), validate_value(opt, &typed)?))
}
