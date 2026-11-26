//! Provenance: PR #3798 review; count readiness checks during Model edits.
//! Gate map (M4.4): covers ui::egui_app::ai_chat::settings,
//! ui::egui_app::ai_chat::settings_panel.

use super::*;
use crate::egui_app::ai_chat::settings::{
    Settings, SettingsAction, SettingsBackend, SettingsRequest,
};

#[derive(Default)]
struct RecordingSettings {
    calls: Mutex<Vec<(String, String, &'static str)>>,
}

impl SettingsBackend for RecordingSettings {
    fn run(&self, request: &SettingsRequest, action: SettingsAction) -> Result<String, String> {
        let action = match action {
            SettingsAction::Check => "check",
            SettingsAction::Test => "test",
            SettingsAction::Save(_) => "save",
            SettingsAction::Remove => "remove",
        };
        self.calls.lock().expect("calls").push((
            request.preset_id.clone(),
            request.model.clone(),
            action,
        ));
        Ok("Ready".into())
    }
}

fn request(preset: &str, model: &str) -> SettingsRequest {
    SettingsRequest {
        preset_id: preset.into(),
        model: model.into(),
        offline_only: false,
    }
}

fn finish(settings: &mut Settings, request: &SettingsRequest, now: Instant) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while settings.busy() {
        settings.sync_at(request.clone(), now);
        assert!(
            Instant::now() < deadline,
            "recording backend did not finish"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn cloud_model_edits_do_not_read_credentials_again_and_test_uses_latest_model() {
    for provider in ["anthropic", "openai-chat", "openai-responses", "gemini"] {
        let backend = Arc::new(RecordingSettings::default());
        let mut settings = Settings::default();
        settings.backend = backend.clone();
        let now = Instant::now();
        let mut current = request(provider, "old");
        settings.sync_at(current.clone(), now);
        finish(&mut settings, &current, now);
        assert_eq!(backend.calls.lock().expect("calls").len(), 1);

        for model in ["n", "ne", "new"] {
            current.model = model.into();
            settings.sync_at(current.clone(), now);
            finish(&mut settings, &current, now);
            assert_eq!(backend.calls.lock().expect("calls").len(), 1, "{provider}");
            assert_eq!(settings.status, "Ready");
            assert!(!settings.has_work());
        }

        settings.start(SettingsAction::Test);
        finish(&mut settings, &current, now);
        assert_eq!(
            backend.calls.lock().expect("calls").last(),
            Some(&(provider.into(), "new".into(), "test"))
        );
        current.preset_id = if provider == "openai-chat" {
            "gemini"
        } else {
            "openai-chat"
        }
        .into();
        settings.sync_at(current.clone(), now);
        finish(&mut settings, &current, now);
        assert_eq!(backend.calls.lock().expect("calls").len(), 3);
    }
}

#[test]
fn ollama_model_edits_check_once_after_the_last_keystroke_pause() {
    let backend = Arc::new(RecordingSettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let mut now = Instant::now();
    let mut current = request("ollama", "old");
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    assert_eq!(backend.calls.lock().expect("calls").len(), 1);

    for model in ["n", "ne", "new"] {
        now += Duration::from_millis(100);
        current.model = model.into();
        settings.sync_at(current.clone(), now);
        finish(&mut settings, &current, now);
        assert_eq!(backend.calls.lock().expect("calls").len(), 1);
        assert!(settings.has_work(), "the queued check must request repaint");
        assert_eq!(settings.status, "Checking provider...");
    }
    settings.sync_at(current.clone(), now + Duration::from_millis(499));
    finish(&mut settings, &current, now + Duration::from_millis(499));
    assert_eq!(backend.calls.lock().expect("calls").len(), 1);

    now += Duration::from_millis(500);
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    assert_eq!(
        *backend.calls.lock().expect("calls"),
        vec![
            ("ollama".into(), "old".into(), "check"),
            ("ollama".into(), "new".into(), "check"),
        ]
    );
    assert_eq!(settings.status, "Ready");
    assert!(!settings.has_work());
    settings.sync_at(current.clone(), now + Duration::from_secs(2));
    assert_eq!(backend.calls.lock().expect("calls").len(), 2);
}

#[test]
fn ollama_explicit_test_supersedes_the_queued_model_check() {
    let backend = Arc::new(RecordingSettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let now = Instant::now();
    let mut current = request("ollama", "old");
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    current.model = "new".into();
    settings.sync_at(current.clone(), now);
    settings.start(SettingsAction::Test);
    finish(&mut settings, &current, now);
    settings.sync_at(current.clone(), now + Duration::from_secs(2));
    finish(&mut settings, &current, now + Duration::from_secs(2));
    assert_eq!(
        *backend.calls.lock().expect("calls"),
        vec![
            ("ollama".into(), "old".into(), "check"),
            ("ollama".into(), "new".into(), "test"),
        ]
    );
    assert!(!settings.has_work());
}

#[test]
fn ollama_model_edit_hides_the_previous_result_while_the_check_is_queued() {
    let backend = Arc::new(RecordingSettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let now = Instant::now();
    let mut current = request("ollama", "old");
    settings.sync_at(current.clone(), now);
    current.model = "new".into();
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    assert_eq!(backend.calls.lock().expect("calls").len(), 1);
    assert_eq!(settings.status, "Checking provider...");
    assert!(settings.has_work());

    let due = now + Duration::from_millis(500);
    settings.sync_at(current.clone(), due);
    finish(&mut settings, &current, due);
    assert_eq!(settings.status, "Ready");
    assert_eq!(backend.calls.lock().expect("calls").len(), 2);
}

#[test]
fn cloud_model_edit_during_readiness_keeps_one_check_and_reaches_ready() {
    let backend = Arc::new(RecordingSettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let now = Instant::now();
    let mut current = request("anthropic", "old");
    settings.sync_at(current.clone(), now);
    current.model = "new".into();
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    assert_eq!(backend.calls.lock().expect("calls").len(), 1);
    assert_eq!(settings.status, "Ready");
    assert!(!settings.has_work());
}

#[test]
fn cloud_model_edit_discards_an_in_flight_test_for_the_previous_model() {
    let backend = Arc::new(RecordingSettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let now = Instant::now();
    let mut current = request("anthropic", "old");
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    settings.start(SettingsAction::Test);
    current.model = "new".into();
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    assert_eq!(backend.calls.lock().expect("calls").len(), 2);
    assert_eq!(
        settings.status,
        "Model changed. Click Test to test this model."
    );
    assert!(!settings.has_work());
    settings.start(SettingsAction::Test);
    finish(&mut settings, &current, now);
    assert_eq!(settings.status, "Ready");
    assert_eq!(
        backend.calls.lock().expect("calls").last(),
        Some(&("anthropic".into(), "new".into(), "test"))
    );
}

#[test]
fn cloud_model_edit_after_a_finished_test_asks_for_a_new_test() {
    let backend = Arc::new(RecordingSettings::default());
    let mut settings = Settings::default();
    settings.backend = backend.clone();
    let now = Instant::now();
    let mut current = request("anthropic", "old");
    settings.sync_at(current.clone(), now);
    finish(&mut settings, &current, now);
    settings.start(SettingsAction::Test);
    finish(&mut settings, &current, now);
    assert_eq!(settings.status, "Ready");

    current.model = "new".into();
    settings.sync_at(current.clone(), now);
    assert_eq!(
        settings.status,
        "Model changed. Click Test to test this model."
    );
    assert!(!settings.has_work());
    assert_eq!(backend.calls.lock().expect("calls").len(), 2);

    settings.start(SettingsAction::Test);
    finish(&mut settings, &current, now);
    assert_eq!(settings.status, "Ready");
    assert_eq!(
        backend.calls.lock().expect("calls").last(),
        Some(&("anthropic".into(), "new".into(), "test"))
    );
}

/// wasm32 has no clock syscall: `std::time::Instant::now()` panics there.
/// AI Chat code reads `camcraft_app::clock::Instant`. Test files are exempt.
#[test]
fn ai_chat_sources_read_the_clock_seam_not_std_instant() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/egui_app/ai_chat");
    let mut scanned = 0;
    for entry in std::fs::read_dir(&dir).expect("ai_chat sources") {
        let path = entry.expect("entry").path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
            continue;
        }
        scanned += 1;
        let source = std::fs::read_to_string(&path).expect("source");
        for (index, line) in source.lines().enumerate() {
            assert!(
                !(line.contains("std::time") && line.contains("Instant")),
                "{name}:{}: use camcraft_app::clock::Instant",
                index + 1
            );
        }
    }
    assert!(scanned >= 10, "the scan found {scanned} files");
    let settings = std::fs::read_to_string(dir.join("settings.rs")).expect("settings");
    assert!(settings.contains("use camcraft_app::clock::Instant;"));
}
