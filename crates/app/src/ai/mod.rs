//! AI provider transport (#1945): where a chat step may go (endpoint and
//! offline law), what it looks like on the wire, where the API key comes
//! from, and the native HTTP client that sends it.
//!
//! The browser build links none of the socket or keychain code: `http`
//! and `keychain` exist only off `wasm32`, the rest is plain data.
//!
//! Provenance: our own policy code over the public Anthropic Messages,
//! OpenAI Chat Completions and Responses, Google Gemini `generateContent`
//! and Ollama `/api/show` API documentation.

pub mod endpoint;
pub mod presets;
pub mod probe;
pub mod secret;
pub mod wire;

#[cfg(not(target_arch = "wasm32"))]
pub mod http;
#[cfg(not(target_arch = "wasm32"))]
pub mod keychain;

pub use endpoint::{Endpoint, Scheme, admit, is_loopback_host};
pub use presets::{ENDPOINTS, ProviderEndpoint, by_id};
pub use secret::{ApiKey, KeyName, KeySource};
pub use wire::{ChatMessage, ChatRequest, Image, WireRequest};
