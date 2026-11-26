//! Provenance: PR #3798 review on cba232f82 (reporter finding 7); our own loopback HTTP fixture,
//! no real credentials or remote services.
//! Gate map (M4.4): covers ui::egui_app::ai_chat::settings_native, ui::egui_app::ai_chat::settings_text.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use camcraft_app::ai::http::{Client, Timeouts};
use camcraft_app::ai::{ApiKey, Endpoint};
use camcraft_app::mcp::provider::Provider;

use crate::egui_app::ai_chat::settings_native::cloud_test;

const KEY: &str = "placeholder-not-a-real-key";

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

/// A provider answer with separate message, type and code fields reaches the
/// Test text and the log. The key is removed, also from the type field.
#[test]
fn failed_cloud_test_shows_provider_type_code_and_endpoint() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let base = format!("http://{}/v1", listener.local_addr().expect("addr"));
    let ep = Endpoint::parse(&base).expect("endpoint");
    let body = serde_json::json!({"error": {
        "message": format!("Incorrect API key provided: {KEY}"),
        "type": format!("invalid_request_error for {KEY}"),
        "code": "invalid_api_key",
    }})
    .to_string();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let client = Client {
        offline_only: true,
        timeouts: Timeouts {
            connect: Duration::from_secs(2),
            total: Duration::from_secs(5),
        },
    };
    let key = ApiKey::new(KEY).expect("placeholder");
    let text = cloud_test(&client, &ep, Provider::OpenAiChat, "gpt-test", &key)
        .expect_err("401 fails the Test");
    server.join().expect("server");
    assert!(text.contains("rejected the key (HTTP 401)"), "{text}");
    assert!(text.contains(&format!("Model tried: gpt-test at {base}")), "{text}");
    assert!(text.contains("Incorrect API key provided"), "message: {text}");
    assert!(text.contains("type: invalid_request_error"), "type: {text}");
    assert!(text.contains("code: invalid_api_key"), "code: {text}");
    assert!(
        text.contains(&format!("endpoint: POST {base}/chat/completions")),
        "request endpoint: {text}"
    );
    assert!(!text.contains(KEY), "key leaked: {text}");
    assert!(!text.contains("placeholder"), "partial key leaked: {text}");
    assert!(text.contains("[redacted]"), "{text}");
}

/// Anthropic authentication failure (HTTP 401 with authentication_error) shows
/// key rejected and preserves provider error type (#3804).
#[test]
fn anthropic_cloud_test_authentication_failure_shows_key_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let base = format!("http://{}/v1", listener.local_addr().expect("addr"));
    let ep = Endpoint::parse(&base).expect("endpoint");
    let body = serde_json::json!({
        "type": "error",
        "error": {
            "type": "authentication_error",
            "message": format!("invalid x-api-key: {KEY}"),
        }
    })
    .to_string();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let client = Client {
        offline_only: true,
        timeouts: Timeouts {
            connect: Duration::from_secs(2),
            total: Duration::from_secs(5),
        },
    };
    let key = ApiKey::new(KEY).expect("placeholder");
    let text = cloud_test(&client, &ep, Provider::Anthropic, "claude-sonnet-test", &key)
        .expect_err("401 fails the Test");
    server.join().expect("server");
    assert!(text.contains("rejected the key (HTTP 401)"), "{text}");
    assert!(text.contains(&format!("Model tried: claude-sonnet-test at {base}")), "{text}");
    assert!(text.contains("invalid x-api-key"), "message: {text}");
    assert!(text.contains("type: authentication_error"), "type: {text}");
    assert!(
        text.contains(&format!("endpoint: POST {base}/messages")),
        "request endpoint: {text}"
    );
    assert!(!text.contains(KEY), "key leaked: {text}");
    assert!(text.contains("[redacted]"), "{text}");
}

/// Google Gemini returns HTTP 400 with API_KEY_INVALID on an invalid key.
/// This must be classified as key rejected, not a bad model name (#3804).
#[test]
fn gemini_cloud_test_bad_request_auth_failure_shows_key_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let base = format!("http://{}/v1beta", listener.local_addr().expect("addr"));
    let ep = Endpoint::parse(&base).expect("endpoint");
    let body = serde_json::json!({
        "error": {
            "code": 400,
            "message": format!("API key not valid. Please pass a valid API key: {KEY}"),
            "status": "INVALID_ARGUMENT",
            "details": [
                {
                    "@type": "type.googleapis.com/google.rpc.ErrorInfo",
                    "reason": "API_KEY_INVALID"
                }
            ]
        }
    })
    .to_string();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 400 Bad Request\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let client = Client {
        offline_only: true,
        timeouts: Timeouts {
            connect: Duration::from_secs(2),
            total: Duration::from_secs(5),
        },
    };
    let key = ApiKey::new(KEY).expect("placeholder");
    let text = cloud_test(&client, &ep, Provider::Gemini, "gemini-test", &key)
        .expect_err("400 fails the Test");
    server.join().expect("server");
    assert!(text.contains("rejected the key (HTTP 400)"), "{text}");
    assert!(text.contains(&format!("Model tried: gemini-test at {base}")), "{text}");
    assert!(text.contains("API key not valid"), "message: {text}");
    assert!(text.contains("status: INVALID_ARGUMENT"), "status: {text}");
    assert!(
        text.contains(&format!("endpoint: POST {base}/models/gemini-test:generateContent")),
        "request endpoint: {text}"
    );
    assert!(!text.contains(KEY), "key leaked: {text}");
    assert!(text.contains("[redacted]"), "{text}");
}

/// Google Gemini HTTP 403 Forbidden with unregistered caller shows key rejected (#3804).
#[test]
fn gemini_cloud_test_forbidden_auth_failure_shows_key_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let base = format!("http://{}/v1beta", listener.local_addr().expect("addr"));
    let ep = Endpoint::parse(&base).expect("endpoint");
    let body = serde_json::json!({
        "error": {
            "code": 403,
            "message": "Method doesn't allow unregistered callers. Please use API key.",
            "status": "PERMISSION_DENIED"
        }
    })
    .to_string();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 403 Forbidden\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let client = Client {
        offline_only: true,
        timeouts: Timeouts {
            connect: Duration::from_secs(2),
            total: Duration::from_secs(5),
        },
    };
    let key = ApiKey::new(KEY).expect("placeholder");
    let text = cloud_test(&client, &ep, Provider::Gemini, "gemini-test", &key)
        .expect_err("403 fails the Test");
    server.join().expect("server");
    assert!(text.contains("rejected the key (HTTP 403)"), "{text}");
    assert!(text.contains("status: PERMISSION_DENIED"), "status: {text}");
    assert!(!text.contains("rejected the key (HTTP 401)"));
}

/// Non-authentication failures (wrong model HTTP 404, rate limit HTTP 429) remain
/// distinct from key rejected (#3804).
#[test]
fn non_auth_cloud_test_failures_remain_distinct_from_key_rejected() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("loopback");
    let base = format!("http://{}/v1", listener.local_addr().expect("addr"));
    let ep = Endpoint::parse(&base).expect("endpoint");
    let body = serde_json::json!({
        "error": {
            "message": "The model `nonexistent-model` does not exist",
            "type": "invalid_request_error",
            "code": "model_not_found"
        }
    })
    .to_string();
    let server = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        read_request(&mut s);
        write!(
            s,
            "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .expect("reply");
    });
    let client = Client {
        offline_only: true,
        timeouts: Timeouts {
            connect: Duration::from_secs(2),
            total: Duration::from_secs(5),
        },
    };
    let key = ApiKey::new(KEY).expect("placeholder");
    let text = cloud_test(&client, &ep, Provider::OpenAiChat, "nonexistent-model", &key)
        .expect_err("404 fails the Test");
    server.join().expect("server");
    assert!(!text.contains("rejected the key"), "must not say key rejected: {text}");
    assert!(text.contains("the provider does not know this model (HTTP 404)"), "{text}");
    assert!(text.contains("code: model_not_found"), "code: {text}");
}
