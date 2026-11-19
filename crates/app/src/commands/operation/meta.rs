//! Undoable operation labels and positions within a setup.
//!
//! Provenance: our own document model and standard machining program order.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::schema::OperationMeta;
use crate::setup;

/// Change the user-facing label while retaining the entity's stable key.
pub struct RenameOperation {
    key: String,
    before: OperationMeta,
    after: OperationMeta,
}

impl RenameOperation {
    pub fn new(world: &World, key: &str, label: &str) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, key)
            .ok_or_else(|| CommandError::NotFound(key.to_owned()))?;
        if !model::is_operation(world, &entity) {
            return Err(CommandError::Invalid(format!("'{key}' is not an operation")));
        }
        let label = label.trim();
        if label.is_empty() || label.len() > 120 || label.chars().any(char::is_control) {
            return Err(CommandError::Invalid("operation name must be 1-120 printable characters".into()));
        }
        // Labels are unique, and cannot shadow another operation's stable
        // key: scripts and commands may still address that key.
        for (other, e) in setup::operation_order(world) {
            if other != key && (other == label || model::operation_label(world, &other, &e) == label) {
                return Err(CommandError::NameConflict(label.to_owned()));
            }
        }
        let before = model::operation_meta_of(world, &entity);
        let mut after = before.clone();
        after.label = (label != key).then(|| label.to_owned());
        if after == before {
            return Err(CommandError::Invalid("operation already has that name".into()));
        }
        Ok(Self { key: key.to_owned(), before, after })
    }
}

impl Command for RenameOperation {
    fn apply(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.key) {
            e.entity_view(world).set(self.after.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.key) {
            e.entity_view(world).set(self.before.clone());
        }
    }
}

/// Place one operation before or after another in the same setup.
pub struct ReorderOperation {
    changes: Vec<(String, OperationMeta, OperationMeta)>,
}

impl ReorderOperation {
    pub fn new(world: &World, key: &str, target: &str, after: bool) -> Result<Self, CommandError> {
        // #155: a Pause / Home entry reorders like an operation, and
        // every position is counted over the list it stands in.
        use crate::program_entry::{effective_setup, is_sequenced, sequence};
        let source = model::find_by_name(world, key)
            .filter(|e| is_sequenced(world, e))
            .ok_or_else(|| CommandError::NotFound(key.to_owned()))?;
        let destination = model::find_by_name(world, target)
            .filter(|e| is_sequenced(world, e))
            .ok_or_else(|| CommandError::NotFound(target.to_owned()))?;
        let source_setup = effective_setup(world, &source);
        let target_setup = effective_setup(world, &destination);
        if source_setup != target_setup {
            return Err(CommandError::Invalid("operations can only be reordered within one setup".into()));
        }
        let original: Vec<String> = sequence(world)
            .into_iter()
            .filter(|(_, e)| effective_setup(world, e) == source_setup)
            .map(|(name, _)| name)
            .collect();
        let mut reordered = original.clone();
        let Some(from) = reordered.iter().position(|n| n == key) else {
            return Err(CommandError::NotFound(key.to_owned()));
        };
        reordered.remove(from);
        let Some(to) = reordered.iter().position(|n| n == target) else {
            return Err(CommandError::Invalid("drop onto another operation".into()));
        };
        reordered.insert(to + usize::from(after), key.to_owned());
        if reordered == original {
            return Err(CommandError::Invalid("operation is already in that position".into()));
        }
        let mut changes = Vec::with_capacity(reordered.len());
        for (index, name) in reordered.into_iter().enumerate() {
            let order = u32::try_from(index)
                .map_err(|_| CommandError::Invalid("too many operations".into()))?;
            let Some(e) = model::find_by_name(world, &name) else {
                return Err(CommandError::NotFound(name));
            };
            let before = model::operation_meta_of(world, &e);
            let mut next = before.clone();
            next.order = Some(order);
            changes.push((name, before, next));
        }
        Ok(Self { changes })
    }
}

impl Command for ReorderOperation {
    fn apply(&self, world: &World) {
        for (name, _, next) in &self.changes {
            if let Some(e) = model::find_by_name(world, name) {
                e.entity_view(world).set(next.clone());
            }
        }
    }

    fn invert(&self, world: &World) {
        for (name, prev, _) in &self.changes {
            if let Some(e) = model::find_by_name(world, name) {
                e.entity_view(world).set(prev.clone());
            }
        }
    }
}

/// Clear migration `needs_review` notes once an operation has been reviewed (#1517).
pub struct MarkOperationReviewed {
    key: String,
    before: OperationMeta,
    after: OperationMeta,
}

impl MarkOperationReviewed {
    pub fn new(world: &World, key: &str) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, key)
            .ok_or_else(|| CommandError::NotFound(key.to_owned()))?;
        if !model::is_operation(world, &entity) {
            return Err(CommandError::Invalid(format!("'{key}' is not an operation")));
        }
        let before = model::operation_meta_of(world, &entity);
        if before.needs_review.is_empty() {
            return Err(CommandError::Invalid(format!(
                "operation '{key}' is not marked as needing review"
            )));
        }
        let after = OperationMeta {
            needs_review: Vec::new(),
            ..before.clone()
        };
        Ok(Self { key: key.to_owned(), before, after })
    }
}

impl Command for MarkOperationReviewed {
    fn apply(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.key) {
            e.entity_view(world).set(self.after.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(e) = model::find_by_name(world, &self.key) {
            e.entity_view(world).set(self.before.clone());
        }
    }
}
