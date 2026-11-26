//! One chat step on the wire (#1945): the request path and JSON body for
//! each provider, from a provider-neutral message list. Tool-call and
//! tool-result envelopes (`raw`) are echoed in the provider's own shape,
//! so reasoning items and thought signatures survive a tool round trip.
//!
//! Provenance: the public request formats of the Anthropic Messages API,
//! OpenAI Chat Completions and Responses APIs and the Gemini
//! `generateContent` API.

use serde_json::{Value, json};

use crate::mcp::provider::Provider;

/// An inline image (a viewport screenshot), base64 encoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub mime_type: String,
    pub data_base64: String,
}

/// One message: `role` is `user`, `assistant` or `tool`. `raw` is the
/// provider envelope of an assistant step or a `tool_result` item.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatMessage {
    pub role: String,
    pub text: String,
    pub image: Option<Image>,
    pub raw: Option<Value>,
}

/// What one step asks of the model.
#[derive(Clone, Debug)]
pub struct ChatRequest<'a> {
    pub provider: Provider,
    pub model: &'a str,
    pub system: Option<String>,
    pub messages: &'a [ChatMessage],
    /// Native tool declarations, already in the provider's shape
    /// (`mcp::provider::tools`); empty sends none.
    pub tools: &'a [Value],
    pub max_tokens: u32,
}

/// The path under the endpoint base, and the body. Never holds a key.
#[derive(Clone, Debug, PartialEq)]
pub struct WireRequest {
    pub path: String,
    pub body: Value,
    /// Fixed, non-secret headers (Anthropic's API version).
    pub headers: Vec<(String, String)>,
}

/// The default answer budget (Anthropic requires one).
pub const DEFAULT_MAX_TOKENS: u32 = 4096;

/// A model id goes into a URL path for Gemini: letters, digits and
/// `.-_:/` only, so it can never add a query or climb the path.
pub fn check_model(model: &str) -> Result<(), String> {
    let ok = !model.is_empty()
        && model.len() <= 200
        && !model.contains("..")
        && model.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '/'));
    if ok { Ok(()) } else { Err(format!("model id '{model}' is not a valid model name")) }
}

/// Split the chat panel's tools payload: `{"tools":[...]}` is native
/// tools; a fallback context (`{"instructions", ...}`) goes into the
/// system prompt as JSON, with no tools sent.
#[must_use]
pub fn split_payload(payload: &Value) -> (Vec<Value>, Option<String>) {
    if payload.get("instructions").is_some() {
        let instructions = payload["instructions"].as_str().unwrap_or_default();
        return (Vec::new(), Some(format!("{instructions}\n\nContext (JSON):\n{payload}")));
    }
    let tools = payload.get("tools").and_then(Value::as_array).cloned().unwrap_or_default();
    (tools, None)
}

/// Build the request for `req.provider`.
pub fn build(req: &ChatRequest<'_>) -> Result<WireRequest, String> {
    check_model(req.model)?;
    let mut headers = Vec::new();
    let (path, body) = match req.provider {
        Provider::Anthropic => {
            headers.push(("anthropic-version".to_owned(), "2023-06-01".to_owned()));
            ("/messages".to_owned(), anthropic(req))
        }
        Provider::OpenAiChat => ("/chat/completions".to_owned(), openai_chat(req)),
        Provider::OpenAiResponses => ("/responses".to_owned(), openai_responses(req)),
        Provider::Gemini => {
            if req.model.contains('/') {
                return Err(format!("Gemini model id '{}' must not hold '/'", req.model));
            }
            (format!("/models/{}:generateContent", req.model), gemini(req))
        }
    };
    Ok(WireRequest { path, body, headers })
}

fn data_url(img: &Image) -> String {
    format!("data:{};base64,{}", img.mime_type, img.data_base64)
}

/// Push `item` into the trailing message of `role` when there is one,
/// else open a new one. Anthropic wants every tool result of a step in
/// one user message; Gemini every function response in one turn.
fn push_merged(out: &mut Vec<Value>, role: &str, key: &str, item: Value) {
    if let Some(last) = out.last_mut()
        && last["role"] == role
        && last.get("__merge").is_some()
        && let Some(items) = last[key].as_array_mut()
    {
        items.push(item);
        return;
    }
    out.push(json!({"role": role, key: [item], "__merge": true}));
}

fn strip_merge_marks(out: &mut [Value]) {
    for m in out {
        if let Some(o) = m.as_object_mut() {
            o.remove("__merge");
        }
    }
}

fn anthropic(req: &ChatRequest<'_>) -> Value {
    let mut msgs: Vec<Value> = Vec::new();
    for m in req.messages {
        match (m.role.as_str(), &m.raw) {
            ("tool", Some(raw)) => push_merged(&mut msgs, "user", "content", raw.clone()),
            ("assistant", Some(raw)) if raw["content"].is_array() => {
                msgs.push(json!({"role":"assistant","content":raw["content"]}));
            }
            ("assistant", _) if !m.text.is_empty() => msgs.push(json!({"role":"assistant","content":m.text})),
            ("user", _) => {
                let mut content = vec![json!({"type":"text","text":m.text})];
                if let Some(img) = &m.image {
                    content.push(json!({"type":"image","source":{
                        "type":"base64","media_type":img.mime_type,"data":img.data_base64}}));
                }
                msgs.push(json!({"role":"user","content":content}));
            }
            _ => {}
        }
    }
    strip_merge_marks(&mut msgs);
    let mut body = json!({"model":req.model,"max_tokens":req.max_tokens,"messages":msgs});
    if let Some(s) = &req.system {
        body["system"] = json!(s);
    }
    if !req.tools.is_empty() {
        body["tools"] = json!(req.tools);
    }
    body
}

fn openai_chat(req: &ChatRequest<'_>) -> Value {
    let mut msgs: Vec<Value> = Vec::new();
    if let Some(s) = &req.system {
        msgs.push(json!({"role":"system","content":s}));
    }
    for m in req.messages {
        match (m.role.as_str(), &m.raw) {
            ("tool", Some(raw)) => msgs.push(raw.clone()),
            ("assistant", Some(raw)) if raw["choices"][0]["message"].is_object() => {
                msgs.push(raw["choices"][0]["message"].clone());
            }
            ("assistant", _) if !m.text.is_empty() => msgs.push(json!({"role":"assistant","content":m.text})),
            ("user", _) => match &m.image {
                None => msgs.push(json!({"role":"user","content":m.text})),
                Some(img) => msgs.push(json!({"role":"user","content":[
                    {"type":"text","text":m.text},
                    {"type":"image_url","image_url":{"url":data_url(img)}}]})),
            },
            _ => {}
        }
    }
    let mut body = json!({"model":req.model,"messages":msgs,"max_tokens":req.max_tokens});
    if !req.tools.is_empty() {
        body["tools"] = json!(req.tools);
    }
    body
}

fn openai_responses(req: &ChatRequest<'_>) -> Value {
    let mut input: Vec<Value> = Vec::new();
    for m in req.messages {
        match (m.role.as_str(), &m.raw) {
            ("tool", Some(raw)) => input.push(raw.clone()),
            ("assistant", Some(raw)) if raw["output"].is_array() => {
                input.extend(raw["output"].as_array().into_iter().flatten().cloned());
            }
            ("assistant", _) if !m.text.is_empty() => input.push(json!({"role":"assistant","content":m.text})),
            ("user", _) => {
                let mut content = vec![json!({"type":"input_text","text":m.text})];
                if let Some(img) = &m.image {
                    content.push(json!({"type":"input_image","image_url":data_url(img)}));
                }
                input.push(json!({"role":"user","content":content}));
            }
            _ => {}
        }
    }
    let mut body = json!({"model":req.model,"input":input,"max_output_tokens":req.max_tokens});
    if let Some(s) = &req.system {
        body["instructions"] = json!(s);
    }
    if !req.tools.is_empty() {
        body["tools"] = json!(req.tools);
    }
    body
}

fn gemini(req: &ChatRequest<'_>) -> Value {
    let mut contents: Vec<Value> = Vec::new();
    for m in req.messages {
        match (m.role.as_str(), &m.raw) {
            ("tool", Some(raw)) => push_merged(&mut contents, "user", "parts", raw.clone()),
            ("assistant", Some(raw)) if raw["candidates"][0]["content"].is_object() => {
                let mut c = raw["candidates"][0]["content"].clone();
                c["role"] = json!("model");
                contents.push(c);
            }
            ("assistant", _) if !m.text.is_empty() => {
                contents.push(json!({"role":"model","parts":[{"text":m.text}]}));
            }
            ("user", _) => {
                let mut parts = vec![json!({"text":m.text})];
                if let Some(img) = &m.image {
                    parts.push(json!({"inlineData":{"mimeType":img.mime_type,"data":img.data_base64}}));
                }
                contents.push(json!({"role":"user","parts":parts}));
            }
            _ => {}
        }
    }
    strip_merge_marks(&mut contents);
    let mut body = json!({"contents":contents,"generationConfig":{"maxOutputTokens":req.max_tokens}});
    if let Some(s) = &req.system {
        body["systemInstruction"] = json!({"parts":[{"text":s}]});
    }
    if !req.tools.is_empty() {
        body["tools"] = json!(req.tools);
    }
    body
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_cannot_escape_the_path() {
        for ok in ["gemini-2.5-flash", "qwen2.5:14b", "models/x", "gpt-4.1"] {
            assert!(check_model(ok).is_ok(), "{ok}");
        }
        for bad in ["", "a?key=1", "../../x", "a b", "a#b", "a%2f"] {
            assert!(check_model(bad).is_err(), "{bad}");
        }
    }
}
