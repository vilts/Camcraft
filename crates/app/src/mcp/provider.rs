//! Provider-neutral schemas for the live MCP tool table (#1943).
//! Provenance: public Anthropic Messages, OpenAI function calling and Gemini FunctionDeclaration documentation.

use super::tools as registry;
use serde_json::{Value, json};

pub use super::fallback::{FallbackMode, PendingCall, parse_prompt, prompt_context, qa_context};
pub use super::provider_calls::{ToolCall, parse_calls, tool_result};

/// Select the wire format by endpoint, including Responses-only models.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Anthropic,
    OpenAiChat,
    OpenAiResponses,
    Gemini,
}

/// The `tools` request field. Read the registry on every request, without a cache.
/// OpenAI stays non-strict: MCP has optional fields and typed additional properties.
#[must_use]
pub fn tools(provider: Provider) -> Vec<Value> {
    let rows: Vec<Value> = registry::list()
        .into_iter()
        .map(|tool| {
            let name = &tool["name"];
            let description = &tool["description"];
            let schema = &tool["inputSchema"];
            match provider {
                Provider::Anthropic => json!({"name":name,"description":description,"input_schema":schema}),
                Provider::OpenAiChat => json!({"type":"function","function":{
                    "name":name,"description":description,"parameters":schema,"strict":false
                }}),
                Provider::OpenAiResponses => json!({"type":"function","name":name,
                "description":description,"parameters":schema,"strict":false}),
                Provider::Gemini => json!({"name":name,"description":description,
                "parametersJsonSchema":gemini_schema(schema)}),
            }
        })
        .collect();
    if provider == Provider::Gemini {
        vec![json!({"functionDeclarations":rows})]
    } else {
        rows
    }
}

/// Walk schema locations only: names and literal enum/default values are data.
fn gemini_schema(schema: &Value) -> Value {
    let Value::Object(source) = schema else {
        return schema.clone();
    };
    let mut out = source.clone();
    out.remove("$schema");
    if let Some(Value::Array(types)) = out.remove("type") {
        // Keep sibling constraints; JSON Schema applies them only to relevant types.
        let union = json!({"anyOf":types.iter().map(|t| json!({"type":t})).collect::<Vec<_>>()});
        if out.contains_key("anyOf") {
            let mut all = out
                .remove("allOf")
                .and_then(|v| v.as_array().cloned())
                .unwrap_or_default();
            all.push(union);
            out.insert("allOf".into(), json!(all));
        } else {
            out.insert("anyOf".into(), union["anyOf"].clone());
        }
    } else if let Some(kind) = source.get("type") {
        out.insert("type".into(), kind.clone());
    }
    for key in ["properties", "$defs", "definitions", "patternProperties"] {
        if let Some(Value::Object(properties)) = out.get_mut(key) {
            for value in properties.values_mut() {
                *value = gemini_schema(value);
            }
        }
    }
    for key in ["items", "additionalProperties", "not", "contains", "if", "then", "else"] {
        if let Some(value) = out.get_mut(key) {
            *value = gemini_schema(value);
        }
    }
    for key in ["anyOf", "oneOf", "allOf", "prefixItems"] {
        if let Some(Value::Array(rows)) = out.get_mut(key) {
            for row in rows {
                *row = gemini_schema(row);
            }
        }
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemini_normalization_strips_schema_keywords_and_preserves_literal_data() {
        let literal = json!({"$schema":"literal","type":["literal","values"]});
        let schema = json!({"$schema":"https://json-schema.org/draft/2020-12/schema",
            "type":"object","properties":{
                "$schema":{"type":"string","default":"literal"},
                "choice":{"$schema":"nested","type":["object","null"],"enum":[literal],"default":literal}
            },"additionalProperties":{"$schema":"nested","type":["number","null"]}});
        let mapped = gemini_schema(&schema);
        assert!(mapped.get("$schema").is_none());
        assert_eq!(mapped["properties"]["$schema"], schema["properties"]["$schema"]);
        let choice = &mapped["properties"]["choice"];
        assert!(choice.get("$schema").is_none());
        assert!(choice.get("type").is_none());
        assert_eq!(choice["anyOf"], json!([{"type":"object"},{"type":"null"}]));
        assert_eq!(choice["enum"], json!([literal]));
        assert_eq!(choice["default"], literal);
        assert_eq!(mapped["additionalProperties"], json!({"anyOf":[{"type":"number"},{"type":"null"}]}));
    }

    #[test]
    fn gemini_type_union_preserves_existing_combinators() {
        let schema = json!({"type":["string","null"],"anyOf":[{"enum":["a",null]}],"allOf":[{"description":"existing"}]});
        let mapped = gemini_schema(&schema);
        assert_eq!(mapped["anyOf"], schema["anyOf"]);
        assert_eq!(mapped["allOf"], json!([
            {"description":"existing"},{"anyOf":[{"type":"string"},{"type":"null"}]}
        ]));
        assert!(mapped.get("type").is_none());
    }
}
