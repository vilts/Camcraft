//! Startup GitHub release update check (#2428 Part B).
//!
//! Checks `api.github.com/repos/NeoTech/Camcraft/releases/latest` at most once
//! every 24 hours when enabled by the user's `prefs.json` setting. Silent on
//! offline, timeout, rate limit, or parse errors; never collects telemetry or
//! credentials; disabled in the browser (`wasm32`) build.
//!
//! Provenance: our own client over the public GitHub Releases REST API docs
//! and the Semantic Versioning 2.0.0 specification; no vendor material consulted.

use serde::Deserialize;
use url::Url;

/// GitHub REST endpoint for the latest published non-prerelease release.
pub const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/NeoTech/Camcraft/releases/latest";

/// Canonical base URL for release-notes pages on GitHub.
pub const RELEASES_TAG_BASE: &str = "https://github.com/NeoTech/Camcraft/releases/tag/";

/// Minimum interval between automatic startup checks (24 hours in seconds).
pub const CHECK_INTERVAL_SECS: u64 = 24 * 60 * 60;

#[cfg(not(target_arch = "wasm32"))]
const MAX_RESPONSE_BYTES: usize = 512 * 1024;
#[cfg(not(target_arch = "wasm32"))]
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(4);
#[cfg(not(target_arch = "wasm32"))]
const TOTAL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(6);

/// Parsed semantic version (`MAJOR.MINOR.PATCH` with optional `-prerelease`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemVer {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: Option<String>,
}

impl SemVer {
    /// Format `MAJOR.MINOR.PATCH` (without leading `v` or build metadata).
    #[must_use]
    pub fn canonical_core(&self) -> String {
        format!("{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Information about a newer release available on GitHub.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// Canonical `MAJOR.MINOR.PATCH` version string (no leading `v`).
    pub version: String,
    /// Validated HTTPS release-notes page on GitHub.
    pub release_notes_url: String,
}

#[derive(Deserialize)]
struct LatestReleasePayload {
    tag_name: String,
    #[serde(default)]
    html_url: Option<String>,
    #[serde(default)]
    prerelease: Option<bool>,
    #[serde(default)]
    draft: Option<bool>,
}

fn parse_core_part(raw: &str) -> Option<u64> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    if raw.len() > 1 && raw.starts_with('0') {
        return None;
    }
    raw.parse::<u64>().ok()
}

fn valid_semver_meta(raw: &str) -> bool {
    !raw.is_empty()
        && raw.split('.').all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Parse a `SemVer` tag such as `"v0.26.0"` or `"0.25.0-rc.1+build.2"`.
#[must_use]
pub fn parse_semver(raw: &str) -> Option<SemVer> {
    let trimmed = raw.trim();
    let without_v = trimmed
        .strip_prefix('v')
        .or_else(|| trimmed.strip_prefix('V'))
        .unwrap_or(trimmed);
    let core_and_pre = match without_v.split_once('+') {
        Some((head, build)) => {
            if !valid_semver_meta(build) {
                return None;
            }
            head
        }
        None => without_v,
    };
    let (core, pre) = match core_and_pre.split_once('-') {
        Some((head, pre)) => {
            if !valid_semver_meta(pre) {
                return None;
            }
            (head, Some(pre.to_owned()))
        }
        None => (core_and_pre, None),
    };
    let mut parts = core.split('.');
    let major = parse_core_part(parts.next()?)?;
    let minor = parse_core_part(parts.next()?)?;
    let patch = parse_core_part(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some(SemVer { major, minor, patch, pre })
}

/// Return `true` when `candidate` is a stable release strictly newer than `current`.
#[must_use]
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    let (Some(cur), Some(cand)) = (parse_semver(current), parse_semver(candidate)) else {
        return false;
    };
    if cand.pre.is_some() {
        return false;
    }
    let cur_tuple = (cur.major, cur.minor, cur.patch);
    let cand_tuple = (cand.major, cand.minor, cand.patch);
    cand_tuple > cur_tuple || (cand_tuple == cur_tuple && cur.pre.is_some())
}

/// Return `true` when automatic checking is enabled and at least 24 hours
/// have elapsed since `last_check_utc` (or no previous check was recorded).
#[must_use]
pub fn should_check_now(check_for_updates: bool, last_check_utc: Option<u64>, now_utc: u64) -> bool {
    if !check_for_updates {
        return false;
    }
    match last_check_utc {
        None => true,
        Some(last) => {
            now_utc.saturating_sub(last) >= CHECK_INTERVAL_SECS
                || last.saturating_sub(now_utc) > CHECK_INTERVAL_SECS
        }
    }
}

/// Current UTC time as whole seconds since the Unix epoch (native only).
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn current_unix_secs() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Canonical release-notes URL for a parsed version string (`"0.26.0"`).
#[must_use]
pub fn default_release_url(version: &str) -> String {
    format!("{RELEASES_TAG_BASE}v{version}")
}

/// Confirm `raw` is a clean HTTPS release page on `github.com/NeoTech/Camcraft`.
#[must_use]
pub fn is_safe_release_url(raw: &str) -> bool {
    if raw.chars().any(char::is_control) {
        return false;
    }
    let Ok(parsed) = Url::parse(raw) else {
        return false;
    };
    parsed.scheme() == "https"
        && parsed
            .host_str()
            .is_some_and(|h| h.eq_ignore_ascii_case("github.com"))
        && parsed
            .path()
            .to_ascii_lowercase()
            .starts_with("/neotech/camcraft/releases/")
}

fn sanitize_release_url(html_url: Option<&str>, version: &str) -> String {
    match html_url.map(str::trim) {
        Some(raw) if is_safe_release_url(raw) => match Url::parse(raw) {
            Ok(url) => url.into(),
            Err(_) => default_release_url(version),
        },
        _ => default_release_url(version),
    }
}

/// Parse GitHub's `/releases/latest` JSON body and return `Some(ReleaseInfo)`
/// only when it names a stable release newer than `current_version`.
pub fn parse_latest_release(current_version: &str, body: &str) -> Result<Option<ReleaseInfo>, String> {
    let payload: LatestReleasePayload =
        serde_json::from_str(body).map_err(|e| format!("release response is not valid JSON: {e}"))?;
    if payload.draft == Some(true) || payload.prerelease == Some(true) {
        return Ok(None);
    }
    let Some(parsed) = parse_semver(&payload.tag_name) else {
        return Err(format!("release tag '{}' is not a semantic version", payload.tag_name));
    };
    if !is_newer_version(current_version, &payload.tag_name) {
        return Ok(None);
    }
    let version = parsed.canonical_core();
    let release_notes_url = sanitize_release_url(payload.html_url.as_deref(), &version);
    Ok(Some(ReleaseInfo { version, release_notes_url }))
}

/// Query GitHub Releases for a version newer than `current_version` (native only).
#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_latest_release(current_version: &str) -> Result<Option<ReleaseInfo>, String> {
    fetch_latest_release_from(LATEST_RELEASE_API_URL, current_version)
}

/// Query `api_url` for a release JSON payload and compare it against `current_version`.
#[cfg(not(target_arch = "wasm32"))]
pub fn fetch_latest_release_from(
    api_url: &str,
    current_version: &str,
) -> Result<Option<ReleaseInfo>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("HTTP runtime unavailable: {e}"))?;
    runtime.block_on(fetch_latest_release_async(api_url, current_version))
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_latest_release_async(
    api_url: &str,
    current_version: &str,
) -> Result<Option<ReleaseInfo>, String> {
    let is_loopback = Url::parse(api_url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.eq_ignore_ascii_case("localhost") || h == "127.0.0.1"))
        .unwrap_or(false);
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(TOTAL_TIMEOUT)
        .user_agent(concat!("camcraft/", env!("CARGO_PKG_VERSION")));
    if is_loopback {
        builder = builder.no_proxy();
    }
    let client = builder
        .build()
        .map_err(|_| String::from("update check HTTP client unavailable"))?;
    let resp = client
        .get(api_url)
        .header("accept", "application/vnd.github+json")
        .header("x-github-api-version", "2022-11-28")
        .send()
        .await
        .map_err(|_| String::from("update check could not reach release server"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("release server returned HTTP {}", status.as_u16()));
    }
    if resp
        .content_length()
        .is_some_and(|len| len > MAX_RESPONSE_BYTES as u64)
    {
        return Err(String::from("release response exceeded size limit"));
    }
    let bytes = resp
        .bytes()
        .await
        .map_err(|_| String::from("release response could not be read"))?;
    if bytes.len() > MAX_RESPONSE_BYTES {
        return Err(String::from("release response exceeded size limit"));
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| String::from("release response was not UTF-8"))?;
    parse_latest_release(current_version, text)
}
