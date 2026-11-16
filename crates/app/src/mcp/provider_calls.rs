//! Translate provider call and result envelopes without executing a tool.
//! Provenance: public Anthropic tool_use/tool_result, OpenAI function calling and Gemini functionCall/functionResponse formats.

use super::{provider::Provider, schema_validation};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

/// Parse a complete, non-streaming response. Select the first completion/candidate.
/// The caller retains the original envelope for reasoning items and thought signatures.
/// This function validates arguments but does not dispatch or grant permission.
pub fn parse_calls(provider: Provider, response: &Value) -> Result<Vec<ToolCall>, String> {
    if !response.is_object() || response.get("error").is_some() {
        return Err("provider response must be an object without an API error".into());
    }
    let blocks = match provider {
        Provider::Anthropic => array(&response["content"], "content")?,
        Provider::OpenAiResponses => array(&response["output"], "output")?,
        Provider::OpenAiChat => array(&response["choices"][0]["message"]["tool_calls"], "tool_calls")?,
        Provider::Gemini => array(&response["candidates"][0]["content"]["parts"], "parts")?,
    };
    let mut calls: Vec<ToolCall> = Vec::new();
    for block in blocks {
        let (raw, id_key, args_key, encoded) = match provider {
            Provider::Anthropic if block["type"] == "tool_use" => (block, "id", "input", false),
            Provider::OpenAiResponses if block["type"] == "function_call" => (block, "call_id", "arguments", true),
            Provider::OpenAiChat => {
                if block["type"] != "function" {
                    return Err("tool_calls.type must be function".into());
                }
                (&block["function"], "id", "arguments", true)
            }
            Provider::Gemini if block.get("functionCall").is_some() => (&block["functionCall"], "id", "args", false),
            _ => continue,
        };
        let name = string(raw, "name")?;
        let id_source = if provider == Provider::OpenAiChat { block } else { raw };
        let id = if provider == Provider::Gemini && id_source.get(id_key).is_none() {
            String::new()
        } else {
            string(id_source, id_key)?.to_owned()
        };
        let arguments = if encoded {
            serde_json::from_str(string(raw, args_key)?).map_err(|e| format!("{name}.{args_key}: {e}"))?
        } else if provider == Provider::Gemini {
            raw.get(args_key).cloned().unwrap_or_else(|| json!({}))
        } else {
            raw.get(args_key)
                .cloned()
                .ok_or_else(|| format!("{name}.{args_key} is required"))?
        };
        schema_validation::arguments(name, &arguments)?;
        if !id.is_empty() && calls.iter().any(|c| c.id == id) {
            return Err(format!("duplicate tool call id '{id}'"));
        }
        calls.push(ToolCall {
            id,
            name: name.to_owned(),
            arguments,
        });
    }
    Ok(calls)
}

fn array<'a>(value: &'a Value, field: &str) -> Result<&'a [Value], String> {
    match value {
        Value::Null => Ok(&[]),
        Value::Array(rows) => Ok(rows),
        _ => Err(format!("provider {field} must be an array")),
    }
}

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str, String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("provider {field} must be a nonempty string"))
}

/// A result block (Anthropic), message (Chat), input item (Responses) or part
/// (Gemini). Pass the MCP result including `isError`; never erase a refusal.
#[must_use]
pub fn tool_result(provider: Provider, call: &ToolCall, result: &Value) -> Value {
    let content = result.to_string();
    match provider {
        Provider::Anthropic => json!({"type":"tool_result","tool_use_id":call.id,
            "content":content,"is_error":result["isError"].as_bool().unwrap_or(false)}),
        Provider::OpenAiChat => json!({"role":"tool","tool_call_id":call.id,"content":content}),
        Provider::OpenAiResponses => json!({"type":"function_call_output","call_id":call.id,"output":content}),
        Provider::Gemini => {
            let mut response = json!({"name":call.name,"response":result});
            if !call.id.is_empty() {
                response["id"] = json!(call.id);
            }
            json!({"functionResponse":response})
        }
    }
}
