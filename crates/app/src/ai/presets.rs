//! The provider endpoints CamCraft knows (#1945): three public cloud APIs
//! over https and four OpenAI-compatible model runtimes on loopback.
//!
//! Provenance: base URLs and paths from each provider's public API
//! documentation (Anthropic Messages, OpenAI Chat Completions and
//! Responses, Google Gemini `generateContent`), and the documented default
//! ports of Ollama, LM Studio and llama.cpp's `llama-server`; Unsloth
//! Studio's from issue #1945.

use crate::mcp::provider::Provider;

use super::secret::KeyName;

/// Cloud defaults verified against the providers' model catalogs on 2026-11-24.
/// Mark a row retired when its provider retires it; the catalog test then fails.
/// Sources: https://platform.claude.com/docs/en/models/overview,
/// https://developers.openai.com/api/docs/models,
/// https://aistudio.google.com/docs/models.
#[derive(Clone, Copy, Debug)]
pub struct CloudDefault {
    pub provider: Provider,
    pub model: &'static str,
    pub retired: bool,
}

pub const CLOUD_DEFAULTS: &[CloudDefault] = &[
    CloudDefault { provider: Provider::Anthropic, model: "claude-sonnet-5-5", retired: false },
    CloudDefault { provider: Provider::OpenAiChat, model: "gpt-6.2-sol", retired: false },
    CloudDefault { provider: Provider::OpenAiResponses, model: "gpt-6.2-sol", retired: false },
    CloudDefault { provider: Provider::Gemini, model: "gemini-4-hydrogen", retired: false },
];

/// The one maintained default for this cloud wire format.
#[must_use]
pub fn default_model(provider: Provider) -> &'static str {
    CLOUD_DEFAULTS.iter().find(|entry| entry.provider == provider)
        .map_or("", |entry| entry.model)
}

/// One selectable endpoint: its wire format, default base URL, and the
/// key it needs (cloud) or the capability probe it offers (Ollama).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderEndpoint {
    pub id: &'static str,
    pub label: &'static str,
    pub provider: Provider,
    /// The default base URL; a user may point a local runtime elsewhere.
    pub base_url: &'static str,
    /// The API key this endpoint needs; `None` for a local runtime.
    pub key: Option<KeyName>,
    /// Ollama's native `/api/show` probe answers "does this model call
    /// tools / see images" (beside the `/v1` OpenAI-compatible base).
    pub ollama_probe: bool,
}

/// Every endpoint, cloud first.
pub const ENDPOINTS: &[ProviderEndpoint] = &[
    ProviderEndpoint {
        id: "anthropic",
        label: "Anthropic Messages",
        provider: Provider::Anthropic,
        base_url: "https://api.anthropic.com/v1",
        key: Some(KeyName::Anthropic),
        ollama_probe: false,
    },
    ProviderEndpoint {
        id: "openai-chat",
        label: "OpenAI Chat Completions",
        provider: Provider::OpenAiChat,
        base_url: "https://api.openai.com/v1",
        key: Some(KeyName::OpenAi),
        ollama_probe: false,
    },
    ProviderEndpoint {
        id: "openai-responses",
        label: "OpenAI Responses (GPT, Codex)",
        provider: Provider::OpenAiResponses,
        base_url: "https://api.openai.com/v1",
        key: Some(KeyName::OpenAi),
        ollama_probe: false,
    },
    ProviderEndpoint {
        id: "gemini",
        label: "Google Gemini",
        provider: Provider::Gemini,
        base_url: "https://generativelanguage.googleapis.com/v1beta",
        key: Some(KeyName::Gemini),
        ollama_probe: false,
    },
    ProviderEndpoint {
        id: "ollama",
        label: "Ollama (local)",
        provider: Provider::OpenAiChat,
        base_url: "http://localhost:11434/v1",
        key: None,
        ollama_probe: true,
    },
    ProviderEndpoint {
        id: "unsloth-studio",
        label: "Unsloth Studio (local)",
        provider: Provider::OpenAiChat,
        base_url: "http://127.0.0.1:8888/v1",
        key: None,
        ollama_probe: false,
    },
    ProviderEndpoint {
        id: "lm-studio",
        label: "LM Studio (local)",
        provider: Provider::OpenAiChat,
        base_url: "http://localhost:1234/v1",
        key: None,
        ollama_probe: false,
    },
    ProviderEndpoint {
        id: "llama-server",
        label: "llama.cpp llama-server (local, start with --jinja for tools)",
        provider: Provider::OpenAiChat,
        base_url: "http://127.0.0.1:8080/v1",
        key: None,
        ollama_probe: false,
    },
];

/// Look an endpoint up by its id.
#[must_use]
pub fn by_id(id: &str) -> Option<&'static ProviderEndpoint> {
    ENDPOINTS.iter().find(|e| e.id == id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::endpoint::{Endpoint, Scheme};

    #[test]
    fn cloud_is_https_with_a_key_and_local_is_loopback_without() {
        for e in ENDPOINTS {
            let ep = Endpoint::parse(e.base_url).expect("preset parses");
            if e.key.is_some() {
                assert_eq!(ep.scheme, Scheme::Https, "{}", e.id);
                assert!(!ep.is_loopback(), "{}", e.id);
            } else {
                assert!(ep.is_loopback(), "{}", e.id);
            }
        }
    }
}
