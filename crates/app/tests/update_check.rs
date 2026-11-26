//! Startup update check tests (#2428 Part B): semantic version comparison,
//! once-per-day rate limiting, GitHub release payload parsing/sanitization,
//! and silent HTTP failure handling via loopback fixtures (zero external calls).
//!
//! Provenance: our own tests specced from issue #2428 Part B; no vendor
//! material consulted.
//!
//! Gate map (M4.4): covers app::update_check.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use camcraft_app::update_check::{
    CHECK_INTERVAL_SECS, ReleaseInfo, default_release_url, fetch_latest_release_from,
    is_newer_version, is_safe_release_url, parse_latest_release, parse_semver, should_check_now,
};

#[test]
fn semver_parsing_and_comparison_cover_stable_prerelease_and_invalid_tags() {
    let parsed = parse_semver("v0.26.1").expect("valid v-prefixed version");
    assert_eq!((parsed.major, parsed.minor, parsed.patch, parsed.pre), (0, 26, 1, None));
    assert_eq!(parse_semver("0.25.0-rc.1+build.9").and_then(|v| v.pre), Some("rc.1".into()));

    assert!(is_newer_version("0.25.0", "v0.26.0"));
    assert!(is_newer_version("0.25.0", "0.25.1"));
    assert!(is_newer_version("0.25.0", "v1.0.0"));
    assert!(is_newer_version("0.25.9", "v0.25.10"));
    assert!(is_newer_version("0.25.0-rc.1", "v0.25.0"));

    assert!(!is_newer_version("0.25.0", "v0.25.0"));
    assert!(!is_newer_version("0.25.0", "0.25.0+meta"));
    assert!(!is_newer_version("0.25.0", "v0.24.9"));
    assert!(!is_newer_version("0.25.0", "v0.26.0-alpha.1"));

    for bad in ["", "v0.25", "0.25.0.1", "v0.25.x", "0.01.0", "-0.25.0", "v0.25.0+"] {
        assert_eq!(parse_semver(bad), None, "expected {bad:?} to be rejected");
        assert!(!is_newer_version("0.25.0", bad));
    }
}

#[test]
fn once_per_day_throttle_enforces_24_hour_window_and_disabled_setting() {
    let base = 1_795_000_000_u64;
    assert!(!should_check_now(false, None, base));
    assert!(!should_check_now(false, Some(base - CHECK_INTERVAL_SECS), base));
    assert!(should_check_now(true, None, base));
    assert!(!should_check_now(true, Some(base), base + CHECK_INTERVAL_SECS - 1));
    assert!(should_check_now(true, Some(base), base + CHECK_INTERVAL_SECS));
    assert!(!should_check_now(true, Some(base), base - 60));
    assert!(should_check_now(true, Some(base + CHECK_INTERVAL_SECS + 1), base));
}

#[test]
fn parse_latest_release_sanitizes_urls_and_ignores_non_upgrades() {
    let newer = r#"{
        "tag_name": "v0.26.0",
        "html_url": "https://github.com/NeoTech/Camcraft/releases/tag/v0.26.0",
        "draft": false,
        "prerelease": false
    }"#;
    assert_eq!(
        parse_latest_release("0.25.0", newer).expect("valid payload"),
        Some(ReleaseInfo {
            version: "0.26.0".into(),
            release_notes_url: "https://github.com/NeoTech/Camcraft/releases/tag/v0.26.0".into(),
        })
    );

    let same = r#"{"tag_name": "v0.25.0", "html_url": "https://github.com/NeoTech/Camcraft/releases/tag/v0.25.0"}"#;
    assert_eq!(parse_latest_release("0.25.0", same).expect("same version"), None);

    let draft = r#"{"tag_name": "v0.26.0", "draft": true}"#;
    assert_eq!(parse_latest_release("0.25.0", draft).expect("draft ignored"), None);

    let pre = r#"{"tag_name": "v0.26.0", "prerelease": true}"#;
    assert_eq!(parse_latest_release("0.25.0", pre).expect("prerelease ignored"), None);

    for unsafe_url in [
        "http://github.com/NeoTech/Camcraft/releases/tag/v0.26.0",
        "https://evil.example/NeoTech/Camcraft/releases/tag/v0.26.0",
        "https://github.com/other/repo/releases/tag/v0.26.0",
        "https://github.com/NeoTech/Camcraft/releases/tag/v0.26.0\ncmd",
    ] {
        assert!(!is_safe_release_url(unsafe_url), "{unsafe_url}");
        let body = serde_json::json!({ "tag_name": "v0.26.0", "html_url": unsafe_url }).to_string();
        let parsed = parse_latest_release("0.25.0", &body)
            .expect("falls back to canonical URL")
            .expect("newer version");
        assert_eq!(parsed.release_notes_url, default_release_url("0.26.0"));
    }

    assert!(parse_latest_release("0.25.0", "{ not json").is_err());
    assert!(parse_latest_release("0.25.0", r#"{"tag_name":"latest"}"#).is_err());
}

fn read_http_request(stream: &mut TcpStream) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(3))).expect("timeout");
    let mut buf = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut chunk).expect("read request");
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn serve_once(status_line: &str, body: &str) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let addr = listener.local_addr().expect("local addr");
    let status = status_line.to_owned();
    let payload = body.to_owned();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let req = read_http_request(&mut stream);
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
            payload.len()
        );
        stream.write_all(response.as_bytes()).expect("write reply");
        req
    });
    (format!("http://{addr}/repos/NeoTech/Camcraft/releases/latest"), handle)
}

#[test]
fn loopback_http_returns_newer_release_without_auth_and_handles_errors_cleanly() {
    let newer_body = r#"{"tag_name":"v99.1.0","html_url":"https://github.com/NeoTech/Camcraft/releases/tag/v99.1.0"}"#;
    let (url, server) = serve_once("200 OK", newer_body);
    let info = fetch_latest_release_from(&url, "0.25.0")
        .expect("200 OK parses")
        .expect("99.1.0 is newer than 0.25.0");
    assert_eq!(info.version, "99.1.0");
    assert_eq!(info.release_notes_url, "https://github.com/NeoTech/Camcraft/releases/tag/v99.1.0");
    let req = server.join().expect("server");
    let req_lower = req.to_ascii_lowercase();
    assert!(req_lower.contains("user-agent: camcraft/"), "User-Agent required by GitHub API: {req}");
    assert!(!req_lower.contains("authorization:"), "no credentials or tokens sent: {req}");

    let same_body = r#"{"tag_name":"v0.25.0"}"#;
    let (url, server) = serve_once("200 OK", same_body);
    assert_eq!(fetch_latest_release_from(&url, "0.25.0").expect("200 same"), None);
    let _ = server.join();

    let (url, server) = serve_once("403 Forbidden", r#"{"message":"API rate limit exceeded"}"#);
    assert!(fetch_latest_release_from(&url, "0.25.0").is_err());
    let _ = server.join();

    let (url, server) = serve_once("500 Internal Server Error", "oops");
    assert!(fetch_latest_release_from(&url, "0.25.0").is_err());
    let _ = server.join();

    let (url, server) = serve_once("200 OK", "not valid json");
    assert!(fetch_latest_release_from(&url, "0.25.0").is_err());
    let _ = server.join();

    let dead = TcpListener::bind("127.0.0.1:0").expect("ephemeral port");
    let closed_port = dead.local_addr().expect("addr").port();
    drop(dead);
    let closed_url = format!("http://127.0.0.1:{closed_port}/releases/latest");
    assert!(fetch_latest_release_from(&closed_url, "0.25.0").is_err());
}
