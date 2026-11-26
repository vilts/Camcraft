//! The native HTTP client for AI providers (#1945). Every request passes
//! the endpoint laws (`endpoint::admit`) before anything resolves, then
//! the resolver re-checks the addresses: offline-only, or plain `http`,
//! reaches loopback addresses alone. No redirect is followed (a local
//! server could otherwise bounce a call off the machine), no proxy is
//! read from the environment, and no error message carries the key.
//!
//! Provenance: our own policy over the `reqwest` crate's public API; the
//! auth headers are each provider's documented ones (`x-api-key`,
//! `Authorization: Bearer`, `x-goog-api-key`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::Value;

use crate::mcp::provider::Provider;

use super::endpoint::{self, Endpoint, Scheme};
use super::probe::{self, Capabilities};
use super::secret::ApiKey;
use super::wire::{self, ChatRequest};

/// How long one call may take: connect, then the whole answer.
#[derive(Clone, Copy, Debug)]
pub struct Timeouts {
    pub connect: Duration,
    pub total: Duration,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            total: Duration::from_secs(300),
        }
    }
}

/// The client: the offline switch and the timeouts. A connection is built
/// per request, so each carries its own loopback rule.
#[derive(Clone, Copy, Debug)]
pub struct Client {
    pub offline_only: bool,
    pub timeouts: Timeouts,
}

/// The longest provider error text kept in a message.
const ERROR_TEXT: usize = 400;

impl Client {
    #[must_use]
    pub fn new(offline_only: bool) -> Self {
        Self {
            offline_only,
            timeouts: Timeouts::default(),
        }
    }

    /// POST `body` as JSON to `path` under `ep`, with `auth` as the auth
    /// header when given, and read a JSON answer. A non-2xx status is an
    /// error naming the status and the provider's message.
    pub fn post_json(
        &self,
        ep: &Endpoint,
        path: &str,
        headers: &[(String, String)],
        auth: Option<(&'static str, String)>,
        body: &Value,
    ) -> Result<Value, String> {
        self.post_json_cancellable(ep, path, headers, auth, body, &AtomicBool::new(false))
    }

    /// As `post_json`, but drops the active connection when cancelled.
    pub fn post_json_cancellable(
        &self,
        ep: &Endpoint,
        path: &str,
        headers: &[(String, String)],
        auth: Option<(&'static str, String)>,
        body: &Value,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        self.request_json_cancellable(ep, path, headers, auth, Some(body), cancel)
    }

    /// GET JSON with the same endpoint, cancellation, and diagnostic policies as POST.
    pub fn get_json_cancellable(
        &self,
        ep: &Endpoint,
        path: &str,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        self.request_json_cancellable(ep, path, &[], None, None, cancel)
    }

    fn request_json_cancellable(
        &self,
        ep: &Endpoint,
        path: &str,
        headers: &[(String, String)],
        auth: Option<(&'static str, String)>,
        body: Option<&Value>,
        cancel: &AtomicBool,
    ) -> Result<Value, String> {
        endpoint::admit(ep, self.offline_only)?;
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".into());
        }
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("HTTP runtime unavailable: {e}"))?;
        runtime.block_on(async {
            let request = self.request_async(ep, path, headers, auth.as_ref(), body);
            tokio::select! {
                result = request => result,
                () = async {
                    while !cancel.load(Ordering::Relaxed) {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                } => Err("cancelled".into()),
            }
        })
    }

    async fn request_async(
        &self,
        ep: &Endpoint,
        path: &str,
        headers: &[(String, String)],
        auth: Option<&(&'static str, String)>,
        body: Option<&Value>,
    ) -> Result<Value, String> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(self.timeouts.connect)
            .timeout(self.timeouts.total)
            .dns_resolver(std::sync::Arc::new(CheckedResolver {
                loopback_only: self.offline_only || ep.scheme == Scheme::Http,
            }))
            .user_agent(concat!("camcraft/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|_| "HTTP client unavailable".to_owned())?;
        let mut req = match body {
            Some(body) => client.post(ep.url(path))
                .header("content-type", "application/json")
                .body(body.to_string()),
            None => client.get(ep.url(path)),
        };
        for (name, value) in headers {
            req = req.header(name, value);
        }
        if let Some((name, value)) = auth {
            req = req.header(*name, value);
        }
        // Do not expose transport diagnostics: they can include header values.
        let resp = req
            .send()
            .await
            .map_err(|_| format!("{} could not be reached", ep.host))?;
        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|_| format!("{} answer not read", ep.host))?;
        if !status.is_success() {
            let secret = auth.map(|(name, value)| {
                if name.eq_ignore_ascii_case("authorization") {
                    value.strip_prefix("Bearer ").unwrap_or(value.as_str())
                } else {
                    value.as_str()
                }
            });
            return Err(format!(
                "{} answered HTTP {}: {} (endpoint: POST {})",
                ep.host,
                status.as_u16(),
                provider_message(&text, secret),
                ep.url(path)
            ));
        }
        serde_json::from_str(&text).map_err(|e| format!("{} answer is not JSON ({e})", ep.host))
    }
}

/// Resolve inside the cancellable future and enforce the address policy before connecting.
struct CheckedResolver {
    loopback_only: bool,
}

impl reqwest::dns::Resolve for CheckedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let loopback_only = self.loopback_only;
        Box::pin(async move {
            let addrs = tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            let checked =
                endpoint::admit_addrs(addrs, loopback_only).map_err(std::io::Error::other)?;
            Ok(Box::new(checked.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// The provider's error text: `error.message`, then the short `error.type`,
/// `error.code` and `error.status` fields when present. Redact before
/// truncating, including keys that cross the text limit.
fn provider_message(text: &str, secret: Option<&str>) -> String {
    let parsed: Option<Value> = serde_json::from_str(text).ok();
    let msg = parsed
        .as_ref()
        .and_then(|v| {
            let error = &v["error"];
            let message = error["message"].as_str().or_else(|| error.as_str())?;
            let fields: Vec<String> = ["type", "code", "status"]
                .iter()
                .filter_map(|name| error_field(&error[*name]).map(|f| format!("{name}: {f}")))
                .collect();
            Some(if fields.is_empty() {
                message.to_owned()
            } else {
                format!("{message} [{}]", fields.join(", "))
            })
        })
        .unwrap_or_else(|| text.to_owned());
    let redacted = match secret.filter(|key| !key.is_empty()) {
        Some(key) => msg.replace(key, "[redacted]"),
        None => msg,
    };
    redacted.chars().take(ERROR_TEXT).collect()
}

/// A scalar error field (a string or a number); other shapes are dropped.
/// It is not cut here: the caller redacts the whole text, then truncates it.
fn error_field(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_owned()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// The auth header for `provider`, per its documentation.
#[must_use]
pub fn auth_header(provider: Provider, key: &ApiKey) -> (&'static str, String) {
    match provider {
        Provider::Anthropic => ("x-api-key", key.expose().to_owned()),
        Provider::OpenAiChat | Provider::OpenAiResponses => {
            ("authorization", format!("Bearer {}", key.expose()))
        }
        Provider::Gemini => ("x-goog-api-key", key.expose().to_owned()),
    }
}

/// Send one chat step and return the provider's response envelope.
pub fn send_chat(
    client: &Client,
    ep: &Endpoint,
    req: &ChatRequest<'_>,
    key: Option<&ApiKey>,
    cancel: &AtomicBool,
) -> Result<Value, String> {
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    let w = wire::build(req)?;
    let auth = key.map(|k| auth_header(req.provider, k));
    let out = client.post_json_cancellable(ep, &w.path, &w.headers, auth, &w.body, cancel)?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    Ok(out)
}

/// Ask an Ollama server what `model` can do. `ep` is the chat base
/// (`.../v1`); the probe goes to `/api/show` on the same host and port.
pub fn probe_ollama(client: &Client, ep: &Endpoint, model: &str) -> Result<Capabilities, String> {
    probe_ollama_cancellable(client, ep, model, &AtomicBool::new(false))
}

/// Probe with the same cancellation policy as chat requests.
pub fn probe_ollama_cancellable(
    client: &Client,
    ep: &Endpoint,
    model: &str,
    cancel: &AtomicBool,
) -> Result<Capabilities, String> {
    wire::check_model(model)?;
    let root = ep.with_base_path("");
    let answer = client.post_json_cancellable(
        &root,
        "/api/show",
        &[],
        None,
        &probe::show_request(model),
        cancel,
    )?;
    probe::parse_show(&answer)
}

/// Choose the first installed model that advertises native tool calls.
/// Discovery stays on the chat endpoint's host and uses its offline policy.
pub fn discover_ollama_cancellable(
    client: &Client,
    ep: &Endpoint,
    cancel: &AtomicBool,
) -> Result<String, String> {
    discover_ollama_within(client, ep, cancel, DISCOVERY_TOTAL)
}

/// One discovery request (the list, or one `/api/show`) may take this long.
const DISCOVERY_STEP: Duration = Duration::from_secs(5);
/// The whole selection over all installed models may take this long.
const DISCOVERY_TOTAL: Duration = Duration::from_secs(20);

/// As `discover_ollama_cancellable`, with the time limit of the whole selection.
/// Each cause has its own message: the server does not answer, no installed
/// model can call tools, the probe of a named model failed, the time ended.
/// A failed probe of one model does not stop the selection.
pub fn discover_ollama_within(
    client: &Client,
    ep: &Endpoint,
    cancel: &AtomicBool,
    total: Duration,
) -> Result<String, String> {
    const ADVICE: &str = "Run `ollama pull ornith-1.5:35b`, then send again. \
                          Get models at https://ollama.com/library";
    let start = crate::clock::Instant::now();
    let step = || {
        let remaining = total.checked_sub(start.elapsed()).filter(|r| !r.is_zero())?;
        Some(Client {
            offline_only: client.offline_only,
            timeouts: Timeouts {
                connect: client.timeouts.connect.min(DISCOVERY_STEP).min(remaining),
                total: client.timeouts.total.min(DISCOVERY_STEP).min(remaining),
            },
        })
    };
    let list_limit = || format!(
        "Ollama model selection stopped after {} s before reading installed models. \
         Enter a model name in Model.", total.as_secs());
    let answer = step().ok_or_else(list_limit)?
        .get_json_cancellable(&ep.with_base_path(""), "/api/tags", cancel)
        .map_err(|e| {
            if cancel.load(Ordering::Relaxed) {
                e
            } else if start.elapsed() >= total {
                list_limit()
            } else {
                format!("Ollama server did not answer its model list ({e}). \
                         Start the Ollama app or run `ollama serve`, then send again.")
            }
        })?;
    let models = probe::parse_tags(&answer)?;
    let mut failed: Vec<(String, String)> = Vec::new();
    let limit = |checked| format!(
        "Ollama model selection stopped after {} s: {checked} of {} installed models \
         checked, and none can call tools. Enter a model name in Model.",
        total.as_secs(), models.len());
    for (checked, model) in models.iter().enumerate() {
        let step = step().ok_or_else(|| limit(checked))?;
        let result = probe_ollama_cancellable(&step, ep, model, cancel);
        if !cancel.load(Ordering::Relaxed) && start.elapsed() >= total {
            return Err(limit(checked + 1));
        }
        match result {
            Ok(caps) if caps.tools => return Ok(model.clone()),
            Ok(_) => {}
            Err(e) if cancel.load(Ordering::Relaxed) => return Err(e),
            Err(e) => failed.push((model.clone(), e)),
        }
    }
    match failed.first() {
        None => Err(format!("Ollama has no installed model that can call tools. {ADVICE}")),
        Some((model, reason)) => Err(format!(
            "Ollama capability probe failed for {} ({model}: {reason}), and no other installed \
             model can call tools. Enter a model name in Model. {ADVICE}",
            failed.iter().map(|(m, _)| format!("'{m}'")).collect::<Vec<_>>().join(", ")
        )),
    }
}
