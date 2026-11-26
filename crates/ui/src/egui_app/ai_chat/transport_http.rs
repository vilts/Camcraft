//! The live provider transport (#1945): sends a chat step through
//! `camcraft_app::ai::http` to the endpoint behind the selected preset,
//! with the key from the environment or the OS keychain. Native only.
//! A local Ollama preset in native-tools mode asks the model's
//! capabilities first and refuses by name when it cannot call tools.
//!
//! Provenance: our own glue over camcraft-app's provider transport; no
//! vendor material involved.

use std::sync::atomic::{AtomicBool, Ordering};

use camcraft_app::ai::http::{Client, discover_ollama_cancellable, probe_ollama_cancellable, send_chat};
use camcraft_app::ai::wire::{ChatMessage, ChatRequest, DEFAULT_MAX_TOKENS, Image, split_payload};
use camcraft_app::ai::{Endpoint, ProviderEndpoint, by_id, keychain, secret};
use camcraft_app::mcp::provider::FallbackMode;

use super::transport::{
    ChatTransport, TransportOutput, TransportRequest, TurnMessage, check_offline_policy,
    extract_assistant_text,
};

/// Sends each step over HTTP(S); holds no state between steps.
pub(crate) struct HttpTransport;

/// The app endpoint behind a panel preset: the cloud ids match, every
/// local preset is an Ollama model (the panel's local runtime today).
fn endpoint_for(preset_id: &str) -> Option<&'static ProviderEndpoint> {
    by_id(preset_id).or_else(|| {
        preset_id
            .starts_with("local-")
            .then(|| by_id("ollama"))
            .flatten()
    })
}

/// The panel's message, with an opted-in selection folded into the text.
fn to_wire(m: &TurnMessage) -> ChatMessage {
    let mut text = m.text.clone();
    if let Some(sel) = &m.selection {
        text.push_str(&format!(
            "\n\n[Selection] entities: {:?}; active setup: {:?}; operation: {:?}",
            sel.selected_entities, sel.active_setup, sel.selected_operation
        ));
    }
    let image = m.screenshot.as_ref().map(|s| Image {
        mime_type: s.mime_type.clone(),
        data_base64: s.data_base64.clone(),
    });
    ChatMessage {
        role: m.role.clone(),
        text,
        image,
        raw: m.raw.clone(),
    }
}

impl ChatTransport for HttpTransport {
    fn complete(
        &self,
        req: &TransportRequest,
        cancel: &AtomicBool,
    ) -> Result<TransportOutput, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_owned());
        }
        check_offline_policy(
            req.offline_only,
            req.preset.endpoint_kind,
            &req.preset.label,
        )?;
        let def = endpoint_for(&req.preset.id)
            .ok_or_else(|| format!("no endpoint is known for '{}'", req.preset.label))?;
        let ep = Endpoint::parse(def.base_url)?;
        let key = match def.key {
            None => None,
            Some(name) => {
                let found = secret::lookup(name, |v| std::env::var(v).ok(), keychain::load)?;
                let (key, _) = found.ok_or_else(|| {
                    format!(
                        "no {} API key: set {} or store one in the OS keychain",
                        name.label(),
                        name.env_var()
                    )
                })?;
                Some(key)
            }
        };
        self.complete_at(req, cancel, &ep, key.as_ref())
    }
}

impl HttpTransport {
    pub(super) fn complete_at(
        &self,
        req: &TransportRequest,
        cancel: &AtomicBool,
        ep: &Endpoint,
        key: Option<&camcraft_app::ai::ApiKey>,
    ) -> Result<TransportOutput, String> {
        let client = Client::new(req.offline_only);
        let ollama = endpoint_for(&req.preset.id).is_some_and(|def| def.ollama_probe);
        let automatic = ollama && req.model.trim().is_empty();
        let model = if automatic {
            discover_ollama_cancellable(&client, ep, cancel)?
        } else if req.model.trim().is_empty()
            && req.preset.endpoint_kind == super::EndpointKind::Cloud
        {
            camcraft_app::ai::presets::default_model(req.preset.provider).to_owned()
        } else {
            req.model.clone()
        };
        if ollama && !automatic
            && req.fallback_mode == FallbackMode::Native
        {
            let caps = probe_ollama_cancellable(&client, ep, &model, cancel)?;
            if !caps.tools {
                return Err(format!(
                    "model '{}' cannot call tools (Ollama lists no 'tools' capability) - \
                     choose 'Local Prompt-JSON' or 'Local Q&A (Read-Only)' for it",
                    model
                ));
            }
        }
        let (tools, system) = split_payload(&req.tools_payload);
        let messages: Vec<ChatMessage> = req.messages.iter().map(to_wire).collect();
        let chat = ChatRequest {
            provider: req.preset.provider,
            model: &model,
            system,
            messages: &messages,
            tools: &tools,
            max_tokens: DEFAULT_MAX_TOKENS,
        };
        // The person did not type the automatic model: a failure names it.
        let envelope = send_chat(&client, ep, &chat, key, cancel).map_err(|e| {
            if automatic && !cancel.load(Ordering::Relaxed) {
                format!("automatic model '{model}': {e}")
            } else {
                e
            }
        })?;
        let text = extract_assistant_text(req.preset.provider, &envelope, "");
        Ok(TransportOutput::AssistantResponse {
            text,
            envelope,
            fallback_override: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_preset_has_an_endpoint() {
        for p in super::super::transport::default_presets() {
            let e = endpoint_for(&p.id).unwrap_or_else(|| panic!("{} has no endpoint", p.id));
            assert_eq!(e.provider, p.provider, "{}", p.id);
        }
    }
}
