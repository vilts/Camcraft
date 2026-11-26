//! Undoable "Do not touch" command: protected faces/edges and standoff clearance (#2055).
//! Provenance: our own domain model over `schema::Operation::area::do_not_touch`.

use flecs_ecs::prelude::World;

use crate::commands::{Command, CommandError};
use crate::geometry::feature;
use crate::model;
use crate::operation::{
    AREA_DISTANCE_MAX, DoNotTouch, FeatureSel, GenStamp, OpType, SrcRef, avoid_overlap_issue,
    describe_sel, do_not_touch_applies, do_not_touch_unsupported,
};

/// Validate `"Do not touch"` picks, clearance, op-kind support, and overlap against `src` / `area`.
pub(super) fn validate_do_not_touch(
    world: &World,
    kind: OpType,
    dnt: &DoNotTouch,
    src: Option<&SrcRef>,
    area: &crate::operation::AreaControls,
) -> Result<(), CommandError> {
    if !dnt.clearance.is_finite() || dnt.clearance < 0.0 || dnt.clearance > AREA_DISTANCE_MAX {
        return Err(CommandError::Invalid(format!(
            "'Do not touch' clearance must be finite and within 0..={AREA_DISTANCE_MAX} mm"
        )));
    }
    if dnt.is_default() {
        return Ok(());
    }
    if let Some(why) = do_not_touch_unsupported(kind) {
        return Err(CommandError::Invalid(why));
    }
    for g in &dnt.picks {
        let desc = describe_sel(g);
        // #1864: a DXF sketch pick is a 2D profile, not a model surface;
        // protection guards model faces and edge chains only.
        if matches!(g.feature, FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_)) {
            return Err(CommandError::Invalid(format!(
                "{desc} is a sketch pick - 'Do not touch' protects model faces or edge chains, not DXF sketch chains or layers"
            )));
        }
        if matches!(g.feature, FeatureSel::Vertex(_)) {
            return Err(CommandError::Invalid(format!(
                "{desc} is a vertex - 'Do not touch' protects faces or edge chains, not vertices"
            )));
        }
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            return Err(CommandError::Invalid(format!(
                "'Do not touch' names '{}', which does not exist",
                g.entity
            )));
        };
        let Some(features) = model::features_of(world, &entity) else {
            return Err(CommandError::Invalid(format!(
                "'{}' has no selectable features for 'Do not touch'",
                g.entity
            )));
        };
        let found = match g.feature {
            FeatureSel::Face(id) => features.faces.iter().any(|f| f.id == id),
            FeatureSel::Patch(id) => feature::patches(&features).iter().any(|p| p.id == id),
            FeatureSel::Chain(id) => feature::resolve_chain(&features, id).is_some(),
            FeatureSel::Vertex(_) | FeatureSel::SketchChain(_) | FeatureSel::SketchLayer(_) => false,
        };
        if !found {
            return Err(CommandError::Invalid(format!(
                "'Do not touch' {desc} is not in '{}' - pick a valid face, patch, or edge chain",
                g.entity
            )));
        }
    }
    if let Some(why) = avoid_overlap_issue(world, &dnt.picks, src, area) {
        return Err(CommandError::Invalid(why));
    }
    let _ = do_not_touch_applies(kind);
    Ok(())
}

/// Set an operation's `"Do not touch"` protected picks and standoff clearance.
pub struct SetOperationDoNotTouch {
    name: String,
    before: DoNotTouch,
    after: DoNotTouch,
    prev_stamp: Option<GenStamp>,
}

impl SetOperationDoNotTouch {
    pub fn new(world: &World, name: &str, dnt: DoNotTouch) -> Result<Self, CommandError> {
        let entity = model::find_by_name(world, name)
            .ok_or_else(|| CommandError::NotFound(name.to_owned()))?;
        let op = model::operation_of(world, &entity)
            .ok_or_else(|| CommandError::Invalid(format!("'{name}' is not an operation")))?;
        validate_do_not_touch(world, op.params.kind(), &dnt, Some(&op.src), &op.area)?;
        Ok(Self {
            name: name.to_owned(),
            before: op.area.do_not_touch.clone(),
            after: dnt,
            prev_stamp: op.stamp,
        })
    }
}

impl Command for SetOperationDoNotTouch {
    fn apply(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return;
        };
        let Some(mut op) = model::operation_of(world, &entity) else {
            return;
        };
        op.area.do_not_touch = self.after.clone();
        op.stamp = None;
        entity.entity_view(world).set(op);
    }

    fn invert(&self, world: &World) {
        let Some(entity) = model::find_by_name(world, &self.name) else {
            return;
        };
        let Some(mut op) = model::operation_of(world, &entity) else {
            return;
        };
        op.area.do_not_touch = self.before.clone();
        op.stamp = self.prev_stamp;
        entity.entity_view(world).set(op);
    }
}
