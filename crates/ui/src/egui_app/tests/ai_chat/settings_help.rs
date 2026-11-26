//! Provenance: issue #3781 provider setup instructions and our structural harness.
//! Gate map (M4.4): covers ui::egui_app::ai_chat::settings_panel.

use super::*;
use crate::egui_app::ai_chat::settings::{SettingsAction, SettingsBackend, SettingsRequest};

struct HelpBackend;

impl SettingsBackend for HelpBackend {
    fn run(&self, _: &SettingsRequest, _: SettingsAction) -> Result<String, String> {
        Ok("Ready".into())
    }
}

#[test]
fn provider_help_explains_ollama_options_and_gemini_subscription() {
    let mut h = blank_harness();
    h.state_mut().state.ai_chat.settings.backend = Arc::new(HelpBackend);
    h.run_steps(2);
    toggle_ai_chat_via_view_door(&mut h);
    select_provider(&mut h, "Offline local (Ollama)");
    h.get_by_label_contains("Ollama setup").click();
    h.run_steps(3);
    for instruction in [
        "start the server with ollama serve",
        "curl http://localhost:11434/api/version",
        "ollama list",
        "OLLAMA_HOST=127.0.0.1:11500",
        "OLLAMA_HOST=0.0.0.0:11434",
        "Anyone on that network can use it",
        "OLLAMA_CONTEXT_LENGTH=32768",
        "OLLAMA_KEEP_ALIVE=30m",
        "the server is already running",
    ] {
        assert!(
            h.query_by_label_contains(instruction).is_some(),
            "missing Ollama setup instruction: {instruction}"
        );
    }
    assert!(
        h.query_by_label_contains(
            "CamCraft does not start Ollama. It connects to Ollama at http://localhost:11434"
        )
        .is_some(),
        "the panel must say who starts the Ollama server"
    );
    select_provider(&mut h, "Anthropic (Cloud)");
    h.get_by_label_contains("API key help").click();
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("choose Somewhere else - Use an API key")
            .is_some(),
        "the help must name the console choice that gives a key"
    );
    // The "API key help" section stays open across cloud providers.
    select_provider(&mut h, "Gemini (Cloud)");
    h.run_steps(3);
    assert!(
        h.query_by_label_contains("A Gemini app subscription does not include an API key")
            .is_some()
    );
}
