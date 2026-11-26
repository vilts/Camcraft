//! PR #3799 pins: the automatic Ollama model selection gives each cause its
//! own message, continues after a failed probe, and has a time limit.
//! Provenance: our own loopback fixtures; no external endpoint or key.

use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use camcraft_app::ai::http::{Client, discover_ollama_within};
use serde_json::json;

use super::transport_http::HttpTransport;
use super::transport_http_tests::{listener, local_request, recorded_ollama, recorded_ollama_delayed};

#[test]
fn ollama_discovery_continues_after_a_failed_probe() {
    let steps = vec![
        ("GET /api/tags HTTP/1.1", None,
            json!({"models":[{"name":"broken:latest"},{"name":"installed-tool:latest"}]})),
        ("POST /api/show HTTP/1.1", Some("broken:latest"),
            json!({"error":"manifest is damaged"})),
        ("POST /api/show HTTP/1.1", Some("installed-tool:latest"),
            json!({"capabilities":["completion","tools"]})),
        ("POST /v1/chat/completions HTTP/1.1", Some("installed-tool:latest"),
            json!({"choices":[{"message":{"content":"ready"}}]})),
    ];
    let (ep, server) = recorded_ollama(steps);
    let result = HttpTransport.complete_at(
        &local_request("local-ollama"), &AtomicBool::new(false), &ep, None);
    let requests = server.join().expect("recorded server");
    assert!(result.is_ok(), "one failed probe must not stop the selection: {result:?}");
    assert_eq!(requests.len(), 4, "the next model must be probed and used");
}

#[test]
fn ollama_discovery_names_the_model_whose_probe_failed() {
    let steps = vec![
        ("GET /api/tags HTTP/1.1", None,
            json!({"models":[{"name":"broken:latest"},{"name":"embed:latest"}]})),
        ("POST /api/show HTTP/1.1", Some("broken:latest"),
            json!({"error":"manifest is damaged"})),
        ("POST /api/show HTTP/1.1", Some("embed:latest"),
            json!({"capabilities":["embedding"]})),
    ];
    let (ep, server) = recorded_ollama(steps);
    let result = HttpTransport.complete_at(
        &local_request("local-ollama"), &AtomicBool::new(false), &ep, None);
    let requests = server.join().expect("recorded server");
    let message = result.expect_err("no model can call tools");
    assert!(message.contains("probe failed for 'broken:latest'"), "model missing: {message}");
    assert!(message.contains("manifest is damaged"), "reason missing: {message}");
    assert!(!message.contains("embed:latest"), "a probed model is not a failed one: {message}");
    assert!(!message.contains("did not answer"), "the server answered: {message}");
    assert_eq!(requests.len(), 3, "each installed model must be probed");
}

#[test]
fn ollama_discovery_says_when_the_server_does_not_answer() {
    let (closed, ep) = listener();
    drop(closed);
    let result = HttpTransport.complete_at(
        &local_request("local-ollama"), &AtomicBool::new(false), &ep, None);
    let message = result.expect_err("a closed port has no model list");
    assert!(message.contains("did not answer its model list"), "cause missing: {message}");
    assert!(message.contains("ollama serve"), "start instruction missing: {message}");
    assert!(!message.contains("ollama pull"), "this is not a missing model: {message}");
}

#[test]
fn ollama_discovery_stops_at_its_time_limit() {
    let (ep, server) = recorded_ollama(vec![]);
    let result = discover_ollama_within(
        &Client::new(true), &ep, &AtomicBool::new(false), Duration::ZERO);
    let requests = server.join().expect("recorded server");
    let message = result.expect_err("the time limit ended before the model list");
    assert!(message.contains("stopped after 0 s"), "limit missing: {message}");
    assert!(message.contains("before reading installed models"), "stage missing: {message}");
    assert!(requests.is_empty(), "no request starts after the limit");
}

#[test]
fn ollama_discovery_caps_the_model_list_by_the_total_time() {
    let (ep, server) = recorded_ollama_delayed(vec![
        ("GET /api/tags HTTP/1.1", None, json!({"models":[]}), Duration::from_secs(1)),
    ]);
    let start = Instant::now();
    let result = discover_ollama_within(
        &Client::new(true), &ep, &AtomicBool::new(false), Duration::from_millis(200));
    let elapsed = start.elapsed();
    server.join().expect("recorded server");
    let message = result.expect_err("the slow model list exceeds the total time");
    assert!(message.contains("before reading installed models"), "stage missing: {message}");
    assert!(elapsed < Duration::from_millis(700), "model list exceeded the total time: {elapsed:?}");
}

#[test]
fn ollama_discovery_caps_a_probe_by_the_remaining_total_time() {
    let (ep, server) = recorded_ollama_delayed(vec![
        ("GET /api/tags HTTP/1.1", None,
            json!({"models":[{"name":"slow-tool:latest"}]}), Duration::from_millis(500)),
        ("POST /api/show HTTP/1.1", Some("slow-tool:latest"),
            json!({"capabilities":["tools"]}), Duration::from_secs(2)),
    ]);
    let start = Instant::now();
    let result = discover_ollama_within(
        &Client::new(true), &ep, &AtomicBool::new(false), Duration::from_secs(1));
    let elapsed = start.elapsed();
    let requests = server.join().expect("recorded server");
    let message = result.expect_err("the slow probe exceeds the remaining total time");
    assert!(message.contains("stopped after 1 s"), "limit missing: {message}");
    assert!(message.contains("1 of 1 installed models"), "progress missing: {message}");
    assert_eq!(requests.len(), 2, "the model list consumed part of the time before the probe");
    assert!(elapsed < Duration::from_millis(1300), "probe exceeded the remaining time: {elapsed:?}");
}

#[test]
fn ollama_chat_failure_names_the_automatic_model() {
    let steps = vec![
        ("GET /api/tags HTTP/1.1", None, json!({"models":[{"name":"installed-tool:latest"}]})),
        ("POST /api/show HTTP/1.1", Some("installed-tool:latest"),
            json!({"capabilities":["completion","tools"]})),
        // The fixture answers HTTP 404 to a model that it does not expect.
        ("POST /v1/chat/completions HTTP/1.1", Some("another-model"), json!({})),
    ];
    let (ep, server) = recorded_ollama(steps);
    let result = HttpTransport.complete_at(
        &local_request("local-ollama"), &AtomicBool::new(false), &ep, None);
    server.join().expect("recorded server");
    let message = result.expect_err("the chat request failed");
    assert!(message.contains("automatic model 'installed-tool:latest'"), "model missing: {message}");
}
