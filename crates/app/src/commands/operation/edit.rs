//! Operation edit commands: parameter edits, kind changes, and tool/source link retargeting.
//!
//! Provenance: our own domain model, following the same command pattern
//! as the import/stock siblings.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::model;
use crate::operation::{self, OpParams, SrcRef};
use crate::schema::Operation;

use super::gates::{area_gate, family_gate, heights_gate, infeed_plan_gate, sinker_gate, tool_door, tool_gate, valid_src, wire_gate};

/// Replace an operation's parameters. The cache is left alone and the
/// stamp is dropped, so the operation reads Stale immediately: the old
/// path stays on screen, visibly marked as not matching its definition.
pub struct EditOperationParams {
    name: String,
    prev: Operation,
    next: Operation,
}

impl EditOperationParams {
    pub fn new(world: &World, name: &str, params: OpParams) -> Result<Self, CommandError> {
        Self::edit(world, name, params, None)
    }

    /// The same edit carrying the area controls too (M11.5): one undo
    /// unit for the page's Apply, whichever half the user touched.
    pub fn with_area(
        world: &World,
        name: &str,
        params: OpParams,
        area: operation::AreaControls,
    ) -> Result<Self, CommandError> {
        Self::edit(world, name, params, Some(area))
    }

    /// Change only the coolant (#112): one undo unit, the parameters and
    /// the area as they stand. The coolant shapes the post's M words and
    /// nothing the generator cuts, so the stamp survives - the cached
    /// path is still exactly what this operation would generate.
    pub fn with_coolant(
        world: &World,
        name: &str,
        coolant: operation::CoolantMode,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let next = Operation { coolant, ..prev.clone() };
        Ok(EditOperationParams { name: name.to_string(), prev, next })
    }

    /// Change only the after-operation action (#110): one undo unit,
    /// the stamp survives (a stop or a trip home is post output, not
    /// geometry - the coolant's law).
    pub fn with_after(
        world: &World,
        name: &str,
        after: operation::AfterOperation,
    ) -> Result<Self, CommandError> {
        let prev = Self::current(world, name)?;
        let next = Operation { after, ..prev.clone() };
        Ok(EditOperationParams { name: name.to_string(), prev, next })
    }

    /// Suppress the operation, or bring it back (#110): one undo unit.
    /// The toolpath and the stamp stay - a suppressed op still
    /// generates and is simply left out of the posted program.
    pub fn with_suppressed(world: &World, name: &str, suppressed: bool) -> Result<Self, CommandError> {
        let prev = Self::current(world, name)?;
        let next = Operation { suppressed, ..prev.clone() };
        Ok(EditOperationParams { name: name.to_string(), prev, next })
    }

    fn current(world: &World, name: &str) -> Result<Operation, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))
    }

    fn edit(
        world: &World,
        name: &str,
        params: OpParams,
        area: Option<operation::AreaControls>,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        // The `OpParams` variant IS the operation's kind, so an edit that
        // carries a different variant would retype the operation in place -
        // replacing one kind's work with another's and reading as the op
        // vanishing from the list. The page carries no kind control at
        // all (M9.1); this is the defense-in-depth refusal behind it, and
        // it names the real door: ChangeOperationKind, the row's menu.
        if params.kind() != prev.params.kind() {
            return Err(CommandError::Invalid(format!(
                "'{name}' is a {} operation; change its kind with the retype menu on its browser row",
                prev.params.kind()
            )));
        }
        infeed_plan_gate(world, &params, &prev.tool)?;
        if let Some(why) = params.named_refusal() {
            return Err(CommandError::Invalid(why));
        }
        sinker_gate(world, prev.setup.as_deref(), &params)?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        heights_gate(world, prev.setup.as_deref(), &prev.src, &params)?;
        // #894: a thread's pitch, infeed and holes against the mounted thread mill.
        super::gates::thread_gate(world, &params, &prev.tool)?;
        if let Some(msg) = crate::operation::side_sane(&params, &prev.src) {
            return Err(CommandError::Invalid(msg));
        }
        // #532: an edited wire cut still names a condition the machine
        // states, with its wire and gap.
        wire_gate(world, prev.setup.as_deref(), &params)?;
        let area = area.unwrap_or_else(|| prev.area.clone());
        if !area.is_sane_for(&params) {
            return Err(CommandError::Invalid(match operation::area_unsupported(params.kind()) {
                None => "the Boundary settings are not usable - distances must be finite, within 10 m, the extension not negative, and every boundary must name its picks".to_string(),
                Some(why) => format!("{why}; its Boundary settings stay at their defaults"),
            }));
        }
        let picks: Vec<_> = area.picks().cloned().collect();
        if !picks.is_empty() {
            valid_src(world, prev.setup.as_deref(), &SrcRef::Selection(picks))?;
        }
        // The Apply door for the boundary shapes (M11.5 UX): an include /
        // exclude spelling an open chain is refused here, by name, never
        // first at Generate. Only a moved Boundary group is judged - a
        // plain parameter edit of an older document keeps working.
        if area != prev.area
            && let Some(msg) = operation::area_pick_issue(world, prev.setup.as_deref(), &area, &params)
        {
            return Err(CommandError::Invalid(msg));
        }
        let next = Operation {
            params,
            stamp: None,
            area,
            ..prev.clone()
        };
        Ok(EditOperationParams { name: name.to_string(), prev, next })
    }
}

impl Command for EditOperationParams {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

/// Change an operation's kind in place (M9.1): the retype the page's
/// Type combo used to do as a draft edit plus an Apply, now one honest
/// command from the op's context menu. The numbers ride
/// `operation::retype_params` - the SAME carry law a ribbon quick-add
/// and the old combo used, so re-typing never throws away feeds and
/// heights - and the result passes every door a creation passes:
/// family against the mounted machine, sanity, depth, side. The stamp
/// drops (the work definition changed; the cached path is another
/// kind's answer), and undo puts the whole old operation back.
pub struct ChangeOperationKind {
    name: String,
    prev: Operation,
    next: Operation,
}

impl ChangeOperationKind {
    pub fn new(world: &World, name: &str, kind: usize) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let params = operation::retype_params(&prev.params, kind);
        if params.kind() == prev.params.kind() {
            return Err(CommandError::Invalid(format!(
                "'{name}' is already a {} operation",
                prev.params.kind()
            )));
        }
        family_gate(world, prev.setup.as_deref(), &params)?;
        infeed_plan_gate(world, &params, &prev.tool)?;
        sinker_gate(world, prev.setup.as_deref(), &params)?;
        tool_door(world, &params, &prev.tool)?;
        wire_gate(world, prev.setup.as_deref(), &params)?;
        if !params.is_sane() {
            return Err(CommandError::Invalid(format!(
                "operation parameters are not usable for a {} operation",
                params.kind()
            )));
        }
        heights_gate(world, prev.setup.as_deref(), &prev.src, &params)?;
        if let Some(msg) = crate::operation::side_sane(&params, &prev.src) {
            return Err(CommandError::Invalid(msg));
        }
        // The retype is a creation of the new kind against the op's
        // current tool: the same tool-law door (M10.1 B).
        tool_gate(world, &params, &prev.tool)?;
        area_gate(world, prev.setup.as_deref(), &prev.src, &params)?;
        // The area controls ride the retype when the new kind has an
        // area (M11.5); a kind without one keeps the identity, never
        // controls it cannot honor.
        let area = if operation::area_applies(params.kind()) {
            prev.area.clone()
        } else {
            operation::AreaControls::default()
        };
        let next = Operation { params, stamp: None, area, ..prev.clone() };
        Ok(ChangeOperationKind { name: name.to_string(), prev, next })
    }
}

impl Command for ChangeOperationKind {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}

/// Retarget an operation (tool or source). Same staleness story as a
/// parameter edit: the cache survives, the stamp drops.
pub struct EditOperationLinks {
    name: String,
    prev: Operation,
    next: Operation,
}

impl EditOperationLinks {
    pub fn new(
        world: &World,
        name: &str,
        tool: Option<&str>,
        src: Option<SrcRef>,
    ) -> Result<Self, CommandError> {
        let Some(entity) = model::find_by_name(world, name) else {
            return Err(CommandError::NotFound(name.to_string()));
        };
        let prev = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        let mut next = Operation {
            stamp: None,
            ..prev.clone()
        };
        if let Some(tool) = tool {
            tool_door(world, &next.params, tool)?;
            // Reassigning the tool meets the tool law (M10.1 B); naming
            // the tool the op already holds is no reassignment, so a
            // source-only edit never trips over a legacy mismatch.
            if tool != prev.tool {
                tool_gate(world, &next.params, tool)?;
                // #890: the new tool's row for the document material
                // seeds the op's milling feed (same undo step).
                if let Some(t) = model::find_by_name(world, tool).and_then(|e| model::tool_of(world, &e))
                    && let Some(p) = crate::cutting_rows::reseeded_params(&next.params, &t, crate::cutting::material_of(world))
                {
                    next.params = p;
                }
            }
            next.tool = tool.to_string();
        }
        if let Some(src) = src {
            valid_src(world, prev.setup.as_deref(), &src)?;
            // Retargeting onto a whole-model source under Inside would
            // arm the mirror law; refused here, and backstopped at
            // generate for commands that arrive via file load.
            if let Some(msg) = crate::operation::side_sane(&next.params, &src) {
                return Err(CommandError::Invalid(msg));
            }
            // From: Selection heights re-resolve against the NEW source
            // (M11.3 A): one it cannot answer, or answers out of order,
            // is refused by name here. Params that never read the
            // selection pass exactly as before.
            let mut reads_sel = false;
            next.params.for_each_height(|h| reads_sel |= h.reference.is_selection());
            if reads_sel {
                heights_gate(world, prev.setup.as_deref(), &src, &next.params)?;
            }
            area_gate(world, prev.setup.as_deref(), &src, &next.params)?;
            next.src = src;
        }
        Ok(EditOperationLinks { name: name.to_string(), prev, next })
    }
}

impl Command for EditOperationLinks {
    fn apply(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.next.clone());
        }
    }

    fn invert(&self, world: &World) {
        if let Some(entity) = model::find_by_name(world, &self.name) {
            entity.entity_view(world).set(self.prev.clone());
        }
    }
}
