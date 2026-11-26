//! #3035: the stdio MCP server runs a `generate` on a worker thread. A read
//! or a save answers while the generate runs, `notifications/cancelled`
//! drops it with no answer, a second generate is refused by name, and a
//! result whose operation changed while it ran is discarded, never landed.
//!
//! Provenance: our own tests over our own server; the cancellation form is
//! the public MCP specification's (revision 2025-06-18).
//!
//! Gate map: covers app::mcp (background, serve), app::script::api (generate_targets).

use camcraft_app::mcp::{self, Server};
use serde_json::{json, Value};
use std::io::{BufReader, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

/// The client's end of the input: each message is one chunk of bytes.
struct Pipe {
    rx: Receiver<Vec<u8>>,
    buf: Vec<u8>,
    pos: usize,
}

impl Read for Pipe {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.pos >= self.buf.len() {
            match self.rx.recv() {
                Ok(b) => {
                    self.buf = b;
                    self.pos = 0;
                }
                Err(_) => return Ok(0),
            }
        }
        let n = out.len().min(self.buf.len() - self.pos);
        out[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}

/// The server's output, one wire line per message.
struct Lines {
    tx: Sender<String>,
    pending: Vec<u8>,
}

impl Write for Lines {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(data);
        while let Some(i) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=i).collect();
            let _ = self.tx.send(String::from_utf8_lossy(&line).trim().to_owned());
        }
        Ok(data.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct Session {
    input: Option<Sender<Vec<u8>>>,
    output: Receiver<String>,
    gate: Sender<()>,
    thread: Option<JoinHandle<std::io::Result<()>>>,
}

impl Session {
    /// A served session whose generate workers wait on `gate`.
    fn start() -> Self {
        let (in_tx, in_rx) = mpsc::channel();
        let (out_tx, out_rx) = mpsc::channel();
        let (gate_tx, gate_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            // The world is not Send: the session is built on its own thread.
            let mut server = Server::new();
            gate_tx.send(server.hold_generations()).expect("gate handed over");
            let input = BufReader::new(Pipe { rx: in_rx, buf: Vec::new(), pos: 0 });
            mcp::serve_with(server, input, Lines { tx: out_tx, pending: Vec::new() })
        });
        let gate = gate_rx.recv().expect("the session starts");
        Self { input: Some(in_tx), output: out_rx, gate, thread: Some(thread) }
    }

    fn send(&self, msg: &Value) {
        self.input.as_ref().expect("input open").send(format!("{msg}\n").into_bytes()).expect("session alive");
    }

    fn call(&self, id: i64, name: &str, args: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } }));
    }

    fn next(&self) -> Value {
        let line = self.output.recv_timeout(Duration::from_secs(120)).expect("an answer within 120 s");
        serde_json::from_str(&line).expect("one JSON object per line")
    }

    /// Call and take the answer, which must be the next line and succeed.
    fn ok(&self, id: i64, name: &str, args: Value) -> Value {
        self.call(id, name, args);
        let reply = self.next();
        assert_eq!(reply["id"], json!(id), "{reply}");
        assert_eq!(reply["result"]["isError"], json!(false), "{name}: {reply}");
        reply["result"]["structuredContent"].clone()
    }

    fn release(&self) {
        self.gate.send(()).expect("a held worker");
    }

    /// End the input; every line the session wrote after this point.
    fn finish(mut self) -> Vec<Value> {
        drop(self.input.take());
        // Drop the gate's sender: any held worker runs, so the session can end.
        self.gate = mpsc::channel::<()>().0;
        let thread = self.thread.take().expect("thread");
        thread.join().expect("the session thread").expect("serve");
        self.output.try_iter().map(|l| serde_json::from_str(&l).expect("JSON")).collect()
    }
}

fn status_of_first(s: &Session, id: i64) -> String {
    let job = s.ok(id, "get_job", json!({}));
    job["operations"][0]["status"].as_str().expect("status").to_owned()
}

fn one_face() -> Session {
    let s = Session::start();
    s.ok(1, "set_stock", json!({ "box": { "dx": 20.0, "dy": 20.0, "dz": 10.0 } }));
    s.ok(2, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    s.ok(3, "add_operation", json!({ "kind": "Face", "name": "First", "tool": "EM-0400-2FL" }));
    s
}

/// The issue's case: a read and a save queued behind a long generate
/// answer before the generate does, and the generate still lands.
#[test]
fn a_read_and_a_save_answer_while_a_generate_runs() {
    let s = one_face();
    s.call(10, "generate", json!({ "name": "First" }));
    assert_eq!(status_of_first(&s, 11), "never", "the held generate has not landed");
    let dir = std::env::temp_dir().join(format!("camcraft-mcp-bg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("held.camproj");
    s.ok(12, "save_project", json!({ "path": path.display().to_string(), "overwrite": true }));
    s.call(13, "generate", json!({}));
    let busy = s.next();
    assert_eq!(busy["id"], json!(13), "{busy}");
    assert_eq!(busy["result"]["isError"], json!(true), "{busy}");
    let text = busy["result"]["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("another generate is running (First)"), "{text}");
    s.release();
    let done = s.next();
    assert_eq!(done["id"], json!(10), "{done}");
    assert_eq!(done["result"]["isError"], json!(false), "{done}");
    assert_eq!(done["result"]["structuredContent"]["generated"][0]["key"], json!("First"), "{done}");
    assert_eq!(status_of_first(&s, 14), "ok");
    s.finish();
    let _ = std::fs::remove_dir_all(&dir);
}

/// `notifications/cancelled` drops the running generate: no answer for
/// its id, nothing lands, and the session goes on serving.
#[test]
fn a_cancelled_generate_gets_no_answer_and_lands_nothing() {
    let s = one_face();
    s.call(10, "generate", json!({}));
    s.send(&json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 10, "reason": "timeout" } }));
    assert_eq!(status_of_first(&s, 11), "never");
    s.release();
    // The cancelled worker keeps its slot until its completion is drained.
    s.release();
    let deadline = std::time::Instant::now() + Duration::from_secs(120);
    let again = loop {
        s.call(12, "generate", json!({ "name": "First" }));
        let reply = s.next();
        assert_eq!(reply["id"], json!(12), "{reply}");
        if reply["result"]["isError"] == json!(false) {
            break reply["result"]["structuredContent"].clone();
        }
        assert!(std::time::Instant::now() < deadline, "cancelled worker did not release its slot");
        std::thread::yield_now();
    };
    assert_eq!(again["generated"][0]["key"], json!("First"));
    let rest = s.finish();
    assert!(rest.iter().all(|r| r["id"] != json!(10)), "a cancelled request gets no answer: {rest:?}");
}

/// A result whose operation was edited while it ran is discarded by
/// name: old parameters never land over the newer ones.
#[test]
fn a_generate_whose_operation_changed_meanwhile_is_discarded() {
    let s = one_face();
    s.call(10, "generate", json!({ "name": "First" }));
    s.ok(11, "edit_operation", json!({ "name": "First", "params": { "stepover": 2.0 } }));
    s.release();
    let done = s.next();
    assert_eq!(done["id"], json!(10), "{done}");
    assert_eq!(done["result"]["isError"], json!(true), "{done}");
    let text = done["result"]["content"][0]["text"].as_str().expect("text");
    assert!(text.contains("First") && text.contains("changed while it generated"), "{text}");
    assert_eq!(status_of_first(&s, 12), "never", "the discarded path did not land");
    s.finish();
}

/// Repeated cancellation cannot detach an unlimited number of held workers.
#[test]
fn cancelled_workers_keep_the_worker_budget_until_they_finish() {
    let s = one_face();
    s.call(10, "generate", json!({ "name": "First" }));
    s.send(&json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": 10 } }));
    for id in 20..28 {
        s.call(id, "generate", json!({ "name": "First" }));
        // This read answers even if a mutation incorrectly starts another
        // held worker, so the test fails immediately instead of hanging.
        s.call(100 + id, "get_job", json!({}));
        let busy = s.next();
        assert_eq!(
            busy["id"],
            json!(id),
            "retry started another held worker: {busy}"
        );
        assert_eq!(busy["result"]["isError"], json!(true), "{busy}");
        assert!(busy["result"]["content"][0]["text"]
            .as_str()
            .expect("text")
            .contains("another generate is running (First)"));
        assert_eq!(s.next()["id"], json!(100 + id));
        s.send(&json!({ "jsonrpc": "2.0", "method": "notifications/cancelled", "params": { "requestId": id } }));
    }
    assert_eq!(status_of_first(&s, 99), "never");
    let rest = s.finish();
    assert!(rest.iter().all(|r| r["id"] != json!(10)), "{rest:?}");
}

/// A null optional name has the same background and busy rules as no name.
#[test]
fn a_null_name_uses_the_background_lane_and_busy_guard() {
    let s = one_face();
    s.call(10, "generate", json!({ "name": null }));
    assert_eq!(
        status_of_first(&s, 11),
        "never",
        "null-name generation must stay held in the background"
    );
    s.call(12, "generate", json!({ "name": null }));
    s.call(13, "get_job", json!({}));
    let busy = s.next();
    assert_eq!(busy["id"], json!(12), "{busy}");
    assert_eq!(busy["result"]["isError"], json!(true), "{busy}");
    assert_eq!(s.next()["id"], json!(13));
    // The synchronous handler accepts every non-null id. Each accepted form
    // must therefore use the background busy guard too.
    for id in [json!(true), json!({ "key": 1 }), json!([1, 2])] {
        s.send(&json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {
            "name": "generate", "arguments": { "name": null }
        } }));
        s.call(13, "get_job", json!({}));
        let busy = s.next();
        assert_eq!(busy["id"], id, "{busy}");
        assert_eq!(busy["result"]["isError"], json!(true), "accepted id bypassed the busy guard: {busy}");
        assert_eq!(s.next()["id"], json!(13));
    }
    s.release();
    let done = s.next();
    assert_eq!(done["id"], json!(10), "{done}");
    assert_eq!(done["result"]["isError"], json!(false), "{done}");
    assert_eq!(status_of_first(&s, 14), "ok");
    s.finish();
}

/// A queued operation can be deleted while the preceding worker runs.
#[test]
fn a_deleted_queued_operation_is_skipped() {
    let s = one_face();
    s.ok(4, "add_operation", json!({ "kind": "Face", "name": "Second", "tool": "EM-0400-2FL" }));
    s.call(10, "generate", json!({}));
    s.ok(11, "delete_operation", json!({ "name": "Second" }));
    s.release();
    let done = s.next();
    assert_eq!(done["id"], json!(10), "{done}");
    assert_eq!(done["result"]["isError"], json!(false), "{done}");
    let generated = done["result"]["structuredContent"]["generated"].as_array().expect("generated operations");
    assert_eq!(generated.len(), 1, "{done}");
    assert_eq!(generated[0]["key"], json!("First"), "{done}");
    let job = s.ok(12, "get_job", json!({}));
    assert_eq!(job["operations"].as_array().expect("operations").len(), 1, "{job}");
    assert_eq!(status_of_first(&s, 13), "ok");
    s.finish();
}

/// Undoing a queued add must not leave an entity handle in the lane.
#[test]
fn an_undone_queued_add_is_skipped() {
    let s = one_face();
    s.ok(4, "add_operation", json!({ "kind": "Face", "name": "Second", "tool": "EM-0400-2FL" }));
    s.call(10, "generate", json!({}));
    s.ok(11, "undo", json!({}));
    s.release();
    let done = s.next();
    assert_eq!(done["id"], json!(10), "{done}");
    assert_eq!(done["result"]["isError"], json!(false), "{done}");
    let generated = done["result"]["structuredContent"]["generated"].as_array().expect("generated operations");
    assert_eq!(generated.len(), 1, "{done}");
    assert_eq!(generated[0]["key"], json!("First"), "{done}");
    let job = s.ok(12, "get_job", json!({}));
    assert_eq!(job["operations"].as_array().expect("operations").len(), 1, "{job}");
    assert_eq!(status_of_first(&s, 13), "ok");
    s.finish();
}
