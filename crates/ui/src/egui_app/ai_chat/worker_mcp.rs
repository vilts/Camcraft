//! MCP proposal parsing, confirmation handshake, and Inbox::channel RPC dispatch (#1946).
//! Provenance: our own code over camcraft-app's Inbox and provider parsers; no vendor material involved.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

use camcraft_app::mcp::Request;
use camcraft_app::mcp::provider::{
    FallbackMode, PendingCall, Provider, ToolCall, parse_calls, parse_prompt, tool_result,
};
use serde_json::{Value, json};

use super::transport::TurnMessage;
use super::worker::{
    ConfirmRequest, POLL_STEP, ToolCardStatus, TurnInput, WorkerEvent, notify_wake, send_event,
};

/// Result of pausing on an inline user confirmation banner.
pub(super) enum ConfirmOutcome {
    Allowed,
    Denied,
    Cancelled,
}

/// Extract proposed tool calls from one provider step respecting `FallbackMode`.
pub(super) fn parse_step_calls(
    provider: Provider,
    mode: FallbackMode,
    text: &str,
    envelope: &Value,
) -> Result<Vec<(ToolCall, Option<PendingCall>)>, String> {
    match mode {
        FallbackMode::ReadOnly => Ok(Vec::new()),
        FallbackMode::Native => {
            if !envelope.is_object() {
                return Ok(Vec::new());
            }
            let calls = parse_calls(provider, envelope)?;
            Ok(calls.into_iter().map(|c| (c, None)).collect())
        }
        FallbackMode::PromptJson => {
            let trimmed = text.trim();
            if !(trimmed.starts_with('{') || trimmed.starts_with("```json")) {
                return Ok(Vec::new());
            }
            let pending = parse_prompt(trimmed)?;
            let call = pending.proposal().clone();
            Ok(vec![(call, Some(pending))])
        }
    }
}

/// Ask the UI for Allow/Deny on a sensitive tool call and poll until answered or cancelled.
pub(super) fn wait_confirmation(
    input: &TurnInput,
    card_id: &str,
    call: &ToolCall,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<WorkerEvent>,
) -> Result<ConfirmOutcome, String> {
    send_event(
        tx,
        &input.wake,
        WorkerEvent::ToolCardUpdated {
            turn_id: input.turn_id,
            id: card_id.to_owned(),
            status: ToolCardStatus::AwaitingConfirmation,
            summary: format!("Waiting for Allow/Deny ({})", call.name),
        },
    );
    let (decision_tx, decision_rx) = mpsc::channel();
    send_event(
        tx,
        &input.wake,
        WorkerEvent::ConfirmNeeded(ConfirmRequest {
            tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
            decision_tx,
        }),
    );
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(ConfirmOutcome::Cancelled);
        }
        match decision_rx.recv_timeout(POLL_STEP) {
            Ok(true) => return Ok(ConfirmOutcome::Allowed),
            Ok(false) => return Ok(ConfirmOutcome::Denied),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(ConfirmOutcome::Cancelled),
        }
    }
}

/// Send one JSON-RPC `tools/call` through the in-app MCP inbox and append the tool result.
pub(super) fn dispatch_mcp_call(
    input: &TurnInput,
    card_id: &str,
    call: &ToolCall,
    rpc_seq: &mut u64,
    cancel: &AtomicBool,
    tx: &mpsc::Sender<WorkerEvent>,
    messages: &mut Vec<TurnMessage>,
) -> Result<bool, String> {
    send_event(
        tx,
        &input.wake,
        WorkerEvent::ToolCardUpdated {
            turn_id: input.turn_id,
            id: card_id.to_owned(),
            status: ToolCardStatus::Running,
            summary: format!("Running {}...", call.name),
        },
    );
    let id = *rpc_seq;
    *rpc_seq += 1;
    let line = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": "tools/call",
        "params": {
            "name": call.name,
            "arguments": call.arguments,
        }
    })
    .to_string();
    let (req, reply_rx) = Request::pair(line);
    input
        .mcp_tx
        .send(req)
        .map_err(|_| "in-process MCP inbox is closed".to_owned())?;
    notify_wake(&input.wake);

    let reply_line = loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(true);
        }
        match reply_rx.recv_timeout(POLL_STEP) {
            Ok(Some(line)) => break line,
            Ok(None) => return Err(format!("tool '{}' returned no JSON-RPC reply", call.name)),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(format!("MCP reply channel dropped for '{}'", call.name));
            }
        }
    };

    let (result_val, is_error, summary) = parse_mcp_reply(&reply_line);
    let status = if is_error {
        ToolCardStatus::Failed
    } else {
        ToolCardStatus::Succeeded
    };
    send_event(
        tx,
        &input.wake,
        WorkerEvent::ToolCardUpdated {
            turn_id: input.turn_id,
            id: card_id.to_owned(),
            status,
            summary: summary.clone(),
        },
    );
    // #1944: the model gets at most the reply cap, with a truncation hint.
    let (result_val, summary) = super::budget::cap_tool_reply(result_val, summary);
    let raw = tool_result(input.preset.provider, call, &result_val);
    messages.push(TurnMessage {
        role: "tool".to_owned(),
        text: summary,
        selection: None,
        screenshot: None,
        raw: Some(raw),
    });
    Ok(false)
}

/// Extract the tool result JSON, error flag, and concise card summary from a JSON-RPC reply line.
fn parse_mcp_reply(line: &str) -> (Value, bool, String) {
    let parsed: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(err) => {
            let msg = format!("malformed MCP reply: {err}");
            return (
                json!({ "content": [{ "type": "text", "text": msg }], "isError": true }),
                true,
                msg,
            );
        }
    };
    if let Some(err) = parsed.get("error") {
        let msg = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("JSON-RPC error")
            .to_owned();
        return (
            json!({ "content": [{ "type": "text", "text": msg }], "isError": true }),
            true,
            msg,
        );
    }
    let result = parsed.get("result").cloned().unwrap_or(Value::Null);
    let is_error = result
        .get("isError")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let text = result
        .get("content")
        .and_then(|c| c.get(0))
        .and_then(|b| b.get("text"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let summary = if text.is_empty() {
        result.to_string()
    } else {
        text
    };
    (result, is_error, summary)
}
