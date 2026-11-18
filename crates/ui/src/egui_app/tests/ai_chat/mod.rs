//! #1946: shared offline fake transports and egui_kittest helpers for AI chat panel tests.
//! Provenance: our own tests over our own shell and egui_kittest; no external SDKs or vendor material.

mod basic;
mod regressions;
mod safety;

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::accesskit::Role;
use serde_json::Value;

use super::super::ai_chat::{ChatTransport, TransportOutput, TransportRequest};
use super::super::browser_tree::Leaf;
use super::*;

pub(super) fn blank_harness() -> Harness<'static, ShellUi> {
    Harness::builder()
        .with_size(Vec2::new(1280.0, 720.0))
        .build_ui_state(frame, ShellUi::blank())
}

/// Toggle AI Chat through the ribbon's View ("Panels") door just like a user.
pub(super) fn toggle_ai_chat_via_view_door(h: &mut Harness<'static, ShellUi>) {
    h.get_by_role_and_label(Role::Button, "Panels").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::MenuItem, "AI Chat").click();
    h.run_steps(2);
}

pub(super) fn select_provider(h: &mut Harness<'static, ShellUi>, preset_label: &str) {
    h.get_by_role_and_label(Role::ComboBox, "Provider").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, preset_label).click();
    h.run_steps(2);
}

pub(super) fn submit_prompt(h: &mut Harness<'static, ShellUi>, text: &str) {
    let input = h.get_by_role_and_label(Role::TextInput, "Prompt");
    input.click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::TextInput, "Prompt")
        .type_text(text);
    h.run_steps(2);
    h.get_by_role_and_label(Role::Button, "Send").click();
    h.run_steps(2);
}

pub(super) fn pump_until_idle(h: &mut Harness<'static, ShellUi>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.state().state.ai_chat.is_running() {
        assert!(
            Instant::now() < deadline,
            "AI chat worker timed out still running"
        );
        std::thread::sleep(Duration::from_millis(5));
        // One step, not `run()`: a scroll-area animation keeps asking for repaints
        // and `run()` panics past max_steps (#2137).
        h.step();
    }
    h.run_steps(2);
}

pub(super) fn pump_until_confirm(h: &mut Harness<'static, ShellUi>) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while h.query_by_role_and_label(Role::Button, "Allow").is_none() {
        assert!(
            Instant::now() < deadline && h.state().state.ai_chat.is_running(),
            "timed out waiting for inline Allow/Deny confirmation prompt"
        );
        std::thread::sleep(Duration::from_millis(5));
        // One step, not `run()`: a scroll-area animation keeps asking for repaints
        // and `run()` panics past max_steps (#2137).
        h.step();
    }
    h.run_steps(2);
}

pub(super) fn has_op_row(state: &State, name: &str) -> bool {
    let (_, leaves) = panes::browser_rows(state);
    leaves.iter().any(|l| *l == Leaf::Op(name.to_owned()))
}

pub(super) struct ScriptedTransport {
    pub requests: Arc<Mutex<Vec<TransportRequest>>>,
    responses: Mutex<VecDeque<Result<TransportOutput, String>>>,
    pub calls: Arc<AtomicUsize>,
}

impl ScriptedTransport {
    pub(super) fn new(responses: Vec<Result<TransportOutput, String>>) -> Arc<Self> {
        Arc::new(Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            responses: Mutex::new(responses.into()),
            calls: Arc::new(AtomicUsize::new(0)),
        })
    }
}

impl ChatTransport for ScriptedTransport {
    fn complete(
        &self,
        req: &TransportRequest,
        cancel: &AtomicBool,
    ) -> Result<TransportOutput, String> {
        if cancel.load(Ordering::Relaxed) {
            return Err("cancelled".to_owned());
        }
        self.calls.fetch_add(1, Ordering::Relaxed);
        self.requests.lock().expect("lock").push(req.clone());
        self.responses
            .lock()
            .expect("lock")
            .pop_front()
            .unwrap_or_else(|| Err("unexpected extra transport call".to_owned()))
    }
}

pub(super) struct BlockingTransport {
    pub started: Arc<AtomicBool>,
    pub saw_cancel: Arc<AtomicBool>,
}

impl ChatTransport for BlockingTransport {
    fn complete(
        &self,
        _req: &TransportRequest,
        cancel: &AtomicBool,
    ) -> Result<TransportOutput, String> {
        self.started.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !cancel.load(Ordering::Relaxed) {
            if Instant::now() >= deadline {
                return Err("cancel never arrived".to_owned());
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        self.saw_cancel.store(true, Ordering::SeqCst);
        Err("cancelled".to_owned())
    }
}

pub(super) fn seed_postable_face_job(h: &mut Harness<'static, ShellUi>) {
    let calls = [
        serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"set_stock","arguments":{"box":{"dx":60.0,"dy":60.0,"dz":20.0}}}}),
        serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"add_tool","arguments":{"catalog":"EM-0400-2FL","holder":"none"}}}),
        serde_json::json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add_operation","arguments":{"kind":"Face","tool":"EM-0400-2FL"}}}),
        serde_json::json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"generate","arguments":{}}}),
    ];
    for line in calls {
        let st = &mut h.state_mut().state;
        let answer = camcraft_app::mcp::answer_live(
            &st.world, &mut st.history, st.project_path.as_deref(), &line.to_string(),
        );
        let reply = answer.reply.expect("reply");
        let v: Value = serde_json::from_str(&reply).expect("json");
        assert_eq!(v["result"]["isError"], serde_json::json!(false), "{v}");
    }
}
