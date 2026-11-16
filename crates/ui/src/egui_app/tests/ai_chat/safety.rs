//! #1946: inline Allow/Deny confirmation gates and Undo Turn safety guard tests.
//! Provenance: our own tests over our own shell and egui_kittest; no external SDKs or vendor material.

use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::kittest::{NodeT, Queryable};
use serde_json::{json, Value};

use super::super::super::ai_chat::{ChatTransport, ToolCardStatus, TransportOutput};
use super::super::CommandKind;
use super::{
    blank_harness, has_op_row, pump_until_confirm, pump_until_idle, seed_postable_face_job,
    select_provider, submit_prompt, toggle_ai_chat_via_view_door, ScriptedTransport,
};

#[test]
fn post_confirmation_allow_and_deny_gate_mcp_dispatch() {
    let mut h = blank_harness();
    h.run();
    seed_postable_face_job(&mut h);
    toggle_ai_chat_via_view_door(&mut h);

    let tmp_dir = std::env::temp_dir().join(format!("camcraft-ai-chat-post-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp_dir);
    std::fs::create_dir_all(&tmp_dir).expect("temp dir");
    let nc_denied = tmp_dir.join("denied.nc");
    let nc_allowed = tmp_dir.join("allowed.nc");

    let transport = ScriptedTransport::new(vec![
        // Turn 1: AI proposes `post` -> user clicks Deny -> AI receives refusal.
        Ok(TransportOutput::AssistantResponse {
            text: String::new(),
            envelope: json!({
                "content": [{ "type": "tool_use", "id": "p1", "name": "post", "input": { "output": nc_denied.display().to_string() } }]
            }),
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Posting cancelled per your denial.".to_owned(),
            envelope: json!({
                "content": [{ "type": "text", "text": "Posting cancelled per your denial." }]
            }),
            fallback_override: None,
        }),
        // Turn 2: AI proposes `post` -> user clicks Allow -> dispatches through Inbox::channel -> mcp_host::pump.
        Ok(TransportOutput::AssistantResponse {
            text: String::new(),
            envelope: json!({
                "content": [{ "type": "tool_use", "id": "p2", "name": "post", "input": { "output": nc_allowed.display().to_string() } }]
            }),
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Posted NC program.".to_owned(),
            envelope: json!({
                "content": [{ "type": "text", "text": "Posted NC program." }]
            }),
            fallback_override: None,
        }),
    ]);
    h.state_mut()
        .state
        .ai_chat
        .set_transport(Arc::clone(&transport) as Arc<dyn ChatTransport>);

    // Turn 1: Deny
    submit_prompt(&mut h, "Post G-code to denied file");
    pump_until_confirm(&mut h);
    assert!(h.query_by_label_contains("Confirmation required").is_some());
    assert!(h.query_by_label("Tool: post").is_some());
    assert!(!nc_denied.exists(), "post must not write before Allow");

    h.get_by_role_and_label(Role::Button, "Deny").click();
    pump_until_idle(&mut h);

    assert!(!nc_denied.exists(), "denied post must never write output file");
    assert!(h.query_all_by_label_contains("Denied").next().is_some());
    {
        let reqs = transport.requests.lock().expect("lock");
        let tool_res_msg = reqs[1]
            .messages
            .iter()
            .find(|m| m.raw.as_ref().is_some_and(|v| v.get("is_error") == Some(&json!(true))))
            .expect("denied call feeds is_error refusal back to model");
        let text = tool_res_msg.raw.as_ref().expect("raw").to_string();
        assert!(text.to_lowercase().contains("denied"), "refusal mentions denial: {text}");
    }

    // Turn 2: Allow
    submit_prompt(&mut h, "Post G-code to allowed file");
    pump_until_confirm(&mut h);
    h.get_by_role_and_label(Role::Button, "Allow").click();
    pump_until_idle(&mut h);

    assert!(
        nc_allowed.exists(),
        "allowed post must execute via Inbox::channel and write NC output (status={:?}, turns={:#?})",
        h.state().state.status,
        h.state().state.ai_chat.turns()
    );
    let turn = h.state().state.ai_chat.turns().last().expect("second turn");
    assert!(
        turn.tool_cards
            .iter()
            .any(|c| c.name == "post" && c.status == ToolCardStatus::Succeeded),
        "allowed post card should succeed: {:?}",
        turn.tool_cards
    );
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn undo_turn_reverts_multi_tool_turn_changes_in_one_click_and_guards_later_edits() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);

    let transport = ScriptedTransport::new(vec![
        Ok(TransportOutput::AssistantResponse {
            text: String::new(),
            envelope: json!({
                "content": [
                    { "type": "tool_use", "id": "u1", "name": "set_stock", "input": { "box": { "dx": 80.0, "dy": 80.0, "dz": 20.0 } } },
                    { "type": "tool_use", "id": "u2", "name": "add_tool", "input": { "catalog": "EM-0400-2FL" } },
                    { "type": "tool_use", "id": "u3", "name": "add_operation", "input": { "kind": "Face", "tool": "EM-0400-2FL" } }
                ]
            }),
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Stock, tool, and Face1 ready.".to_owned(),
            envelope: json!({
                "content": [{ "type": "text", "text": "Stock, tool, and Face1 ready." }]
            }),
            fallback_override: None,
        }),
    ]);
    h.state_mut().state.ai_chat.set_transport(transport);
    let pre_depth = h.state().state.history.done_len();

    submit_prompt(&mut h, "Set stock and add Face op");
    pump_until_idle(&mut h);

    assert_eq!(h.state().state.history.done_len(), pre_depth + 3);
    assert!(has_op_row(&h.state().state, "Face1"));
    assert!(
        h.query_all_by_label_contains("3 changes").next().is_some()
            || h.query_all_by_label_contains("3 document").next().is_some()
    );

    h.get_by_role_and_label(Role::Button, "Undo Turn").click();
    h.run_steps(2);

    assert_eq!(
        h.state().state.history.done_len(),
        pre_depth,
        "Undo Turn reverted every command applied by the turn"
    );
    assert!(!has_op_row(&h.state().state, "Face1"));
    let turn = h.state().state.ai_chat.turns().last().expect("turn");
    assert!(turn.undone);
    assert!(
        h.get_by_role_and_label(Role::Button, "Undo Turn")
            .accesskit_node()
            .is_disabled(),
        "already-undone turn disables Undo Turn"
    );
}

#[test]
fn undo_turn_is_disabled_and_refused_after_later_manual_edit() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);

    let transport = ScriptedTransport::new(vec![
        Ok(TransportOutput::AssistantResponse {
            text: String::new(),
            envelope: json!({
                "content": [{ "type": "tool_use", "id": "t1", "name": "add_tool", "input": { "catalog": "EM-0400-2FL" } }]
            }),
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Added EM-0400-2FL.".to_owned(),
            envelope: json!({
                "content": [{ "type": "text", "text": "Added EM-0400-2FL." }]
            }),
            fallback_override: None,
        }),
    ]);
    h.state_mut().state.ai_chat.set_transport(transport);
    submit_prompt(&mut h, "Add a 4 mm end mill");
    pump_until_idle(&mut h);

    assert!(!h.get_by_role_and_label(Role::Button, "Undo Turn").accesskit_node().is_disabled());

    let turn_id = h.state().state.ai_chat.turns().last().expect("turn").id;

    // Simulate a subsequent user edit after the AI turn completed.
    h.state_mut().state.pending.push_back(CommandKind::SetWcs {
        pos: [0.0, 0.0, 0.0],
        roll_deg: 0.0,
        pitch_deg: 0.0,
        yaw_deg: 0.0,
    });
    h.run_steps(2);
    let after_manual = h.state().state.history.done_len();

    assert!(
        h.get_by_role_and_label(Role::Button, "Undo Turn")
            .accesskit_node()
            .is_disabled(),
        "later manual edit must disable Undo Turn"
    );

    // Direct call to undo_turn must also refuse and preserve history when later edits exist.
    {
        let st = &mut h.state_mut().state;
        st.ai_chat.undo_turn(
            turn_id,
            &st.world,
            &mut st.history,
            &mut st.doc_version,
            &mut st.status,
        );
    }
    assert_eq!(h.state().state.history.done_len(), after_manual);
    assert!(
        h.state()
            .state
            .ai_chat
            .last_error()
            .is_some_and(|m| m.contains("later document edits"))
    );
}

#[test]
fn prompt_json_fallback_gates_mutating_proposals_behind_allow_deny() {
    let mut h = blank_harness();
    h.run();
    toggle_ai_chat_via_view_door(&mut h);
    select_provider(&mut h, "Local Prompt-JSON");

    let transport = ScriptedTransport::new(vec![
        Ok(TransportOutput::AssistantResponse {
            text: r#"```json
{"tool":"set_stock","arguments":{"box":{"dx":50.0,"dy":50.0,"dz":10.0}}}
```"#
            .to_owned(),
            envelope: Value::Null,
            fallback_override: None,
        }),
        Ok(TransportOutput::AssistantResponse {
            text: "Stock configured after confirmation.".to_owned(),
            envelope: Value::Null,
            fallback_override: None,
        }),
    ]);
    h.state_mut().state.ai_chat.set_transport(transport);
    let before = h.state().state.history.done_len();

    submit_prompt(&mut h, "Set stock to 50x50x10");
    pump_until_confirm(&mut h);
    assert_eq!(h.state().state.history.done_len(), before);
    assert!(h.query_by_label("Tool: set_stock").is_some());

    h.get_by_role_and_label(Role::Button, "Allow").click();
    pump_until_idle(&mut h);

    assert_eq!(h.state().state.history.done_len(), before + 1);
    assert!(
        h.query_all_by_label_contains("Stock configured after confirmation.")
            .next()
            .is_some()
    );
}
