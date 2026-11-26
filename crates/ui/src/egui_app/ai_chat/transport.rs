//! Provider transport abstraction, offline/confirmation guards, and envelope helpers (#1946).
//! Provenance: our own code wrapping camcraft-app's provider schemas and fallback helpers; no vendor material involved.

#[cfg(test)]
use std::io::Cursor;
use std::sync::atomic::AtomicBool;
#[cfg(target_arch = "wasm32")]
use std::sync::atomic::Ordering;

use camcraft_app::mcp::provider::{
    CORE_TOOLS, FallbackMode, Provider, ToolSet, is_read_only_tool, prompt_context, qa_context, tools_in,
};
use camcraft_app::mcp::requires_approval;
use serde_json::{Value, json};

/// Whether a preset targets a remote cloud API or a local/loopback model runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EndpointKind {
    Cloud,
    Local,
}

/// Static configuration for one selectable AI provider/endpoint option in the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProviderPreset {
    pub id: String,
    pub label: String,
    pub provider: Provider,
    pub endpoint_kind: EndpointKind,
    pub default_model: String,
    pub fallback_mode: FallbackMode,
}

/// Optional snapshot of currently selected document entities/operation sent only on opt-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SelectionSnapshot {
    pub selected_entities: Vec<String>,
    pub active_setup: Option<String>,
    pub selected_operation: Option<String>,
}

/// Optional PNG viewport screenshot attachment sent only on opt-in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ScreenshotAttachment {
    pub mime_type: String,
    pub data_base64: String,
}

/// One message entry carried in a provider request across tool steps.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TurnMessage {
    pub role: String,
    pub text: String,
    pub selection: Option<SelectionSnapshot>,
    pub screenshot: Option<ScreenshotAttachment>,
    pub raw: Option<Value>,
}

/// Complete request passed to a [`ChatTransport`] for one model completion step.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransportRequest {
    pub preset: ProviderPreset,
    pub model: String,
    pub offline_only: bool,
    pub fallback_mode: FallbackMode,
    pub tools_payload: Value,
    pub messages: Vec<TurnMessage>,
}

/// Normalized response from a [`ChatTransport`].
// Constructed by scripted test backends and the upcoming live HTTP provider (#1945).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TransportOutput {
    AssistantResponse {
        text: String,
        envelope: Value,
        fallback_override: Option<FallbackMode>,
    },
}

/// Boundary between the background chat worker and network/scripted transports.
pub(crate) trait ChatTransport: Send + Sync {
    fn complete(
        &self,
        req: &TransportRequest,
        cancel: &AtomicBool,
    ) -> Result<TransportOutput, String>;
}

/// Offline-safe transport for the browser build, which links no sockets;
/// native builds use `transport_http::HttpTransport` (#1945).
#[cfg(target_arch = "wasm32")]
pub(crate) struct DefaultTransport;

#[cfg(target_arch = "wasm32")]
impl ChatTransport for DefaultTransport {
    fn complete(
        &self,
        req: &TransportRequest,
        cancel: &AtomicBool,
    ) -> Result<TransportOutput, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_owned());
        }
        check_offline_policy(req.offline_only, req.preset.endpoint_kind, &req.preset.label)?;
        Err(format!(
            "no live transport endpoint is configured for '{}'",
            req.preset.label
        ))
    }
}

/// Built-in provider presets offered in the AI Chat panel's Provider selector.
#[must_use]
pub(crate) fn default_presets() -> Vec<ProviderPreset> {
    vec![
        ProviderPreset {
            id: "anthropic".to_owned(),
            label: "Anthropic (Cloud)".to_owned(),
            provider: Provider::Anthropic,
            endpoint_kind: EndpointKind::Cloud,
            default_model: camcraft_app::ai::presets::default_model(Provider::Anthropic).to_owned(),
            fallback_mode: FallbackMode::Native,
        },
        ProviderPreset {
            id: "openai-responses".to_owned(),
            label: "OpenAI Responses (Cloud)".to_owned(),
            provider: Provider::OpenAiResponses,
            endpoint_kind: EndpointKind::Cloud,
            default_model: camcraft_app::ai::presets::default_model(Provider::OpenAiResponses).to_owned(),
            fallback_mode: FallbackMode::Native,
        },
        ProviderPreset {
            id: "openai-chat".to_owned(),
            label: "OpenAI Chat (Cloud)".to_owned(),
            provider: Provider::OpenAiChat,
            endpoint_kind: EndpointKind::Cloud,
            default_model: camcraft_app::ai::presets::default_model(Provider::OpenAiChat).to_owned(),
            fallback_mode: FallbackMode::Native,
        },
        ProviderPreset {
            id: "gemini".to_owned(),
            label: "Gemini (Cloud)".to_owned(),
            provider: Provider::Gemini,
            endpoint_kind: EndpointKind::Cloud,
            default_model: camcraft_app::ai::presets::default_model(Provider::Gemini).to_owned(),
            fallback_mode: FallbackMode::Native,
        },
        ProviderPreset {
            id: "local-ollama".to_owned(),
            label: "Offline local (Ollama)".to_owned(),
            provider: Provider::OpenAiChat,
            endpoint_kind: EndpointKind::Local,
            default_model: String::new(),
            fallback_mode: FallbackMode::Native,
        },
        ProviderPreset {
            id: "local-prompt-json".to_owned(),
            label: "Local Prompt-JSON".to_owned(),
            provider: Provider::OpenAiChat,
            endpoint_kind: EndpointKind::Local,
            default_model: String::new(),
            fallback_mode: FallbackMode::PromptJson,
        },
        ProviderPreset {
            id: "local-readonly".to_owned(),
            label: "Local Q&A (Read-Only)".to_owned(),
            provider: Provider::OpenAiChat,
            endpoint_kind: EndpointKind::Local,
            default_model: String::new(),
            fallback_mode: FallbackMode::ReadOnly,
        },
    ]
}

/// Guard that refuses any cloud preset when offline-only mode is active.
pub(crate) fn check_offline_policy(
    offline_only: bool,
    endpoint: EndpointKind,
    preset_label: &str,
) -> Result<(), String> {
    if offline_only && endpoint == EndpointKind::Cloud {
        return Err(format!(
            "offline-only mode blocks cloud provider '{preset_label}'"
        ));
    }
    Ok(())
}

/// Guard deciding whether a tool invocation must wait for explicit inline Allow/Deny.
#[must_use]
pub(crate) fn requires_confirmation(
    tool_name: &str,
    arguments: &Value,
    mode: FallbackMode,
) -> bool {
    match mode {
        FallbackMode::ReadOnly => true,
        FallbackMode::PromptJson => !is_read_only_tool(tool_name),
        FallbackMode::Native => {
            tool_name.starts_with("delete_")
                || tool_name == "open_project"
                || requires_approval(tool_name, arguments).is_some()
        }
    }
}

/// Build the provider/fallback tool schema payload sent alongside chat messages.
/// `set` picks the whole tool table or the core subset (#1944).
#[must_use]
pub(crate) fn build_tools_payload(provider: Provider, mode: FallbackMode, set: ToolSet, project: &Value) -> Value {
    match mode {
        FallbackMode::Native => json!({ "tools": tools_in(provider, set) }),
        FallbackMode::PromptJson => {
            let mut context = prompt_context(project);
            if set == ToolSet::Core && let Some(tools) = context["tools"].as_array_mut() {
                tools.retain(|tool| tool["name"].as_str().is_some_and(|name| CORE_TOOLS.contains(&name)));
            }
            context
        }
        FallbackMode::ReadOnly => qa_context(project),
    }
}

/// Extract readable assistant prose from an explicit string or provider response envelope.
#[must_use]
pub(crate) fn extract_assistant_text(
    provider: Provider,
    envelope: &Value,
    explicit_text: &str,
) -> String {
    if !explicit_text.is_empty() {
        return explicit_text.to_owned();
    }
    let mut parts = Vec::new();
    match provider {
        Provider::Anthropic => {
            if let Some(items) = envelope.get("content").and_then(Value::as_array) {
                for item in items {
                    if item.get("type").and_then(Value::as_str) == Some("text")
                        && let Some(t) = item.get("text").and_then(Value::as_str)
                    {
                        parts.push(t);
                    }
                }
            }
        }
        Provider::OpenAiChat => {
            if let Some(msg) = envelope
                .get("choices")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("message"))
                .and_then(|m| m.get("content"))
                .and_then(Value::as_str)
            {
                parts.push(msg);
            }
        }
        Provider::OpenAiResponses => {
            if let Some(t) = envelope.get("output_text").and_then(Value::as_str) {
                parts.push(t);
            } else if let Some(items) = envelope.get("output").and_then(Value::as_array) {
                for item in items {
                    if let Some(content) = item.get("content").and_then(Value::as_array) {
                        for block in content {
                            if let Some(t) = block.get("text").and_then(Value::as_str) {
                                parts.push(t);
                            }
                        }
                    }
                }
            }
        }
        Provider::Gemini => {
            if let Some(items) = envelope
                .get("candidates")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("content"))
                .and_then(|c| c.get("parts"))
                .and_then(Value::as_array)
            {
                for item in items {
                    if let Some(t) = item.get("text").and_then(Value::as_str) {
                        parts.push(t);
                    }
                }
            }
        }
    }
    parts.join("\n")
}

/// Encode RGBA pixels into an in-memory PNG byte buffer without unwrap/expect.
#[cfg(test)]
pub(crate) fn encode_png_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Vec<u8>, String> {
    let img = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "screenshot buffer size did not match image dimensions".to_owned())?;
    let mut out = Vec::new();
    img.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .map_err(|e| format!("failed to encode PNG screenshot: {e}"))?;
    Ok(out)
}

/// Standard RFC 4648 base64 encoder for inline PNG attachments.
#[must_use]
pub(crate) fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = usize::from(chunk[0]);
        let b1 = usize::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = usize::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(TABLE[(triple >> 18) & 0x3f]));
        out.push(char::from(TABLE[(triple >> 12) & 0x3f]));
        out.push(if chunk.len() > 1 {
            char::from(TABLE[(triple >> 6) & 0x3f])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(TABLE[triple & 0x3f])
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_confirmation_covers_every_published_delete_tool() {
        let deletes: Vec<_> = camcraft_app::mcp::catalog::tool_names().into_iter()
            .filter(|name| name.starts_with("delete_")).collect();
        assert!(!deletes.is_empty(), "the published delete tools must be covered");
        for name in deletes {
            assert!(requires_confirmation(name, &json!({}), FallbackMode::Native),
                "native {name} must wait for Allow/Deny");
        }
        for name in ["get_job", "add_operation"] {
            assert!(!requires_confirmation(name, &json!({}), FallbackMode::Native),
                "ordinary {name} must retain its native policy");
        }
    }

    #[test]
    fn offline_policy_refuses_cloud_and_allows_local() {
        assert!(check_offline_policy(true, EndpointKind::Cloud, "Anthropic").is_err());
        assert!(check_offline_policy(true, EndpointKind::Local, "Ollama").is_ok());
        assert!(check_offline_policy(false, EndpointKind::Cloud, "Anthropic").is_ok());
    }

    #[test]
    fn base64_and_png_round_trip() {
        assert_eq!(encode_base64(b"hi!"), "aGkh");
        let png = encode_png_rgba(2, 2, vec![40; 16]).expect("valid png");
        assert!(png.starts_with(b"\x89PNG"));
        assert!(!encode_base64(&png).is_empty());
    }
}
