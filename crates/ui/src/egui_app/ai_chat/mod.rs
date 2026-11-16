//! Native AI chat dock panel, background worker, and offline/safety policies (#1946).
//! Provenance: our own code over our own shell, app MCP inbox, and egui; no vendor material involved.

mod panel;
mod state;
mod transport;
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
    worker::ToolCardStatus,
};
