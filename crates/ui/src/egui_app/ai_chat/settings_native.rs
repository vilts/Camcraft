//! Provenance: our keychain/HTTP APIs and provider documentation; issue #3781.

use std::sync::atomic::AtomicBool;
use std::time::Duration;

use camcraft_app::ai::http::{Client, Timeouts, probe_ollama, send_chat};
use camcraft_app::ai::wire::{ChatMessage, ChatRequest};
use camcraft_app::ai::{ApiKey, Endpoint, KeySource, by_id, keychain, secret};

use super::settings::{SettingsAction, SettingsBackend, SettingsRequest};
use super::settings_text::{NO_KEY, ollama_status, test_failure};

pub(crate) struct NativeSettings;

impl SettingsBackend for NativeSettings {
    fn run(&self, request: &SettingsRequest, action: SettingsAction) -> Result<String, String> {
        let endpoint = by_id(&request.preset_id)
            .or_else(|| {
                request
                    .preset_id
                    .starts_with("local-")
                    .then(|| by_id("ollama"))
                    .flatten()
            })
            .ok_or("This provider has no configured endpoint.")?;
        let ep = Endpoint::parse(endpoint.base_url)?;
        let client = Client {
            offline_only: request.offline_only,
            timeouts: Timeouts {
                connect: Duration::from_secs(3),
                total: Duration::from_secs(15),
            },
        };
        let Some(name) = endpoint.key else {
            // An empty Model still tells if the server answers: a probe of an
            // unknown name gives HTTP 404 from a running server.
            let probe_name = if request.model.is_empty() {
                "camcraft-readiness-check"
            } else {
                request.model.as_str()
            };
            let probe = probe_ollama(&client, &ep, probe_name).map(|_| ());
            let test = matches!(action, SettingsAction::Test);
            return ollama_status(probe, &request.model, test);
        };
        match action {
            SettingsAction::Save(ref key) => keychain::store(name, key)
                .map_err(|_| "The key could not be saved. Unlock the OS keychain and try again.")?,
            SettingsAction::Remove => keychain::forget(name).map_err(
                |_| "The key could not be removed. Unlock the OS keychain and try again.",
            )?,
            SettingsAction::Check | SettingsAction::Test => {}
        }
        let found = secret::lookup(name, |v| std::env::var(v).ok(), keychain::load)
            .map_err(|_| "The key could not be read. Check the environment variable or unlock the OS keychain.")?;
        let Some((key, source)) = found else {
            return Ok(NO_KEY.into());
        };
        let source = match source {
            KeySource::Environment => "environment; overrides the keychain",
            KeySource::Keychain => "keychain",
        };
        if matches!(action, SettingsAction::Test) {
            if request.model.is_empty() {
                return Err(
                    "Model is empty and this provider has no default. Type a model name.".into(),
                );
            }
            cloud_test(&client, &ep, endpoint.provider, &request.model, &key)?;
            return Ok(format!(
                "Ready (key from {source}) - Test passed (model {})",
                request.model
            ));
        }
        Ok(format!("Ready (key from {source})"))
    }
}

/// One cloud Test request. A failure gives the panel text: the cause, the model,
/// the endpoint and the provider details (message, type, code, request path).
pub(crate) fn cloud_test(
    client: &Client,
    ep: &Endpoint,
    provider: camcraft_app::mcp::provider::Provider,
    model: &str,
    key: &ApiKey,
) -> Result<(), String> {
    test_key(client, ep, provider, model, key)
        .map_err(|error| test_failure(&error, model, &ep.url("")))
}

fn test_key(
    client: &Client,
    ep: &Endpoint,
    provider: camcraft_app::mcp::provider::Provider,
    model: &str,
    key: &ApiKey,
) -> Result<(), String> {
    let messages = [ChatMessage {
        role: "user".into(),
        text: "Reply OK.".into(),
        image: None,
        raw: None,
    }];
    let request = ChatRequest {
        provider,
        model,
        system: Some("This is a connection test. Do not call tools.".into()),
        messages: &messages,
        tools: &[],
        max_tokens: 16,
    };
    // `http.rs` removes the key from the error text before it returns.
    send_chat(client, ep, &request, Some(key), &AtomicBool::new(false)).map(|_| ())
}
