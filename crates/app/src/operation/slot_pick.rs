//! The plain Slot read off a geometry selection (#1504): two opposite
//! straight edges, or one face (or closed edge ring), are the room the
//! slot may cut in. The pick seeds the slot that fills the room into the
//! op's endpoints and width (in the pick's own command, so one undo
//! takes both back); generation and the preview refuse by name a slot
//! that does not fit the room - the confine law of
//! `machining2d::SlotBound`, shared with the trochoidal slot (#1503).
//! With nothing picked the typed endpoints rule, as before.
//!
//! Provenance: our own law over our own selection model; the plane
//! geometry is `machining2d::slot_room`'s. No vendor material consulted.

use super::*;
use crate::machining2d::{SlotBound, SlotFit};

/// The room a Slot's picks allow, as the world stands now: two open
/// edges give the strip between them, one face or closed ring its
/// region. Any other selection is refused by name - a slot is never
/// guessed from picks that do not bound one.
///
/// # Errors
/// A vanished pick, a point or wall pick, or a selection that is not
/// two opposite edges or one face.
pub fn slot_bound_of(world: &World, items: &[GeoSel]) -> Result<SlotBound, GenError> {
    let mut edges: Vec<Vec<Point2>> = Vec::new();
    let mut rings: Vec<Loop> = Vec::new();
    let mut areas = 0usize;
    for g in items {
        let Some(entity) = model::find_by_name(world, &g.entity) else {
            return Err(GenError::Invalid(format!("the selection names '{}', which is gone", g.entity)));
        };
        let Some(features) = model::features_of(world, &entity) else {
            return Err(GenError::Invalid(format!("'{}' has no geometry to select from", g.entity)));
        };
        let loc = model::transform_of(world, &entity).unwrap_or_default();
        let xy = |p: &[f64; 3]| {
            let w = transform_point(Point3::new(p[0], p[1], p[2]), loc.pos, loc.rot);
            Point2::new(w.x, w.y)
        };
        let label = format!("{} of '{}'", g.feature, g.entity);
        let gone = || GenError::Invalid(format!("{label} is no longer in the model: re-pick it"));
        match g.feature {
            FeatureSel::Face(fid) => {
                let f = features.faces.iter().find(|f| f.id == fid).ok_or_else(gone)?;
                for l in f.loops.iter().filter(|l| !matches!(l.kind, LoopKind::Open)) {
                    let pts: Vec<Point2> = l.verts.iter().filter_map(|&vi| features.verts.get(vi as usize)).map(xy).collect();
                    rings.push(Loop::new(pts).map_err(|_| {
                        GenError::Invalid(format!("{label} bounds no area seen from above - pick the slot's floor face or two opposite edges"))
                    })?);
                }
                areas += 1;
            }
            FeatureSel::Chain(cid) => {
                let c = feature::resolve_chain(&features, cid).ok_or_else(gone)?;
                let pts: Vec<Point2> = feature::chain_points(&features, &c).iter().map(xy).collect();
                if c.closed {
                    rings.push(Loop::new(pts).map_err(|_| {
                        GenError::Invalid(format!("{label} bounds no area seen from above - pick the slot's floor face or two opposite edges"))
                    })?);
                    areas += 1;
                } else {
                    edges.push(pts);
                }
            }
            FeatureSel::Patch(_) | FeatureSel::Vertex(_) => {
                return Err(GenError::Invalid(format!(
                    "{label} is not an edge or a face - a Slot is picked by two opposite straight edges or one floor face"
                )));
            }
        }
    }
    match (edges.as_slice(), areas) {
        ([e1, e2], 0) => SlotBound::from_edges(e1, e2),
        ([], 1) => SlotBound::from_face(Region::new(rings)?),
        _ => Err(GenError::Invalid(format!(
            "a Slot is picked by two opposite straight edges or one floor face - this selection has {} open edge{} and {} face{} or ring{}",
            edges.len(),
            if edges.len() == 1 { "" } else { "s" },
            areas,
            if areas == 1 { "" } else { "s" },
            if areas == 1 { "" } else { "s" },
        ))),
    }
}

/// The confine law at the doors that read the world (the preview): a
/// Slot with picks must lie inside the room they bound. `Ok` for every
/// other kind, and for a Slot without picks (the typed endpoints rule).
///
/// # Errors
/// The picks bound no slot room, or the slot does not fit it.
pub(super) fn confine_slot(world: &World, src: Option<&SrcRef>, params: &OpParams) -> Result<(), GenError> {
    match (params, src) {
        (OpParams::Slot { a, b, width, .. }, Some(SrcRef::Selection(items))) if !items.is_empty() => {
            slot_bound_of(world, items)?.judge(*a, *b, *width)
        }
        _ => Ok(()),
    }
}

/// The Slot `params` with its endpoints and width set to the slot that
/// fills the room `items` bound - what a pick seeds (`SetOperationGeometry`,
/// `CreateOperation`). `None` for another kind, or picks that bound no
/// slot room yet (one edge of two picked so far): the typed numbers
/// stand, and generation names why.
#[must_use]
pub fn slot_from_picks(world: &World, params: &OpParams, items: &[GeoSel]) -> Option<OpParams> {
    if !matches!(params, OpParams::Slot { .. }) || items.is_empty() {
        return None;
    }
    let fit = match slot_bound_of(world, items).and_then(|b| b.fit()) {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!("slot picks seed nothing: {e}");
            return None;
        }
    };
    let mut seeded = params.clone();
    if let OpParams::Slot { a, b, width, .. } = &mut seeded {
        let SlotFit { a: fa, b: fb, width: fw } = fit;
        (*a, *b, *width) = (fa, fb, fw);
    }
    Some(seeded)
}

/// What the op page echoes for a Slot with picks: the room and the
/// slot that fills it when the typed slot fits, else the refusal
/// generation would make. `None` without picks or for another kind.
#[must_use]
pub fn slot_fit_of(world: &World, op: &Operation) -> Option<Result<String, String>> {
    let (OpParams::Slot { a, b, width, .. }, SrcRef::Selection(items)) = (&op.params, &op.src) else {
        return None;
    };
    if items.is_empty() {
        return None;
    }
    let bound = match slot_bound_of(world, items) {
        Ok(bd) => bd,
        Err(e) => return Some(Err(e.to_string())),
    };
    if let Err(e) = bound.judge(*a, *b, *width) {
        return Some(Err(e.to_string()));
    }
    let (l, w) = bound.room();
    Some(Ok(format!("From selection: room {l:.3} x {w:.3} mm - the slot stays inside it")))
}
