//! #1946: layout, opt-in attachments, cancellation, offline policy, and read-only fallback tests.
//! Provenance: our own tests over our own shell and egui_kittest; no external SDKs or vendor material.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::kittest::Queryable;
use serde_json::{json, Value};

use super::super::super::ai_chat::{
    ChatTransport, EndpointKind, FallbackMode, TransportOutput,
};
use super::super::super::layout;
use super::super::{shell_harness, CommandKind, Tab};
use super::{
    blank_harness, has_op_row, pump_until_idle, select_provider, submit_prompt,
    toggle_ai_chat_via_view_door, BlockingTransport, ScriptedTransport,
};

#[test]
fn view_door_opens_ai_chat_tab_beside_properties_with_expected_controls() {
    let mut h = blank_harness();
    h.run();
    assert!(
        !layout::is_open(&h.state().dock, &Tab::AiChat),
        "AI Chat pane starts closed so default 3-pane dock is unchanged"
    );
    assert!(!h.state().state.panels_open.ai_chat);

    toggle_ai_chat_via_view_door(&mut h);
    let dock = &h.state().dock;
    let chat = dock.find_tab(&Tab::AiChat).expect("AI Chat opened");
    let props = dock.find_tab(&Tab::Properties).expect("Properties still open");
    assert_eq!(chat.node_path(), props.node_path(), "opens beside Properties");
    assert!(h.state().state.panels_open.ai_chat);
    assert!(h.state().state.pending.is_empty());
    assert!(h.state().state.history.is_empty());

    assert!(h.query_by_role_and_label(Role::ComboBox, "Provider").is_some());
    assert!(h.query_by_role_and_label(Role::TextInput, "Model").is_some());
    assert!(h.query_by_role_and_label(Role::CheckBox, "Offline only").is_some());
    assert!(h.query_by_role_and_label(Role::CheckBox, "Attach selection").is_some());
    assert!(h.query_by_role_and_label(Role::CheckBox, "Attach screenshot").is_some());
    assert!(h.query_by_role_and_label(Role::TextInput, "Prompt").is_some());
    assert!(h.query_by_role_and_label(Role::Button, "Send").is_some());

    toggle_ai_chat_via_view_door(&mut h);
    assert!(!layout::is_open(&h.state().dock, &Tab::AiChat));
}

#[test]
fn chat_submission_dispatches_tools_through_inbox_shows_cards_and_opt_in_attachments() {
    let mut h = shell_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);

    // Ensure doc has a box stock and we select the model so opt-in selection has content.
    h.state_mut()
        .state
        .pending
        .push_back(CommandKind::Select(Some("Block".to_owned())));
    h.run_steps(2);

    let wakes = Arc::new(AtomicUsize::new(0));
    let wakes_cb = Arc::clone(&wakes);
    h.state_mut()
        .state
        .set_ai_wake(Arc::new(move || {
            wakes_cb.fetch_add(1, Ordering::Relaxed);
        }));

    let transport = ScriptedTransport::new(vec![
        Ok(TransportOutput::AssistantResponse {
            text: String::new(),
            envelope: json!({
                "content": [
                    { "type": "tool_use", "id": "c1", "name": "add_tool", "input": { "catalog": "EM-0400-2FL" } },
                    { "type": "tool_use", "id": "c2", "name": "add_operation", "input": { "kind": "Face", "tool": "EM-0400-2FL" } }
                ]
            }),
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Added EM-0400-2FL and Face1.".to_owned(),
            envelope: json!({
                "content": [{ "type": "text", "text": "Added EM-0400-2FL and Face1." }]
            }),
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Inspected attached selection.".to_owned(),
            envelope: json!({
                "content": [{ "type": "text", "text": "Inspected attached selection." }]
            }),
            fallback_override: None,
        }),
    ]);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(Arc::clone(&transport) as Arc<dyn ChatTransport>);

    // Turn 1: opt-in selection and screenshot are UNCHECKED -> neither is attached.
    submit_prompt(&mut h, "Create a Face op");
    pump_until_idle(&mut h);

    assert!(wakes.load(Ordering::Relaxed) >= 1, "worker and MCP calls triggered wake callback");
    assert!(has_op_row(&h.state().state, "Face1"), "Face1 created via Inbox::channel -> mcp_host::pump");
    assert!(h.query_all_by_label_contains("Added EM-0400-2FL and Face1.").next().is_some());
    assert!(h.query_all_by_label_contains("add_tool").next().is_some());
    assert!(h.query_all_by_label_contains("add_operation").next().is_some());

    {
        let reqs = transport.requests.lock().expect("lock");
        assert_eq!(reqs.len(), 2);
        let first_user = reqs[0].messages.last().expect("user msg");
        assert!(first_user.selection.is_none(), "selection must not be sent without opt-in");
        assert!(first_user.screenshot.is_none(), "screenshot must not be sent without opt-in");
    }

    // Turn 2: selection can be attached; screenshot capture remains unavailable.
    h.get_by_role_and_label(Role::CheckBox, "Attach selection").click();
    h.run_steps(2);
    submit_prompt(&mut h, "Inspect current state");
    pump_until_idle(&mut h);

    let reqs = transport.requests.lock().expect("lock");
    assert_eq!(reqs.len(), 3);
    let last_user = reqs[2].messages.last().expect("second user msg");
    let sel = last_user.selection.as_ref().expect("selection attached when opted in");
    assert!(
        sel.selected_entities.iter().any(|n| n == "Block")
            || sel.selected_operation.as_deref() == Some("Face1"),
        "expected selected entity or operation in snapshot: {sel:?}"
    );
    assert!(last_user.screenshot.is_none(), "unavailable capture must not attach a placeholder");
}

#[test]
fn cancel_stops_running_worker_via_atomic_flag_and_leaves_document_unchanged() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);

    let started = Arc::new(AtomicBool::new(false));
    let saw_cancel = Arc::new(AtomicBool::new(false));
    let transport = Arc::new(BlockingTransport {
        started: Arc::clone(&started),
        saw_cancel: Arc::clone(&saw_cancel),
    });
    h.state_mut().state.ai_chat.set_transport(transport);
    let before_done = h.state().state.history.done_len();

    submit_prompt(&mut h, "Long running request");
    let deadline = Instant::now() + Duration::from_secs(5);
    while !started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "worker never started");
        std::thread::sleep(Duration::from_millis(5));
        h.run();
    }
    assert!(h.state().state.ai_chat.is_running());
    h.get_by_role_and_label(Role::Button, "Cancel").click();
    pump_until_idle(&mut h);

    assert!(saw_cancel.load(Ordering::SeqCst), "Arc<AtomicBool> cancel stopped worker");
    assert!(h.query_by_label_contains("Cancelled").is_some());
    assert_eq!(h.state().state.history.done_len(), before_done);
}

#[test]
fn offline_only_blocks_cloud_providers_before_transport_and_allows_local() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);

    let transport = ScriptedTransport::new(vec![Ok(TransportOutput::AssistantResponse {
        text: "Local model answered offline.".to_owned(),
        envelope: json!({
            "choices": [{ "message": { "role": "assistant", "content": "Local model answered offline." } }]
        }),
        fallback_override: None,
    })]);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(Arc::clone(&transport) as Arc<dyn ChatTransport>);

    // Turn on Offline only while Cloud provider is selected.
    h.get_by_role_and_label(Role::CheckBox, "Offline only").click();
    h.run_steps(2);
    select_provider(&mut h, "Anthropic (Cloud)");

    submit_prompt(&mut h, "Should be blocked offline");
    pump_until_idle(&mut h);

    assert_eq!(
        transport.calls.load(Ordering::Relaxed),
        0,
        "offline-only mode must never call transport for a Cloud provider"
    );
    assert!(h.query_all_by_label_contains("offline-only").next().is_some());

    // Switching to a Local endpoint works cleanly even with Offline only active.
    select_provider(&mut h, "Offline local (Ollama)");
    submit_prompt(&mut h, "Hello local model");
    pump_until_idle(&mut h);

    assert_eq!(transport.calls.load(Ordering::Relaxed), 1);
    let reqs = transport.requests.lock().expect("lock");
    assert!(reqs[0].offline_only);
    assert_eq!(reqs[0].preset.endpoint_kind, EndpointKind::Local);
    assert!(h.query_by_label_contains("Local model answered offline.").is_some());
}

#[test]
fn fallback_readonly_mode_shows_warning_banner_and_blocks_all_tools() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);

    select_provider(&mut h, "Local Q&A (Read-Only)");
    assert!(
        h.query_by_label("Read-only Q&A mode: tool execution is disabled.").is_some(),
        "ReadOnly fallback must display warning banner before sending"
    );

    let transport = ScriptedTransport::new(vec![Ok(TransportOutput::AssistantResponse {
        text: r#"{"tool":"post","arguments":{"output":"/tmp/should-not-run.nc"}}"#.to_owned(),
        envelope: Value::Null,
        fallback_override: Some(FallbackMode::ReadOnly),
    })]);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(Arc::clone(&transport) as Arc<dyn ChatTransport>);
    let before_done = h.state().state.history.done_len();

    submit_prompt(&mut h, "Summarize the current job");
    pump_until_idle(&mut h);

    assert_eq!(transport.calls.load(Ordering::Relaxed), 1);
    let reqs = transport.requests.lock().expect("lock");
    assert_eq!(
        reqs[0].tools_payload["tools"],
        json!([]),
        "ReadOnly mode sends qa_context with empty tools array"
    );
    let turn = h.state().state.ai_chat.turns().last().expect("turn");
    assert!(turn.tool_cards.is_empty(), "ReadOnly mode must not emit tool cards");
    assert!(
        h.query_by_role_and_label(Role::Button, "Allow").is_none(),
        "ReadOnly mode must never execute or prompt for tool JSON"
    );
    assert_eq!(h.state().state.history.done_len(), before_done);
}
