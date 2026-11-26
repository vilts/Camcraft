//! #3035: the stdio server (`camcraft-cli mcp`) runs a `generate` on a
//! worker thread. The session answers every other call (`get_job`,
//! `save_project`, ...) while the worker runs, and a client can drop the
//! generate with `notifications/cancelled`.
//!
//! Provenance: MCP specification revision 2025-06-18, "Cancellation"
//! (`notifications/cancelled` with `params.requestId`); the world-read /
//! pure-finish split is our own #2257 `StagedGeneration` seam, landed
//! with the same `still_current` law as the GUI's generation worker.
//!
//! The world stays on the session's thread. A worker gets only the staged
//! job, which reads no world. Its result lands through the History, as
//! the synchronous tool does, only if the operation and its cached path
//! are still what they were when the job was staged. A generator has no
//! cancel point: a cancelled worker runs to its end, and its result is
//! dropped.

use std::collections::VecDeque;
use std::io::{BufRead, Read, Write};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use super::{tool_err, tool_ok, Server, MAX_LINE_BYTES};
use crate::commands::{CommandError, GenerateOperation, StagedGeneration};
use crate::model;
use crate::operation::OpStatus;
use crate::script::api;

/// One message for the session's own thread.
pub(super) enum Event {
    /// One wire line, or the parse-error text of a line that is not one.
    Line(Result<String, String>),
    /// The input ended.
    Eof,
    /// Reading the input failed.
    Failed(std::io::Error),
    /// A worker finished the job of `ticket`.
    Done { ticket: u64, result: Box<Result<GenerateOperation, CommandError>> },
}

/// A test hook: each worker waits for one message before it runs.
pub(super) type Gate = Arc<Mutex<Receiver<()>>>;

/// The serve loop's state for background generation.
pub(super) struct Background {
    events: SyncSender<Event>,
    running: Option<Running>,
    next_ticket: u64,
}

/// The one `generate` call the session waits on.
struct Running {
    id: Value,
    ticket: u64,
    current: String,
    queue: VecDeque<String>,
    unnamed: bool,
    done: Vec<Value>,
    cancelled: bool,
}

impl Server {
    fn generating(&self) -> bool {
        self.bg.as_ref().is_some_and(|b| b.running.as_ref().is_some_and(|r| !r.cancelled))
    }

    fn on_event(&mut self, ev: Event) -> Option<String> {
        match ev {
            Event::Line(Ok(line)) => self.on_line(&line),
            Event::Line(Err(msg)) => Some(super::parse_error(&msg)),
            Event::Done { ticket, result } => self.on_done(ticket, *result).map(|v| v.to_string()),
            Event::Eof | Event::Failed(_) => None,
        }
    }

    fn on_line(&mut self, line: &str) -> Option<String> {
        let Ok(msg) = serde_json::from_str::<Value>(line.trim()) else {
            return self.handle_line(line);
        };
        if let Some(id) = cancelled_request(&msg) {
            self.cancel(&id);
            return None;
        }
        if let Some((id, names)) = generate_call(&msg) {
            return self.start(id, names).map(|v| v.to_string());
        }
        // `open_project` replaces the world that the worker's result lands in.
        if let Some(id) = tool_call_id(&msg, "open_project")
            && let Some(name) = self.running_name()
        {
            let why = format!("open_project: refused while generate runs ({name}) - wait for its worker to finish");
            return Some(answer(&id, tool_err(&why)).to_string());
        }
        self.handle_line(line)
    }

    fn running_name(&self) -> Option<String> {
        self.bg.as_ref().and_then(|b| b.running.as_ref()).map(|r| r.current.clone())
    }

    /// Keep a cancelled worker in the single-worker budget until it finishes.
    /// MCP: no answer goes out for a cancelled request.
    fn cancel(&mut self, id: &Value) {
        let Some(bg) = self.bg.as_mut() else { return };
        if let Some(run) = bg.running.as_mut().filter(|r| r.id == *id) {
            run.cancelled = true;
            run.queue.clear();
            tracing::info!(op = %run.current, "mcp: generate cancelled by the client; its result will be dropped");
        }
    }

    fn start(&mut self, id: Value, names: Vec<Value>) -> Option<Value> {
        if let Some(name) = self.running_name() {
            let why = format!("generate: another generate is running ({name}) - wait for its worker to finish");
            return Some(answer(&id, tool_err(&why)));
        }
        let queue = match api::generate_targets(&self.env, &names) {
            Ok(t) => t.into_iter().map(|(name, _)| name).collect(),
            Err(e) => return Some(answer(&id, tool_err(&format!("generate: {e}")))),
        };
        let unnamed = names.is_empty();
        self.advance(Running { id, ticket: 0, current: String::new(), queue, unnamed, done: Vec::new(), cancelled: false })
    }

    /// Stage the next operation and hand it to a worker; `None` while a
    /// worker runs. The answer when the queue is empty or a stage refuses.
    fn advance(&mut self, mut run: Running) -> Option<Value> {
        loop {
            let Some(name) = run.queue.pop_front() else {
                return Some(answer(&run.id, tool_ok(json!({ "generated": run.done }))));
            };
            // Other calls can delete or re-create operations while a worker runs.
            // Retain names only and resolve the current entity at each use.
            let Some(entity) = model::find_by_name(&self.env.world, &name) else {
                continue;
            };
            if run.unnamed && crate::operation::status_of(&self.env.world, &entity) == OpStatus::Ok {
                continue;
            }
            let staged = match StagedGeneration::new(&self.env.world, &name) {
                Ok(s) => s,
                Err(e) => return Some(refused(&run.id, &name, &e)),
            };
            run.current = name;
            let Some(bg) = self.bg.as_mut() else {
                return Some(answer(&run.id, tool_err("generate: the session has no background lane")));
            };
            bg.next_ticket = bg.next_ticket.wrapping_add(1);
            let ticket = bg.next_ticket;
            match spawn(staged, ticket, bg.events.clone(), self.gate.clone()) {
                Ok(()) => {
                    run.ticket = ticket;
                    bg.running = Some(run);
                    return None;
                }
                // No thread: run the job here, as the synchronous tool does.
                Err(Some(staged)) => {
                    if let Err(reply) = self.land(&mut run, staged.finish(), false) {
                        return Some(reply);
                    }
                }
                Err(None) => {
                    let why = format!("generate: {} - the worker could not start", run.current);
                    return Some(answer(&run.id, tool_err(&why)));
                }
            }
        }
    }

    fn on_done(&mut self, ticket: u64, result: Result<GenerateOperation, CommandError>) -> Option<Value> {
        let bg = self.bg.as_mut()?;
        if !bg.running.as_ref().is_some_and(|r| r.ticket == ticket) {
            tracing::info!("mcp: dropped the result of a cancelled generate");
            return None;
        }
        let mut run = bg.running.take()?;
        if run.cancelled {
            return None;
        }
        match self.land(&mut run, result, true) {
            Ok(()) => self.advance(run),
            Err(reply) => Some(reply),
        }
    }

    /// Write one finished generation through the History, or the answer
    /// that refuses the call. A background result whose operation changed
    /// while it ran is discarded by name (#2257).
    fn land(&mut self, run: &mut Running, result: Result<GenerateOperation, CommandError>, background: bool) -> Result<(), Value> {
        let name = run.current.clone();
        let cmd = result.map_err(|e| refused(&run.id, &name, &e))?;
        if background && !cmd.still_current(&self.env.world) {
            let why = format!("generate: {name} - discarded: the operation changed while it generated - generate it again");
            return Err(answer(&run.id, tool_err(&why)));
        }
        self.env.history.execute(&self.env.world, cmd);
        let Some(entity) = model::find_by_name(&self.env.world, &name) else {
            return Err(answer(&run.id, tool_err(&format!("generate: {name} - the operation no longer exists"))));
        };
        run.done.push(api::op_view(&self.env, &name, &entity, false));
        Ok(())
    }
}

fn refused(id: &Value, name: &str, e: &CommandError) -> Value {
    answer(id, tool_err(&format!("generate: {}", api::refusal_text(name, e))))
}

fn answer(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

/// Run `staged` on a worker thread; hand the job back if no thread starts.
fn spawn(staged: StagedGeneration, ticket: u64, events: SyncSender<Event>, gate: Option<Gate>) -> Result<(), Option<Box<StagedGeneration>>> {
    let slot = Arc::new(Mutex::new(Some(staged)));
    let worker_slot = Arc::clone(&slot);
    let started = std::thread::Builder::new().name("mcp-generate".into()).spawn(move || {
        let Some(job) = worker_slot.lock().ok().and_then(|mut s| s.take()) else { return };
        if let Some(gate) = gate
            && let Ok(rx) = gate.lock()
        {
            let _ = rx.recv();
        }
        let name = job.name().to_owned();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.finish())).unwrap_or_else(|_| {
            Err(CommandError::Machining(format!(
                "{name}: internal error - the generator stopped unexpectedly (a bug; please report it)"
            )))
        });
        // The session may have ended; nobody waits for the result then.
        let _ = events.send(Event::Done { ticket, result: Box::new(result) });
    });
    match started {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::warn!("mcp: no generate worker ({e}); generating on the session thread");
            Err(slot.lock().ok().and_then(|mut s| s.take()).map(Box::new))
        }
    }
}

/// A `tools/call` of `generate` that the background lane takes: a request
/// with an id and no argument but an optional non-empty `name`. Each other
/// form goes to the synchronous handler, which answers or refuses it.
fn generate_call(msg: &Value) -> Option<(Value, Vec<Value>)> {
    let id = tool_call_id(msg, "generate")?;
    let names = match msg.get("params").and_then(|p| p.get("arguments")) {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Object(a)) if a.is_empty() => Vec::new(),
        Some(Value::Object(a)) if a.len() == 1 => match a.get("name") {
            Some(Value::Null) => Vec::new(),
            Some(Value::String(n)) if !n.is_empty() => vec![json!(n)],
            _ => return None,
        },
        Some(_) => return None,
    };
    Some((id, names))
}

/// The id of a well-formed `tools/call` request of tool `name`.
fn tool_call_id(msg: &Value, name: &str) -> Option<Value> {
    let id = msg.get("id")?;
    // Match the synchronous handler: every non-null id gets an answer.
    let id_ok = !id.is_null();
    let call = msg.get("jsonrpc") == Some(&json!("2.0"))
        && msg.get("method").and_then(Value::as_str) == Some("tools/call")
        && msg.get("params").and_then(|p| p.get("name")).and_then(Value::as_str) == Some(name);
    (id_ok && call).then(|| id.clone())
}

/// The `requestId` of a `notifications/cancelled` notification.
fn cancelled_request(msg: &Value) -> Option<Value> {
    let note = msg.get("id").is_none() && msg.get("method").and_then(Value::as_str) == Some("notifications/cancelled");
    note.then(|| msg.get("params").and_then(|p| p.get("requestId")).cloned()).flatten()
}

/// The stdio loop with the background lane: a reader thread moves lines
/// in, workers move results in, and this thread owns the world and the
/// output. At the end of input a running generate still gets its answer.
pub(super) fn serve_with<R: BufRead + Send + 'static>(mut server: Server, input: R, mut output: impl Write) -> std::io::Result<()> {
    // One queued event plus the reader's current line bounds input memory.
    // Worker completion shares the queue and cannot grow it.
    let (tx, rx) = mpsc::sync_channel(1);
    server.bg = Some(Background { events: tx.clone(), running: None, next_ticket: 0 });
    // A generic blocking reader cannot be interrupted. Give it owned input so
    // an output error can return immediately. It exits on its next read/send
    // after the session drops the receiver, without retaining the session.
    std::thread::Builder::new().name("mcp-stdin".into()).spawn(move || read_lines(input, &tx))?;
    let mut ended = false;
    loop {
        if ended && !server.generating() {
            return Ok(());
        }
        // The session holds a sender itself, so the channel never closes here.
        let Ok(ev) = rx.recv() else { return Ok(()) };
        let reply = match ev {
            Event::Eof => {
                ended = true;
                None
            }
            Event::Failed(e) => return Err(e),
            other => server.on_event(other),
        };
        if let Some(reply) = reply {
            writeln!(output, "{reply}")?;
            output.flush()?;
        }
    }
}

/// One line at a time, capped at [`MAX_LINE_BYTES`]; a longer line or one
/// that is not UTF-8 goes over as its parse-error text.
fn read_lines(mut input: impl BufRead, tx: &SyncSender<Event>) {
    let mut buf = Vec::new();
    let cap = u64::try_from(MAX_LINE_BYTES).unwrap_or(u64::MAX).saturating_add(1);
    loop {
        buf.clear();
        let ev = match (&mut input).take(cap).read_until(b'\n', &mut buf) {
            Ok(0) => Event::Eof,
            Ok(_) if !buf.ends_with(b"\n") && buf.len() > MAX_LINE_BYTES => match super::skip_line(&mut input) {
                Ok(()) => Event::Line(Err(format!("line longer than {MAX_LINE_BYTES} bytes"))),
                Err(e) => Event::Failed(e),
            },
            Ok(_) => Event::Line(String::from_utf8(std::mem::take(&mut buf)).map_err(|e| format!("the line is not UTF-8: {}", e.utf8_error()))),
            Err(e) => Event::Failed(e),
        };
        let last = matches!(ev, Event::Eof | Event::Failed(_));
        if tx.send(ev).is_err() || last {
            return;
        }
    }
}
