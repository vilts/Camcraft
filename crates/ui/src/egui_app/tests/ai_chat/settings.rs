//! Provenance: issue #3781 requirements and our own egui structural test harness.
//! Gate map (M4.4): covers ui::egui_app::ai_chat::panel, ui::egui_app::ai_chat::settings_panel,
//! ui::egui_app::ai_chat::settings, ui::egui_app::ai_chat::settings_native.

use super::*;
use crate::egui_app::ai_chat::EndpointKind;

#[test]
fn cloud_provider_offers_key_setup_and_readiness_before_send() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);
    select_provider(&mut h, "Anthropic (Cloud)");

    assert!(
        h.query_by_role_and_label(Role::Button, "Set API key...")
            .is_some(),
        "a new user must be able to set a cloud key before Send"
    );
    assert!(h.query_by_role_and_label(Role::Button, "Test").is_some());
    assert!(h.query_by_role_and_label(Role::Button, "Remove").is_some());
    assert!(
        h.query_by_label_contains("No key").is_some()
            || h.query_by_label_contains("Ready").is_some()
            || h.query_by_label_contains("Checking").is_some(),
        "the selected provider must show readiness before Send"
    );
}

use crate::egui_app::ai_chat::settings::{
    Settings, SettingsAction, SettingsBackend, SettingsRequest,
};
use camcraft_app::ai::ApiKey;

#[derive(Default)]
struct MemorySettings {
    stored: Mutex<Option<ApiKey>>,
    calls: Mutex<Vec<(String, String)>>,
    source_environment: AtomicBool,
    status_override: Mutex<Option<String>>,
}

impl SettingsBackend for MemorySettings {
    fn run(&self, request: &SettingsRequest, action: SettingsAction) -> Result<String, String> {
        let name = match action {
            SettingsAction::Check => "check",
            SettingsAction::Save(key) => {
                *self.stored.lock().expect("stored key") = Some(key);
                "save"
            }
            SettingsAction::Remove => {
                *self.stored.lock().expect("stored key") = None;
                "remove"
            }
            SettingsAction::Test => "test",
        };
        self.calls
            .lock()
            .expect("calls")
            .push((request.preset_id.clone(), name.into()));
        if let Some(status) = self.status_override.lock().expect("status").clone() {
            Ok(status)
        } else if self.source_environment.load(Ordering::Relaxed) {
            Ok("Ready (key from environment)".into())
        } else if self.stored.lock().expect("stored key").is_some() {
            Ok("Ready (key from keychain)".into())
        } else {
            Ok("No key - Set API key...".into())
        }
    }
}

fn setup_harness() -> (Harness<'static, ShellUi>, Arc<MemorySettings>) {
    let mut h = blank_harness();
    let backend = Arc::new(MemorySettings::default());
    h.state_mut().state.ai_chat.settings.backend = backend.clone();
    h.run_steps(2);
    toggle_ai_chat_via_view_door(&mut h);
    settle(&mut h);
    (h, backend)
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

fn open_editor(h: &mut Harness<'static, ShellUi>) {
    h.get_by_role_and_label(Role::Button, "Set API key...")
        .click();
    h.run_steps(2);
    assert!(h.state().state.ai_chat.settings.editing);
}

fn save_placeholder(h: &mut Harness<'static, ShellUi>, placeholder: &str) {
    open_editor(h);
    h.state_mut().state.ai_chat.settings.draft = placeholder.into();
    h.run_steps(2);
    assert!(h.query_by_label_contains(placeholder).is_none());
    assert!(
        !super::super::structural::painted(h)
            .iter()
            .any(|(text, _)| text.contains(placeholder))
    );
    h.get_by_role_and_label(Role::Button, "Save key").click();
    settle(h);
    assert!(!h.state().state.ai_chat.settings.editing);
    assert!(h.state().state.ai_chat.settings.draft.is_empty());
    assert!(h.query_by_label_contains(placeholder).is_none());
    assert!(
        h.query_by_role_and_label(Role::Button, "Save key")
            .is_none()
    );
    assert!(
        !super::super::structural::painted(h)
            .iter()
            .any(|(text, _)| text.contains(placeholder))
    );
}

#[test]
fn cloud_key_save_replace_remove_and_test_never_draw_saved_secret() {
    let (mut h, backend) = setup_harness();
    select_provider(&mut h, "Anthropic (Cloud)");
    settle(&mut h);
    assert!(h.query_by_label_contains("No key").is_some());
    save_placeholder(&mut h, "placeholder-first-not-a-key");
    assert!(
        h.query_by_label_contains("Ready (key from keychain)")
            .is_some()
    );
    save_placeholder(&mut h, "placeholder-replacement-not-a-key");
    assert_eq!(
        backend
            .stored
            .lock()
            .expect("stored")
            .as_ref()
            .expect("key")
            .expose(),
        "placeholder-replacement-not-a-key"
    );
    h.get_by_role_and_label(Role::Button, "Test").click();
    settle(&mut h);
    assert!(
        backend
            .calls
            .lock()
            .expect("calls")
            .iter()
            .any(|(_, action)| action == "test")
    );
    h.get_by_role_and_label(Role::Button, "Remove").click();
    settle(&mut h);
    assert!(backend.stored.lock().expect("stored").is_none());
    assert!(h.query_by_label_contains("No key").is_some());
}

#[test]
fn cloud_key_editor_clears_secret_on_cancel_and_provider_change() {
    let (mut h, _) = setup_harness();
    open_editor(&mut h);
    h.state_mut().state.ai_chat.settings.draft = "placeholder-cancel-not-a-key".into();
    h.get_by_role_and_label(Role::Button, "Cancel key entry")
        .click();
    h.run_steps(2);
    assert!(h.state().state.ai_chat.settings.draft.is_empty());
    assert!(!h.state().state.ai_chat.settings.editing);
    open_editor(&mut h);
    h.state_mut().state.ai_chat.settings.draft = "placeholder-provider-not-a-key".into();
    // Select through the same state entry point as the provider ComboBox; the window is modal for pointer input.
    h.state_mut().state.ai_chat.select_preset(1);
    h.run_steps(2);
    settle(&mut h);
    assert!(h.state().state.ai_chat.settings.draft.is_empty());
    assert!(!h.state().state.ai_chat.settings.editing);
}

#[test]
fn cloud_key_status_names_environment_source_and_still_frames_do_not_recheck() {
    let (mut h, backend) = setup_harness();
    backend.source_environment.store(true, Ordering::Relaxed);
    // Test waits for a key: a check finds the environment key first.
    h.state_mut()
        .state
        .ai_chat
        .settings
        .start(SettingsAction::Check);
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Test").click();
    settle(&mut h);
    assert!(
        h.query_by_label_contains("Ready (key from environment)")
            .is_some()
    );
    let before = backend.calls.lock().expect("calls").len();
    h.run_steps(10);
    assert_eq!(backend.calls.lock().expect("calls").len(), before);
}

#[test]
fn every_cloud_provider_has_key_controls_and_local_provider_has_install_help() {
    let (mut h, _) = setup_harness();
    let providers: Vec<String> = h
        .state()
        .state
        .ai_chat
        .presets()
        .iter()
        .filter(|preset| preset.endpoint_kind == EndpointKind::Cloud)
        .map(|preset| preset.label.clone())
        .collect();
    assert!(providers.len() >= 3);
    for label in providers {
        select_provider(&mut h, &label);
        settle(&mut h);
        assert!(
            h.query_by_role_and_label(Role::Button, "Set API key...")
                .is_some(),
            "missing setup for {label}"
        );
        assert!(
            h.query_by_role_and_label(Role::Button, "Remove").is_some(),
            "missing Remove for {label}"
        );
        assert!(
            h.query_by_role_and_label(Role::Button, "Test").is_some(),
            "missing Test for {label}"
        );
    }
    select_provider(&mut h, "Offline local (Ollama)");
    settle(&mut h);
    assert!(
        h.query_by_role_and_label(Role::Button, "Set API key...")
            .is_none()
    );
    assert!(h.query_by_label_contains("Ollama setup").is_some());
}

#[test]
fn cloud_key_editor_refuses_empty_and_control_character_input() {
    let mut settings = Settings::default();
    for invalid in ["   ", "placeholder\nheader"] {
        settings.editing = true;
        settings.draft = invalid.into();
        settings.save();
        assert!(settings.editing, "invalid input must keep the editor open");
        assert!(!settings.busy());
        assert!(settings.status.contains("empty") || settings.status.contains("control character"));
        assert!(!settings.status.contains("placeholder"));
    }
}

#[test]
fn local_provider_status_shows_missing_server_and_model_before_send() {
    let (mut h, backend) = setup_harness();
    select_provider(&mut h, "Offline local (Ollama)");
    settle(&mut h);
    for status in [
        "Ollama not running at http://localhost:11434",
        "model local-model not installed",
    ] {
        *backend.status_override.lock().expect("status") = Some(status.into());
        h.get_by_role_and_label(Role::Button, "Test").click();
        settle(&mut h);
        assert!(h.query_by_label(status).is_some());
    }
}

#[test]
fn cloud_provider_test_refuses_offline_mode_before_backend_dispatch() {
    let backend = Arc::new(MemorySettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let request = SettingsRequest {
        preset_id: "anthropic".into(),
        model: "model".into(),
        offline_only: true,
    };
    settings.sync(request.clone());
    settle_settings(&mut settings, &request);
    settings.start(SettingsAction::Test);
    settle_settings(&mut settings, &request);
    assert!(settings.status.contains("Offline only"));
    assert!(
        !backend
            .calls
            .lock()
            .expect("calls")
            .iter()
            .any(|(_, action)| action == "test")
    );
}

fn settle_settings(settings: &mut Settings, request: &SettingsRequest) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while settings.busy() {
        settings.sync(request.clone());
        assert!(Instant::now() < deadline, "settings fake did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn provider_settings_ignores_second_action_while_check_is_busy() {
    let backend = Arc::new(MemorySettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let request = SettingsRequest {
        preset_id: "anthropic".into(),
        model: "model".into(),
        offline_only: false,
    };
    settings.sync(request.clone());
    assert!(settings.busy());
    settings.start(SettingsAction::Test);
    settle_settings(&mut settings, &request);
    std::thread::sleep(Duration::from_millis(10));
    assert!(
        !backend
            .calls
            .lock()
            .expect("calls")
            .iter()
            .any(|(_, action)| action == "test")
    );
}

struct DelayedSettings {
    release: AtomicBool,
    started: AtomicBool,
}

impl SettingsBackend for DelayedSettings {
    fn run(&self, request: &SettingsRequest, _: SettingsAction) -> Result<String, String> {
        if request.preset_id == "anthropic" {
            self.started.store(true, Ordering::Release);
            let deadline = Instant::now() + Duration::from_secs(5);
            while !self.release.load(Ordering::Acquire) {
                assert!(Instant::now() < deadline, "delayed fake was not released");
                std::thread::sleep(Duration::from_millis(1));
            }
            Ok("OLD PROVIDER RESULT".into())
        } else {
            Ok("NEW PROVIDER RESULT".into())
        }
    }
}

#[test]
fn provider_switch_discards_stale_async_result_without_rechecking_still_frames() {
    let backend = Arc::new(DelayedSettings {
        release: AtomicBool::new(false),
        started: AtomicBool::new(false),
    });
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let old = SettingsRequest {
        preset_id: "anthropic".into(),
        model: "old".into(),
        offline_only: false,
    };
    let new = SettingsRequest {
        preset_id: "gemini".into(),
        model: "new".into(),
        offline_only: false,
    };
    settings.sync(old);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !backend.started.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "fake did not start");
        std::thread::sleep(Duration::from_millis(1));
    }
    settings.sync(new.clone());
    assert!(
        settings.busy(),
        "switching must keep the active job nonblocking"
    );
    backend.release.store(true, Ordering::Release);
    while settings.busy() {
        settings.sync(new.clone());
        assert_ne!(settings.status, "OLD PROVIDER RESULT");
        assert!(Instant::now() < deadline, "fake did not finish");
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(settings.status, "NEW PROVIDER RESULT");
}
