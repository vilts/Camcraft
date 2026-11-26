//! Native AI chat dock panel, background worker, and offline/safety policies (#1946).
//! Provenance: our own code over our own shell, app MCP inbox, and egui; no vendor material involved.

pub(crate) mod budget;
mod panel;
pub(crate) mod settings;
#[cfg(not(target_arch = "wasm32"))]
// Structural tests use a fake backend, so they never read the real OS keychain.
#[cfg_attr(test, allow(dead_code))]
pub(crate) mod settings_native;
mod settings_panel;
pub(crate) mod settings_text;
mod state;
mod transport;
#[cfg(not(target_arch = "wasm32"))]
mod transport_http;
mod turn;
mod worker;
#[cfg(not(target_arch = "wasm32"))]
mod worker_mcp;

pub(crate) use panel::draw;
pub(crate) use state::AiChatState;
pub(crate) use transport::EndpointKind;
pub(crate) use worker::WakeFn;
#[cfg(test)]
pub(crate) use {
    camcraft_app::mcp::provider::FallbackMode,
    transport::{ChatTransport, TransportOutput, TransportRequest},
    turn::TurnStatus,
    worker::ToolCardStatus,
};

#[cfg(all(test, not(target_arch = "wasm32")))]
mod transport_http_tests;
#[cfg(all(test, not(target_arch = "wasm32")))]
mod transport_discovery_tests;
