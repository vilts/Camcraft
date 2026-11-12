//! `camcraft-cli mcp` (#128): a Model Context Protocol server on stdio,
//! so an AI agent can drive a headless CamCraft document a step at a
//! time - import, set up, add and tune operations, generate, verify,
//! post - and read every answer back as JSON.
//!
//! Provenance: the wire is JSON-RPC 2.0 (jsonrpc.org/specification) as
//! the public MCP specification (modelcontextprotocol.io, revision
//! 2025-06-18) frames it: newline-delimited messages on stdin/stdout,
//! the `initialize` handshake, `tools/list` and `tools/call`. Our own
//! code over serde_json; no MCP SDK. Clean-room, public sources only.
//!
//! The seam laws hold here exactly as in the script host (script/mod.rs):
//! the server owns one document and one command History, every world
//! write a tool makes rides a real command (so `undo` / `redo` walk it),
//! and every failure is an MCP tool error (`isError: true`) naming the
//! reason - never a panic, never a silent success.

mod align_tools;
mod area;
mod bridge;
// #93 slice 2: a milling tool's drawn cutter outline.
mod cutter_tools;
mod edit;
mod features;
mod fixture_tools;
mod holders;
mod job;
mod listen;
mod live;
mod model_tools;
// #45: an operation's display name and its place in its setup.
mod order_tools;
mod pick;
mod pockets;
mod render;
mod setup_tools;
mod tools;
mod vise_tools;
mod accessory_tools;
mod plate_tools;
mod modvise_tools;
mod workholding_tools;

// #183: the same tool table served live from inside the GUI, over a
// loopback socket, against the app's own document and History.
pub use bridge::bridge;
pub use listen::{port_from, Inbox, Request, Wake, IDLE_TIMEOUT};
pub use live::{answer_live, LiveAnswer};

use crate::script::Env;

use serde_json::{json, Value};
use std::io::{BufRead, Read, Write};

/// The protocol revisions this server speaks, newest first. A client
/// asking for one of them gets it echoed; anything else gets the newest
/// (the spec's negotiation rule - the client decides whether to stay).
pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

// JSON-RPC 2.0's reserved error codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// One MCP session: one headless document and its history. In-process
/// testable - `handle` is the whole protocol, `serve` only moves lines.
pub struct Server {
    env: Env,
}

impl Default for Server {
    fn default() -> Self {
        Self::new()
    }
}

impl Server {
    #[must_use]
    pub fn new() -> Self {
        Self { env: Env::fresh() }
    }

    /// One wire line in, at most one wire line out (notifications get
    /// no answer). Unparseable text is a JSON-RPC parse error, not a
    /// crash: the session goes on reading.
    pub fn handle_line(&mut self, line: &str) -> Option<String> {
        let text = line.trim();
        if text.is_empty() {
            return None;
        }
        let reply = match serde_json::from_str::<Value>(text) {
            Ok(msg) => self.handle(&msg),
            Err(e) => Some(rpc_error(Value::Null, PARSE_ERROR, &format!("parse error: {e}"))),
        };
        reply.map(|v| v.to_string())
    }

    /// One JSON-RPC message. Requests (with an `id`) get a response;
    /// notifications (no `id`) get none, as the spec requires.
    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let Value::Object(obj) = msg else {
            // Batches left the protocol at 2025-06-18; a bare array or
            // scalar is an invalid request, answered once.
            return Some(rpc_error(Value::Null, INVALID_REQUEST, "a single JSON-RPC object was expected"));
        };
        let id = obj.get("id").cloned();
        if id == Some(Value::Null) {
            // MCP: a request id is a string or a number, never null.
            return Some(rpc_error(Value::Null, INVALID_REQUEST, "\"id\" must be a string or a number, not null"));
        }
        if obj.get("jsonrpc") != Some(&json!("2.0")) {
            return id.map(|id| rpc_error(id, INVALID_REQUEST, "\"jsonrpc\" must be \"2.0\""));
        }
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // A response from the client (we send no requests) or junk.
            return id.map(|id| rpc_error(id, INVALID_REQUEST, "a \"method\" string is required"));
        };
        let params = obj.get("params").cloned().unwrap_or(Value::Null);
        let Some(id) = id else {
            // Notifications: `notifications/initialized`, cancellations.
            // Nothing to answer, nothing to do for a synchronous server.
            return None;
        };
        let answer = match method {
            "initialize" => Ok(initialize(&params)),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tools::list() })),
            "tools/call" => self.call(&params),
            other => Err((METHOD_NOT_FOUND, format!("method not found: {other}"))),
        };
        Some(match answer {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, msg)) => rpc_error(id, code, &msg),
        })
    }

    /// `tools/call`: malformed call params are a protocol error; anything
    /// the tool itself refuses - unknown tool name included - is a tool
    /// result with `isError`, which is what an agent can read and act on.
    fn call(&mut self, params: &Value) -> Result<Value, (i64, String)> {
        let Some(name) = params.get("name").and_then(Value::as_str) else {
            return Err((INVALID_PARAMS, "tools/call needs a \"name\" string".into()));
        };
        let args = match params.get("arguments") {
            None | Some(Value::Null) => json!({}),
            Some(v @ Value::Object(_)) => v.clone(),
            Some(_) => return Err((INVALID_PARAMS, "\"arguments\" must be an object".into())),
        };
        // Backstop only: no tool path panics by law, but a bug in a
        // generator must still come back as a refusal, not kill the
        // session the agent is driving.
        let env = &mut self.env;
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tools::call(env, name, &args)));
        let outcome = match run {
            Ok(r) => r,
            Err(_) => Err(format!("{name}: internal error - the tool stopped unexpectedly (a bug; please report it)")),
        };
        Ok(match outcome {
            Ok(value) => tool_ok(value),
            Err(msg) => tool_err(&msg),
        })
    }
}

fn initialize(params: &Value) -> Value {
    let asked = params.get("protocolVersion").and_then(Value::as_str);
    let version = asked
        .and_then(|a| PROTOCOL_VERSIONS.iter().find(|v| **v == a))
        .copied()
        .unwrap_or(PROTOCOL_VERSIONS[0]);
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": { "name": "camcraft", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "CamCraft headless CAM document. Typical flow: import_model, set_stock, \
add_tool, add_operation, generate, verify, post. Every change is undoable (undo / redo); \
get_job shows the whole document; list_operation_types shows each kind's parameters.",
    })
}

/// A tool answer: the JSON as text (every client reads that) and as
/// `structuredContent` (clients that read data take it from there).
/// A picture (#182) rides the answer under `render::PNG_KEY` and is
/// lifted out into an image content block after the text, so the JSON
/// copies never carry the base64 twice.
fn tool_ok(value: Value) -> Value {
    let mut structured = match value {
        Value::Object(_) => value,
        other => json!({ "result": other }),
    };
    let png = structured.as_object_mut().and_then(|m| m.remove(render::PNG_KEY));
    let text = serde_json::to_string_pretty(&structured).unwrap_or_default();
    let mut content = vec![json!({ "type": "text", "text": text })];
    if let Some(Value::String(data)) = png {
        content.push(json!({ "type": "image", "data": data, "mimeType": "image/png" }));
    }
    json!({
        "content": content,
        "structuredContent": structured,
        "isError": false,
    })
}

fn tool_err(msg: &str) -> Value {
    json!({ "content": [{ "type": "text", "text": msg }], "isError": true })
}

fn rpc_error(id: Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

/// The longest wire line the server reads, in bytes (16 MiB). A longer
/// line is answered with a parse error and skipped to its newline
/// without being held in memory - one runaway message must not take
/// the session (or the machine) down with it.
pub const MAX_LINE_BYTES: usize = 16 * 1024 * 1024;

/// The stdio loop: one message per line in, one per line out, flushed
/// at once (the client waits on it). Ends cleanly at end of input.
/// A line that is not UTF-8, or longer than [`MAX_LINE_BYTES`], is a
/// JSON-RPC parse error for that line; the loop reads on. Only an I/O
/// failure of the streams themselves ends it early.
/// Diagnostics never go to stdout - that stream is the protocol's.
pub fn serve(mut input: impl BufRead, mut output: impl Write) -> std::io::Result<()> {
    let mut server = Server::new();
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let cap = u64::try_from(MAX_LINE_BYTES).unwrap_or(u64::MAX).saturating_add(1);
        let n = (&mut input).take(cap).read_until(b'\n', &mut buf)?;
        if n == 0 {
            return Ok(());
        }
        let reply = if !buf.ends_with(b"\n") && buf.len() > MAX_LINE_BYTES {
            skip_line(&mut input)?;
            Some(parse_error(&format!("line longer than {MAX_LINE_BYTES} bytes")))
        } else {
            match std::str::from_utf8(&buf) {
                Ok(line) => server.handle_line(line),
                Err(e) => Some(parse_error(&format!("the line is not UTF-8: {e}"))),
            }
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
}

fn parse_error(msg: &str) -> String {
    rpc_error(Value::Null, PARSE_ERROR, &format!("parse error: {msg}")).to_string()
}

/// Drop input up to and including the next newline (or to the end),
/// one buffer at a time - nothing of the skipped line is kept.
fn skip_line(input: &mut impl BufRead) -> std::io::Result<()> {
    loop {
        let chunk = input.fill_buf()?;
        if chunk.is_empty() {
            return Ok(());
        }
        if let Some(i) = chunk.iter().position(|&b| b == b'\n') {
            input.consume(i.saturating_add(1));
            return Ok(());
        }
        let len = chunk.len();
        input.consume(len);
    }
}
