//! Background thread turn loop, confirmation wait, and Inbox::channel MCP dispatch (#1946).
//! Provenance: our own std::thread + mpsc worker over camcraft-app's Inbox/provider helpers; no vendor material involved.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use camcraft_app::mcp::Request;
use camcraft_app::mcp::provider::{FallbackMode, PendingCall, tool_result};
use serde_json::{Value, json};

use super::transport::{
    ChatTransport, ProviderPreset, TransportOutput, TransportRequest, TurnMessage,
    build_tools_payload, check_offline_policy, extract_assistant_text, requires_confirmation,
};
#[cfg(not(target_arch = "wasm32"))]
use super::worker_mcp::{ConfirmOutcome, dispatch_mcp_call, parse_step_calls, wait_confirmation};

const MAX_TURN_STEPS: usize = 12;
pub(super) const POLL_STEP: Duration = Duration::from_millis(20);

/// Event-loop wake callback invoked whenever worker events or MCP requests are queued.
pub(crate) type WakeFn = Arc<dyn Fn() + Send + Sync>;

/// Execution status rendered on each inline tool card inside a chat turn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolCardStatus {
    Pending,
    AwaitingConfirmation,
    Running,
    Succeeded,
    Failed,
    Denied,
}

impl ToolCardStatus {
    /// Short user-facing status label for the card header.
    #[must_use]
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Pending => "Pending",
            Self::AwaitingConfirmation => "Awaiting confirmation",
            Self::Running => "Running",
            Self::Succeeded => "Succeeded",
            Self::Failed => "Failed",
            Self::Denied => "Denied",
        }
    }
}

/// Visual record of a single tool invocation proposed or run within a turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToolCard {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub status: ToolCardStatus,
    pub summary: String,
}

/// Worker request pausing execution until the user clicks Allow or Deny inline.
#[derive(Clone, Debug)]
pub(crate) struct ConfirmRequest {
    pub tool_name: String,
    pub arguments: Value,
    pub decision_tx: mpsc::Sender<bool>,
}

/// Outcome reported when a worker finishes a submitted turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TurnFinish {
    Completed,
    Cancelled,
    Failed(String),
}

/// Asynchronous messages streamed from the background worker to the UI thread.
#[derive(Debug)]
pub(crate) enum WorkerEvent {
    AssistantText { turn_id: u64, text: String },
    ToolCardAdded { turn_id: u64, card: ToolCard },
    ToolCardUpdated { turn_id: u64, id: String, status: ToolCardStatus, summary: String },
    ConfirmNeeded(ConfirmRequest),
    Finished { turn_id: u64, status: TurnFinish },
}

/// Immutable turn parameters handed to the worker thread upon Send.
pub(crate) struct TurnInput {
    pub turn_id: u64,
    pub preset: ProviderPreset,
    pub model: String,
    pub offline_only: bool,
    pub initial_messages: Vec<TurnMessage>,
    pub project_snapshot: Value,
    pub transport: Arc<dyn ChatTransport>,
    pub mcp_tx: mpsc::Sender<Request>,
    pub wake: Option<WakeFn>,
}

/// UI-owned handle that drains worker events and joins cleanly on Cancel or Drop.
pub(crate) struct WorkerHandle {
    pub cancel: Arc<AtomicBool>,
    pub rx: mpsc::Receiver<WorkerEvent>,
    #[cfg(not(target_arch = "wasm32"))]
    join: Option<std::thread::JoinHandle<()>>,
}

impl Drop for WorkerHandle {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(handle) = self.join.take() {
            let _ = handle.join();
        }
    }
}

pub(super) fn send_event(
    tx: &mpsc::Sender<WorkerEvent>,
    wake: &Option<WakeFn>,
    event: WorkerEvent,
) {
    let _ = tx.send(event);
    notify_wake(wake);
}

pub(super) fn notify_wake(wake: &Option<WakeFn>) {
    if let Some(cb) = wake {
        cb();
    }
}

/// Launch a background turn worker on native targets using `std::thread`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn spawn_turn(input: TurnInput) -> WorkerHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    let worker_cancel = Arc::clone(&cancel);
    let join = match std::thread::Builder::new()
        .name("camcraft-ai-chat".to_owned())
        .spawn(move || run_turn_loop(input, worker_cancel, tx))
    {
        Ok(h) => Some(h),
        Err(err) => {
            let _ = rx;
            // Channel is still open locally only through rx; spawning failure is unreachable in tests.
            let _ = err;
            None
        }
    };
    WorkerHandle { cancel, rx, join }
}

/// WebAssembly build stub returning an immediate unsupported refusal without spawning threads.
#[cfg(target_arch = "wasm32")]
pub(crate) fn spawn_turn(input: TurnInput) -> WorkerHandle {
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();
    send_event(
        &tx,
        &input.wake,
        WorkerEvent::Finished {
            turn_id: input.turn_id,
            status: TurnFinish::Failed(
                "background AI chat worker is unavailable in WebAssembly builds".to_owned(),
            ),
        },
    );
    WorkerHandle { cancel, rx }
}

#[cfg(not(target_arch = "wasm32"))]
fn run_turn_loop(input: TurnInput, cancel: Arc<AtomicBool>, tx: mpsc::Sender<WorkerEvent>) {
    let finish = match run_turn_steps(&input, &cancel, &tx) {
        Ok(cancelled) => {
            if cancelled {
                TurnFinish::Cancelled
            } else {
                TurnFinish::Completed
            }
        }
        Err(err) => {
            if cancel.load(Ordering::Relaxed) {
                TurnFinish::Cancelled
            } else {
                TurnFinish::Failed(err)
            }
        }
    };
    send_event(
        &tx,
        &input.wake,
        WorkerEvent::Finished {
            turn_id: input.turn_id,
            status: finish,
        },
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn run_turn_steps(
    input: &TurnInput,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<WorkerEvent>,
) -> Result<bool, String> {
    check_offline_policy(
        input.offline_only,
        input.preset.endpoint_kind,
        &input.preset.label,
    )?;
    let mut messages = input.initial_messages.clone();
    let mut rpc_seq = 1u64;

    for step in 0..MAX_TURN_STEPS {
        if cancel.load(Ordering::Relaxed) {
            return Ok(true);
        }
        let configured_mode = input.preset.fallback_mode;
        let tools_payload =
            build_tools_payload(input.preset.provider, configured_mode, &input.project_snapshot);
        let req = TransportRequest {
            preset: input.preset.clone(),
            model: input.model.clone(),
            offline_only: input.offline_only,
            fallback_mode: configured_mode,
            tools_payload,
            messages: messages.clone(),
        };
        let TransportOutput::AssistantResponse {
            text,
            envelope,
            fallback_override,
        } = input.transport.complete(&req, cancel)?;
        if cancel.load(Ordering::Relaxed) {
            return Ok(true);
        }

        let effective_mode = fallback_override.unwrap_or(configured_mode);
        let reply_text = extract_assistant_text(input.preset.provider, &envelope, &text);

        // Guard: ReadOnly mode must never parse, prompt for, or dispatch any tool calls.
        if configured_mode == FallbackMode::ReadOnly || effective_mode == FallbackMode::ReadOnly {
            if !reply_text.is_empty() {
                send_event(
                    tx,
                    &input.wake,
                    WorkerEvent::AssistantText {
                        turn_id: input.turn_id,
                        text: reply_text,
                    },
                );
            }
            return Ok(false);
        }

        let parsed = parse_step_calls(input.preset.provider, effective_mode, &text, &envelope)?;
        if !reply_text.is_empty() {
            send_event(
                tx,
                &input.wake,
                WorkerEvent::AssistantText {
                    turn_id: input.turn_id,
                    text: reply_text.clone(),
                },
            );
        }
        messages.push(TurnMessage {
            role: "assistant".to_owned(),
            text: reply_text,
            selection: None,
            screenshot: None,
            raw: if envelope.is_null() {
                None
            } else {
                Some(envelope.clone())
            },
        });
        if parsed.is_empty() {
            return Ok(false);
        }

        for (idx, (call, pending_call)) in parsed.into_iter().enumerate() {
            let card_id = if call.id.is_empty() {
                format!("step-{step}-call-{idx}")
            } else {
                call.id.clone()
            };
            send_event(
                tx,
                &input.wake,
                WorkerEvent::ToolCardAdded {
                    turn_id: input.turn_id,
                    card: ToolCard {
                        id: card_id.clone(),
                        name: call.name.clone(),
                        arguments: call.arguments.clone(),
                        status: ToolCardStatus::Pending,
                        summary: String::new(),
                    },
                },
            );

            let needs_confirm = pending_call
                .as_ref()
                .map_or_else(|| requires_confirmation(&call.name, effective_mode), PendingCall::requires_confirmation);
            if needs_confirm {
                match wait_confirmation(input, &card_id, &call, cancel, tx)? {
                    ConfirmOutcome::Cancelled => return Ok(true),
                    ConfirmOutcome::Denied => {
                        let refusal = json!({
                            "content": [{ "type": "text", "text": format!("User denied tool '{}'.", call.name) }],
                            "isError": true
                        });
                        if let Some(p) = pending_call {
                            let _ = p.authorize(false);
                        }
                        send_event(
                            tx,
                            &input.wake,
                            WorkerEvent::ToolCardUpdated {
                                turn_id: input.turn_id,
                                id: card_id,
                                status: ToolCardStatus::Denied,
                                summary: "Denied by user".to_owned(),
                            },
                        );
                        let raw = tool_result(input.preset.provider, &call, &refusal);
                        messages.push(TurnMessage {
                            role: "tool".to_owned(),
                            text: format!("User denied tool '{}'.", call.name),
                            selection: None,
                            screenshot: None,
                            raw: Some(raw),
                        });
                        continue;
                    }
                    ConfirmOutcome::Allowed => {
                        if let Some(p) = pending_call {
                            p.authorize(true)?;
                        }
                    }
                }
            }

            let cancelled_mid_call =
                dispatch_mcp_call(input, &card_id, &call, &mut rpc_seq, cancel, tx, &mut messages)?;
            if cancelled_mid_call {
                return Ok(true);
            }
        }
    }
    Err(format!(
        "stopped after {MAX_TURN_STEPS} tool steps without a final answer"
    ))
}
