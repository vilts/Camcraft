//! #3424: native AI delete calls wait for Allow/Deny before changing the document.
//! Gate map: covers ui::egui_app::ai_chat::worker and ui::egui_app::ai_chat::transport.
//! Provenance: our own scripted transport tests over the shipping AI chat panel and MCP commands.

use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use serde_json::{Value, json};

use super::super::super::ai_chat::{ChatTransport, ToolCardStatus, TransportOutput};
use super::super::State;
use super::{
    ScriptedTransport, blank_harness, pump_until_confirm, pump_until_idle, select_provider,
    submit_prompt, toggle_ai_chat_via_view_door,
};

fn call(state: &mut State, tool: &str, arguments: Value) -> Value {
    let request = json!({"jsonrpc":"2.0", "id":1, "method":"tools/call",
        "params":{"name":tool, "arguments":arguments}});
    let answer = camcraft_app::mcp::answer_live(
        &state.world,
        &mut state.history,
        state.project_path.as_deref(),
        &request.to_string(),
    );
    let reply: Value = serde_json::from_str(&answer.reply.expect("MCP reply")).expect("JSON");
    assert_eq!(reply["result"]["isError"], false, "{reply}");
    assert!(!reply["result"]["structuredContent"].is_null(), "{reply}");
    reply["result"]["structuredContent"].clone()
}

fn deletes() -> Result<TransportOutput, String> {
    Ok(TransportOutput::AssistantResponse {
        text: String::new(),
        envelope: json!({"choices":[{"message":{"role":"assistant", "tool_calls":[
            {"id":"delete-1", "type":"function", "function":{"name":"delete_operation", "arguments":json!({"name":"Face1"}).to_string()}},
            {"id":"delete-2", "type":"function", "function":{"name":"delete_operation", "arguments":json!({"name":"Face2"}).to_string()}}
        ]}}]}),
        fallback_override: None,
    })
}

fn done() -> Result<TransportOutput, String> {
    Ok(TransportOutput::AssistantResponse {
        text: "Done.".to_owned(),
        envelope: json!({"choices":[{"message":{"role":"assistant", "content":"Done."}}]}),
        fallback_override: None,
    })
}

#[test]
fn native_two_deletes_wait_for_confirmation_and_denial_preserves_job() {
    let mut h = blank_harness();
    h.run();
    call(
        &mut h.state_mut().state,
        "set_stock",
        json!({"box":{"dx":60.0,"dy":60.0,"dz":20.0}}),
    );
    call(
        &mut h.state_mut().state,
        "add_tool",
        json!({"catalog":"EM-0400-2FL","holder":"none"}),
    );
    for _ in 0..2 {
        call(
            &mut h.state_mut().state,
            "add_operation",
            json!({"kind":"Face","tool":"EM-0400-2FL"}),
        );
    }
    let before = call(&mut h.state_mut().state, "get_job", json!({}));
    let history_before = h.state().state.history.done_len();
    toggle_ai_chat_via_view_door(&mut h);
    select_provider(&mut h, "Offline local (Ollama)");
    let transport = ScriptedTransport::new(vec![deletes(), done(), deletes(), done()]);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(Arc::clone(&transport) as Arc<dyn ChatTransport>);

    submit_prompt(&mut h, "Delete every operation.");
    for _ in 0..2 {
        pump_until_confirm(&mut h);
        assert!(h.query_by_label_contains("Confirmation required").is_some());
        assert_eq!(call(&mut h.state_mut().state, "get_job", json!({})), before);
        h.get_by_role_and_label(Role::Button, "Deny").click();
        // Let the worker consume this decision before waiting for the next card.
        h.run_steps(2);
    }
    pump_until_idle(&mut h);
    assert_eq!(call(&mut h.state_mut().state, "get_job", json!({})), before);
    assert_eq!(h.state().state.history.done_len(), history_before);
    assert_eq!(
        h.state()
            .state
            .ai_chat
            .turns()
            .last()
            .expect("turn")
            .tool_cards
            .iter()
            .filter(|card| card.status == ToolCardStatus::Denied)
            .count(),
        2
    );
    {
        let requests = transport.requests.lock().expect("requests");
        for id in ["delete-1", "delete-2"] {
            let refusal = requests[1]
                .messages
                .iter()
                .find_map(|message| message.raw.as_ref().filter(|raw| raw["tool_call_id"] == id))
                .expect("each denied tool result reaches the model");
            assert!(
                refusal["content"]
                    .as_str()
                    .expect("content")
                    .contains("User denied tool 'delete_operation'.")
            );
            assert!(
                refusal["content"]
                    .as_str()
                    .expect("content")
                    .contains("\"isError\":true")
            );
        }
    }

    // A fresh turn can still delete both operations after explicit approval.
    submit_prompt(&mut h, "Delete every operation after approval.");
    for _ in 0..2 {
        pump_until_confirm(&mut h);
        h.get_by_role_and_label(Role::Button, "Allow").click();
        h.run_steps(2);
    }
    pump_until_idle(&mut h);
    assert!(camcraft_app::model::find_by_name(&h.state().state.world, "Face1").is_none());
    assert!(camcraft_app::model::find_by_name(&h.state().state.world, "Face2").is_none());
    assert_eq!(
        h.state()
            .state
            .ai_chat
            .turns()
            .last()
            .expect("allowed turn")
            .tool_cards
            .iter()
            .filter(|card| card.status == ToolCardStatus::Succeeded)
            .count(),
        2
    );
}
