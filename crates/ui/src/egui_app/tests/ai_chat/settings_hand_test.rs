//! Provenance: PR #3798 reporter hand-test findings (2026-11-24) and our structural harness.
//! Gate map (M4.4): covers ui::egui_app::ai_chat::settings, ui::egui_app::ai_chat::settings_panel,
//! ui::egui_app::ai_chat::settings_text, ui::egui_app::ai_chat::panel.

use super::*;
use crate::egui_app::ai_chat::settings::{
    LOCAL_RECHECK, Settings, SettingsAction, SettingsBackend, SettingsRequest,
};
use crate::egui_app::ai_chat::settings_text::{NO_KEY, ollama_status, test_failure};
use egui_kittest::kittest::NodeT;

/// Records each (model, action) and answers with `status`.
struct Recorder {
    calls: Mutex<Vec<(String, &'static str)>>,
    status: Mutex<Result<String, String>>,
}

impl Recorder {
    fn new(status: Result<&str, &str>) -> Arc<Self> {
        Arc::new(Self {
            calls: Mutex::new(Vec::new()),
            status: Mutex::new(status.map(str::to_owned).map_err(str::to_owned)),
        })
    }
}

impl SettingsBackend for Recorder {
    fn run(&self, request: &SettingsRequest, action: SettingsAction) -> Result<String, String> {
        let name = match action {
            SettingsAction::Check => "check",
            SettingsAction::Test => "test",
            SettingsAction::Save(_) => "save",
            SettingsAction::Remove => "remove",
        };
        self.calls
            .lock()
            .expect("calls")
            .push((request.model.clone(), name));
        self.status.lock().expect("status").clone()
    }
}

fn harness(backend: Arc<Recorder>, provider: &str) -> Harness<'static, ShellUi> {
    let mut h = blank_harness();
    h.state_mut().state.ai_chat.settings.backend = backend;
    h.run_steps(2);
    toggle_ai_chat_via_view_door(&mut h);
    select_provider(&mut h, provider);
    settle(&mut h);
    h
}

fn settle(h: &mut Harness<'static, ShellUi>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.step();
        if !h.state().state.ai_chat.settings.busy() {
            h.run_steps(2);
            return;
        }
        assert!(
            Instant::now() < deadline,
            "provider settings did not finish"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn tested_models(backend: &Recorder) -> Vec<String> {
    let calls = backend.calls.lock().expect("calls");
    calls
        .iter()
        .filter(|(_, action)| *action == "test")
        .map(|(m, _)| m.clone())
        .collect()
}

#[test]
fn empty_cloud_model_test_uses_the_provider_default() {
    let backend = Recorder::new(Ok("Ready (key from keychain)"));
    let mut h = harness(backend.clone(), "Anthropic (Cloud)");
    let default = h
        .state()
        .state
        .ai_chat
        .selected_preset()
        .default_model
        .clone();
    assert!(!default.is_empty());
    h.state_mut().state.ai_chat.model_input.clear();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Test").click();
    settle(&mut h);
    assert_eq!(tested_models(&backend), vec![default]);
}

#[test]
fn model_spaces_are_trimmed_before_test() {
    let backend = Recorder::new(Ok("Ready (key from keychain)"));
    let mut h = harness(backend.clone(), "Anthropic (Cloud)");
    h.state_mut().state.ai_chat.model_input = "  claude-sonnet-5-5 ".into();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Test").click();
    settle(&mut h);
    assert_eq!(
        tested_models(&backend),
        vec!["claude-sonnet-5-5".to_owned()]
    );
}

#[test]
fn test_is_disabled_while_no_key_is_found() {
    let backend = Recorder::new(Ok(NO_KEY));
    let mut h = harness(backend.clone(), "Anthropic (Cloud)");
    assert!(h.query_by_label(NO_KEY).is_some());
    let test = h.get_by_role_and_label(Role::Button, "Test");
    assert!(
        test.accesskit_node().is_disabled(),
        "Test must wait for a key"
    );
    *backend.status.lock().expect("status") = Ok("Ready (key from keychain)".into());
    select_provider(&mut h, "Gemini (Cloud)");
    settle(&mut h);
    let test = h.get_by_role_and_label(Role::Button, "Test");
    assert!(!test.accesskit_node().is_disabled());
    assert!(tested_models(&backend).is_empty());
}

#[test]
fn failed_test_names_the_cause_the_model_and_what_to_try() {
    for (error, cause, hint) in [
        (
            "api.anthropic.com answered HTTP 401: invalid x-api-key",
            "rejected the key",
            "Set API key",
        ),
        (
            "generativelanguage.googleapis.com answered HTTP 400: API key not valid. Please pass a valid API key. [status: INVALID_ARGUMENT]",
            "rejected the key",
            "Set API key",
        ),
        (
            "generativelanguage.googleapis.com answered HTTP 403: Method doesn't allow unregistered callers. [status: PERMISSION_DENIED]",
            "rejected the key",
            "Set API key",
        ),
        (
            "api.anthropic.com answered HTTP 404: model: nope",
            "does not know this model",
            "Model name",
        ),
        (
            "api.anthropic.com answered HTTP 400: Your credit balance is too low",
            "billing or credits",
            "billing page",
        ),
        (
            "api.anthropic.com answered HTTP 429: rate_limit",
            "rate limit",
            "Wait one minute",
        ),
        (
            "api.anthropic.com answered HTTP 503: overloaded",
            "not available now",
            "Wait some minutes",
        ),
        (
            "api.anthropic.com could not be reached",
            "network did not reach",
            "proxy",
        ),
        (
            "model id ' x' is not a valid model name",
            "not valid",
            "letters, digits",
        ),
    ] {
        let text = test_failure(error, "nope", "api.anthropic.com");
        assert!(text.starts_with("Test failed: "), "{text}");
        assert!(text.contains(cause), "{error}: {text}");
        assert!(
            text.contains("Try: ") && text.contains(hint),
            "{error}: {text}"
        );
        assert!(
            text.contains("Model tried: nope at api.anthropic.com"),
            "{text}"
        );
        assert!(text.contains(&format!("Details: {error}")), "{text}");
    }
    assert!(test_failure("h answered HTTP 404: x", "m", "h").contains("(HTTP 404)"));
}

#[test]
fn model_not_found_is_not_a_rejected_key() {
    let text = test_failure(
        "api.openai.com answered HTTP 404: The model `invalid_api_key` does not exist (endpoint: POST https://api.openai.com/v1/models/invalid_api_key)",
        "invalid_api_key",
        "https://api.openai.com/v1",
    );
    assert!(
        !text.contains("rejected the key"),
        "must not say key rejected: {text}"
    );
    assert!(
        text.contains("the provider does not know this model (HTTP 404)"),
        "{text}"
    );
    assert!(text.contains("Model tried: invalid_api_key"), "{text}");
}

#[test]
fn service_failure_is_not_a_rejected_key() {
    let text = test_failure(
        "api.anthropic.com answered HTTP 503: upstream service is unauthenticated (endpoint: POST https://api.anthropic.com/v1/messages)",
        "claude-sonnet",
        "https://api.anthropic.com/v1",
    );
    assert!(
        !text.contains("rejected the key"),
        "must not say key rejected: {text}"
    );
    assert!(
        text.contains("the provider is not available now (HTTP 503)"),
        "{text}"
    );
}

#[test]
fn bad_request_model_is_not_authentication_evidence() {
    for (model, error) in [
        (
            "authentication_error",
            "host answered HTTP 400: Invalid model name: authentication_error [status: INVALID_ARGUMENT] (endpoint: POST https://host/models/authentication_error:generateContent)",
        ),
        (
            "api_key_invalid",
            "host answered HTTP 400: Invalid model name: api_key_invalid [status: INVALID_ARGUMENT] (endpoint: POST https://host/models/api_key_invalid:generateContent)",
        ),
        (
            "api key not valid",
            "host answered HTTP 400: Invalid model name: 'api key not valid' [status: INVALID_ARGUMENT] (endpoint: POST https://host/models/api-key:generateContent)",
        ),
        (
            "api key expired",
            "host answered HTTP 400: The model `api key expired` is not found [status: INVALID_ARGUMENT] (endpoint: POST https://host/models/expired:generateContent)",
        ),
    ] {
        let text = test_failure(error, model, "https://host");
        assert!(
            !text.contains("rejected the key"),
            "must not treat echoed model '{model}' as key rejection: {text}"
        );
        assert!(
            text.contains("the provider refused the request or the model name (HTTP 400)"),
            "must show model refusal for '{model}': {text}"
        );
    }
}

/// Fails the first check like a stopped server, then answers like a started one.
struct StartsLater(AtomicUsize);

impl SettingsBackend for StartsLater {
    fn run(&self, request: &SettingsRequest, action: SettingsAction) -> Result<String, String> {
        let test = matches!(action, SettingsAction::Test);
        let probe = if self.0.fetch_add(1, Ordering::SeqCst) == 0 {
            Err("localhost could not be reached".to_owned())
        } else {
            Ok(())
        };
        ollama_status(probe, &request.model, test)
    }
}

fn finish(settings: &mut Settings, request: &SettingsRequest, now: Instant) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while settings.busy() {
        settings.sync_at(request.clone(), now);
        assert!(Instant::now() < deadline, "backend did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn failed_local_check_runs_again_and_finds_the_started_server() {
    let backend = Arc::new(StartsLater(AtomicUsize::new(0)));
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let request = SettingsRequest {
        preset_id: "local-ollama".into(),
        model: "qwen3.8:latest".into(),
        offline_only: false,
    };
    let now = Instant::now();
    settings.sync_at(request.clone(), now);
    finish(&mut settings, &request, now);
    assert!(
        settings.status.contains("CamCraft does not start it"),
        "{}",
        settings.status
    );
    assert!(settings.status.contains("ollama serve"));
    assert!(
        settings.next_wake(now).is_some(),
        "a failed local check must run again"
    );

    let later = now + LOCAL_RECHECK + Duration::from_millis(1);
    settings.sync_at(request.clone(), later);
    assert!(
        settings.busy(),
        "the check must start again after the delay"
    );
    assert!(
        settings.status.contains("not running"),
        "the status must not flicker"
    );
    finish(&mut settings, &request, later);
    assert_eq!(settings.status, "Ready (local model qwen3.8:latest)");
    assert!(settings.next_wake(later).is_none());
}

#[test]
fn local_test_shows_testing_then_a_named_result() {
    let backend = Arc::new(StartsLater(AtomicUsize::new(1)));
    let mut settings = Settings::default();
    settings.backend = backend;
    let request = SettingsRequest {
        preset_id: "local-ollama".into(),
        model: "qwen3.8:latest".into(),
        offline_only: false,
    };
    let now = Instant::now();
    settings.sync_at(request.clone(), now);
    finish(&mut settings, &request, now);
    settings.start(SettingsAction::Test);
    assert_eq!(settings.status, "Testing...");
    finish(&mut settings, &request, now);
    assert_eq!(
        settings.status,
        "Ready (local model qwen3.8:latest) - Test passed"
    );

    let failed = ollama_status(Err("localhost could not be reached".into()), "m", true);
    assert!(failed.is_err_and(|text| text.starts_with("Test failed: Ollama is not running")));
    let missing = ollama_status(
        Err("localhost answered HTTP 404: not found".into()),
        "m",
        true,
    );
    assert!(missing.is_err_and(|text| text.contains("ollama pull m")));
    let empty = ollama_status(
        Err("localhost answered HTTP 404: not found".into()),
        "",
        false,
    );
    assert!(empty.is_ok_and(|text| text.starts_with("Ollama is running")));
}
