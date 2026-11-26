//! PR #2122 live-transport pins through the real turn worker and shutdown path.
//! Provenance: our own loopback fixtures; no external endpoint or key.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use camcraft_app::ai::Endpoint;
use serde_json::{Value, json};

use super::transport::{ChatTransport, TransportOutput, TransportRequest, default_presets};
use super::transport_http::HttpTransport;
use super::worker::{TurnInput, WorkerEvent, spawn_turn};

fn read_request(s: &mut TcpStream) -> String {
    s.set_read_timeout(Some(Duration::from_secs(5)))
        .expect("request timeout");
    let mut received = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let count = s.read(&mut chunk).expect("request bytes");
        assert!(count > 0, "request ended early");
        received.extend_from_slice(&chunk[..count]);
        if let Some(end) = received.windows(4).position(|w| w == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&received[..end]);
            let length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().expect("body length"))
                })
                .unwrap_or(0);
            if received.len() >= end + 4 + length {
                return String::from_utf8(received).expect("UTF-8 request");
            }
        }
    }
}

pub(super) fn recorded_ollama(
    steps: Vec<(&'static str, Option<&'static str>, Value)>,
) -> (Endpoint, std::thread::JoinHandle<Vec<String>>) {
    recorded_ollama_delayed(steps.into_iter().map(|(line, model, answer)| {
        (line, model, answer, Duration::ZERO)
    }).collect())
}

pub(super) fn recorded_ollama_delayed(
    steps: Vec<(&'static str, Option<&'static str>, Value, Duration)>,
) -> (Endpoint, std::thread::JoinHandle<Vec<String>>) {
    let (listener, ep) = listener();
    listener.set_nonblocking(true).expect("nonblocking fixture");
    let server = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (request_line, model, answer, delay) in steps {
            let start = Instant::now();
            let mut stream = loop {
                match listener.accept() {
                    Ok((s, _)) => break s,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        if start.elapsed() > Duration::from_secs(3) {
                            return requests;
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                }
            };
            // On macOS an accepted socket inherits the listener's nonblocking mode.
            stream.set_nonblocking(false).expect("blocking stream");
            let request = read_request(&mut stream);
            let matches_line = request.lines().next() == Some(request_line);
            let matches_model = model.is_none_or(|model| {
                let (_, body) = request.split_once("\r\n\r\n").expect("body separator");
                let body: Value = serde_json::from_str(body).expect("request JSON");
                body["model"] == model
            });
            requests.push(request);
            let status = if matches_line && matches_model { "200 OK" } else { "404 Not Found" };
            let body = if matches_line && matches_model {
                answer.to_string()
            } else {
                json!({"error":"unexpected model or endpoint in recorded fixture"}).to_string()
            };
            std::thread::sleep(delay);
            let written = write!(stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len());
            // A delayed answer can arrive after the client closes on its deadline.
            if delay.is_zero() {
                written.expect("fixture answer");
            }
            if !matches_line || !matches_model {
                return requests;
            }
        }
        requests
    });
    (ep, server)
}

pub(super) fn local_request(id: &str) -> TransportRequest {
    let preset = default_presets().into_iter().find(|p| p.id == id).expect("local preset");
    TransportRequest {
        model: preset.default_model.clone(),
        fallback_mode: preset.fallback_mode,
        preset,
        offline_only: true,
        tools_payload: json!({"tools":[]}),
        messages: vec![],
    }
}

#[test]
fn cloud_presets_use_current_shared_model_defaults() {
    let presets = default_presets();
    for (id, model) in [
        ("anthropic", "claude-sonnet-5-5"),
        ("openai-chat", "gpt-6.2-sol"),
        ("openai-responses", "gpt-6.2-sol"),
        ("gemini", "gemini-4-hydrogen"),
    ] {
        let preset = presets.iter().find(|p| p.id == id).expect("cloud preset");
        assert_eq!(preset.default_model, model, "{id} must use its current default");
    }
}

#[test]
fn empty_cloud_request_uses_table_default_without_local_discovery() {
    for input in ["", "   ", "explicit-cloud-model"] {
        let preset = default_presets()
            .into_iter()
            .find(|p| p.id == "anthropic")
            .expect("Anthropic preset");
        let expected = if input.trim().is_empty() {
            "claude-sonnet-5-5"
        } else {
            input
        };
        let (ep, server) = recorded_ollama(vec![(
            "POST /v1/messages HTTP/1.1",
            Some(expected),
            json!({"content":[{"type":"text","text":"ready"}],"stop_reason":"end_turn"}),
        )]);
        let request = TransportRequest {
            model: input.to_owned(),
            fallback_mode: preset.fallback_mode,
            preset,
            offline_only: true,
            tools_payload: json!({"tools":[]}),
            messages: vec![],
        };
        let result = HttpTransport.complete_at(&request, &AtomicBool::new(false), &ep, None);
        let requests = server.join().expect("cloud fixture");
        assert!(result.is_ok(), "cloud model {input:?}: {result:?}");
        assert_eq!(
            requests.len(),
            1,
            "cloud requests must not discover local models"
        );
    }
}

#[test]
fn ollama_defaults_discover_installed_tool_model_before_first_send() {
    for id in ["local-ollama", "local-prompt-json", "local-readonly"] {
        let steps = vec![
            ("GET /api/tags HTTP/1.1", None,
                json!({"models":[{"name":"embed:latest"},{"name":"installed-tool:latest"}]})),
            ("POST /api/show HTTP/1.1", Some("embed:latest"),
                json!({"capabilities":["embedding"]})),
            ("POST /api/show HTTP/1.1", Some("installed-tool:latest"),
                json!({"capabilities":["completion","tools"]})),
            ("POST /v1/chat/completions HTTP/1.1", Some("installed-tool:latest"),
                json!({"choices":[{"message":{"content":"ready"}}]})),
        ];
        let (ep, server) = recorded_ollama(steps);
        let result = HttpTransport.complete_at(&local_request(id), &AtomicBool::new(false), &ep, None);
        let requests = server.join().expect("recorded server");
        assert!(result.is_ok(), "{id} must use an installed tool-capable model: {result:?}");
        assert_eq!(requests.len(), 4, "{id} must discover, probe, and send");
        assert!(requests[0].starts_with("GET /api/tags HTTP/1.1\r\n"));
    }
}

#[test]
fn ollama_defaults_without_tool_models_give_install_guidance() {
    for empty in [false, true] {
        let mut steps = vec![("GET /api/tags HTTP/1.1", None,
            if empty { json!({"models":[]}) }
            else { json!({"models":[{"name":"embed:latest"}]}) })];
        if !empty {
            steps.push(("POST /api/show HTTP/1.1", Some("embed:latest"),
                json!({"capabilities":["embedding"]})));
        }
        let count = steps.len();
        let (ep, server) = recorded_ollama(steps);
        let result = HttpTransport.complete_at(
            &local_request("local-ollama"), &AtomicBool::new(false), &ep, None);
        let requests = server.join().expect("recorded server");
        let message = result.expect_err("a model without tools must not receive chat");
        assert!(message.contains("ollama pull ornith-1.5:35b"), "install command missing: {message}");
        assert!(message.contains("https://ollama.com"), "model link missing: {message}");
        assert_eq!(requests.len(), count);
        assert!(requests.iter().all(|r| !r.starts_with("POST /v1/")));
    }
}

#[test]
fn ollama_malformed_tags_are_refused_before_show_or_chat() {
    for (answer, reason) in [
        (Value::Null, "models list"),
        (json!({"models":"invalid"}), "models list"),
        (json!({"models":[{}]}), "without a name"),
        (json!({"models":[{"name":""}]}), "invalid model name"),
    ] {
        let (ep, server) = recorded_ollama(vec![("GET /api/tags HTTP/1.1", None, answer)]);
        let result = HttpTransport.complete_at(
            &local_request("local-ollama"), &AtomicBool::new(false), &ep, None);
        let requests = server.join().expect("recorded server");
        let message = result.expect_err("malformed discovery must stop before probing");
        assert!(message.contains("Ollama /api/tags"), "endpoint missing: {message}");
        assert!(message.contains(reason), "refusal reason missing: {message}");
        assert_eq!(requests.len(), 1);
    }
}

#[test]
fn ollama_explicit_model_is_not_replaced_by_discovery() {
    let steps = vec![
        ("POST /api/show HTTP/1.1", Some("chosen:latest"),
            json!({"capabilities":["completion","tools"]})),
        ("POST /v1/chat/completions HTTP/1.1", Some("chosen:latest"),
            json!({"choices":[{"message":{"content":"ready"}}]})),
    ];
    let (ep, server) = recorded_ollama(steps);
    let mut request = local_request("local-ollama");
    request.model = "chosen:latest".into();
    let result = HttpTransport.complete_at(&request, &AtomicBool::new(false), &ep, None);
    let requests = server.join().expect("recorded server");
    assert!(result.is_ok(), "explicit model must remain selected: {result:?}");
    assert_eq!(requests.len(), 2);
    assert!(requests.iter().all(|r| !r.starts_with("GET /api/tags")));
}

struct LoopbackTransport(Endpoint);

impl ChatTransport for LoopbackTransport {
    fn complete(
        &self,
        req: &TransportRequest,
        cancel: &AtomicBool,
    ) -> Result<TransportOutput, String> {
        HttpTransport.complete_at(req, cancel, &self.0, None)
    }
}

pub(super) fn listener() -> (TcpListener, Endpoint) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let ep = Endpoint::parse(&format!(
        "http://{}/v1",
        listener.local_addr().expect("addr")
    ))
    .expect("endpoint");
    (listener, ep)
}

#[test]
fn http_prompt_json_reply_reaches_worker_tool_dispatch() {
    let (listener, ep) = listener();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        let proposal = json!({"tool":"get_job", "arguments":{}}).to_string();
        let body = json!({"choices":[{"message":{"content":proposal}}]}).to_string();
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let preset = default_presets()
        .into_iter()
        .find(|p| p.id == "local-prompt-json")
        .expect("preset");
    let (mcp_tx, mcp_rx) = mpsc::channel();
    let worker = spawn_turn(TurnInput {
        turn_id: 1,
        model: "test-model".into(),
        preset,
        offline_only: true,
        initial_messages: vec![],
        project_snapshot: Value::Null,
        transport: Arc::new(LoopbackTransport(ep)),
        mcp_tx,
        wake: None,
    });
    let request = mcp_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("HTTP proposal must dispatch get_job through worker");
    let rpc: Value = serde_json::from_str(request.line()).expect("JSON-RPC");
    assert_eq!(rpc["params"]["name"], "get_job");
    assert_eq!(rpc["params"]["arguments"], json!({}));
    assert!(worker.rx.try_iter().any(
        |event| matches!(event, WorkerEvent::ToolCardAdded { card, .. } if card.name == "get_job")
    ));
    drop(worker);
    server.join().expect("server");
}

#[test]
fn cancel_interrupts_worker_waiting_for_http_reply() {
    assert_silent_server_stops(true);
}

#[test]
fn shutdown_interrupts_worker_waiting_for_http_reply() {
    assert_silent_server_stops(false);
}

fn assert_silent_server_stops(cancel_first: bool) {
    let (listener, ep) = listener();
    let (started_tx, started_rx) = mpsc::channel();
    let (closed_tx, closed_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut buf = [0; 8192];
        read_request(&mut s);
        started_tx.send(()).expect("started");
        loop {
            match s.read(&mut buf) {
                Ok(0) => {
                    let _ = closed_tx.send(());
                    break;
                }
                Ok(_) => {}
                Err(_) => break,
            }
        }
    });
    let mut state = crate::state::State::new();
    let index = state
        .ai_chat
        .presets()
        .iter()
        .position(|p| p.id == "local-readonly")
        .expect("preset");
    state.ai_chat.select_preset(index);
    state.ai_chat.set_transport(Arc::new(LoopbackTransport(ep)));
    state.ai_chat.prompt_draft = "Hello".into();
    state.submit_ai_prompt();
    started_rx
        .recv_timeout(Duration::from_secs(3))
        .expect("worker reached server");
    let start = Instant::now();
    if cancel_first {
        state.ai_chat.cancel();
        while state.ai_chat.is_running() && start.elapsed() < Duration::from_secs(1) {
            state.ai_chat.poll(&mut state.history);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            !state.ai_chat.is_running(),
            "Cancel must finish the active HTTP turn promptly"
        );
    } else {
        state.ai_chat.shutdown();
    }
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "cancel={cancel_first}: shutdown blocked for {:?}",
        start.elapsed()
    );
    assert!(!state.ai_chat.is_running());
    closed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("shutdown must close active socket");
    server.join().expect("server");
}
