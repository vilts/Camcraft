//! The live MCP seam (#183): the tool table answered against a world and
//! History the caller owns (the GUI's), so undo there reverts an agent's
//! step; the loopback listener that carries it; and the CLI's flag.
//!
//! Gate map (M4.4): covers app::mcp (live, listen, bridge).

use camcraft_app::commands::History;
use camcraft_app::mcp::{answer_live, port_from, Inbox};
use camcraft_app::model;
use camcraft_app::world::new_world;

use flecs_ecs::prelude::World;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

fn call(id: u64, name: &str, args: Value) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "method": "tools/call", "params": { "name": name, "arguments": args } })
        .to_string()
}

/// One live call; the answer must be a tool success.
fn ok(world: &World, history: &mut History, id: u64, name: &str, args: Value) -> Value {
    let answer = answer_live(world, history, None, &call(id, name, args));
    assert_eq!(answer.tool.as_deref(), Some(name));
    let reply: Value = serde_json::from_str(&answer.reply.expect("a reply")).expect("json");
    assert_eq!(reply["id"], json!(id));
    assert_eq!(reply["result"]["isError"], json!(false), "{name}: {reply}");
    reply["result"]["structuredContent"].clone()
}

fn face_job(world: &World, history: &mut History) {
    ok(world, history, 1, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } }));
    ok(world, history, 2, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    let add = ok(world, history, 3, "add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    assert_eq!(add["operation"], json!("Face1"));
}

#[test]
fn a_live_call_lands_on_the_callers_history_and_its_undo_reverts_it() {
    let world = new_world();
    let mut history = History::default();
    face_job(&world, &mut history);
    // The caller's own world shows the operation, and the caller's own
    // history - not a copy the session kept - holds the step.
    assert!(model::find_by_name(&world, "Face1").is_some_and(|e| model::is_operation(&world, &e)));
    assert!(history.can_undo());
    assert!(history.undo(&world), "the caller's undo walks the agent's step");
    assert!(model::find_by_name(&world, "Face1").is_none(), "undo removed the agent's operation");
    assert!(history.redo(&world));
    assert!(model::find_by_name(&world, "Face1").is_some(), "redo brings it back");
    // The session's own undo tool walks the same history.
    ok(&world, &mut history, 4, "undo", json!({}));
    assert!(model::find_by_name(&world, "Face1").is_none());
}

#[test]
fn each_agent_step_is_one_undo_step_on_the_shared_history() {
    let world = new_world();
    let mut history = History::default();
    ok(&world, &mut history, 1, "set_stock", json!({ "box": { "dx": 50.0, "dy": 50.0, "dz": 10.0 } }));
    ok(&world, &mut history, 2, "add_tool", json!({ "catalog": "EM-0400-2FL" }));
    // Three agent steps, three undos, then nothing: no step was lost or
    // duplicated in the hand-over.
    ok(&world, &mut history, 3, "add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" }));
    assert!(history.undo(&world) && history.undo(&world) && history.undo(&world));
    assert!(!history.can_undo());
}

#[test]
fn the_live_session_refuses_open_project_and_reads_bad_lines_as_errors() {
    let world = new_world();
    let mut history = History::default();
    let a = answer_live(&world, &mut history, None, &call(9, "open_project", json!({ "path": "x.camproj" })));
    let reply: Value = serde_json::from_str(&a.reply.expect("reply")).expect("json");
    assert_eq!(reply["result"]["isError"], json!(true));
    assert!(reply["result"]["content"][0]["text"].as_str().is_some_and(|t| t.contains("File > Open")));
    // Malformed JSON: a parse error, the history untouched.
    let bad = answer_live(&world, &mut history, None, "{not json");
    assert!(bad.reply.is_some_and(|r| r.contains("-32700")));
    assert!(bad.tool.is_none());
    // A notification: no answer.
    let note = answer_live(&world, &mut history, None, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    assert!(note.reply.is_none());
    assert!(!history.can_undo());
}

#[test]
fn the_port_flag_is_opt_in_and_checked() {
    let args = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    assert_eq!(port_from(&args(&[]), None), Ok(None));
    assert_eq!(port_from(&args(&["--mcp-port", "7333"]), None), Ok(Some(7333)));
    assert_eq!(port_from(&args(&["--mcp-port=0"]), None), Ok(Some(0)));
    assert_eq!(port_from(&args(&[]), Some("7400")), Ok(Some(7400)));
    assert_eq!(port_from(&args(&["--mcp-port", "7333"]), Some("7400")), Ok(Some(7333)), "the flag wins");
    assert_eq!(port_from(&args(&[]), Some("  ")), Ok(None));
    assert!(port_from(&args(&["--mcp-port"]), None).is_err());
    assert!(port_from(&args(&["--mcp-port", "99999"]), None).is_err());
}

/// Answer the inbox's requests against `world` until `done` or timeout.
fn pump_until(inbox: &Inbox, world: &World, history: &mut History, done: &dyn Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "the client did not finish in time");
        match inbox.next() {
            Some(req) => {
                let a = answer_live(world, history, None, req.line());
                req.answer(a.reply);
            }
            None => std::thread::sleep(Duration::from_millis(5)),
        }
    }
}

#[test]
fn a_loopback_client_drives_the_owners_document_and_survives_bad_input() {
    let woke = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let w = std::sync::Arc::clone(&woke);
    let inbox = Inbox::listen(0, Box::new(move || {
        w.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }))
    .expect("listen on loopback");
    let addr = inbox.addr().expect("bound address");
    assert!(addr.ip().is_loopback(), "bound to loopback only: {addr}");

    let finished = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fin = std::sync::Arc::clone(&finished);
    let client = std::thread::spawn(move || -> Vec<Value> {
        // A first client that sends junk and hangs up mid-line.
        {
            let mut s = TcpStream::connect(addr).expect("connect");
            s.write_all(b"{garbage\n{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}").expect("write");
            let mut r = BufReader::new(s);
            let mut line = String::new();
            r.read_line(&mut line).expect("parse error line");
            assert!(line.contains("-32700"), "{line}");
        }
        // The listener takes the next client.
        let s = TcpStream::connect(addr).expect("reconnect");
        let mut w = s.try_clone().expect("clone");
        let mut r = BufReader::new(s);
        let mut replies = Vec::new();
        let lines = [
            json!({ "jsonrpc": "2.0", "id": 0, "method": "initialize", "params": { "protocolVersion": "2025-06-18" } }).to_string(),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_owned(),
            call(1, "set_stock", json!({ "box": { "dx": 60.0, "dy": 60.0, "dz": 20.0 } })),
            call(2, "add_tool", json!({ "catalog": "EM-0400-2FL" })),
            call(3, "add_operation", json!({ "kind": "Face", "tool": "EM-0400-2FL" })),
        ];
        for l in &lines {
            writeln!(w, "{l}").expect("send");
            if l.contains("\"id\"") {
                let mut line = String::new();
                r.read_line(&mut line).expect("answer");
                replies.push(serde_json::from_str(&line).expect("json answer"));
            }
        }
        fin.store(true, std::sync::atomic::Ordering::Relaxed);
        replies
    });

    let world = new_world();
    let mut history = History::default();
    pump_until(&inbox, &world, &mut history, &|| finished.load(std::sync::atomic::Ordering::Relaxed));
    let replies = client.join().expect("client thread");
    assert_eq!(replies.len(), 4);
    assert_eq!(replies[0]["result"]["serverInfo"]["name"], json!("camcraft"));
    assert_eq!(replies[3]["result"]["isError"], json!(false), "{}", replies[3]);
    assert!(model::find_by_name(&world, "Face1").is_some(), "the agent's operation is in the owner's world");
    assert!(history.undo(&world) && model::find_by_name(&world, "Face1").is_none());
    assert!(woke.load(std::sync::atomic::Ordering::Relaxed) >= 4, "each request woke the owner");

    // Dropping the inbox stops and joins the listener promptly, and the
    // port is free again.
    let t = Instant::now();
    drop(inbox);
    assert!(t.elapsed() < Duration::from_secs(2), "the listener stopped in {:?}", t.elapsed());
    assert!(TcpStream::connect_timeout(&addr, Duration::from_millis(200)).is_err(), "nothing listens after drop");
}

#[test]
fn the_bridge_copies_stdio_to_a_live_session_and_back() {
    let inbox = Inbox::listen(0, Box::new(|| {})).expect("listen");
    let addr = inbox.addr().expect("addr").to_string();
    let input = format!("{}\n", json!({ "jsonrpc": "2.0", "id": 7, "method": "ping" }));
    let bridge = std::thread::spawn(move || {
        let mut out = Vec::new();
        camcraft_app::mcp::bridge(&addr, std::io::Cursor::new(input.into_bytes()), &mut out).map(|()| out)
    });
    let world = new_world();
    let mut history = History::default();
    pump_until(&inbox, &world, &mut history, &|| bridge.is_finished());
    let out = bridge.join().expect("bridge thread").expect("bridge ran");
    let text = String::from_utf8(out).expect("utf8");
    assert!(text.contains("\"id\":7") && text.contains("\"result\""), "{text}");
    assert!(camcraft_app::mcp::bridge("127.0.0.1:1", std::io::empty(), std::io::sink()).is_err(), "no server: a clean error");
}

/// #474 review: loopback is no trust boundary - a web page in the user's
/// browser can POST to 127.0.0.1. Its HTTP request line closes the
/// connection, so the JSON-RPC body it carries never reaches the app;
/// a real client on the next connection is served as before.
#[test]
fn a_browser_post_never_reaches_the_app() {
    let inbox = Inbox::listen(0, Box::new(|| {})).expect("listen on loopback");
    let addr = inbox.addr().expect("bound address");
    let body = call(9, "save_project", json!({ "path": "/tmp/camcraft-drive-by.ccam" }));
    let post = format!(
        "POST / HTTP/1.1\r\nHost: {addr}\r\nOrigin: http://evil.example\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}\n",
        body.len() + 1
    );
    let mut s = TcpStream::connect(addr).expect("connect");
    // A regression must fail, never hang: without the JSON-only rule the
    // page's body would sit unanswered in the app's inbox.
    s.set_read_timeout(Some(Duration::from_secs(3))).expect("timeout");
    s.write_all(post.as_bytes()).expect("write");
    let mut r = BufReader::new(s);
    let mut line = String::new();
    r.read_line(&mut line).expect("a refusal, not silence - the page's request reached the app");
    assert!(line.contains("-32600") && line.contains("not a JSON-RPC message"), "{line}");
    let mut rest = String::new();
    let n = r.read_line(&mut rest).unwrap_or(0);
    assert_eq!(n, 0, "the connection is closed after the refusal, not reading on: {rest:?}");
    // Nothing of the page's request was handed to the app.
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        assert!(inbox.next().is_none(), "a browser line reached the app");
        std::thread::sleep(Duration::from_millis(10));
    }

    // A JSON-RPC client is still served on the next connection.
    let s = TcpStream::connect(addr).expect("reconnect");
    let mut w = s.try_clone().expect("clone");
    let mut r = BufReader::new(s);
    let ping = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
    writeln!(w, "{ping}").expect("send");
    let world = new_world();
    let mut history = History::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let req = loop {
        if let Some(req) = inbox.next() {
            break req;
        }
        assert!(Instant::now() < deadline, "the JSON client was not served");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(req.line().trim(), ping);
    let a = answer_live(&world, &mut history, None, req.line());
    req.answer(a.reply);
    let mut line = String::new();
    r.read_line(&mut line).expect("answer");
    assert!(line.contains("\"id\":1"), "{line}");
}

/// The listener's log lines, captured: a global subscriber writing into
/// a shared buffer, installed once per test binary by [`log_capture`]
/// (the idle tests read it by their own client's address).
#[derive(Clone, Default)]
struct LogBuf(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl Write for LogBuf {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("log lock").extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The process-wide captured log, installed on first use; `None` when
/// another subscriber was installed first.
fn log_capture() -> Option<&'static LogBuf> {
    static LOG: std::sync::OnceLock<Option<LogBuf>> = std::sync::OnceLock::new();
    LOG.get_or_init(|| {
        let log = LogBuf::default();
        let sink = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_ansi(false)
            .with_writer(move || sink.clone())
            .finish();
        tracing::subscriber::set_global_default(subscriber).is_ok().then_some(log)
    })
    .as_ref()
}

/// Wait for one request and answer it against `world`.
fn answer_one(inbox: &Inbox, world: &World, history: &mut History) -> String {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(req) = inbox.next() {
            let line = req.line().to_owned();
            let a = answer_live(world, history, None, req.line());
            req.answer(a.reply);
            return line;
        }
        assert!(Instant::now() < deadline, "the client was not served");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// #567: a client that connects and says nothing holds the single
/// session only for the idle window; then it is told why, closed,
/// logged by its address, and the next client is served.
#[test]
fn an_idle_client_is_dropped_by_name_and_the_next_one_is_served() {
    let log = log_capture();
    let inbox = Inbox::listen_with_idle(0, Box::new(|| {}), Duration::from_millis(300)).expect("listen");
    let addr = inbox.addr().expect("bound address");
    let silent = TcpStream::connect(addr).expect("connect");
    let silent_addr = silent.local_addr().expect("client address");
    // A regression must fail, never hang.
    silent.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
    let t = Instant::now();
    let mut r = BufReader::new(silent);
    let mut line = String::new();
    r.read_line(&mut line).expect("an idle note, not silence - the idle client was never dropped");
    assert!(line.contains("notifications/message") && line.contains("idle"), "{line}");
    let mut rest = String::new();
    assert_eq!(r.read_line(&mut rest).unwrap_or(0), 0, "the connection is closed after the note: {rest:?}");
    assert!(t.elapsed() >= Duration::from_millis(250), "dropped before the window: {:?}", t.elapsed());

    // The next client is served.
    let s = TcpStream::connect(addr).expect("reconnect");
    let mut w = s.try_clone().expect("clone");
    let mut r = BufReader::new(s);
    writeln!(w, r#"{{"jsonrpc":"2.0","id":1,"method":"ping"}}"#).expect("send");
    let world = new_world();
    let mut history = History::default();
    answer_one(&inbox, &world, &mut history);
    let mut answer = String::new();
    r.read_line(&mut answer).expect("answer");
    assert!(answer.contains("\"id\":1") && answer.contains("\"result\""), "{answer}");

    if let Some(log) = log {
        let text = String::from_utf8_lossy(&log.0.lock().expect("log lock")).into_owned();
        assert!(text.contains(&format!("client {silent_addr} dropped: idle")), "the log names the idle client: {text}");
    }
}

/// #567: a client that keeps sending complete lines inside the window
/// keeps its session however long it runs.
#[test]
fn a_client_that_keeps_talking_is_never_dropped_as_idle() {
    let window = Duration::from_millis(400);
    let inbox = Inbox::listen_with_idle(0, Box::new(|| {}), window).expect("listen");
    let addr = inbox.addr().expect("bound address");
    let client = std::thread::spawn(move || -> Vec<String> {
        let s = TcpStream::connect(addr).expect("connect");
        s.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
        let mut w = s.try_clone().expect("clone");
        let mut r = BufReader::new(s);
        let mut answers = Vec::new();
        // Eight calls 150 ms apart: three windows long in all.
        for id in 0..8 {
            std::thread::sleep(Duration::from_millis(150));
            writeln!(w, r#"{{"jsonrpc":"2.0","id":{id},"method":"ping"}}"#).expect("send");
            let mut line = String::new();
            r.read_line(&mut line).expect("answer");
            answers.push(line);
        }
        answers
    });
    let world = new_world();
    let mut history = History::default();
    pump_until(&inbox, &world, &mut history, &|| client.is_finished());
    let answers = client.join().expect("client thread");
    assert_eq!(answers.len(), 8);
    for (id, a) in answers.iter().enumerate() {
        assert!(a.contains(&format!("\"id\":{id}")) && !a.contains("idle"), "call {id}: {a}");
    }
}

/// #567 review (slow loris): a client trickling one byte faster than
/// the poll step, never a newline, is dropped on the idle schedule all
/// the same - a partial line never resets the clock - logged by its
/// address, and the next client is served.
#[test]
fn a_trickling_client_is_dropped_on_the_idle_schedule() {
    let log = log_capture();
    let window = Duration::from_millis(400);
    let inbox = Inbox::listen_with_idle(0, Box::new(|| {}), window).expect("listen");
    let addr = inbox.addr().expect("bound address");
    let s = TcpStream::connect(addr).expect("connect");
    let trickler_addr = s.local_addr().expect("client address");
    // A regression must fail, never hang.
    s.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
    let mut w = s.try_clone().expect("clone");
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = std::sync::Arc::clone(&stop);
    // One byte every 10 ms (the poll step is 50 ms), for at most 5 s.
    let trickle = std::thread::spawn(move || {
        let end = Instant::now() + Duration::from_secs(5);
        while !flag.load(std::sync::atomic::Ordering::Relaxed) && Instant::now() < end {
            if w.write_all(b"{").and_then(|()| w.flush()).is_err() {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });
    let t = Instant::now();
    let mut r = BufReader::new(s);
    let mut line = String::new();
    let got = r.read_line(&mut line);
    let after = t.elapsed();
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    trickle.join().expect("trickle thread");
    // The drop is a close; the server's close with the trickler's unread
    // bytes still queued may reset the connection (RST) and discard the
    // idle note (#1366), so a reset or an EOF is the drop itself. Only a
    // read that times out - the client still connected - is a failure.
    match got {
        Ok(0) => {}
        Ok(_) => assert!(line.contains("notifications/message") && line.contains("idle"), "{line}"),
        Err(e) if matches!(e.kind(), std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted | std::io::ErrorKind::BrokenPipe) => {}
        Err(e) => panic!("the trickling client was never dropped: {e:?}"),
    }
    assert!(after >= Duration::from_millis(350), "dropped before the window: {after:?}");
    assert!(after < Duration::from_millis(1500), "dropped late, not on the idle schedule: {after:?}");

    // The next client is served.
    let s = TcpStream::connect(addr).expect("reconnect");
    s.set_read_timeout(Some(Duration::from_secs(5))).expect("timeout");
    let mut w = s.try_clone().expect("clone");
    let mut r = BufReader::new(s);
    writeln!(w, r#"{{"jsonrpc":"2.0","id":1,"method":"ping"}}"#).expect("send");
    let world = new_world();
    let mut history = History::default();
    answer_one(&inbox, &world, &mut history);
    let mut answer = String::new();
    r.read_line(&mut answer).expect("answer");
    assert!(answer.contains("\"id\":1") && answer.contains("\"result\""), "{answer}");

    if let Some(log) = log {
        let text = String::from_utf8_lossy(&log.0.lock().expect("log lock")).into_owned();
        assert!(text.contains(&format!("client {trickler_addr} dropped: idle")), "the log names the trickling client: {text}");
    }
}
