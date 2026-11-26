//! Empty-model provider hints use the same cloud defaults as the request transport.
//! Provenance: our own structural UI tests over the public provider table.

use super::super::structural;
use super::{blank_harness, select_provider, toggle_ai_chat_via_view_door};

#[test]
fn empty_cloud_model_shows_table_default_and_local_hint_stays_local() {
    let mut h = blank_harness();
    h.run_steps(2);
    toggle_ai_chat_via_view_door(&mut h);
    for (label, model) in [
        ("Anthropic (Cloud)", "claude-sonnet-5-5"),
        ("OpenAI Chat (Cloud)", "gpt-6.2-sol"),
        ("OpenAI Responses (Cloud)", "gpt-6.2-sol"),
        ("Gemini (Cloud)", "gemini-4-hydrogen"),
    ] {
        select_provider(&mut h, label);
        h.state_mut().state.ai_chat.model_input.clear();
        h.run_steps(2);
        let texts = structural::painted(&h);
        assert!(
            texts.iter().any(|(text, _)| text == model),
            "{label} must display its effective cloud model: {texts:?}"
        );
        assert!(
            !texts.iter().any(|(text, _)| text.contains("Automatic:")),
            "{label} must not promise installed model discovery"
        );
    }
    for label in [
        "Offline local (Ollama)",
        "Local Prompt-JSON",
        "Local Q&A (Read-Only)",
    ] {
        select_provider(&mut h, label);
        let texts = structural::painted(&h);
        assert!(
            texts
                .iter()
                .any(|(text, _)| text == "Automatic: installed tool-capable model"),
            "{label} must show installed model discovery: {texts:?}"
        );
        assert!(h.state().state.ai_chat.model_input.is_empty());
    }
}
