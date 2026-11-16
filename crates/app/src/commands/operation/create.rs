//! Operation lifecycle commands: create a new operation entity, delete one with its cache.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::World;

use crate::cl::Toolpath as ClPath;
use crate::commands::{Command, CommandError};
use crate::model;
use crate::operation::{OpParams, SrcRef};
use crate::schema::{Location, Operation, OperationMeta, Toolpath};
use crate::setup;
use crate::verify::Warning;

use super::gates::{area_gate, family_gate, heights_gate, infeed_plan_gate, sinker_gate, tool_door, tool_gate, valid_src, wire_gate};

/// Create an operation entity. Deliberately does not generate: a new
/// operation reads "never generated" until `GenerateOperation` runs, the
/// same lifecycle the tree and status surface are built around.
pub struct CreateOperation {
    name: String,
    operation: Operation,
    /// The op's explicit position: the end of its setup's program order
    /// (#167), so the post and the tree run it after every sibling.
    meta: OperationMeta,
    /// Siblings whose positions were made explicit (before, after).
    reordered: Reordered,
}

/// `(key, before, after)` for each sibling whose position an append pins.
type Reordered = Vec<(String, OperationMeta, OperationMeta)>;

/// Pin every operation of the setup `context` resolves to at its current
/// program position (0..n) and return those changes with the next free
/// index `n` - so a new op lands last even when an older project's
/// siblings carry no stored order (their name order is kept, made explicit).
fn append_position(world: &World, context: Option<&str>) -> Result<(Reordered, u32), CommandError> {
    let effective = setup::effective(world, context);
    // #155: the list's Pause / Home entries hold positions too.
    let siblings: Vec<_> = crate::program_entry::sequence(world)
        .into_iter()
        .filter(|(_, entity)| crate::program_entry::effective_setup(world, entity) == effective)
        .collect();
    let mut reordered = Vec::with_capacity(siblings.len());
    for (index, (sibling, entity)) in siblings.into_iter().enumerate() {
        let order = u32::try_from(index)
            .map_err(|_| CommandError::Invalid("too many operations".into()))?;
        let before = model::operation_meta_of(world, &entity);
        let mut after = before.clone();
        after.order = Some(order);
        reordered.push((sibling, before, after));
    }
    let order = u32::try_from(reordered.len())
        .map_err(|_| CommandError::Invalid("too many operations".into()))?;
    Ok((reordered, order))
}

fn set_metas(world: &World, rows: &Reordered, after: bool) {
    for (name, before, next) in rows {
        if let Some(entity) = model::find_by_name(world, name) {
            entity.entity_view(world).set(if after { next.clone() } else { before.clone() });
        }
    }
}

impl CreateOperation {
    pub fn new(
        world: &World,
        name: &str,
        tool: &str,
        src: SrcRef,
        params: OpParams,
    ) -> Result<Self, CommandError> {
        if name.is_empty() {
            return Err(CommandError::Invalid("operation name must not be empty".into()));
        }
        if model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_string()));
        }
        tool_door(world, &params, tool)?;
        // The new operation JOINS the active setup (M3.2 A) - declared at
        // creation, never inferred later, so undo replays membership
        // exactly. With no setups the name is `None`: the implicit-setup
        // law makes that the first/legacy context byte-identically.
        let setup = crate::setup::active(world);
        valid_src(world, setup.as_deref(), &src)?;
        // #1504: a Slot created on picks takes the slot that fills them.
        let params = match &src {
            SrcRef::Selection(items) => crate::operation::slot_from_picks(world, &params, items).unwrap_or(params),
            _ => params,
        };
        infeed_plan_gate(world, &params, tool)?;
        if let Some(why) = params.named_refusal() {
            return Err(CommandError::Invalid(why));
        }
        sinker_gate(world, setup.as_deref(), &params)?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        // Ordering needs resolved heights, and the world is in hand right
        // here: an operation that would cut upward is refused at creation
        // (AGENTS.md: validate early), not deep in the generator - and
        // so is a From: Selection height its source cannot answer.
        heights_gate(world, setup.as_deref(), &src, &params)?;
        if let Some(msg) = crate::operation::side_sane(&params, &src) {
            return Err(CommandError::Invalid(msg));
        }
        family_gate(world, setup.as_deref(), &params)?;
        wire_gate(world, setup.as_deref(), &params)?;
        tool_gate(world, &params, tool)?;
        area_gate(world, setup.as_deref(), &src, &params)?;
        let (reordered, order) = append_position(world, setup.as_deref())?;
        // #869: a new op starts on its machine's default coolant - Air on
        // a machine whose only coolant is an air blast, Flood elsewhere -
        // so its first post never asks for a fluid the machine lacks.
        let coolant = crate::setup_mount::machine_of(world, setup.as_deref())
            .1
            .map_or_else(Default::default, |m| crate::operation::CoolantMode::machine_default(&m.coolants));
        Ok(CreateOperation {
            name: name.to_string(),
            meta: OperationMeta { label: None, order: Some(order) },
            reordered,
            operation: Operation {
                tool: tool.to_string(),
                src,
                params,
                stamp: None,
                setup,
                area: Default::default(),
                coolant,
                after: Default::default(),
                suppressed: false,
            },
        })
    }
}

impl Command for CreateOperation {
    fn apply(&self, world: &World) {
        set_metas(world, &self.reordered, true);
        let entity = match model::find_by_name(world, &self.name) {
            Some(entity) => entity,
            None => model::spawn(world, &self.name),
        };
        entity.entity_view(world).set(self.operation.clone());
        entity.entity_view(world).set(self.meta.clone());
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
        set_metas(world, &self.reordered, false);
    }
}

/// Paste a copied operation as one undoable change. Existing positions in
/// its setup are made explicit so the new operation lands at the end even
/// when an older project has no stored order indices.
pub struct PasteOperation {
    name: String,
    operation: Operation,
    meta: OperationMeta,
    reordered: Reordered,
}

impl PasteOperation {
    pub fn new(world: &World, name: &str, mut operation: Operation) -> Result<Self, CommandError> {
        if name.trim().is_empty() || model::find_by_name(world, name).is_some() {
            return Err(CommandError::NameConflict(name.to_owned()));
        }
        if let Some(setup_name) = &operation.setup
            && setup::def_of(world, setup_name).is_none()
        {
            return Err(CommandError::NotFound(setup_name.clone()));
        }
        let context = operation.setup.as_deref();
        tool_door(world, &operation.params, &operation.tool)?;
        valid_src(world, context, &operation.src)?;
        infeed_plan_gate(world, &operation.params, &operation.tool)?;
        sinker_gate(world, context, &operation.params)?;
        if !operation.params.is_sane() {
            return Err(CommandError::Invalid("copied operation parameters are not usable".into()));
        }
        heights_gate(world, context, &operation.src, &operation.params)?;
        if let Some(msg) = crate::operation::side_sane(&operation.params, &operation.src) {
            return Err(CommandError::Invalid(msg));
        }
        family_gate(world, context, &operation.params)?;
        wire_gate(world, context, &operation.params)?;
        tool_gate(world, &operation.params, &operation.tool)?;
        area_gate(world, context, &operation.src, &operation.params)?;
        operation.stamp = None;

        let (reordered, order) = append_position(world, context)?;
        Ok(Self {
            name: name.to_owned(),
            operation,
            meta: OperationMeta { label: None, order: Some(order) },
            reordered,
        })
    }
}

impl Command for PasteOperation {
    fn apply(&self, world: &World) {
        set_metas(world, &self.reordered, true);
        if model::find_by_name(world, &self.name).is_none() {
            let entity = model::spawn(world, &self.name);
            entity.entity_view(world).set(self.operation.clone());
            entity.entity_view(world).set(self.meta.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
        set_metas(world, &self.reordered, false);
    }
}

/// Delete an operation with its cached path; the snapshot restores both.
pub struct DeleteOperation {
    name: String,
    operation: Operation,
    cache: Option<(ClPath, Vec<Warning>)>,
    location: Location,
    meta: OperationMeta,
}

impl DeleteOperation {
    pub fn new(world: &World, name: &str) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let operation = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let cache = model::toolpath_of(world, &entity).map(|t| (t.path, t.warnings));
        Ok(DeleteOperation {
            name: name.to_string(),
            operation,
            cache,
            location: model::transform_of(world, &entity).unwrap_or_default(),
            meta: model::operation_meta_of(world, &entity),
        })
    }
}

impl Command for DeleteOperation {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            model::remove(world, &entity);
        }
    }

    fn invert(&self, world: &World) {
        if model::find_by_name(world, &self.name).is_some() {
            return; // already restored (idempotent redo path)
        }
        let entity = model::spawn(world, &self.name);
        model::set_transform(world, &entity, self.location);
        entity.entity_view(world).set(self.operation.clone());
        entity.entity_view(world).set(self.meta.clone());
        if let Some((path, warnings)) = &self.cache {
            entity.entity_view(world).set(Toolpath::new(path.clone(), warnings.clone()));
        }
    }
}
