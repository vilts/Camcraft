//! Per-turn transcript record, history span tracking, and undo guard (#1946).
//! Provenance: our own turn model over camcraft-app History; no vendor material involved.

use camcraft_app::commands::History;
use flecs_ecs::prelude::World;

use super::budget;
use super::transport::{ScreenshotAttachment, SelectionSnapshot, TurnMessage};
use super::worker::{ToolCard, ToolCardStatus, TurnFinish};

/// Lifecycle state of one submitted user/assistant turn in the panel log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TurnStatus {
    Running,
    Completed,
    Cancelled,
    Failed(String),
}

/// One user prompt plus the streamed assistant reply, tool cards, and undo span.
#[derive(Clone, Debug)]
pub(crate) struct ChatTurn {
    pub id: u64,
    pub prompt: String,
    pub assistant_text: String,
    pub tool_cards: Vec<ToolCard>,
    pub status: TurnStatus,
    pub start_depth: usize,
    pub end_depth: Option<usize>,
    pub change_count: usize,
    pub undone: bool,
    document_id: u64,
    expected_entries: Option<Vec<u64>>,
    selection: Option<SelectionSnapshot>,
    screenshot: Option<ScreenshotAttachment>,
}

impl ChatTurn {
    /// Construct a turn record that immediately failed pre-flight policy checks.
    pub(super) fn failed(id: u64, prompt: String, start_depth: usize, err: String) -> Self {
        Self {
            id,
            prompt,
            assistant_text: String::new(),
            tool_cards: Vec::new(),
            status: TurnStatus::Failed(err),
            start_depth,
            end_depth: None,
            change_count: 0,
            undone: false,
            document_id: 0,
            expected_entries: None,
            selection: None,
            screenshot: None,
        }
    }

    /// Construct a running turn capturing the baseline history depth and optional attachments.
    pub(super) fn running(
        id: u64,
        prompt: String,
        history: &History,
        selection: Option<SelectionSnapshot>,
        screenshot: Option<ScreenshotAttachment>,
    ) -> Self {
        Self {
            id,
            prompt,
            assistant_text: String::new(),
            tool_cards: Vec::new(),
            status: TurnStatus::Running,
            start_depth: history.done_len(),
            end_depth: None,
            change_count: 0,
            undone: false,
            document_id: history.identity(),
            expected_entries: Some(history.done_entry_ids()),
            selection,
            screenshot,
        }
    }

    /// True only while this turn sits at the very top of `History` and has not yet been undone.
    #[must_use]
    pub(crate) fn can_undo(&self, history: &History) -> bool {
        !matches!(self.status, TurnStatus::Running)
            && !self.undone
            && self.change_count > 0
            && self.end_depth == Some(history.done_len())
            && self.document_id == history.identity()
            && self.expected_entries.as_ref() == Some(&history.done_entry_ids())
    }

    /// Extend the owned span only when no manual command intervened.
    pub(super) fn record_execution(&mut self, before: &[u64], history: &History) {
        let after = history.done_entry_ids();
        self.change_count += after.len().saturating_sub(before.len());
        self.expected_entries = (self.document_id == history.identity()
            && self.expected_entries.as_deref() == Some(before)
            && after.starts_with(before))
        .then_some(after);
    }

    /// Retire undo permissions when the document is replaced.
    pub(super) fn invalidate(&mut self) {
        self.expected_entries = None;
        self.end_depth = None;
    }

    /// Append assistant output streamed from the worker.
    pub(super) fn append_assistant_text(&mut self, text: &str) {
        if !self.assistant_text.is_empty() {
            self.assistant_text.push('\n');
        }
        self.assistant_text.push_str(text);
    }

    /// Update one inline tool card's status and summary in place.
    pub(super) fn update_tool_card(&mut self, id: &str, status: ToolCardStatus, summary: String) {
        if let Some(card) = self.tool_cards.iter_mut().find(|c| c.id == id) {
            card.status = status;
            card.summary = summary;
        }
    }

    /// True when every executed command in this turn still sits contiguously
    /// at the top of `history` and can be committed as one [`BatchCommand`].
    #[must_use]
    pub(super) fn can_commit_group(&self, history: &History) -> bool {
        self.change_count > 0
            && self.document_id == history.identity()
            && self.expected_entries.as_ref() == Some(&history.done_entry_ids())
            && history.done_len() == self.start_depth + self.change_count
    }

    /// Finalize this turn's terminal status and fold contiguous commands into one batch step.
    pub(super) fn finish(&mut self, status: TurnFinish, history: &mut History) -> Option<String> {
        let contiguous = self.can_commit_group(history);
        if contiguous && history.in_group() {
            let _ = history.commit_group();
        } else {
            let _ = history.cancel_group();
        }
        // Only record an undo span when every post-turn command belongs to this turn contiguously.
        if contiguous {
            self.end_depth = Some(history.done_len());
            self.expected_entries = Some(history.done_entry_ids());
        } else {
            self.end_depth = None;
            self.expected_entries = None;
        }
        match status {
            TurnFinish::Completed => {
                self.status = TurnStatus::Completed;
                None
            }
            TurnFinish::Cancelled => {
                self.status = TurnStatus::Cancelled;
                None
            }
            TurnFinish::Failed(msg) => {
                self.status = TurnStatus::Failed(msg.clone());
                Some(msg)
            }
        }
    }

    /// Revert all contiguous commands created during this turn, refusing if later edits exist.
    pub(super) fn undo(
        &mut self,
        world: &World,
        history: &mut History,
        doc_version: &mut u64,
    ) -> Result<String, String> {
        if !self.can_undo(history) {
            return Err(
                "cannot undo AI turn: later document edits exist or turn already undone".to_owned(),
            );
        }
        let undo_steps = self
            .end_depth
            .unwrap_or(self.start_depth)
            .saturating_sub(self.start_depth);
        for _ in 0..undo_steps {
            let _ = history.undo(world);
        }
        self.undone = true;
        *doc_version += 1;
        Ok(format!("undid AI turn ({} changes)", self.change_count))
    }
}

/// Assemble ordered conversation messages from prior turns plus the new prompt and opt-in context.
pub(super) fn build_turn_messages(
    prior_turns: &[ChatTurn],
    prompt: &str,
    selection: Option<SelectionSnapshot>,
    screenshot: Option<ScreenshotAttachment>,
) -> Vec<TurnMessage> {
    let mut messages = Vec::new();
    // #1944: rolling history; a dropped span leaves the latest get_job reply.
    let (prior_turns, kept_job) = budget::prior_window(prior_turns);
    for prior in prior_turns {
        messages.push(TurnMessage {
            role: "user".to_owned(),
            text: prior.prompt.clone(),
            selection: prior.selection.clone(),
            screenshot: prior.screenshot.clone(),
            raw: None,
        });
        if !prior.assistant_text.is_empty() {
            messages.push(TurnMessage {
                role: "assistant".to_owned(),
                text: prior.assistant_text.clone(),
                selection: None,
                screenshot: None,
                raw: None,
            });
        }
    }
    messages.push(TurnMessage {
        role: "user".to_owned(),
        text: prompt.to_owned(),
        selection,
        screenshot,
        raw: None,
    });
    if let (Some(job), Some(first)) = (kept_job, messages.first_mut()) {
        first.text.insert_str(0, &job);
    }
    budget::cap_images(&mut messages);
    messages
}
