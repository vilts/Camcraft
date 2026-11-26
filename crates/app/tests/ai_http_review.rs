//! PR #2122 pins: provider error redaction and cancellation of an active request.
//! Provenance: our own loopback HTTP fixtures; no real credentials or remote services.
//! Gate map (M4.4): covers app::ai::http.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use camcraft_app::ai::http::{Client, Timeouts, send_chat};
use camcraft_app::ai::wire::ChatRequest;
use camcraft_app::ai::{ApiKey, Endpoint};
use camcraft_app::mcp::provider::Provider;
use serde_json::json;

fn read_request(s: &mut TcpStream) {
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
                return;
            }
        }
    }
}

const KEY: &str = "placeholder-not-a-real-key";

fn request() -> ChatRequest<'static> {
    ChatRequest {
        provider: Provider::OpenAiChat,
        model: "m",
        system: None,
        messages: &[],
        tools: &[],
        max_tokens: 16,
    }
}

#[test]
fn provider_errors_redact_supplied_key_in_json() {
    assert_error_redacted(json!({"error":{"message":format!("invalid key: {KEY}")}}).to_string());
}

#[test]
fn provider_errors_redact_supplied_key_in_plain_text() {
    assert_error_redacted(format!("invalid key: {KEY}"));
}

#[test]
fn provider_errors_redact_supplied_key_before_truncation() {
    assert_error_redacted(format!("{}{KEY}", "x".repeat(390)));
}

#[test]
fn provider_errors_keep_type_code_and_request_endpoint() {
    let body = json!({"error":{
        "message": "bad key",
        "type": format!("{}{KEY}", "t".repeat(360)),
        "code": 401,
    }})
    .to_string();
    let (err, base) = error_for(body);
    assert!(err.contains("bad key [type: tttt"), "{err}");
    assert!(err.contains("[redacted]"), "redaction must precede truncation: {err}");
    assert!(!err.contains("placeholder"), "partial key leaked: {err}");
    assert!(
        err.ends_with(&format!("(endpoint: POST {base}/chat/completions)")),
        "{err}"
    );
    let (err, _) = error_for(
        json!({"error":{"message":"m","type":"invalid_request_error","code":"model_not_found"}})
            .to_string(),
    );
    assert!(
        err.contains("m [type: invalid_request_error, code: model_not_found]"),
        "{err}"
    );
}

fn assert_error_redacted(body: String) {
    let (err, _) = error_for(body);
    assert!(err.contains("401"), "{err}");
    assert!(!err.contains(KEY), "key leaked: {err}");
    assert!(
        !err.contains("placeholder"),
        "partial key leaked at truncation: {err}"
    );
    assert!(
        err.contains("[redacted]"),
        "redaction must precede truncation: {err}"
    );
}

/// The error of one chat request that a loopback server answers with HTTP 401 and `body`.
fn error_for(body: String) -> (String, String) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let base = format!("http://{}/v1", listener.local_addr().expect("addr"));
    let ep = Endpoint::parse(&base).expect("endpoint");
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 401 Unauthorized\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let key = ApiKey::new(KEY).expect("placeholder");
    let err = send_chat(
        &Client::new(true),
        &ep,
        &request(),
        Some(&key),
        &AtomicBool::new(false),
    )
    .expect_err("401");
    server.join().expect("server");
    (err, base)
}

#[test]
fn cancel_interrupts_active_http_request_and_closes_socket() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let ep = Endpoint::parse(&format!(
        "http://{}/v1",
        listener.local_addr().expect("addr")
    ))
    .expect("endpoint");
    let (started_tx, started_rx) = mpsc::channel();
    let (closed_tx, closed_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        s.set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        let mut buf = [0; 4096];
        read_request(&mut s);
        started_tx.send(()).expect("signal");
        // Never send a response. The client must close the active socket on cancel.
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
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let worker = std::thread::spawn(move || {
        let client = Client {
            offline_only: true,
            timeouts: Timeouts {
                connect: Duration::from_secs(2),
                total: Duration::from_secs(4),
            },
        };
        send_chat(&client, &ep, &request(), None, &worker_cancel)
    });
    started_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("request reached server");
    let start = Instant::now();
    cancel.store(true, Ordering::Relaxed);
    let error = worker.join().expect("worker").expect_err("cancelled");
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "cancel blocked for {:?}",
        start.elapsed()
    );
    assert!(error.contains("cancelled"), "{error}");
    closed_rx
        .recv_timeout(Duration::from_secs(1))
        .expect("cancellation must close active socket");
    server.join().expect("server");
}

#[test]
fn active_http_request_can_wait_for_delayed_reply_without_cancellation() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let ep = Endpoint::parse(&format!(
        "http://{}/v1",
        listener.local_addr().expect("addr")
    ))
    .expect("endpoint");
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        std::thread::sleep(Duration::from_millis(250));
        let body = r#"{"ok":true}"#;
        write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let out = send_chat(
        &Client::new(true),
        &ep,
        &request(),
        None,
        &AtomicBool::new(false),
    )
    .expect("delayed reply");
    assert_eq!(out, json!({"ok":true}));
    server.join().expect("server");
}
