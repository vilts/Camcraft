//! Labelled command batching (#1942): fold multiple commands into one
//! `History` undo/redo step (`BatchCommand`), so a multi-call AI chat
//! turn reverts in a single `Ctrl+Z`.
//!
//! Provenance: standard Command-pattern composite batching over our own
//! `Command` trait; clean-room, no external code.

use flecs_ecs::prelude::World;

use super::{Command, HistoryEntry, next_history_id};

pub(super) struct GroupFrame {
    pub(super) label: String,
    pub(super) start_len: usize,
}

pub(super) fn commit_frame(
    frame: GroupFrame,
    done: &mut Vec<HistoryEntry>,
    saved: &mut Option<usize>,
) -> bool {
    if done.len() <= frame.start_len {
        return false;
    }
    if let Some(s) = *saved {
        if s == done.len() {
            *saved = Some(frame.start_len + 1);
        } else if s > frame.start_len {
            *saved = None;
        }
    }
    let commands: Vec<Box<dyn Command>> = done
        .drain(frame.start_len..)
        .map(|entry| entry.command)
        .collect();
    let batch = BatchCommand::new(frame.label.clone(), commands);
    tracing::debug!(label = %frame.label, count = batch.len(), "commit_group");
    done.push(HistoryEntry {
        id: next_history_id(),
        label: Some(frame.label),
        command: Box::new(batch),
    });
    true
}

/// One undoable/redoable unit made of commands executed in order.
pub struct BatchCommand {
    label: String,
    commands: Vec<Box<dyn Command>>,
}

impl BatchCommand {
    /// Wrap `commands` (in execution order) under a human-readable `label`.
    #[must_use]
    pub fn new(label: impl Into<String>, commands: Vec<Box<dyn Command>>) -> Self {
        Self {
            label: label.into(),
            commands,
        }
    }

    /// The label given when the batch was opened.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Number of inner commands grouped into this batch.
    #[must_use]
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// True when the batch contains no commands.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }
}

impl Command for BatchCommand {
    fn apply(&self, world: &World) {
        for cmd in &self.commands {
            cmd.apply(world);
        }
    }

    fn invert(&self, world: &World) {
        for cmd in self.commands.iter().rev() {
            cmd.invert(world);
        }
    }
}
