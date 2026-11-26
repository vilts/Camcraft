//! AI provider transport (#1945) against a fake loopback HTTP server:
//! every provider's wire format (path, auth header, body), Ollama's
//! capability probe, offline-only refusing every non-loopback host before
//! a socket opens, plain http refused off the machine, no redirect
//! followed, and no key in any error. No real key is used anywhere: the
//! auth value is a fixed placeholder sent only to 127.0.0.1.
//!
//! Provenance: our own tests, specced from the issue's "Done means" and
//! the public provider request formats.
//!
//! Gate map (M4.4): covers app::ai, app::ai::endpoint, app::ai::http, app::ai::wire, app::ai::probe, app::ai::secret, app::ai::presets.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use camcraft_app::ai::endpoint::{Endpoint, admit_addrs};
use camcraft_app::ai::http::{Client, probe_ollama, send_chat};
use camcraft_app::ai::wire::{ChatMessage, ChatRequest, DEFAULT_MAX_TOKENS, Image};
use camcraft_app::ai::{ApiKey, ENDPOINTS, by_id};
use camcraft_app::mcp::provider::{FallbackMode, Provider};
use serde_json::{Value, json};

const PLACEHOLDER: &str = "placeholder-not-a-real-key";

#[test]
fn cloud_default_catalog_has_one_active_model_per_provider() {
    use camcraft_app::ai::presets::{CLOUD_DEFAULTS, default_model};

    for endpoint in ENDPOINTS.iter().filter(|e| e.key.is_some()) {
        let rows: Vec<_> = CLOUD_DEFAULTS.iter()
            .filter(|entry| entry.provider == endpoint.provider).collect();
        assert_eq!(rows.len(), 1, "{} needs exactly one default", endpoint.id);
        let row = rows[0];
        assert!(!row.retired, "{} default {} is retired", endpoint.id, row.model);
        assert!(!row.model.is_empty(), "{} needs a model name", endpoint.id);
        assert_eq!(default_model(endpoint.provider), row.model);
    }
}

/// One request as the fake server saw it.
struct Seen {
    line: String,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Seen {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

/// Serve `replies` (status, extra header, body) one connection each on a
/// loopback port; every request read is sent back on the channel.
fn fake_server(replies: Vec<(u16, Option<&'static str>, String)>) -> (u16, mpsc::Receiver<Seen>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let port = listener.local_addr().expect("addr").port();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for (status, extra, reply) in replies {
            let Ok((mut s, _)) = listener.accept() else { return };
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            let head_end = loop {
                let Ok(n) = s.read(&mut chunk) else { return };
                if n == 0 {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break p + 4;
                }
            };
            let head = String::from_utf8_lossy(&buf[..head_end]).to_string();
            let mut lines = head.split("\r\n");
            let line = lines.next().unwrap_or_default().to_owned();
            let headers: Vec<(String, String)> = lines
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
                .collect();
            let len: usize = headers
                .iter()
                .find(|(k, _)| k == "content-length")
                .and_then(|(_, v)| v.parse().ok())
                .unwrap_or(0);
            while buf.len() < head_end + len {
                let Ok(n) = s.read(&mut chunk) else { break };
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let body = serde_json::from_slice(&buf[head_end..]).unwrap_or(Value::Null);
            let _ = tx.send(Seen { line, headers, body });
            let extra = extra.map(|h| format!("{h}\r\n")).unwrap_or_default();
            let out = format!(
                "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\n{extra}connection: close\r\n\r\n{reply}",
                reply.len()
            );
            let _ = s.write_all(out.as_bytes());
        }
    });
    (port, rx)
}

fn local(port: u16, base: &str) -> Endpoint {
    Endpoint::parse(&format!("http://127.0.0.1:{port}{base}")).expect("loopback endpoint")
}

fn key() -> ApiKey {
    ApiKey::new(PLACEHOLDER).expect("placeholder")
}

fn user(text: &str) -> ChatMessage {
    ChatMessage { role: "user".into(), text: text.into(), image: None, raw: None }
}

fn tool(raw: Value) -> ChatMessage {
    ChatMessage { role: "tool".into(), text: "done".into(), image: None, raw: Some(raw) }
}

fn assistant(raw: Value) -> ChatMessage {
    ChatMessage { role: "assistant".into(), text: String::new(), image: None, raw: Some(raw) }
}

fn image_user(text: &str) -> ChatMessage {
    let image = Some(Image { mime_type: "image/png".into(), data_base64: "iVBORw0K".into() });
    ChatMessage { image, ..user(text) }
}

/// Send one step to a fake server answering `reply`, return what it saw.
fn round_trip(provider: Provider, base: &str, model: &str, msgs: &[ChatMessage], key: Option<&ApiKey>) -> (Seen, Value) {
    let reply = json!({"ok": true}).to_string();
    let (port, rx) = fake_server(vec![(200, None, reply)]);
    let tools = [json!({"name": "get_job"})];
    let req = ChatRequest {
        provider,
        model,
        system: Some("be brief".into()),
        messages: msgs,
        tools: &tools,
        max_tokens: DEFAULT_MAX_TOKENS,
    };
    let out = send_chat(&Client::new(true), &local(port, base), &req, key, &AtomicBool::new(false))
        .expect("the loopback call succeeds, offline-only included");
    (rx.recv_timeout(Duration::from_secs(5)).expect("the server saw the request"), out)
}

#[test]
fn anthropic_messages_wire_format() {
    let msgs = [
        image_user("what is this"),
        assistant(json!({"content":[{"type":"text","text":"checking"},{"type":"tool_use","id":"t1","name":"get_job","input":{}}]})),
        tool(json!({"type":"tool_result","tool_use_id":"t1","content":"{}","is_error":false})),
        tool(json!({"type":"tool_result","tool_use_id":"t2","content":"{}","is_error":false})),
    ];
    let (seen, out) = round_trip(Provider::Anthropic, "/v1", "claude-x", &msgs, Some(&key()));
    assert_eq!(out, json!({"ok": true}));
    assert_eq!(seen.line, "POST /v1/messages HTTP/1.1");
    assert_eq!(seen.header("x-api-key"), Some(PLACEHOLDER));
    assert_eq!(seen.header("anthropic-version"), Some("2023-06-01"));
    assert_eq!(seen.header("authorization"), None);
    let b = &seen.body;
    assert_eq!((b["model"].as_str(), b["max_tokens"].as_u64()), (Some("claude-x"), Some(4096)));
    assert_eq!(b["system"], "be brief");
    assert_eq!(b["tools"][0]["name"], "get_job");
    let m = b["messages"].as_array().expect("messages");
    assert_eq!(m.len(), 3, "both tool results ride one user message: {m:?}");
    assert_eq!(m[0]["content"][1]["source"]["media_type"], "image/png");
    assert_eq!(m[1]["content"][1]["type"], "tool_use");
    assert_eq!(m[2]["role"], "user");
    assert_eq!(m[2]["content"].as_array().map(Vec::len), Some(2));
    assert!(m[2].get("__merge").is_none());
}

#[test]
fn openai_chat_completions_wire_format() {
    let msgs = [
        image_user("look"),
        assistant(json!({"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"c1","type":"function","function":{"name":"get_job","arguments":"{}"}}]}}]})),
        tool(json!({"role":"tool","tool_call_id":"c1","content":"{}"})),
    ];
    let (seen, _) = round_trip(Provider::OpenAiChat, "/v1", "gpt-x", &msgs, Some(&key()));
    assert_eq!(seen.line, "POST /v1/chat/completions HTTP/1.1");
    assert_eq!(seen.header("authorization"), Some(format!("Bearer {PLACEHOLDER}").as_str()));
    let m = seen.body["messages"].as_array().expect("messages");
    assert_eq!(m[0], json!({"role":"system","content":"be brief"}));
    assert_eq!(m[1]["content"][1]["image_url"]["url"], "data:image/png;base64,iVBORw0K");
    assert_eq!(m[2]["tool_calls"][0]["id"], "c1");
    assert_eq!(m[3]["role"], "tool");
    assert_eq!(seen.body["tools"][0]["name"], "get_job");
}

#[test]
fn openai_responses_wire_format() {
    let msgs = [
        user("plan it"),
        assistant(json!({"output":[{"type":"reasoning","id":"r1"},{"type":"function_call","call_id":"c1","name":"get_job","arguments":"{}"}]})),
        tool(json!({"type":"function_call_output","call_id":"c1","output":"{}"})),
    ];
    let (seen, _) = round_trip(Provider::OpenAiResponses, "/v1", "gpt-5-codex", &msgs, Some(&key()));
    assert_eq!(seen.line, "POST /v1/responses HTTP/1.1");
    assert_eq!(seen.header("authorization"), Some(format!("Bearer {PLACEHOLDER}").as_str()));
    let b = &seen.body;
    assert_eq!(b["instructions"], "be brief");
    assert_eq!(b["max_output_tokens"], 4096);
    let i = b["input"].as_array().expect("input");
    assert_eq!(i[0]["content"][0], json!({"type":"input_text","text":"plan it"}));
    assert_eq!(i[1]["type"], "reasoning", "reasoning items are echoed back");
    assert_eq!(i[2]["type"], "function_call");
    assert_eq!(i[3]["type"], "function_call_output");
}

#[test]
fn gemini_generate_content_wire_format() {
    let msgs = [
        image_user("see"),
        assistant(json!({"candidates":[{"content":{"role":"model","parts":[{"functionCall":{"name":"get_job","args":{}},"thoughtSignature":"sig"}]}}]})),
        tool(json!({"functionResponse":{"name":"get_job","response":{}}})),
        tool(json!({"functionResponse":{"name":"list_features","response":{}}})),
    ];
    let (seen, _) = round_trip(Provider::Gemini, "/v1beta", "gemini-2.5-flash", &msgs, Some(&key()));
    assert_eq!(seen.line, "POST /v1beta/models/gemini-2.5-flash:generateContent HTTP/1.1");
    assert_eq!(seen.header("x-goog-api-key"), Some(PLACEHOLDER));
    let b = &seen.body;
    assert_eq!(b["systemInstruction"]["parts"][0]["text"], "be brief");
    let c = b["contents"].as_array().expect("contents");
    assert_eq!(c.len(), 3, "both function responses ride one turn: {c:?}");
    assert_eq!(c[0]["parts"][1]["inlineData"]["mimeType"], "image/png");
    assert_eq!(c[1]["role"], "model");
    assert_eq!(c[1]["parts"][0]["thoughtSignature"], "sig", "thought signatures survive");
    assert_eq!(c[2]["parts"].as_array().map(Vec::len), Some(2));
}

#[test]
fn local_runtimes_send_no_auth_header() {
    for id in ["ollama", "unsloth-studio", "lm-studio", "llama-server"] {
        let e = by_id(id).expect("preset");
        assert!(e.key.is_none(), "{id}");
        let (seen, _) = round_trip(e.provider, "/v1", "qwen2.5:7b", &[user("hi")], None);
        assert_eq!(seen.line, "POST /v1/chat/completions HTTP/1.1", "{id}");
        assert_eq!(seen.header("authorization"), None, "{id}");
    }
}

#[test]
fn ollama_capability_probe() {
    let replies = vec![
        (200, None, json!({"capabilities":["completion","tools","vision"]}).to_string()),
        (200, None, json!({"capabilities":["completion"]}).to_string()),
    ];
    let (port, rx) = fake_server(replies);
    let ep = local(port, "/v1");
    let c = probe_ollama(&Client::new(true), &ep, "qwen2.5:14b").expect("probe");
    assert_eq!((c.tools, c.vision, c.mode), (true, true, FallbackMode::Native));
    let seen = rx.recv_timeout(Duration::from_secs(5)).expect("seen");
    assert_eq!(seen.line, "POST /api/show HTTP/1.1", "the probe sits beside /v1");
    assert_eq!(seen.body, json!({"model":"qwen2.5:14b"}));
    let c = probe_ollama(&Client::new(true), &ep, "llama3.2").expect("probe");
    assert_eq!((c.tools, c.mode), (false, FallbackMode::PromptJson));
}

#[test]
fn offline_only_blocks_every_non_loopback_host_before_a_socket() {
    let client = Client::new(true);
    let req = ChatRequest {
        provider: Provider::OpenAiChat,
        model: "m",
        system: None,
        messages: &[],
        tools: &[],
        max_tokens: 16,
    };
    let mut hosts: Vec<String> = ENDPOINTS.iter().filter(|e| e.key.is_some()).map(|e| e.base_url.to_owned()).collect();
    // TEST-NET-1 (RFC 5737) and a private address: a socket would hang, a refusal is instant.
    hosts.extend(["https://192.0.2.1/v1".to_owned(), "http://10.1.2.3:11434/v1".to_owned(), "http://[2001:db8::1]/v1".to_owned()]);
    for url in hosts {
        let ep = Endpoint::parse(&url).expect("parses");
        let t = Instant::now();
        let err = send_chat(&client, &ep, &req, Some(&key()), &AtomicBool::new(false)).expect_err("blocked");
        assert!(err.contains("offline-only"), "{url}: {err}");
        assert!(!err.contains(PLACEHOLDER), "{url}: the key leaked into '{err}'");
        assert!(t.elapsed() < Duration::from_millis(500), "{url}: refused before any socket");
    }
    // The resolver half: a loopback-only call refuses any non-loopback address.
    let remote: SocketAddr = "93.184.216.34:443".parse().expect("addr");
    let lo: SocketAddr = "127.0.0.1:11434".parse().expect("addr");
    assert!(admit_addrs(vec![lo, remote], true).is_err());
    assert!(admit_addrs(vec![lo], true).is_ok());
}

#[test]
fn plain_http_off_this_machine_is_refused() {
    let ep = Endpoint::parse("http://192.0.2.1/v1").expect("parses");
    let err = Client::new(false).post_json(&ep, "/x", &[], None, &json!({})).expect_err("refused");
    assert!(err.contains("plain http://"), "{err}");
}

#[test]
fn redirects_are_not_followed_and_errors_never_carry_the_key() {
    let replies = vec![
        (307, Some("location: http://192.0.2.1/v1/messages"), "{}".to_owned()),
        (401, None, json!({"error":{"message":"invalid x-api-key"}}).to_string()),
    ];
    let (port, rx) = fake_server(replies);
    let msgs = [user("hi")];
    let req = ChatRequest {
        provider: Provider::Anthropic,
        model: "m",
        system: None,
        messages: &msgs,
        tools: &[],
        max_tokens: 16,
    };
    let ep = local(port, "/v1");
    let err = send_chat(&Client::new(false), &ep, &req, Some(&key()), &AtomicBool::new(false)).expect_err("307");
    assert!(err.contains("307"), "{err}");
    let err = send_chat(&Client::new(false), &ep, &req, Some(&key()), &AtomicBool::new(false)).expect_err("401");
    assert!(err.contains("401") && err.contains("invalid x-api-key"), "{err}");
    assert!(!err.contains(PLACEHOLDER), "the key leaked into '{err}'");
    assert_eq!(rx.try_iter().count(), 2, "one request each; the redirect was not followed");
}
