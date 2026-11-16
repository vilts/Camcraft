//! Validate provider arguments against the live MCP schema before dispatch.
//! Provenance: JSON Schema validation vocabulary (json-schema.org); the subset used by our MCP registry.

use super::tools;
use serde_json::Value;

pub(super) fn arguments(name: &str, args: &Value) -> Result<(), String> {
    let tool = tools::list()
        .into_iter()
        .find(|t| t["name"].as_str() == Some(name))
        .ok_or_else(|| format!("unknown tool '{name}'"))?;
    if !args.is_object() {
        return Err(format!("{name}.arguments must be an object"));
    }
    validate(&tool["inputSchema"], args, &format!("{name}.arguments"))
}

fn validate(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    if schema == &Value::Bool(true) {
        return Ok(());
    }
    if schema == &Value::Bool(false) {
        return Err(format!("{path} is not allowed"));
    }
    let fields = schema
        .as_object()
        .ok_or_else(|| format!("{path}: invalid tool schema"))?;
    // New validation keywords must be implemented before they can authorize a local edit.
    for key in fields.keys() {
        if !matches!(
            key.as_str(),
            "type"
                | "properties"
                | "required"
                | "additionalProperties"
                | "items"
                | "enum"
                | "minimum"
                | "maximum"
                | "minItems"
                | "maxItems"
                | "anyOf"
                | "allOf"
                | "oneOf"
                | "description"
                | "title"
                | "default"
                | "$schema"
                | "$comment"
                | "examples"
        ) {
            return Err(format!("{path}: unsupported schema keyword '{key}'"));
        }
    }
    if let Some(types) = fields.get("type") {
        let valid = match types {
            Value::String(kind) => has_type(value, kind),
            Value::Array(kinds) => kinds.iter().any(|t| t.as_str().is_some_and(|t| has_type(value, t))),
            _ => false,
        };
        if !valid {
            return Err(format!("{path} must have type {types}"));
        }
    }
    if let Some(choices) = fields.get("enum").and_then(Value::as_array)
        && !choices.contains(value)
    {
        return Err(format!("{path} must be one of {}", Value::Array(choices.clone())));
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(branches) = fields.get(key).and_then(Value::as_array) {
            let passed = branches.iter().filter(|s| validate(s, value, path).is_ok()).count();
            let valid = match key {
                "anyOf" => passed > 0,
                "oneOf" => passed == 1,
                _ => passed == branches.len(),
            };
            if !valid {
                return Err(format!("{path} does not satisfy {key}"));
            }
        }
    }
    if let Some(number) = value.as_f64() {
        for (key, lower) in [("minimum", true), ("maximum", false)] {
            if let Some(limit) = fields.get(key).and_then(Value::as_f64)
                && ((lower && number < limit) || (!lower && number > limit))
            {
                return Err(format!("{path} violates {key} {limit}"));
            }
        }
    }
    if let Some(array) = value.as_array() {
        for (key, lower) in [("minItems", true), ("maxItems", false)] {
            if let Some(limit) = fields.get(key).and_then(Value::as_u64)
                && ((lower && (array.len() as u64) < limit) || (!lower && (array.len() as u64) > limit))
            {
                return Err(format!("{path} violates {key} {limit}"));
            }
        }
        if let Some(item_schema) = fields.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate(item_schema, item, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(object) = value.as_object() {
        if let Some(required) = fields.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    return Err(format!("{path}.{key} is required"));
                }
            }
        }
        for (key, value) in object {
            let child = format!("{path}.{key}");
            if let Some(property) = fields.get("properties").and_then(|p| p.get(key)) {
                validate(property, value, &child)?;
            } else if let Some(extra) = fields.get("additionalProperties") {
                validate(extra, value, &child)?;
            }
        }
    }
    Ok(())
}

fn has_type(value: &Value, kind: &str) -> bool {
    match kind {
        "null" => value.is_null(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "number" => value.is_number(),
        "integer" => value.as_f64().is_some_and(|n| n.fract() == 0.0),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn boolean_schemas_and_invalid_schema_shapes_refuse_by_name() {
        assert!(validate(&json!(true), &json!({}), "argument").is_ok());
        let error = validate(&json!(false), &json!({}), "argument").expect_err("false refuses");
        assert!(error.contains("argument is not allowed"), "{error}");
        for schema in [Value::Null, json!("invalid"), json!([])] {
            let error = validate(&schema, &json!({}), "argument").expect_err("invalid schema refuses");
            assert!(error.contains("argument: invalid tool schema"), "{error}");
        }
    }

    #[test]
    fn new_schema_keywords_refuse_authorization_by_name() {
        let error = validate(&json!({"type":"string","pattern":"^safe$"}), &json!("unsafe"), "test.argument")
            .expect_err("unimplemented keyword must refuse");
        assert!(error.contains("pattern"), "{error}");
        assert!(error.contains("test.argument"), "{error}");
    }

    #[test]
    fn schema_combinators_enforce_their_distinct_match_counts() {
        for (schema, accepted, refused) in [
            (json!({"anyOf":[{"type":"string"},{"type":"number"}]}), json!(3), json!(true)),
            (json!({"allOf":[{"type":"number"},{"minimum":2}]}), json!(3), json!(1)),
            (json!({"oneOf":[{"type":"number"},{"enum":[2]}]}), json!(3), json!(2)),
        ] {
            assert!(validate(&schema, &accepted, "test.argument").is_ok(), "{schema}");
            assert!(validate(&schema, &refused, "test.argument").is_err(), "{schema}");
        }
    }

    #[test]
    fn nullable_types_keep_the_literal_enum_constraint() {
        let schema = json!({"type":["string","null"],"enum":["steel"]});
        assert!(validate(&schema, &json!("steel"), "material").is_ok());
        assert!(validate(&schema, &Value::Null, "material").is_err());
        assert!(validate(&json!({"type":["number","null"]}), &Value::Null, "number").is_ok());
        assert!(validate(&json!({"type":["number","null"]}), &json!(3.5), "number").is_ok());
        assert!(validate(&json!({"type":["number","null"]}), &json!("3.5"), "number").is_err());
    }
}
