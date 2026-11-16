//! PR #2094 review regressions: exact turn history, queued calls and honest capture controls.
//! Provenance: our own offline shell tests; no vendor material.

use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::json;

use super::super::super::ai_chat::TransportOutput;
use super::super::{CommandKind, ShellUi};
use super::{
    ScriptedTransport, blank_harness, pump_until_idle, submit_prompt, toggle_ai_chat_via_view_door,
};

fn add_tool_transport() -> std::sync::Arc<ScriptedTransport> {
    ScriptedTransport::new(vec![
        Ok(TransportOutput::AssistantResponse {
            text: String::new(),
            envelope: json!({"content": [{"type": "tool_use", "id": "review-tool", "name": "add_tool", "input": {"catalog": "EM-0400-2FL"}}]}),
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Tool added.".into(),
            envelope: json!({"content": [{"type": "text", "text": "Tool added."}]}),
            fallback_override: None,
        }),
    ])
}

fn manual_edit(st: &mut crate::state::State) {
    st.run(CommandKind::SetWcs {
        pos: [10.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
}

/// Wait for an actual worker-generated request without pumping the UI, then requeue it.
fn queue_tool(st: &mut crate::state::State) {
    st.ai_chat.set_transport(add_tool_transport());
    st.ai_chat.prompt_draft = "Add a tool".into();
    st.submit_ai_prompt();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(request) = st.ai_mcp_inbox.as_ref().expect("AI inbox").next() {
            assert!(request.line().contains("add_tool"));
            st.ai_mcp_tx
                .send(request)
                .expect("requeue pending worker request");
            return;
        }
        assert!(Instant::now() < deadline, "worker never queued add_tool");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn undo_turn_refuses_manual_replacement_at_the_same_history_depth() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(add_tool_transport());
    submit_prompt(&mut h, "Add a tool");
    pump_until_idle(&mut h);
    let turn_id = h.state().state.ai_chat.turns().last().expect("turn").id;
    let depth = h.state().state.history.done_len();
    assert!(
        !h.get_by_role_and_label(Role::Button, "Undo Turn")
            .accesskit_node()
            .is_disabled()
    );
    let st = &mut h.state_mut().state;
    st.run(CommandKind::Undo);
    manual_edit(st);
    assert_eq!(
        st.history.done_len(),
        depth,
        "manual edit replaced the AI entry at the same depth"
    );
    h.run_steps(2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Undo Turn")
            .accesskit_node()
            .is_disabled(),
        "same-depth manual replacement must disable Undo Turn"
    );
    let st = &mut h.state_mut().state;
    st.ai_chat.undo_turn(
        turn_id,
        &st.world,
        &mut st.history,
        &mut st.doc_version,
        &mut st.status,
    );
    assert_eq!(
        st.history.done_len(),
        depth,
        "direct Undo Turn must preserve the manual edit"
    );
    assert!(
        st.ai_chat
            .last_error()
            .is_some_and(|e| e.contains("later document edits"))
    );
}

#[test]
fn interleaved_manual_edit_is_not_owned_by_ai_turn() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);
    let st = &mut h.state_mut().state;
    queue_tool(st);
    manual_edit(st);
    crate::mcp_host::pump(st);
    pump_until_idle(&mut h);
    assert_eq!(h.state().state.history.done_len(), 2);
    assert!(
        h.get_by_role_and_label(Role::Button, "Undo Turn")
            .accesskit_node()
            .is_disabled(),
        "a manual command interleaved with the turn must not be undone as part of it"
    );
}

#[test]
fn new_document_discards_queued_ai_call_and_stops_worker() {
    let mut st = ShellUi::blank().state;
    queue_tool(&mut st);
    let old_sender = st.ai_mcp_tx.clone();
    st.run(CommandKind::NewDocument);
    crate::mcp_host::pump(&mut st);
    assert_eq!(
        st.history.done_len(),
        0,
        "queued old-document call must not edit New"
    );
    assert!(!st.ai_chat.is_running(), "New must stop the old worker");
    let (req, _) = camcraft_app::mcp::Request::pair("stale");
    assert!(
        old_sender.send(req).is_err(),
        "the old document's inbox must be disconnected"
    );
}

#[test]
fn open_document_discards_queued_ai_call_and_stops_worker() {
    let path =
        std::env::temp_dir().join(format!("camcraft-2094-open-{}.camproj", std::process::id()));
    let mut st = ShellUi::blank().state;
    st.run(CommandKind::SaveProjectFile(path.clone()));
    assert!(st.status.starts_with("saved"), "{}", st.status);
    queue_tool(&mut st);
    let old_sender = st.ai_mcp_tx.clone();
    st.run(CommandKind::OpenProjectFile(path.clone()));
    assert!(st.status.starts_with("opened"), "{}", st.status);
    crate::mcp_host::pump(&mut st);
    assert_eq!(
        st.history.done_len(),
        0,
        "queued old-document call must not edit Open"
    );
    assert!(!st.ai_chat.is_running(), "Open must stop the old worker");
    let (req, _) = camcraft_app::mcp::Request::pair("stale");
    assert!(
        old_sender.send(req).is_err(),
        "Open must disconnect the old inbox"
    );
    std::fs::remove_file(path).expect("remove fixture");
}

#[test]
fn cancellation_rejects_already_queued_ai_call() {
    let mut st = ShellUi::blank().state;
    queue_tool(&mut st);
    st.ai_chat.cancel();
    crate::mcp_host::pump(&mut st);
    assert_eq!(
        st.history.done_len(),
        0,
        "Cancel must reject an already queued add_tool"
    );
    st.ai_chat.shutdown();
}

#[test]
fn document_replacement_invalidates_completed_turn_undo() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(add_tool_transport());
    submit_prompt(&mut h, "Add a tool");
    pump_until_idle(&mut h);
    let st = &mut h.state_mut().state;
    let turn_id = st.ai_chat.turns().last().expect("turn").id;
    st.history.mark_clean();
    st.run(CommandKind::NewDocument);
    manual_edit(st);
    let before = st.history.done_len();
    st.ai_chat.undo_turn(
        turn_id,
        &st.world,
        &mut st.history,
        &mut st.doc_version,
        &mut st.status,
    );
    assert_eq!(
        st.history.done_len(),
        before,
        "old turn undo must not touch the new document"
    );
    assert!(st.ai_chat.last_error().is_some());
}

#[test]
fn screenshot_control_is_disabled_and_forced_attachment_refuses_submission() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);
    assert!(
        h.get_by_role_and_label(Role::CheckBox, "Attach screenshot")
            .accesskit_node()
            .is_disabled(),
        "capture must stay disabled until a real viewport image is available"
    );
    assert!(
        h.query_by_label("Viewport screenshot capture is unavailable.")
            .is_some()
    );
    let transport = add_tool_transport();
    h.state_mut().state.ai_chat.set_transport(transport.clone());
    h.state_mut().state.ai_chat.attach_screenshot = true;
    submit_prompt(&mut h, "Inspect the viewport");
    assert_eq!(
        transport.calls.load(Ordering::Relaxed),
        0,
        "never send a fabricated viewport image"
    );
    assert!(
        h.state()
            .state
            .ai_chat
            .last_error()
            .is_some_and(|e| e.contains("unavailable"))
    );
    assert!(!h.state().state.ai_chat.is_running());
}

#[test]
fn undo_redo_preserves_exact_ai_entry_undo_permission() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(add_tool_transport());
    submit_prompt(&mut h, "Add a tool");
    pump_until_idle(&mut h);
    let st = &mut h.state_mut().state;
    st.run(CommandKind::Undo);
    st.run(CommandKind::Redo);
    h.run_steps(2);
    assert!(
        !h.get_by_role_and_label(Role::Button, "Undo Turn")
            .accesskit_node()
            .is_disabled(),
        "Redo restored the exact AI-owned history entry"
    );
    h.get_by_role_and_label(Role::Button, "Undo Turn").click();
    h.run_steps(2);
    assert_eq!(h.state().state.history.done_len(), 0);
}

#[test]
fn previous_turn_sender_cannot_reach_the_next_turn() {
    let mut h = blank_harness();
    let st = &mut h.state_mut().state;
    queue_tool(st);
    let old_sender = st.ai_mcp_tx.clone();
    crate::mcp_host::pump(st);
    pump_until_idle(&mut h);
    let st = &mut h.state_mut().state;
    queue_tool(st);
    let (request, _) = camcraft_app::mcp::Request::pair("old turn");
    assert!(
        old_sender.send(request).is_err(),
        "new turn must disconnect every previous worker sender"
    );
    st.ai_chat.shutdown();
}
