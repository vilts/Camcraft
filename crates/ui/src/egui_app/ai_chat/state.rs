//! Panel conversation state, opt-in context capture, and worker event draining (#1946).
//! Provenance: our own state model over camcraft-app History and mpsc worker events; no vendor material involved.

use std::sync::atomic::Ordering;
use std::sync::{Arc, mpsc};

use camcraft_app::commands::History;
use camcraft_app::mcp::{ApprovedCall, Request};
use camcraft_app::model;
use flecs_ecs::prelude::World;
use serde_json::Value;

use super::transport::{
    ChatTransport, DefaultTransport, ProviderPreset, ScreenshotAttachment, SelectionSnapshot,
    check_offline_policy, default_presets, encode_base64,
};
use super::turn::{ChatTurn, TurnStatus, build_turn_messages};
use super::worker::{
    ConfirmRequest, TurnFinish, TurnInput, WakeFn, WorkerEvent, WorkerHandle, spawn_turn,
};

/// UI-side state for the AI Chat dock tab, isolated from the CAM document.
pub struct AiChatState {
    presets: Vec<ProviderPreset>,
    selected_preset: usize,
    pub(crate) model_input: String,
    pub offline_only: bool,
    pub attach_selection: bool,
    pub attach_screenshot: bool,
    pub(crate) prompt_draft: String,
    turns: Vec<ChatTurn>,
    next_turn_id: u64,
    pending_confirm: Option<ConfirmRequest>,
    approved_call: Option<ApprovedCall>,
    last_error: Option<String>,
    worker: Option<(u64, WorkerHandle)>,
    wake: Option<WakeFn>,
    transport: Arc<dyn ChatTransport>,
}

impl Default for AiChatState {
    fn default() -> Self {
        Self::new()
    }
}

impl AiChatState {
    /// Construct initial AI Chat state with the first preset active and attachments off by default.
    #[must_use]
    pub fn new() -> Self {
        let presets = default_presets();
        let model_input = presets
            .first()
            .map_or_else(String::new, |p| p.default_model.clone());
        Self {
            presets,
            selected_preset: 0,
            model_input,
            offline_only: false,
            attach_selection: false,
            attach_screenshot: false,
            prompt_draft: String::new(),
            turns: Vec::new(),
            next_turn_id: 1,
            pending_confirm: None,
            approved_call: None,
            last_error: None,
            worker: None,
            wake: None,
            transport: Arc::new(DefaultTransport),
        }
    }

    pub(crate) fn presets(&self) -> &[ProviderPreset] {
        &self.presets
    }

    pub(crate) fn selected_preset_index(&self) -> usize {
        self.selected_preset.min(self.presets.len().saturating_sub(1))
    }

    pub(crate) fn selected_preset(&self) -> &ProviderPreset {
        let idx = self.selected_preset_index();
        &self.presets[idx]
    }

    /// Switch active preset and reset the model input to that preset's default model.
    pub(crate) fn select_preset(&mut self, index: usize) {
        if let Some(p) = self.presets.get(index) {
            self.selected_preset = index;
            self.model_input = p.default_model.clone();
            self.last_error = None;
        }
    }

    /// Replace the transport backend (used by offline tests and live provider configuration).
    // Constructed by offline tests today and the live HTTP backend in #1945.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn set_transport(&mut self, transport: Arc<dyn ChatTransport>) {
        self.transport = transport;
    }

    /// Register the UI event-loop wakeup callback invoked from background threads.
    pub(crate) fn set_wake(&mut self, wake: WakeFn) {
        self.wake = Some(wake);
    }

    /// True while a background worker thread is currently executing a turn.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.worker.is_some()
    }

    pub(crate) fn turns(&self) -> &[ChatTurn] {
        &self.turns
    }

    pub(crate) fn pending_confirm(&self) -> Option<&ConfirmRequest> {
        self.pending_confirm.as_ref()
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    /// Capture state and launch one asynchronous turn on the background thread.
    pub(crate) fn submit(
        &mut self,
        world: &World,
        selected_op: Option<String>,
        history: &History,
        mcp_tx: &mpsc::Sender<Request>,
        screenshot_png: Option<Result<Vec<u8>, String>>,
        project_snapshot: Value,
    ) {
        if self.is_running() {
            return;
        }
        let prompt = self.prompt_draft.trim().to_owned();
        if prompt.is_empty() {
            self.last_error = Some("Enter a prompt before sending.".to_owned());
            return;
        }
        let preset = self.selected_preset().clone();
        if let Err(err) =
            check_offline_policy(self.offline_only, preset.endpoint_kind, &preset.label)
        {
            self.last_error = Some(err.clone());
            self.prompt_draft.clear();
            let turn_id = self.next_turn_id;
            self.next_turn_id += 1;
            self.turns
                .push(ChatTurn::failed(turn_id, prompt, history.done_len(), err));
            return;
        }

        let screenshot = match self.capture_opt_in_screenshot(screenshot_png) {
            Ok(shot) => shot,
            Err(err) => {
                self.last_error = Some(err);
                return;
            }
        };
        let selection = self.attach_selection.then(|| SelectionSnapshot {
            selected_entities: model::selected_names(world),
            active_setup: camcraft_app::setup::active(world),
            selected_operation: selected_op,
        });

        self.prompt_draft.clear();
        self.last_error = None;
        self.pending_confirm = None;
        self.approved_call = None;

        let turn_id = self.next_turn_id;
        self.next_turn_id += 1;
        let initial_messages = build_turn_messages(&self.turns, &prompt, selection.clone(), screenshot.clone());
        self.turns.push(ChatTurn::running(
            turn_id,
            prompt,
            history,
            selection,
            screenshot,
        ));

        let model = if self.model_input.trim().is_empty() {
            preset.default_model.clone()
        } else {
            self.model_input.trim().to_owned()
        };
        let handle = spawn_turn(TurnInput {
            turn_id,
            preset,
            model,
            offline_only: self.offline_only,
            initial_messages,
            project_snapshot,
            transport: Arc::clone(&self.transport),
            mcp_tx: mcp_tx.clone(),
            wake: self.wake.clone(),
        });
        self.worker = Some((turn_id, handle));
    }

    fn capture_opt_in_screenshot(
        &self,
        screenshot_png: Option<Result<Vec<u8>, String>>,
    ) -> Result<Option<ScreenshotAttachment>, String> {
        if !self.attach_screenshot {
            return Ok(None);
        }
        match screenshot_png {
            Some(Ok(bytes)) => Ok(Some(ScreenshotAttachment {
                mime_type: "image/png".to_owned(),
                data_base64: encode_base64(&bytes),
            })),
            Some(Err(err)) => Err(format!("screenshot capture failed: {err}")),
            None => Err("screenshot capture unavailable".to_owned()),
        }
    }

    /// Record command commits performed by an in-app MCP tool call on the running turn.
    pub(crate) fn note_mcp_execution(&mut self, before: &[u64], history: &History) {
        if let Some((turn_id, _)) = &self.worker
            && let Some(turn) = self.turns.iter_mut().find(|t| t.id == *turn_id)
        {
            turn.record_execution(before, history);
        }
    }

    /// Take the single-call explicit approval granted when the user clicked Allow.
    pub(crate) fn take_approved_call(&mut self) -> Option<ApprovedCall> {
        self.approved_call.take()
    }

    /// Only the active, uncancelled turn may execute requests from its inbox.
    pub(crate) fn accepts_mcp_requests(&self) -> bool {
        self.worker
            .as_ref()
            .is_some_and(|(_, handle)| !handle.cancel.load(Ordering::Relaxed))
    }

    /// A replacement document must never inherit an earlier turn's Undo permission.
    pub(crate) fn invalidate_document(&mut self) {
        self.shutdown();
        for turn in &mut self.turns {
            turn.invalidate();
        }
    }

    /// Drain pending worker events onto UI state without blocking.
    pub fn poll(&mut self, history: &mut History) {
        let mut events = Vec::new();
        let mut disconnected_turn = None;
        if let Some((turn_id, handle)) = &self.worker {
            loop {
                match handle.rx.try_recv() {
                    Ok(ev) => events.push(ev),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected_turn = Some(*turn_id);
                        break;
                    }
                }
            }
        }
        for ev in events {
            self.apply_event(ev, history);
        }
        if let Some(turn_id) = disconnected_turn
            && self.worker.as_ref().is_some_and(|(id, _)| *id == turn_id)
        {
            self.finish_turn(
                turn_id,
                TurnFinish::Failed("AI chat worker disconnected unexpectedly".to_owned()),
                history,
            );
        }
    }

    fn apply_event(&mut self, ev: WorkerEvent, history: &mut History) {
        match ev {
            WorkerEvent::AssistantText { turn_id, text } => {
                if let Some(turn) = self.turns.iter_mut().find(|t| t.id == turn_id) {
                    turn.append_assistant_text(&text);
                }
            }
            WorkerEvent::ToolCardAdded { turn_id, card } => {
                if let Some(turn) = self.turns.iter_mut().find(|t| t.id == turn_id) {
                    turn.tool_cards.push(card);
                }
            }
            WorkerEvent::ToolCardUpdated {
                turn_id,
                id,
                status,
                summary,
            } => {
                if let Some(turn) = self.turns.iter_mut().find(|t| t.id == turn_id) {
                    turn.update_tool_card(&id, status, summary);
                }
            }
            WorkerEvent::ConfirmNeeded(req) => {
                self.pending_confirm = Some(req);
            }
            WorkerEvent::Finished { turn_id, status } => {
                self.finish_turn(turn_id, status, history);
            }
        }
    }

    fn finish_turn(&mut self, turn_id: u64, status: TurnFinish, history: &mut History) {
        self.pending_confirm = None;
        self.approved_call = None;
        self.worker = None;
        if let Some(turn) = self.turns.iter_mut().find(|t| t.id == turn_id)
            && let Some(err) = turn.finish(status, history)
        {
            self.last_error = Some(err);
        }
    }

    /// Deliver the user's inline Allow or Deny choice back to the waiting worker.
    pub(crate) fn resolve_confirmation(&mut self, allow: bool) {
        if let Some(req) = self.pending_confirm.take() {
            self.approved_call = allow.then(|| ApprovedCall::new(req.tool_name, req.arguments));
            let _ = req.decision_tx.send(allow);
        }
    }

    /// Signal the active worker via `Arc<AtomicBool>` to cancel promptly.
    pub(crate) fn cancel(&mut self) {
        if let Some((_, handle)) = &self.worker {
            handle.cancel.store(true, Ordering::Relaxed);
        }
        self.pending_confirm = None;
        self.approved_call = None;
    }

    /// Revert all contiguous commands executed during `turn_id` when no later edits exist.
    pub(crate) fn undo_turn(
        &mut self,
        turn_id: u64,
        world: &World,
        history: &mut History,
        doc_version: &mut u64,
        status: &mut String,
    ) {
        let Some(turn) = self.turns.iter_mut().find(|t| t.id == turn_id) else {
            return;
        };
        match turn.undo(world, history, doc_version) {
            Ok(msg) => {
                self.last_error = None;
                *status = msg;
            }
            Err(msg) => {
                self.last_error = Some(msg.clone());
                *status = msg;
            }
        }
    }

    /// Cancel and join any running worker thread cleanly before shutdown or document reset.
    pub fn shutdown(&mut self) {
        self.pending_confirm = None;
        self.approved_call = None;
        if let Some((turn_id, handle)) = self.worker.take() {
            handle.cancel.store(true, Ordering::Relaxed);
            drop(handle);
            if let Some(turn) = self.turns.iter_mut().find(|t| t.id == turn_id)
                && matches!(turn.status, TurnStatus::Running)
            {
                turn.status = TurnStatus::Cancelled;
            }
        }
    }
}
